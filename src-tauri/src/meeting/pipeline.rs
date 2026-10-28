//! Turns a finished recording into a transcript (PRD §7.2). Audio is read from disk in 5-minute
//! windows so an hour-long meeting never sits in memory as a whole (except the system track when
//! diarization runs, about 230 MB per hour).

use std::path::Path;

use anyhow::Result;

use super::merge::merge;
use super::recorder::{MIC_FILE, SYSTEM_FILE};
use crate::asr::diarize::{self, Diarizer};
use crate::asr::{Segment as AudioSegment, Transcript};
use crate::audio::resample::{read_wav_16k_mono, SAMPLE_RATE};
use crate::output::Segment;
use crate::pipeline::Loaded;
use crate::vocab::{rules, Rule};
use crate::voices::Voice;

const WINDOW: usize = 300 * SAMPLE_RATE as usize;
/// Speech that may continue past the window's end is left for the next window.
const MARGIN: usize = 30 * SAMPLE_RATE as usize;

pub const ME: &str = "Me";
pub const OTHERS: &str = "Others";

fn wav_len(path: &Path) -> Result<usize> {
    Ok(hound::WavReader::open(path)?.duration() as usize)
}

/// VAD + ASR over a 16 kHz 16-bit mono WAV, window by window. `on_samples` reports how many
/// samples have been consumed so far.
fn transcribe_file(
    loaded: &Loaded,
    path: &Path,
    mut on_samples: impl FnMut(usize),
) -> Result<Vec<Transcript>> {
    let mut reader = hound::WavReader::open(path)?;
    let mut samples = reader.samples::<i16>();
    let mut buf: Vec<f32> = Vec::with_capacity(WINDOW);
    let mut base = 0usize;
    let mut eof = false;
    let mut out = Vec::new();
    while !eof || !buf.is_empty() {
        while buf.len() < WINDOW {
            match samples.next() {
                // A crash can leave a partial last sample; stop there.
                Some(Ok(s)) => buf.push(s as f32 / 32768.0),
                Some(Err(_)) | None => {
                    eof = true;
                    break;
                }
            }
        }
        let segs = loaded.vad.segment(&buf)?;
        let (keep, cut) = if eof {
            (segs.len(), buf.len())
        } else {
            let limit = buf.len().saturating_sub(MARGIN);
            let keep = segs
                .iter()
                .take_while(|s| s.start_sample + s.samples.len() <= limit)
                .count();
            let cut = match (keep, segs.get(keep)) {
                (0, Some(next)) => next.start_sample.min(limit),
                (0, None) => limit,
                (k, _) => segs[k - 1].start_sample + segs[k - 1].samples.len(),
            };
            (keep, cut.max(1))
        };
        for s in segs.into_iter().take(keep) {
            let seg = AudioSegment {
                start_sample: base + s.start_sample,
                samples: s.samples,
            };
            let t = loaded.engine.transcribe(&seg)?;
            if !t.text.trim().is_empty() {
                out.push(t);
            }
        }
        base += cut;
        buf.drain(..cut.min(buf.len()));
        on_samples(base);
        if eof {
            break;
        }
    }
    Ok(out)
}

/// Learned spellings (§6.7) applied to one segment, keeping the model's text in `original_text`.
pub fn to_segment(t: Transcript, speaker: &str, vocab: &[Rule]) -> Segment {
    let new_text = rules::apply_text(&t.text, vocab);
    let words = rules::apply_words(&t.words, vocab).unwrap_or(t.words);
    let (text, original_text) = match new_text {
        Some(n) => (n, Some(t.text)),
        None => (t.text, None),
    };
    Segment {
        start: t.start,
        end: t.end,
        speaker: speaker.to_string(),
        text,
        words,
        original_text,
    }
}

/// A voiceprint per speaker label.
pub type Voiceprints = Vec<(String, Vec<f32>)>;

/// What processing a recording produces.
pub struct Processed {
    pub segments: Vec<Segment>,
    pub duration_secs: f32,
    /// Voiceprints by final speaker label, only when voice memory is on.
    pub voiceprints: Voiceprints,
    /// Seconds spent on each stage, for the local timing log.
    pub timings: Timings,
}

/// Seconds per processing stage. Durations only, never content.
#[derive(Debug, Default, Clone, Copy)]
pub struct Timings {
    pub mic_asr: f32,
    pub system_asr: f32,
    pub diarize: f32,
}

