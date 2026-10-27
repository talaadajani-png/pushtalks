//! Merge the two tracks into one timeline (PRD §7.2): mic is "Me", system audio is everyone else.

use crate::output::Segment;

/// Mic and system segments starting within this many seconds can be echoes of each other.
const ECHO_WINDOW_SECS: f32 = 2.0;
/// Share of words two segments must have in common to count as the same speech.
const ECHO_OVERLAP: f32 = 0.7;

fn words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|w| {
            w.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|w| !w.is_empty())
        .collect()
}

/// Share of the shorter text's words that also appear in the longer one.
fn overlap(a: &str, b: &str) -> f32 {
    let (a, b) = (words(a), words(b));
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    if short.is_empty() {
        return 0.0;
    }
    let mut pool = long;
    let mut common = 0;
    for w in &short {
        if let Some(i) = pool.iter().position(|x| x == w) {
            pool.swap_remove(i);
            common += 1;
        }
    }
    common as f32 / short.len() as f32
}

/// Interleaves both tracks by start time. A mic segment that repeats a system segment within
/// 2 seconds is the call's audio leaking from the speakers into the mic, so it's dropped.
/// Genuine crosstalk (different words) keeps both.
pub fn merge(mic: Vec<Segment>, system: Vec<Segment>) -> Vec<Segment> {
    let mut out: Vec<Segment> = mic
        .into_iter()
        .filter(|m| {
            !system.iter().any(|s| {
                (m.start - s.start).abs() <= ECHO_WINDOW_SECS
                    && overlap(&m.text, &s.text) >= ECHO_OVERLAP
            })
        })
        .collect();
    out.extend(system);
    out.sort_by(|a, b| a.start.total_cmp(&b.start));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(start: f32, speaker: &str, text: &str) -> Segment {
        Segment {
            start,
            end: start + 2.0,
            speaker: speaker.into(),
            text: text.into(),
            words: vec![],
            original_text: None,
        }
    }

    #[test]
    fn interleaves_by_time_and_drops_echo() {
        let mic = vec![
            seg(1.0, "Me", "Hi all."),
            seg(10.4, "Me", "the budget is final"),
            seg(20.0, "Me", "Sounds good."),
        ];
        let sys = vec![
            seg(10.0, "Others", "The budget is final."),
            seg(19.5, "Others", "Any questions?"),
        ];
        let out = merge(mic, sys);
        let got: Vec<(&str, &str)> = out
            .iter()
            .map(|s| (s.speaker.as_str(), s.text.as_str()))
            .collect();
        assert_eq!(
            got,
            vec![
                ("Me", "Hi all."),
                ("Others", "The budget is final."),
                ("Others", "Any questions?"),
                ("Me", "Sounds good."),
            ]
        );
    }

    #[test]
    fn same_words_far_apart_are_kept() {
        let out = merge(
            vec![seg(30.0, "Me", "yes")],
            vec![seg(1.0, "Others", "yes")],
        );
        assert_eq!(out.len(), 2);
    }
}
