//! Voice memory: recognise people by voice in later meetings, after the user names them once.
//! A "voice" is a TitaNet speaker embedding (a list of numbers, not audio), kept only in the
//! local SQLite library. Off by default: voiceprints are biometric data and need approval and
//! consent before use (PRD §6.3).

use std::collections::HashMap;

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Voice {
    pub id: i64,
    pub name: String,
    #[serde(skip)]
    pub embedding: Vec<f32>,
    /// How many meetings this voice was learned from.
    pub samples: u32,
}

/// Similarity two TitaNet embeddings of the same person usually exceed. Calibrated on
/// synthetic voices only (PRD §19); to verify on real meetings.
pub const MATCH_THRESHOLD: f32 = 0.6;

pub fn normalize(v: &mut [f32]) {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n > 0.0 {
        v.iter_mut().for_each(|x| *x /= n);
    }
}

pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        dot / (na * nb)
    }
}

/// Assigns known names to this meeting's speakers: best matches first, each name used once,
/// only above `threshold`. Speakers without a match stay unnamed ("Speaker N").
pub fn match_voices(
    speakers: &[(i32, Vec<f32>)],
    voices: &[Voice],
    threshold: f32,
) -> HashMap<i32, String> {
    let mut pairs: Vec<(f32, i32, usize)> = Vec::new();
    for (sid, emb) in speakers {
        for (vi, v) in voices.iter().enumerate() {
            let s = cosine(emb, &v.embedding);
            if s >= threshold {
                pairs.push((s, *sid, vi));
            }
        }
    }
    pairs.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut out = HashMap::new();
    let mut used = Vec::new();
    for (_, sid, vi) in pairs {
        if out.contains_key(&sid) || used.contains(&vi) {
            continue;
        }
        used.push(vi);
        out.insert(sid, voices[vi].name.clone());
    }
    out
}

/// Folds a new sample into a saved voice (running average), so it improves with each meeting.
pub fn merge(saved: &[f32], samples: u32, new: &[f32]) -> Vec<f32> {
    if saved.len() != new.len() || samples == 0 {
        let mut v = new.to_vec();
        normalize(&mut v);
        return v;
    }
    let w = samples as f32;
    let mut v: Vec<f32> = saved
        .iter()
        .zip(new)
        .map(|(a, b)| (a * w + b) / (w + 1.0))
        .collect();
    normalize(&mut v);
    v
}

pub fn to_bytes(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

pub fn from_bytes(b: &[u8]) -> Vec<f32> {
    b.as_chunks::<4>()
        .0
        .iter()
        .map(|c| f32::from_le_bytes(*c))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn voice(id: i64, name: &str, e: Vec<f32>) -> Voice {
        Voice {
            id,
            name: name.into(),
            embedding: e,
            samples: 1,
        }
    }

    #[test]
    fn matches_best_pairs_once_and_leaves_strangers_unnamed() {
        let voices = [
            voice(1, "Jordan", vec![1.0, 0.0, 0.0]),
            voice(2, "Sam", vec![0.0, 1.0, 0.0]),
        ];
        let speakers = [
            (0, vec![0.9, 0.1, 0.0]),
            (1, vec![0.8, 0.3, 0.0]),
            (2, vec![0.0, 0.0, 1.0]),
        ];
        let m = match_voices(&speakers, &voices, 0.6);
        assert_eq!(m.get(&0).map(String::as_str), Some("Jordan"));
        assert_eq!(
            m.get(&1),
            None,
            "Jordan is already taken, and Sam is too far"
        );
        assert_eq!(m.get(&2), None);
    }

    #[test]
    fn merge_and_bytes_round_trip() {
        let m = merge(&[1.0, 0.0], 1, &[0.0, 1.0]);
        assert!((m[0] - m[1]).abs() < 1e-6 && (cosine(&m, &[1.0, 1.0]) - 1.0).abs() < 1e-6);
        assert_eq!(from_bytes(&to_bytes(&[0.5, -2.0])), vec![0.5, -2.0]);
    }
}
