//! The unified meeting transcript (PRD §6.4): one Markdown file and one JSON sidecar.

pub mod json;
pub mod markdown;
pub mod timestamps;

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

use crate::asr::Word;

pub const AI_TAG: &str = "[AI: DRAFT]";
pub const CONFIDENTIALITY: &str = "Internal. May contain confidential client information.";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Segment {
    pub start: f32,
    pub end: f32,
    pub speaker: String,
    pub text: String,
    pub words: Vec<Word>,
    /// The model's raw text, present only when a learned spelling changed it (PRD §6.7).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Meeting {
    pub title: String,
    pub started_at: DateTime<Local>,
    pub duration_secs: f32,
    pub asr_model: String,
    pub segments: Vec<Segment>,
    /// Minutes and summary written by Claude through the PushTalks connector (or by hand).
    /// Markdown; shown under "## Notes" and stored as `notes` in the JSON sidecar.
    #[serde(default)]
    pub notes: Option<String>,
}

impl Meeting {
    /// The text indexed for library search: notes plus every line with its speaker.
    pub fn search_body(&self) -> String {
        let lines = self
            .segments
            .iter()
            .map(|s| format!("{}: {}", s.speaker, s.text));
        self.notes
            .iter()
            .cloned()
            .chain(lines)
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Speakers in order of first appearance.
    pub fn speakers(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for s in &self.segments {
            if !out.contains(&s.speaker) {
                out.push(s.speaker.clone());
            }
        }
        out
    }

    pub fn duration_minutes(&self) -> u32 {
        (self.duration_secs / 60.0).round() as u32
    }

    /// `YYYY-MM-DD_HHMM_<slug>` without extension.
    pub fn file_stem(&self) -> String {
        format!(
            "{}_{}",
            self.started_at.format("%Y-%m-%d_%H%M"),
            slugify(&self.title)
        )
    }

    /// Writes `<stem>.md` and `<stem>.json` into `dir`, adding a suffix if the name is taken
    /// by another meeting. Returns the Markdown path.
    pub fn write(&self, dir: &Path, existing: Option<&Path>) -> Result<PathBuf> {
        fs::create_dir_all(dir)?;
        let stem = self.file_stem();
        let mut md = dir.join(format!("{stem}.md"));
        let mut n = 2;
        while md.exists() && Some(md.as_path()) != existing {
            md = dir.join(format!("{stem}-{n}.md"));
            n += 1;
        }
        write_atomic(&md, markdown::render(self).as_bytes())?;
        write_atomic(&md.with_extension("json"), &json::render(self)?)?;
        Ok(md)
    }
}

/// Rebuilds a segment's timed words after its text was edited by hand: keeps each word's timing
/// when the word count is unchanged, otherwise spreads the new words evenly over the segment.
pub fn retime(seg: &Segment, new_text: &str) -> Vec<Word> {
    let tokens: Vec<&str> = new_text.split_whitespace().collect();
    if tokens.len() == seg.words.len() {
        return seg
            .words
            .iter()
            .zip(&tokens)
            .map(|(w, t)| Word {
                w: (*t).to_string(),
                s: w.s,
                e: w.e,
            })
            .collect();
    }
    let step = (seg.end - seg.start) / tokens.len().max(1) as f32;
    tokens
        .iter()
        .enumerate()
        .map(|(i, t)| Word {
            w: (*t).to_string(),
            s: seg.start + step * i as f32,
            e: seg.start + step * (i + 1) as f32,
        })
        .collect()
}

/// What a JSON sidecar holds besides the metadata the library already indexes.
pub struct Sidecar {
    pub segments: Vec<Segment>,
    pub asr_model: String,
    pub notes: Option<String>,
}

/// Reads a JSON sidecar written by [`json::render`].
pub fn read_sidecar(json_path: &Path) -> Result<Sidecar> {
    #[derive(Deserialize)]
    struct Meta {
        asr_model: String,
    }
    #[derive(Deserialize)]
    struct Doc {
        metadata: Meta,
        segments: Vec<Segment>,
        #[serde(default)]
        notes: Option<String>,
    }
    let doc: Doc = serde_json::from_slice(&fs::read(json_path)?)?;
    Ok(Sidecar {
        segments: doc.segments,
        asr_model: doc.metadata.asr_model,
        notes: doc.notes,
    })
}

pub fn default_title(started_at: &DateTime<Local>) -> String {
    format!("Meeting {}", started_at.format("%Y-%m-%d %H:%M"))
}

/// True for titles that say nothing about the meeting: the dated default, or "<app> call".
/// Claude may replace these with a descriptive title (PRD §6.8).
pub fn is_generic_title(title: &str) -> bool {
    let t = title.trim();
    let dated = t
        .strip_prefix("Meeting ")
        .is_some_and(|rest| chrono::NaiveDateTime::parse_from_str(rest, "%Y-%m-%d %H:%M").is_ok());
    let app_call = t.strip_suffix(" call").is_some_and(|app| {
        [
            "Microsoft Teams",
            "Zoom",
            "Webex",
            "Slack",
            "FaceTime",
            "Google Meet",
        ]
        .contains(&app)
    });
    t.is_empty() || dated || app_call
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

pub fn slugify(title: &str) -> String {
    let mut out = String::new();
    for c in title.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-');
    let out: String = out.chars().take(60).collect();
    if out.is_empty() {
        "meeting".into()
    } else {
        out.trim_end_matches('-').to_string()
    }
}

#[cfg(test)]
pub(crate) fn sample() -> Meeting {
    use chrono::TimeZone;
    let w = |w: &str, s, e| Word { w: w.into(), s, e };
    Meeting {
        title: "Weekly sync: Q4 plan".into(),
        started_at: Local.with_ymd_and_hms(2026, 10, 20, 14, 0, 0).unwrap(),
        duration_secs: 42.0 * 60.0,
        asr_model: "parakeet-tdt-0.6b-v2-int8".into(),
        segments: vec![
            Segment {
                start: 4.12,
                end: 6.0,
                speaker: "Me".into(),
                text: "Thanks everyone.".into(),
                words: vec![w("Thanks", 4.12, 4.4), w("everyone.", 4.4, 6.0)],
                original_text: None,
            },
            Segment {
                start: 6.5,
                end: 9.0,
                speaker: "Me".into(),
                text: "Let's start.".into(),
                words: vec![],
                original_text: None,
            },
            Segment {
                start: 19.0,
                end: 25.0,
                speaker: "Speaker 1".into(),
                text: "Quick update on TikTok.".into(),
                words: vec![],
                original_text: Some("Quick update on tick tock.".into()),
            },
            Segment {
                start: 80.0,
                end: 85.0,
                speaker: "Speaker 1".into(),
                text: "Still going.".into(),
                words: vec![],
                original_text: None,
            },
        ],
        notes: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_titles() {
        assert!(is_generic_title("Meeting 2026-10-23 10:00"));
        assert!(is_generic_title("Microsoft Teams call"));
        assert!(!is_generic_title("Q4 planning"));
        assert!(!is_generic_title("Meeting with the brand team"));
    }

    #[test]
    fn slugs_and_file_names() {
        assert_eq!(slugify("Weekly sync: Q4 plan!"), "weekly-sync-q4-plan");
        assert_eq!(slugify("***"), "meeting");
        assert_eq!(sample().file_stem(), "2026-10-20_1400_weekly-sync-q4-plan");
        assert_eq!(sample().speakers(), vec!["Me", "Speaker 1"]);
    }

    #[test]
    fn retimes_edited_words() {
        let m = sample();
        let same = retime(&m.segments[0], "Thanks everybody.");
        assert_eq!((same[1].w.as_str(), same[1].s), ("everybody.", 4.4));
        let more = retime(&m.segments[0], "Thanks a lot everyone.");
        assert_eq!(more.len(), 4);
        assert_eq!(more[3].e, m.segments[0].end);
    }

    #[test]
    fn writes_both_files_without_clobbering() {
        let dir = std::env::temp_dir().join(format!("pt-out-{}", std::process::id()));
        let m = sample();
        let a = m.write(&dir, None).unwrap();
        let b = m.write(&dir, None).unwrap();
        assert_ne!(a, b);
        assert!(a.with_extension("json").exists());
        assert_eq!(m.write(&dir, Some(&a)).unwrap(), a);
        let side = read_sidecar(&a.with_extension("json")).unwrap();
        assert_eq!(side.segments, m.segments);
        assert_eq!(side.asr_model, m.asr_model);
        assert_eq!(side.notes, None);
        let with_notes = Meeting {
            notes: Some("## Summary\n- Agreed".into()),
            ..m.clone()
        };
        with_notes.write(&dir, Some(&a)).unwrap();
        let side = read_sidecar(&a.with_extension("json")).unwrap();
        assert_eq!(side.notes.as_deref(), Some("## Summary\n- Agreed"));
        fs::remove_dir_all(dir).unwrap();
    }
}
