//! Silero VAD: drops silence and splits speech into segments the encoder can take in one pass.

use std::path::Path;

use anyhow::{anyhow, Result};
use sherpa_onnx::{SileroVadModelConfig, VadModelConfig, VoiceActivityDetector};

use crate::audio::resample::SAMPLE_RATE;

/// Hard cap on audio sent to the ASR encoder in one call (PRD §7.2, CLAUDE.md).
pub const MAX_SEGMENT_SECS: f32 = 30.0;
/// The VAD splits a bit before the cap so segments never reach it.
const VAD_MAX_SPEECH_SECS: f32 = 29.5;
/// Audio kept on each side of detected speech, so word onsets and endings aren't clipped.
const PAD_SECS: f32 = 0.25;
/// Neighbouring speech separated by a pause this short is sent to the encoder together:
/// fewer cuts mid-sentence and more context per pass.
const MERGE_GAP_SECS: f32 = 1.0;
/// Merged segments stay well under the encoder cap.
const MERGE_MAX_SECS: f32 = 25.0;

/// Tunable VAD settings. Defaults are the values validated in PRD §18.
#[derive(Debug, Clone, Copy)]
pub struct VadSettings {
    /// Speech probability above which audio counts as speech (lower keeps quieter voices).
    pub threshold: f32,
    pub min_silence_secs: f32,
    /// Longest merged segment sent to the encoder (always kept under the 30 s cap).
    pub merge_max_secs: f32,
    /// Longest stretch of continuous speech the VAD emits before forcing a split.
    pub max_speech_secs: f32,
}

impl Default for VadSettings {
    fn default() -> Self {
        Self {
            threshold: 0.5,
            min_silence_secs: 0.5,
            merge_max_secs: MERGE_MAX_SECS,
            max_speech_secs: VAD_MAX_SPEECH_SECS,
        }
    }
}

/// One stretch of speech, positioned in the original audio.
#[derive(Debug, Clone)]
pub struct Segment {
    pub start_sample: usize,
    pub samples: Vec<f32>,
}

impl Segment {
    pub fn start_secs(&self) -> f32 {
        self.start_sample as f32 / SAMPLE_RATE as f32
    }

    pub fn duration_secs(&self) -> f32 {
        self.samples.len() as f32 / SAMPLE_RATE as f32
    }
}

pub struct Vad {
    model: String,
    threads: i32,
    settings: VadSettings,
}

impl Vad {
    pub fn new(model: &Path, threads: i32) -> Self {
        Self::with_settings(model, threads, VadSettings::default())
    }

    pub fn with_settings(model: &Path, threads: i32, settings: VadSettings) -> Self {
        Self {
            model: model.to_string_lossy().into_owned(),
            threads,
            settings,
        }
    }

    /// Splits 16 kHz mono audio into speech segments of at most 30 s.
    pub fn segment(&self, samples: &[f32]) -> Result<Vec<Segment>> {
        let config = VadModelConfig {
            silero_vad: SileroVadModelConfig {
                model: Some(self.model.clone()),
                threshold: self.settings.threshold,
                min_silence_duration: self.settings.min_silence_secs,
                min_speech_duration: 0.25,
                window_size: 512,
                max_speech_duration: self.settings.max_speech_secs.min(VAD_MAX_SPEECH_SECS),
            },
            sample_rate: SAMPLE_RATE as i32,
            num_threads: self.threads,
            ..Default::default()
        };
        let buffer_secs = (samples.len() as f32 / SAMPLE_RATE as f32).max(60.0) + 5.0;
        let vad = VoiceActivityDetector::create(&config, buffer_secs)
            .ok_or_else(|| anyhow!("could not load the Silero VAD model"))?;

        let mut out = Vec::new();
        let window = 512;
        for chunk in samples.chunks(window) {
            vad.accept_waveform(chunk);
            drain(&vad, &mut out);
        }
        vad.flush();
        drain(&vad, &mut out);
        let spans: Vec<(usize, usize)> = out
            .iter()
            .map(|s| (s.start_sample, s.start_sample + s.samples.len()))
            .collect();
        let merge_max = self.settings.merge_max_secs.min(VAD_MAX_SPEECH_SECS);
        let merged = pad_and_merge_with(&spans, samples.len(), merge_max);
        let segments = merged
            .into_iter()
            .map(|(a, b)| Segment {
                start_sample: a,
                samples: samples[a..b].to_vec(),
            })
            .collect();
        Ok(enforce_cap(segments))
    }
}

