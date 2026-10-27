//! Markdown transcript, exactly as PRD §6.4 specifies.

use std::fmt::Write;

use super::{timestamps::hhmmss, Meeting, AI_TAG, CONFIDENTIALITY};

/// A new timestamped paragraph starts on a speaker change or every 60 seconds.
const PARAGRAPH_SECS: f32 = 60.0;

fn yaml_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

pub fn render(m: &Meeting) -> String {
    let speakers = m.speakers();
    let list = speakers
        .iter()
        .map(|s| yaml_str(s))
        .collect::<Vec<_>>()
        .join(", ");
    let mut out = String::new();
    // Writing to a String can't fail.
    let _ = write!(
        out,
        "---\ntitle: {}\ndate: {}\nstart_time: \"{}\"\nduration_minutes: {}\nspeakers: [{}]\nlanguages_detected: [\"en\"]\nasr_model: {}\napp_version: \"{}\"\nai_tag: \"{}\"\nconfidentiality: \"{}\"\n---\n\n",
        yaml_str(&m.title),
        m.started_at.format("%Y-%m-%d"),
        m.started_at.format("%H:%M"),
        m.duration_minutes(),
        list,
        yaml_str(&m.asr_model),
        env!("CARGO_PKG_VERSION"),
        AI_TAG,
        CONFIDENTIALITY,
    );
    let _ = write!(
        out,
        "# {}\n\n**Date:** {} | **Duration:** {} min | **Speakers:** {}\n\n## Notes\n",
        m.title,
        m.started_at.format("%Y-%m-%d"),
        m.duration_minutes(),
        speakers.join(", "),
    );
    match m.notes.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
        Some(notes) => {
            let _ = write!(out, "\n{notes}\n");
        }
        None => out.push_str(
            "<!-- Empty until you ask Claude for minutes through the PushTalks connector. -->\n",
        ),
    }
    out.push_str("\n## Transcript\n");
    let mut para: Option<(&str, f32)> = None;
    for seg in &m.segments {
        let text = seg.text.trim();
        if text.is_empty() {
            continue;
        }
        let continues =
            matches!(para, Some((sp, at)) if sp == seg.speaker && seg.start - at < PARAGRAPH_SECS);
        if continues {
            out.push(' ');
            out.push_str(text);
        } else {
            if para.is_some() {
                out.push('\n');
            }
            let _ = write!(
                out,
                "\n**[{}] {}:** {}",
                hhmmss(seg.start),
                seg.speaker,
                text
            );
            para = Some((&seg.speaker, seg.start));
        }
    }
    if para.is_some() {
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_header_and_merged_paragraphs() {
        let md = render(&crate::output::sample());
        assert!(md.starts_with("---\ntitle: \"Weekly sync: Q4 plan\"\ndate: 2026-10-20\nstart_time: \"14:00\"\nduration_minutes: 42\nspeakers: [\"Me\", \"Speaker 1\"]\n"));
        assert!(md.contains("ai_tag: \"[AI: DRAFT]\""));
        assert!(md.contains("\n**[00:00:04] Me:** Thanks everyone. Let's start.\n"));
        assert!(md.contains("\n**[00:00:19] Speaker 1:** Quick update on TikTok.\n"));
        // Over 60 s since the paragraph started: new timestamp, same speaker.
        assert!(md.contains("\n**[00:01:20] Speaker 1:** Still going.\n"));
        assert!(!md.contains("tick tock"));
        assert!(md.contains("## Notes\n<!-- Empty until"));
    }

    #[test]
    fn renders_notes_above_the_transcript() {
        let m = crate::output::Meeting {
            notes: Some("### Summary\n- Budget moves to social.".into()),
            ..crate::output::sample()
        };
        let md = render(&m);
        assert!(
            md.contains("## Notes\n\n### Summary\n- Budget moves to social.\n\n## Transcript\n")
        );
    }
}
