//! On-device speaker diarization (PRD §6.3 P1): pyannote segmentation + TitaNet embeddings via
//! sherpa-onnx. Splits the system-audio track into speakers, named from voice memory when known
//! (`crate::voices`) and Speaker 1, Speaker 2, ... otherwise.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{anyhow, Result};
use sherpa_onnx::{
    FastClusteringConfig, OfflineSpeakerDiarization, OfflineSpeakerDiarizationConfig,
    OfflineSpeakerSegmentationModelConfig, OfflineSpeakerSegmentationPyannoteModelConfig,
    SpeakerEmbeddingExtractor, SpeakerEmbeddingExtractorConfig,
};

use crate::asr::Word;
use crate::models::{download, PYANNOTE_SEGMENTATION, TITANET_SMALL};
use crate::output::Segment;

/// A stretch of audio attributed to one speaker (0-based cluster index).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Turn {
    pub start: f32,
    pub end: f32,
    pub speaker: i32,
}

pub struct Diarizer {
    inner: OfflineSpeakerDiarization,
    /// TitaNet on its own, for one voiceprint per speaker (voice memory).
    extractor: SpeakerEmbeddingExtractor,
}

/// Up to this much of a speaker's audio goes into their voiceprint.
const VOICEPRINT_SECS: f32 = 30.0;
/// Speakers heard for less than this get no voiceprint (too little to be reliable).
const MIN_VOICEPRINT_SECS: f32 = 2.0;

impl Diarizer {
    /// Loads the diarization models if they're installed and match their pinned hashes.
    /// Returns Ok(None) when they aren't installed, so meetings still process without them.
    pub fn load(models_dir: &Path, threads: i32) -> Result<Option<Self>> {
        for spec in [&PYANNOTE_SEGMENTATION, &TITANET_SMALL] {
            if !download::verify(spec, models_dir).is_empty() {
                return Ok(None);
            }
        }
        let path = |spec: &crate::models::ModelSpec, name: &str| {
            Some(
                spec.file_path(models_dir, name)
                    .to_string_lossy()
                    .into_owned(),
            )
        };
        let config = OfflineSpeakerDiarizationConfig {
            segmentation: OfflineSpeakerSegmentationModelConfig {
                pyannote: OfflineSpeakerSegmentationPyannoteModelConfig {
                    model: path(&PYANNOTE_SEGMENTATION, "model.onnx"),
                    ..Default::default()
                },
                num_threads: threads,
                ..Default::default()
            },
            embedding: SpeakerEmbeddingExtractorConfig {
                model: path(&TITANET_SMALL, "nemo_en_titanet_small.onnx"),
                num_threads: threads,
                ..Default::default()
            },
            clustering: FastClusteringConfig::default(),
            ..Default::default()
        };
        let inner = OfflineSpeakerDiarization::create(&config)
            .ok_or_else(|| anyhow!("could not load the diarization models"))?;
        let extractor = SpeakerEmbeddingExtractor::create(&config.embedding)
            .ok_or_else(|| anyhow!("could not load the speaker embedding model"))?;
        Ok(Some(Self { inner, extractor }))
    }

    /// Speaker turns for 16 kHz mono audio, sorted by start time.
    pub fn turns(&self, samples: &[f32]) -> Result<Vec<Turn>> {
        let result = self
            .inner
            .process(samples)
            .ok_or_else(|| anyhow!("diarization failed"))?;
        Ok(result
            .sort_by_start_time()
            .into_iter()
            .map(|s| Turn {
                start: s.start,
                end: s.end,
                speaker: s.speaker,
            })
            .collect())
    }
}

impl Diarizer {
    /// One normalized voiceprint per speaker, from their longest turns (up to 30 s).
    pub fn voiceprints(&self, samples: &[f32], turns: &[Turn]) -> Vec<(i32, Vec<f32>)> {
        let rate = crate::audio::resample::SAMPLE_RATE as f32;
        let mut ids: Vec<i32> = turns.iter().map(|t| t.speaker).collect();
        ids.sort_unstable();
        ids.dedup();
        let mut out = Vec::new();
        for id in ids {
            let mut mine: Vec<&Turn> = turns.iter().filter(|t| t.speaker == id).collect();
            mine.sort_by(|a, b| (b.end - b.start).total_cmp(&(a.end - a.start)));
            let Some(stream) = self.extractor.create_stream() else {
                continue;
            };
            let mut secs = 0.0;
            for t in mine {
                if secs >= VOICEPRINT_SECS {
                    break;
                }
                let a = ((t.start * rate) as usize).min(samples.len());
                let b = ((t.end * rate) as usize).min(samples.len());
                if b > a {
                    stream.accept_waveform(rate as i32, &samples[a..b]);
                    secs += (b - a) as f32 / rate;
                }
            }
            stream.input_finished();
            if secs < MIN_VOICEPRINT_SECS || !self.extractor.is_ready(&stream) {
                continue;
            }
            if let Some(mut e) = self.extractor.compute(&stream) {
                crate::voices::normalize(&mut e);
                out.push((id, e));
            }
        }
        out
    }
}

