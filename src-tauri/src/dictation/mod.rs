//! Dictation mode (PRD §6.1): hold a key, speak, and the text is pasted where you're typing.

pub mod cleanup;
pub mod hotkey;
pub mod insert;

use anyhow::Result;

use crate::pipeline::{self, Loaded};
use crate::vocab::{rules, Rule};

/// Recordings shorter than this are treated as accidental taps.
pub const MIN_SECS: f32 = 0.3;

/// Transcribes one dictation: VAD + Parakeet, then learned spellings, then cleanup (PRD §7.2).
/// Returns None when nothing was said.
pub fn transcribe(
    loaded: &Loaded,
    samples: &[f32],
    vocab: &[Rule],
    remove_fillers: bool,
) -> Result<Option<String>> {
    let run = pipeline::transcribe(loaded, samples)?;
    let raw = pipeline::full_text(&run);
    if raw.trim().is_empty() {
        return Ok(None);
    }
    let text = rules::apply_text(&raw, vocab).unwrap_or(raw);
    let text = cleanup::clean(&text, remove_fillers);
    Ok((!text.is_empty()).then_some(text))
}
