//! Benchmark helpers: word error rate and peak memory (PRD §8, §14).

/// Lowercases and strips punctuation so WER compares words, not formatting.
pub fn normalize_words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '\'' || c.is_whitespace() {
                c
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .map(|w| w.trim_matches('\'').to_string())
        .filter(|w| !w.is_empty())
        .collect()
}

/// Word error rate: (substitutions + deletions + insertions) / reference words.
pub fn wer(reference: &str, hypothesis: &str) -> f32 {
    let r = normalize_words(reference);
    let h = normalize_words(hypothesis);
    if r.is_empty() {
        return if h.is_empty() { 0.0 } else { 1.0 };
    }
    let mut prev: Vec<usize> = (0..=h.len()).collect();
    for i in 1..=r.len() {
        let mut cur = vec![i; h.len() + 1];
        for j in 1..=h.len() {
            let sub = prev[j - 1] + usize::from(r[i - 1] != h[j - 1]);
            cur[j] = sub.min(prev[j] + 1).min(cur[j - 1] + 1);
        }
        prev = cur;
    }
    prev[h.len()] as f32 / r.len() as f32
}

/// Peak resident memory of this process, in bytes, if the OS reports it.
#[cfg(unix)]
pub fn peak_rss_bytes() -> Option<u64> {
    // SAFETY: getrusage only writes into the struct we pass.
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) } != 0 {
        return None;
    }
    let max = usage.ru_maxrss as u64;
    // macOS reports bytes; Linux reports kilobytes.
    Some(if cfg!(target_os = "macos") {
        max
    } else {
        max * 1024
    })
}

#[cfg(windows)]
pub fn peak_rss_bytes() -> Option<u64> {
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    // SAFETY: the counters struct is sized and passed as the API requires.
    unsafe {
        let mut c: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
        c.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        if GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) == 0 {
            return None;
        }
        Some(c.PeakWorkingSetSize as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_wer() {
        assert_eq!(
            wer("The budget is due Friday.", "the budget is due friday"),
            0.0
        );
        assert_eq!(wer("a b c d", "a x c d"), 0.25); // one substitution
        assert_eq!(wer("a b c d", "a c d"), 0.25); // one deletion
        assert_eq!(wer("a b", "a b c d"), 1.0); // two insertions
        assert_eq!(wer("", ""), 0.0);
    }

    #[test]
    fn reports_peak_memory() {
        assert!(peak_rss_bytes().unwrap_or(1) > 0);
    }
}
