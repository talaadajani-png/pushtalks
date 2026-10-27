//! JSON sidecar with the same content as the Markdown file (PRD §6.4).

use anyhow::Result;
use serde::Serialize;

use super::{Meeting, Segment, AI_TAG, CONFIDENTIALITY};

#[derive(Serialize)]
struct Metadata<'a> {
    title: &'a str,
    date: String,
    start_time: String,
    duration_minutes: u32,
    speakers: Vec<String>,
    languages_detected: [&'static str; 1],
    asr_model: &'a str,
    app_version: &'static str,
    ai_tag: &'static str,
    confidentiality: &'static str,
}

#[derive(Serialize)]
struct Sidecar<'a> {
    metadata: Metadata<'a>,
    /// Minutes and summary (Markdown), when they've been written.
    #[serde(skip_serializing_if = "Option::is_none")]
    notes: Option<&'a str>,
    segments: &'a [Segment],
}

pub fn render(m: &Meeting) -> Result<Vec<u8>> {
    let doc = Sidecar {
        metadata: Metadata {
            title: &m.title,
            date: m.started_at.format("%Y-%m-%d").to_string(),
            start_time: m.started_at.format("%H:%M").to_string(),
            duration_minutes: m.duration_minutes(),
            speakers: m.speakers(),
            languages_detected: ["en"],
            asr_model: &m.asr_model,
            app_version: env!("CARGO_PKG_VERSION"),
            ai_tag: AI_TAG,
            confidentiality: CONFIDENTIALITY,
        },
        notes: m.notes.as_deref().filter(|n| !n.trim().is_empty()),
        segments: &m.segments,
    };
    Ok(serde_json::to_vec_pretty(&doc)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_metadata_segments_and_original_text_only_when_changed() {
        let v: serde_json::Value =
            serde_json::from_slice(&render(&crate::output::sample()).unwrap()).unwrap();
        assert_eq!(v["metadata"]["duration_minutes"], 42);
        assert_eq!(v["segments"][0]["words"][0]["w"], "Thanks");
        assert_eq!(v["segments"][0]["start"].as_f64().unwrap() as f32, 4.12);
        assert!(v["segments"][0].get("original_text").is_none());
        assert_eq!(
            v["segments"][2]["original_text"],
            "Quick update on tick tock."
        );
    }
}
