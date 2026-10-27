//! Timestamp formatting used in transcripts (PRD §6.4: `[HH:MM:SS]`).

/// `[00:01:05]`-style timestamp for transcript lines.
pub fn hhmmss(secs: f32) -> String {
    let total = secs.max(0.0).floor() as u64;
    format!(
        "{:02}:{:02}:{:02}",
        total / 3600,
        (total % 3600) / 60,
        total % 60
    )
}

/// `00:01:05.20` with hundredths, for CLI segment listings.
pub fn hhmmss_cs(secs: f32) -> String {
    let cs = (secs.max(0.0) * 100.0).round() as u64;
    format!(
        "{:02}:{:02}:{:02}.{:02}",
        cs / 360_000,
        (cs % 360_000) / 6000,
        (cs % 6000) / 100,
        cs % 100
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_timestamps() {
        assert_eq!(hhmmss(4.9), "00:00:04");
        assert_eq!(hhmmss(3725.0), "01:02:05");
        assert_eq!(hhmmss_cs(65.2), "00:01:05.20");
        assert_eq!(hhmmss_cs(-1.0), "00:00:00.00");
    }
}