/// Splits `segs` into speakers with the diarizer, naming the voices it knows. Returns the
/// labelled segments and, when voice memory is on, a voiceprint per label.
fn diarize_track(
    d: &Diarizer,
    path: &Path,
    segs: Vec<Segment>,
    voices: Option<&[Voice]>,
    keep_single_as: Option<&str>,
) -> Result<(Vec<Segment>, Voiceprints)> {
    let audio = read_wav_16k_mono(path)?;
    let turns = d.turns(&audio)?;
    let mut ids: Vec<i32> = turns.iter().map(|t| t.speaker).collect();
    ids.sort_unstable();
    ids.dedup();
    // A mic track with one voice is just the user: keep "Me".
    if ids.len() <= 1 && keep_single_as.is_some() {
        return Ok((segs, Vec::new()));
    }
    let prints = match voices {
        Some(_) => d.voiceprints(&audio, &turns),
        None => Vec::new(),
    };
    let known = voices
        .map(|v| crate::voices::match_voices(&prints, v, crate::voices::MATCH_THRESHOLD))
        .unwrap_or_default();
    let labelled = diarize::label(segs, &turns, &known);
    // Map each cluster to the label it ended up with, for learning names later.
    let by_label = prints
        .into_iter()
        .filter_map(|(id, e)| {
            let label = known.get(&id).cloned().or_else(|| {
                turns.iter().filter(|t| t.speaker == id).find_map(|t| {
                    labelled
                        .iter()
                        .find(|s| s.start >= t.start - 0.5 && s.start <= t.end)
                        .map(|s| s.speaker.clone())
                })
            })?;
            Some((label, e))
        })
        .collect();
    Ok((labelled, by_label))
}

/// Transcribes the mic and system tracks in `dir`, labels speakers and merges them.
/// `voices` turns on voice memory (known names, and voiceprints back for learning).
/// `on_progress` receives the fraction done, 0.0 to 1.0.
pub fn process(
    loaded: &Loaded,
    diarizer: Option<&Diarizer>,
    dir: &Path,
    vocab: &[Rule],
    voices: Option<&[Voice]>,
    mut on_progress: impl FnMut(f32),
) -> Result<Processed> {
    let mic_path = dir.join(MIC_FILE);
    let sys_path = dir.join(SYSTEM_FILE);
    // Recordings interrupted by a crash or force quit have unfinished WAV headers.
    for p in [&mic_path, &sys_path] {
        if p.exists() {
            let _ = super::recorder::repair_wav(p);
        }
    }
    let mic_len = wav_len(&mic_path).unwrap_or(0);
    let sys_len = if sys_path.exists() {
        wav_len(&sys_path).unwrap_or(0)
    } else {
        0
    };
    // Diarization costs roughly half as much again as transcribing the track it runs on.
    let diar_track = if sys_len > 0 { sys_len } else { mic_len };
    let diar_weight = if diarizer.is_some() {
        diar_track / 2
    } else {
        0
    };
    let total = (mic_len + sys_len + diar_weight).max(1) as f32;

    let mut timings = Timings::default();
    let t = std::time::Instant::now();
    let mut mic: Vec<Segment> = if mic_len > 0 {
        transcribe_file(loaded, &mic_path, |n| on_progress(n as f32 / total))?
            .into_iter()
            .map(|t| to_segment(t, ME, vocab))
            .collect()
    } else {
        Vec::new()
    };
    timings.mic_asr = t.elapsed().as_secs_f32();
    let t = std::time::Instant::now();
    let mut system: Vec<Segment> = if sys_len > 0 {
        transcribe_file(loaded, &sys_path, |n| {
            on_progress((mic_len + n) as f32 / total)
        })?
        .into_iter()
        .map(|t| to_segment(t, OTHERS, vocab))
        .collect()
    } else {
        Vec::new()
    };
    timings.system_asr = t.elapsed().as_secs_f32();
    let t = std::time::Instant::now();
    let mut voiceprints = Vec::new();
    if let Some(d) = diarizer {
        if !system.is_empty() {
            let (s, p) = diarize_track(d, &sys_path, system, voices, None)?;
            system = s;
            voiceprints = p;
        } else if sys_len == 0 && !mic.is_empty() {
            // In-room meeting (mic only): split the room's voices too.
            let (m, p) = diarize_track(d, &mic_path, mic, voices, Some(ME))?;
            mic = m;
            voiceprints = p;
        }
    }
    timings.diarize = t.elapsed().as_secs_f32();
    on_progress(1.0);
    let duration_secs = mic_len.max(sys_len) as f32 / SAMPLE_RATE as f32;
    Ok(Processed {
        segments: merge(mic, system),
        duration_secs,
        voiceprints,
        timings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asr::Word;
    use crate::vocab::RuleMode;

    #[test]
    fn vocab_sets_original_text_only_when_it_changes_something() {
        let t = |text: &str| Transcript {
            start: 1.0,
            end: 2.0,
            text: text.into(),
            words: vec![Word {
                w: text.into(),
                s: 1.0,
                e: 2.0,
            }],
        };
        let rules = [Rule {
            id: 1,
            heard: "stefan".into(),
            spelling: "Stephan".into(),
            mode: RuleMode::Auto,
        }];
        let changed = to_segment(t("stefan"), ME, &rules);
        assert_eq!(changed.text, "Stephan");
        assert_eq!(changed.original_text.as_deref(), Some("stefan"));
        assert_eq!(changed.words[0].w, "Stephan");
        let same = to_segment(t("hello"), ME, &rules);
        assert_eq!(same.original_text, None);
    }
}
