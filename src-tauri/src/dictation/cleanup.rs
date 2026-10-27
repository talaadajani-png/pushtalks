//! Light, rule-based cleanup for dictation (PRD §6.1). Parakeet already punctuates and capitalises.

const FILLERS: &[&str] = &["um", "uh", "erm", "er", "uhm", "umm", "hmm", "mm"];

/// Removes filler words, fixes spacing around punctuation and capitalises sentence starts.
pub fn clean(text: &str, remove_fillers: bool) -> String {
    let mut words: Vec<String> = Vec::new();
    for raw in text.split_whitespace() {
        let core: String = raw
            .trim_matches(|c: char| !c.is_alphanumeric())
            .to_lowercase();
        if remove_fillers && FILLERS.contains(&core.as_str()) {
            // Keep sentence-ending punctuation that was attached to the filler.
            if let Some(p) = raw.chars().last().filter(|c| matches!(c, '.' | '?' | '!')) {
                if let Some(prev) = words.last_mut() {
                    let trimmed = prev.trim_end_matches([',', ';', ':']).to_string();
                    *prev = trimmed;
                    if !prev.ends_with(['.', '?', '!']) {
                        prev.push(p);
                    }
                }
            }
            continue;
        }
        words.push(raw.to_string());
    }
    let mut out = words.join(" ");
    // No space before punctuation; collapse doubled commas left by removed fillers.
    for p in [" ,", " .", " ?", " !", " ;", " :"] {
        out = out.replace(p, &p[1..]);
    }
    while out.contains(",,") {
        out = out.replace(",,", ",");
    }
    let out = out.trim_start_matches([',', ' ']).trim().to_string();
    capitalise_sentences(&out)
}

fn capitalise_sentences(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut cap_next = true;
    for c in text.chars() {
        if cap_next && c.is_alphabetic() {
            out.extend(c.to_uppercase());
            cap_next = false;
        } else {
            out.push(c);
            if matches!(c, '.' | '?' | '!') {
                cap_next = true;
            } else if !c.is_whitespace() && !matches!(c, '"' | '\'' | '(') {
                cap_next = false;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_fillers_and_fixes_spacing() {
        assert_eq!(
            clean("um, so I think , uh, we should go .", true),
            "So I think, we should go."
        );
    }

    #[test]
    fn keeps_fillers_when_disabled() {
        assert_eq!(clean("um okay", false), "Um okay");
    }

    #[test]
    fn capitalises_sentence_starts() {
        assert_eq!(
            clean("hello. how are you? fine", true),
            "Hello. How are you? Fine"
        );
    }

    #[test]
    fn does_not_strip_words_containing_fillers() {
        assert_eq!(
            clean("Umbrella under the hummus", true),
            "Umbrella under the hummus"
        );
    }

    #[test]
    fn moves_end_punctuation_off_a_trailing_filler() {
        assert_eq!(clean("That's it, um.", true), "That's it.");
    }
}
