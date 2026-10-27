//! Parakeet TDT 0.6B (v2 by default, int8) through sherpa-onnx. Load once, keep warm, transcribe ≤30 s segments.

use std::path::Path;

use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use sherpa_onnx::{OfflineRecognizer, OfflineRecognizerConfig, OfflineTransducerModelConfig};

use super::vad::{Segment, MAX_SEGMENT_SECS};
use crate::audio::resample::SAMPLE_RATE;
use crate::models::{manifest::ModelSpec, PARAKEET_V2_INT8};

/// A word with its start and end time in seconds, in the original audio's timeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Word {
    pub w: String,
    pub s: f32,
    pub e: f32,
}

/// The result for one segment.
#[derive(Debug, Clone, Serialize)]
pub struct Transcript {
    pub start: f32,
    pub end: f32,
    pub text: String,
    pub words: Vec<Word>,
}

pub struct Engine {
    recognizer: OfflineRecognizer,
}

impl Engine {
    /// Loads Parakeet from `models_dir`. Callers should verify the files first (`models::download::verify`).
    pub fn load(models_dir: &Path, threads: i32) -> Result<Self> {
        Self::load_with_decoding(models_dir, threads, false)
    }

    /// `beam` switches from greedy decoding to modified beam search (slower, sometimes more accurate).
    pub fn load_with_decoding(models_dir: &Path, threads: i32, beam: bool) -> Result<Self> {
        Self::load_model(models_dir, &PARAKEET_V2_INT8, threads, beam)
    }

    /// Loads a specific Parakeet transducer model from the manifest.
    pub fn load_model(
        models_dir: &Path,
        spec: &ModelSpec,
        threads: i32,
        beam: bool,
    ) -> Result<Self> {
        let path = |name: &str| {
            Some(
                spec.file_path(models_dir, name)
                    .to_string_lossy()
                    .into_owned(),
            )
        };
        let mut config = OfflineRecognizerConfig::default();
        config.model_config.transducer = OfflineTransducerModelConfig {
            encoder: path("encoder.int8.onnx"),
            decoder: path("decoder.int8.onnx"),
            joiner: path("joiner.int8.onnx"),
        };
        config.model_config.tokens = path("tokens.txt");
        config.model_config.model_type = Some("nemo_transducer".into());
        config.model_config.num_threads = threads;
        config.decoding_method = Some(
            if beam {
                "modified_beam_search"
            } else {
                "greedy_search"
            }
            .into(),
        );
        let recognizer = OfflineRecognizer::create(&config).ok_or_else(|| {
            anyhow!(
                "could not load the Parakeet model from {}",
                models_dir.display()
            )
        })?;
        Ok(Self { recognizer })
    }

    /// Transcribes one segment. Refuses audio longer than the 30 s encoder cap.
    pub fn transcribe(&self, seg: &Segment) -> Result<Transcript> {
        if seg.duration_secs() > MAX_SEGMENT_SECS + 0.01 {
            bail!(
                "segment is {:.1} s; the encoder limit is {MAX_SEGMENT_SECS} s",
                seg.duration_secs()
            );
        }
        let stream = self.recognizer.create_stream();
        stream.accept_waveform(SAMPLE_RATE as i32, &seg.samples);
        self.recognizer.decode(&stream);
        let result = stream
            .get_result()
            .ok_or_else(|| anyhow!("no result from the recognizer"))?;
        let offset = seg.start_secs();
        let end = offset + seg.duration_secs();
        let words = words_from_tokens(&result.tokens, result.timestamps.as_deref(), offset, end);
        Ok(Transcript {
            start: offset,
            end,
            text: result.text.trim().to_string(),
            words,
        })
    }
}

/// Groups SentencePiece tokens ("▁" marks a word start) into words with real model timestamps.
/// A word ends where the next word starts (or at the segment end).
pub fn words_from_tokens(
    tokens: &[String],
    timestamps: Option<&[f32]>,
    offset: f32,
    seg_end: f32,
) -> Vec<Word> {
    let Some(ts) = timestamps else {
        return Vec::new();
    };
    if ts.len() != tokens.len() {
        return Vec::new();
    }
    let mut words: Vec<Word> = Vec::new();
    // A lone "▁" token (SentencePiece emits one before digits) starts the next piece's word.
    let mut pending_start = false;
    for (tok, &t) in tokens.iter().zip(ts) {
        let starts_word =
            pending_start || tok.starts_with('▁') || tok.starts_with(' ') || words.is_empty();
        let piece = tok.trim_start_matches('▁').trim_start();
        if piece.is_empty() {
            pending_start |= starts_word;
            continue;
        }
        pending_start = false;
        match words.last_mut() {
            Some(last) if !starts_word => last.w.push_str(piece),
            _ => words.push(Word {
                w: piece.to_string(),
                s: offset + t,
                e: offset + t,
            }),
        }
    }
    for i in 0..words.len() {
        let next = if i + 1 < words.len() {
            words[i + 1].s
        } else {
            seg_end
        };
        words[i].e = next.max(words[i].s);
        words[i].s = round2(words[i].s);
        words[i].e = round2(words[i].e);
    }
    words
}

fn round2(x: f32) -> f32 {
    (x * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_sentencepiece_tokens_into_timed_words() {
        let tokens: Vec<String> = ["▁Th", "anks", "▁every", "one", "."]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let ts = [0.0, 0.16, 0.40, 0.64, 0.80];
        let w = words_from_tokens(&tokens, Some(&ts), 4.0, 5.2);
        assert_eq!(
            w,
            vec![
                Word {
                    w: "Thanks".into(),
                    s: 4.0,
                    e: 4.4
                },
                Word {
                    w: "everyone.".into(),
                    s: 4.4,
                    e: 5.2
                },
            ]
        );
    }

    #[test]
    fn lone_boundary_token_starts_a_new_word() {
        let tokens: Vec<String> = ["▁about", "▁", "1", "0", "%"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let ts = [0.0, 0.3, 0.32, 0.4, 0.5];
        let w: Vec<String> = words_from_tokens(&tokens, Some(&ts), 0.0, 1.0)
            .into_iter()
            .map(|w| w.w)
            .collect();
        assert_eq!(w, vec!["about", "10%"]);
    }

    #[test]
    fn no_words_without_timestamps() {
        assert!(words_from_tokens(&["▁hi".into()], None, 0.0, 1.0).is_empty());
    }
}