fn drain(vad: &VoiceActivityDetector, out: &mut Vec<Segment>) {
    while let Some(seg) = vad.front() {
        out.push(Segment {
            start_sample: seg.start().max(0) as usize,
            samples: seg.samples().to_vec(),
        });
        vad.pop();
    }
}

/// Pads each speech span and merges spans separated by short pauses (sample ranges, end-exclusive).
pub fn pad_and_merge(spans: &[(usize, usize)], len: usize) -> Vec<(usize, usize)> {
    pad_and_merge_with(spans, len, MERGE_MAX_SECS)
}

fn pad_and_merge_with(
    spans: &[(usize, usize)],
    len: usize,
    merge_max_secs: f32,
) -> Vec<(usize, usize)> {
    let rate = SAMPLE_RATE as f32;
    let pad = (PAD_SECS * rate) as usize;
    let gap = (MERGE_GAP_SECS * rate) as usize;
    let max = (merge_max_secs * rate) as usize;
    let mut out: Vec<(usize, usize)> = Vec::new();
    for &(a, b) in spans {
        let a = a.saturating_sub(pad);
        let b = (b + pad).min(len);
        if a >= b {
            continue;
        }
        match out.last_mut() {
            Some(last) if a <= last.1 + gap && b - last.0 <= max => last.1 = last.1.max(b),
            Some(last) if a < last.1 => {
                // Overlaps but would grow too long: start the new span where the last one ends.
                let start = last.1;
                if start < b {
                    out.push((start, b));
                }
            }
            _ => out.push((a, b)),
        }
    }
    out
}

/// Belt and braces: split anything that still exceeds the cap into equal pieces.
pub fn enforce_cap(segments: Vec<Segment>) -> Vec<Segment> {
    let cap = (MAX_SEGMENT_SECS * SAMPLE_RATE as f32) as usize;
    let mut out = Vec::with_capacity(segments.len());
    for seg in segments {
        if seg.samples.len() <= cap {
            out.push(seg);
            continue;
        }
        let pieces = seg.samples.len().div_ceil(cap);
        let size = seg.samples.len().div_ceil(pieces);
        for (i, part) in seg.samples.chunks(size).enumerate() {
            out.push(Segment {
                start_sample: seg.start_sample + i * size,
                samples: part.to_vec(),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pads_and_merges_close_speech() {
        let sr = SAMPLE_RATE as usize;
        // Two phrases 0.5 s apart merge; a third 3 s later stays separate. Padding is 0.25 s.
        let spans = [(sr, 2 * sr), (2 * sr + sr / 2, 4 * sr), (7 * sr, 8 * sr)];
        let out = pad_and_merge(&spans, 10 * sr);
        assert_eq!(
            out,
            vec![
                (sr - sr / 4, 4 * sr + sr / 4),
                (7 * sr - sr / 4, 8 * sr + sr / 4)
            ]
        );
        // Padding never runs past the audio.
        assert_eq!(pad_and_merge(&[(0, sr)], sr), vec![(0, sr)]);
    }

    #[test]
    fn never_merges_past_the_merge_limit() {
        let sr = SAMPLE_RATE as usize;
        let spans: Vec<(usize, usize)> =
            (0..10).map(|i| (i * 6 * sr, i * 6 * sr + 5 * sr)).collect();
        let out = pad_and_merge(&spans, 70 * sr);
        assert!(out
            .iter()
            .all(|(a, b)| b - a <= (MERGE_MAX_SECS * SAMPLE_RATE as f32) as usize));
        assert!(
            out.windows(2).all(|w| w[0].1 <= w[1].0),
            "spans overlap: {out:?}"
        );
    }

    #[test]
    fn splits_overlong_segments_under_the_cap() {
        let long = Segment {
            start_sample: 1000,
            samples: vec![0.0; 75 * SAMPLE_RATE as usize],
        };
        let out = enforce_cap(vec![long]);
        assert_eq!(out.len(), 3);
        assert!(out.iter().all(|s| s.duration_secs() <= MAX_SEGMENT_SECS));
        assert_eq!(out[1].start_sample, 1000 + out[0].samples.len());
        assert_eq!(
            out.iter().map(|s| s.samples.len()).sum::<usize>(),
            75 * SAMPLE_RATE as usize
        );
    }
}