/// The speaker whose turn covers time `t`, or the nearest turn if none does.
fn speaker_at(turns: &[Turn], t: f32) -> Option<i32> {
    turns
        .iter()
        .find(|x| x.start <= t && t <= x.end)
        .or_else(|| {
            turns.iter().min_by(|a, b| {
                let da = (a.start - t).abs().min((a.end - t).abs());
                let db = (b.start - t).abs().min((b.end - t).abs());
                da.total_cmp(&db)
            })
        })
        .map(|x| x.speaker)
}

/// Splits each segment where the speaker changes, word by word, and names speakers
/// "Speaker 1", "Speaker 2", ... in order of first appearance.
pub fn label(segments: Vec<Segment>, turns: &[Turn], known: &HashMap<i32, String>) -> Vec<Segment> {
    if turns.is_empty() {
        return segments;
    }
    // Unknown voices are numbered in order of first appearance; known ones keep their names.
    let mut order: Vec<i32> = Vec::new();
    let mut name = |id: i32| {
        if let Some(n) = known.get(&id) {
            return n.clone();
        }
        let i = order.iter().position(|x| *x == id).unwrap_or_else(|| {
            order.push(id);
            order.len() - 1
        });
        format!("Speaker {}", i + 1)
    };
    let mut out = Vec::new();
    for seg in segments {
        if seg.words.is_empty() {
            let mid = (seg.start + seg.end) / 2.0;
            let speaker = speaker_at(turns, mid)
                .map(&mut name)
                .unwrap_or(seg.speaker.clone());
            out.push(Segment { speaker, ..seg });
            continue;
        }
        let mut runs: Vec<(i32, Vec<Word>)> = Vec::new();
        for w in seg.words {
            let id = speaker_at(turns, (w.s + w.e) / 2.0).unwrap_or(-1);
            match runs.last_mut() {
                Some((last, ws)) if *last == id => ws.push(w),
                _ => runs.push((id, vec![w])),
            }
        }
        let single = runs.len() == 1;
        for (id, words) in runs {
            let speaker = if id < 0 {
                seg.speaker.clone()
            } else {
                name(id)
            };
            let text = words
                .iter()
                .map(|w| w.w.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            out.push(Segment {
                start: words.first().map_or(seg.start, |w| w.s),
                end: words.last().map_or(seg.end, |w| w.e),
                speaker,
                // Keep the segment's exact text when it isn't split.
                text: if single { seg.text.clone() } else { text },
                original_text: if single {
                    seg.original_text.clone()
                } else {
                    None
                },
                words,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_segments_at_speaker_changes() {
        let w = |w: &str, s: f32| Word {
            w: w.into(),
            s,
            e: s + 0.4,
        };
        let seg = Segment {
            start: 0.0,
            end: 4.0,
            speaker: "Others".into(),
            text: "Hi there. Hello back.".into(),
            words: vec![
                w("Hi", 0.0),
                w("there.", 0.5),
                w("Hello", 2.5),
                w("back.", 3.0),
            ],
            original_text: None,
        };
        let turns = [
            Turn {
                start: 0.0,
                end: 2.0,
                speaker: 4,
            },
            Turn {
                start: 2.2,
                end: 4.0,
                speaker: 1,
            },
        ];
        let out = label(vec![seg.clone()], &turns, &HashMap::new());
        let named = label(vec![seg], &turns, &HashMap::from([(1, "Sam".to_string())]));
        assert_eq!(
            (named[0].speaker.as_str(), named[1].speaker.as_str()),
            ("Speaker 1", "Sam")
        );
        assert_eq!(out.len(), 2);
        assert_eq!(
            (out[0].speaker.as_str(), out[0].text.as_str()),
            ("Speaker 1", "Hi there.")
        );
        assert_eq!(
            (out[1].speaker.as_str(), out[1].text.as_str()),
            ("Speaker 2", "Hello back.")
        );
        assert_eq!(out[1].start, 2.5);
    }

    #[test]
    fn no_turns_leaves_segments_alone() {
        let seg = Segment {
            start: 0.0,
            end: 1.0,
            speaker: "Others".into(),
            text: "x".into(),
            words: vec![],
            original_text: None,
        };
        assert_eq!(label(vec![seg.clone()], &[], &HashMap::new()), vec![seg]);
    }
}
