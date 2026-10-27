//! Matching and applying learned spellings. Whole words or phrases only, case-insensitive,
//! keeping the saved capitalisation (PRD §6.7). Storage lives in `db::library`.

use serde::{Deserialize, Serialize};

use crate::asr::Word;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleMode {
    /// Applied automatically to every new transcript and dictation.
    Auto,
    /// The misheard form is a common English word: never replaced without the user confirming
    /// each occurrence in the transcript editor.
    Confirm,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    pub id: i64,
    /// What the model wrote, e.g. "tick tock".
    pub heard: String,
    /// The correct spelling, e.g. "TikTok".
    pub spelling: String,
    pub mode: RuleMode,
}

/// A replacement found in a token sequence: tokens `start..end` become `spelling`.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub start: usize,
    pub end: usize,
    pub rule_id: i64,
    pub spelling: String,
}

/// The lowercase alphanumeric core of a token: "Stefan," -> "stefan".
fn core(token: &str) -> String {
    token
        .trim_matches(|c: char| !c.is_alphanumeric() && c != '&' && c != '\'')
        .trim_end_matches("'s")
        .to_lowercase()
}

fn key(phrase: &str) -> Vec<String> {
    phrase
        .split_whitespace()
        .map(core)
        .filter(|w| !w.is_empty())
        .collect()
}

/// Finds non-overlapping matches, longest rule first, left to right.
pub fn find(tokens: &[&str], rules: &[Rule], mode: RuleMode) -> Vec<Hit> {
    let cores: Vec<String> = tokens.iter().map(|t| core(t)).collect();
    let mut keyed: Vec<(Vec<String>, &Rule)> = rules
        .iter()
        .filter(|r| r.mode == mode)
        .map(|r| (key(&r.heard), r))
        .filter(|(k, _)| !k.is_empty())
        .collect();
    keyed.sort_by_key(|k| std::cmp::Reverse(k.0.len()));
    let mut hits = Vec::new();
    let mut i = 0;
    while i < cores.len() {
        let found = keyed
            .iter()
            .find(|(k, _)| i + k.len() <= cores.len() && cores[i..i + k.len()] == k[..]);
        match found {
            Some((k, r)) => {
                // Already correct (same spelling, same case): nothing to change.
                let same = tokens[i..i + k.len()]
                    .iter()
                    .map(|t| t.trim_matches(|c: char| !c.is_alphanumeric() && c != '&'))
                    .collect::<Vec<_>>()
                    .join(" ")
                    == r.spelling;
                if !same {
                    hits.push(Hit {
                        start: i,
                        end: i + k.len(),
                        rule_id: r.id,
                        spelling: r.spelling.clone(),
                    });
                }
                i += k.len();
            }
            None => i += 1,
        }
    }
    hits
}

/// Leading and trailing punctuation around a token's core, so "Stefan," keeps its comma.
fn affixes(token: &str) -> (&str, &str) {
    let start = token
        .find(|c: char| c.is_alphanumeric() || c == '&')
        .unwrap_or(token.len());
    let end = token
        .rfind(|c: char| c.is_alphanumeric() || c == '&')
        .map(|i| i + token[i..].chars().next().map_or(1, char::len_utf8))
        .unwrap_or(token.len());
    let suffix = &token[end.max(start)..];
    // Keep a possessive ending: "stefan's" -> "Stephan's".
    let suffix = if token[start..end.max(start)].to_lowercase().ends_with("'s") {
        &token[end - 2..]
    } else {
        suffix
    };
    (&token[..start], suffix)
}

/// Applies `Auto` rules to plain text. Returns None when nothing changed.
pub fn apply_text(text: &str, rules: &[Rule]) -> Option<String> {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    let hits = find(&tokens, rules, RuleMode::Auto);
    if hits.is_empty() {
        return None;
    }
    Some(replace_tokens(&tokens, &hits).join(" "))
}

/// Replaces each hit's token range with its spelling, keeping outer punctuation.
pub fn replace_tokens(tokens: &[&str], hits: &[Hit]) -> Vec<String> {
    let mut out = Vec::with_capacity(tokens.len());
    let mut i = 0;
    for h in hits {
        out.extend(tokens[i..h.start].iter().map(|t| t.to_string()));
        let (pre, _) = affixes(tokens[h.start]);
        let (_, post) = affixes(tokens[h.end - 1]);
        out.push(format!("{pre}{}{post}", h.spelling));
        i = h.end;
    }
    out.extend(tokens[i..].iter().map(|t| t.to_string()));
    out
}

/// Applies `Auto` rules to timed words. Replaced spans take the first word's start and the
/// last word's end; a multi-word spelling splits that span evenly.
pub fn apply_words(words: &[Word], rules: &[Rule]) -> Option<Vec<Word>> {
    let tokens: Vec<&str> = words.iter().map(|w| w.w.as_str()).collect();
    let hits = find(&tokens, rules, RuleMode::Auto);
    if hits.is_empty() {
        return None;
    }
    let mut out = Vec::with_capacity(words.len());
    let mut i = 0;
    for h in &hits {
        out.extend_from_slice(&words[i..h.start]);
        let (pre, _) = affixes(tokens[h.start]);
        let (_, post) = affixes(tokens[h.end - 1]);
        let parts: Vec<&str> = h.spelling.split_whitespace().collect();
        let (s, e) = (words[h.start].s, words[h.end - 1].e);
        let step = (e - s) / parts.len().max(1) as f32;
        for (k, p) in parts.iter().enumerate() {
            let mut w = (*p).to_string();
            if k == 0 {
                w.insert_str(0, pre);
            }
            if k + 1 == parts.len() {
                w.push_str(post);
            }
            out.push(Word {
                w,
                s: s + step * k as f32,
                e: if k + 1 == parts.len() {
                    e
                } else {
                    s + step * (k + 1) as f32
                },
            });
        }
        i = h.end;
    }
    out.extend_from_slice(&words[i..]);
    Some(out)
}

/// True when `heard` is a single common English word, so a blanket replacement would likely hit
/// correct uses (e.g. "humble" for a client called "Humber"). Phrases like "push media" are
/// specific enough to apply automatically.
pub fn is_common(heard: &str) -> bool {
    let k = key(heard);
    k.len() == 1 && COMMON.binary_search(&k[0].as_str()).is_ok()
}

/// Picks the mode for a new rule.
pub fn mode_for(heard: &str) -> RuleMode {
    if is_common(heard) {
        RuleMode::Confirm
    } else {
        RuleMode::Auto
    }
}

/// A correction the user just made, for the "Always write X as Y?" prompt (§6.7).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Correction {
    pub heard: String,
    pub spelling: String,
    pub common: bool,
}

/// Longest corrected span the prompt offers to remember; larger edits are rewrites, not spellings.
const MAX_CORRECTION_WORDS: usize = 4;

/// Finds the one changed span between the old and new text of a segment, ignoring punctuation
/// around words. None if nothing changed, the edit only added or removed words, or it's too long.
pub fn correction(old: &str, new: &str) -> Option<Correction> {
    let clean = |t: &str| -> Vec<String> {
        t.split_whitespace()
            .map(|w| {
                w.trim_matches(|c: char| !c.is_alphanumeric() && c != '&' && c != '\'')
                    .to_string()
            })
            .filter(|w| !w.is_empty())
            .collect()
    };
    let (a, b) = (clean(old), clean(new));
    let prefix = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let suffix = a[prefix..]
        .iter()
        .rev()
        .zip(b[prefix..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let heard = &a[prefix..a.len() - suffix];
    let spelling = &b[prefix..b.len() - suffix];
    if heard.is_empty()
        || spelling.is_empty()
        || heard.len() > MAX_CORRECTION_WORDS
        || spelling.len() > MAX_CORRECTION_WORDS
    {
        return None;
    }
    let (heard, spelling) = (heard.join(" "), spelling.join(" "));
    // Changing only capitalisation still teaches a spelling ("tiktok" -> "TikTok").
    (heard != spelling).then(|| Correction {
        common: is_common(&heard),
        heard,
        spelling,
    })
}

/// Around 600 of the most frequent English words, sorted for binary search.
const COMMON: &[&str] = &[
    "a",
    "able",
    "about",
    "above",
    "accept",
    "across",
    "act",
    "actually",
    "add",
    "after",
    "again",
    "against",
    "age",
    "ago",
    "agree",
    "air",
    "all",
    "allow",
    "almost",
    "alone",
    "along",
    "already",
    "also",
    "although",
    "always",
    "am",
    "among",
    "amount",
    "an",
    "and",
    "another",
    "answer",
    "any",
    "anyone",
    "anything",
    "appear",
    "apply",
    "are",
    "area",
    "arm",
    "around",
    "art",
    "as",
    "ask",
    "at",
    "away",
    "back",
    "bad",
    "ball",
    "bank",
    "base",
    "be",
    "bear",
    "beat",
    "beautiful",
    "because",
    "become",
    "bed",
    "been",
    "before",
    "begin",
    "behind",
    "being",
    "believe",
    "best",
    "better",
    "between",
    "big",
    "bill",
    "bit",
    "black",
    "blue",
    "board",
    "body",
    "book",
    "both",
    "box",
    "boy",
    "brand",
    "break",
    "bring",
    "brother",
    "brown",
    "build",
    "building",
    "business",
    "but",
    "buy",
    "by",
    "call",
    "camp",
    "can",
    "car",
    "card",
    "care",
    "carry",
    "case",
    "catch",
    "cause",
    "center",
    "certain",
    "chance",
    "change",
    "check",
    "child",
    "choice",
    "choose",
    "church",
    "city",
    "class",
    "clear",
    "close",
    "cold",
    "college",
    "color",
    "come",
    "common",
    "company",
    "complete",
    "cost",
    "could",
    "country",
    "couple",
    "course",
    "court",
    "cover",
    "create",
    "cross",
    "cup",
    "current",
    "cut",
    "dark",
    "data",
    "day",
    "dead",
    "deal",
    "dear",
    "decide",
    "deep",
    "did",
    "die",
    "different",
    "do",
    "does",
    "dog",
    "done",
    "door",
    "down",
    "draw",
    "dream",
    "drive",
    "drop",
    "during",
    "each",
    "early",
    "earth",
    "east",
    "easy",
    "eat",
    "edge",
    "effect",
    "eight",
    "either",
    "else",
    "end",
    "enough",
    "even",
    "evening",
    "event",
    "ever",
    "every",
    "everyone",
    "everything",
    "exactly",
    "example",
    "eye",
    "face",
    "fact",
    "fall",
    "family",
    "far",
    "fast",
    "father",
    "fear",
    "feel",
    "feet",
    "few",
    "field",
    "fight",
    "figure",
    "fill",
    "final",
    "find",
    "fine",
    "fire",
    "first",
    "fish",
    "five",
    "floor",
    "fly",
    "follow",
    "food",
    "foot",
    "for",
    "force",
    "form",
    "forward",
    "four",
    "free",
    "friend",
    "from",
    "front",
    "full",
    "fund",
    "game",
    "garden",
    "gas",
    "general",
    "get",
    "girl",
    "give",
    "glass",
    "go",
    "goal",
    "going",
    "gold",
    "gone",
    "good",
    "got",
    "great",
    "green",
    "ground",
    "group",
    "grow",
    "guess",
    "gun",
    "guy",
    "had",
    "hair",
    "half",
    "hall",
    "hand",
    "hang",
    "happen",
    "happy",
    "hard",
    "has",
    "hat",
    "have",
    "he",
    "head",
    "hear",
    "heard",
    "heart",
    "heat",
    "heavy",
    "held",
    "hello",
    "help",
    "her",
    "here",
    "high",
    "hill",
    "him",
    "his",
    "history",
    "hit",
    "hold",
    "home",
    "hope",
    "horse",
    "hot",
    "hotel",
    "hour",
    "house",
    "how",
    "however",
    "huge",
    "human",
    "humble",
    "hundred",
    "i",
    "idea",
    "if",
    "image",
    "important",
    "in",
    "include",
    "increase",
    "indeed",
    "inside",
    "instead",
    "interest",
    "into",
    "is",
    "issue",
    "it",
    "its",
    "job",
    "join",
    "just",
    "keep",
    "key",
    "kid",
    "kill",
    "kind",
    "king",
    "kitchen",
    "knew",
    "know",
    "land",
    "language",
    "large",
    "last",
    "late",
    "later",
    "law",
    "lay",
    "lead",
    "learn",
    "least",
    "leave",
    "left",
    "leg",
    "less",
    "let",
    "letter",
    "level",
    "lie",
    "life",
    "light",
    "like",
    "line",
    "list",
    "listen",
    "little",
    "live",
    "long",
    "look",
    "lose",
    "lot",
    "love",
    "low",
    "main",
    "major",
    "make",
    "man",
    "many",
    "mark",
    "market",
    "matter",
    "may",
    "maybe",
    "me",
    "mean",
    "media",
    "meet",
    "meeting",
    "member",
    "men",
    "might",
    "mind",
    "minute",
    "miss",
    "model",
    "moment",
    "money",
    "month",
    "more",
    "morning",
    "most",
    "mother",
    "move",
    "much",
    "music",
    "must",
    "my",
    "name",
    "nation",
    "near",
    "need",
    "never",
    "new",
    "news",
    "next",
    "nice",
    "night",
    "nine",
    "no",
    "none",
    "nor",
    "north",
    "not",
    "note",
    "nothing",
    "notice",
    "now",
    "number",
    "of",
    "off",
    "offer",
    "office",
    "often",
    "oh",
    "oil",
    "ok",
    "okay",
    "old",
    "on",
    "once",
    "one",
    "only",
    "open",
    "or",
    "order",
    "other",
    "our",
    "out",
    "outside",
    "over",
    "own",
    "page",
    "paper",
    "parent",
    "park",
    "part",
    "party",
    "pass",
    "past",
    "pay",
    "people",
    "per",
    "perhaps",
    "person",
    "pick",
    "picture",
    "piece",
    "place",
    "plan",
    "plant",
    "play",
    "please",
    "point",
    "police",
    "pool",
    "poor",
    "position",
    "possible",
    "post",
    "power",
    "present",
    "press",
    "pretty",
    "price",
    "probably",
    "problem",
    "process",
    "produce",
    "program",
    "project",
    "pull",
    "push",
    "put",
    "question",
    "quick",
    "quite",
    "race",
    "rain",
    "raise",
    "rather",
    "reach",
    "read",
    "ready",
    "real",
    "really",
    "reason",
    "red",
    "remember",
    "report",
    "rest",
    "result",
    "return",
    "right",
    "river",
    "road",
    "rock",
    "room",
    "round",
    "rule",
    "run",
    "said",
    "sale",
    "same",
    "save",
    "saw",
    "say",
    "school",
    "sea",
    "second",
    "see",
    "seem",
    "sell",
    "send",
    "sense",
    "serve",
    "set",
    "seven",
    "several",
    "shall",
    "share",
    "she",
    "ship",
    "shop",
    "short",
    "should",
    "show",
    "side",
    "simple",
    "since",
    "sing",
    "single",
    "sister",
    "sit",
    "six",
    "size",
    "small",
    "so",
    "social",
    "some",
    "someone",
    "something",
    "sometimes",
    "son",
    "soon",
    "sorry",
    "sort",
    "sound",
    "south",
    "space",
    "speak",
    "special",
    "spend",
    "spring",
    "staff",
    "stage",
    "stand",
    "star",
    "start",
    "state",
    "stay",
    "step",
    "still",
    "stock",
    "stop",
    "store",
    "story",
    "street",
    "strong",
    "student",
    "study",
    "stuff",
    "such",
    "summer",
    "sun",
    "sure",
    "system",
    "table",
    "take",
    "talk",
    "tall",
    "team",
    "tell",
    "ten",
    "term",
    "test",
    "than",
    "thank",
    "thanks",
    "that",
    "the",
    "their",
    "them",
    "then",
    "there",
    "these",
    "they",
    "thing",
    "think",
    "third",
    "this",
    "those",
    "though",
    "thought",
    "three",
    "through",
    "tick",
    "time",
    "to",
    "today",
    "together",
    "told",
    "too",
    "took",
    "top",
    "total",
    "touch",
    "toward",
    "town",
    "track",
    "trade",
    "tree",
    "trip",
    "true",
    "try",
    "turn",
    "two",
    "type",
    "under",
    "understand",
    "until",
    "up",
    "upon",
    "us",
    "use",
    "usually",
    "very",
    "view",
    "voice",
    "wait",
    "walk",
    "wall",
    "want",
    "war",
    "was",
    "watch",
    "water",
    "way",
    "we",
    "week",
    "well",
    "went",
    "were",
    "west",
    "what",
    "when",
    "where",
    "whether",
    "which",
    "while",
    "white",
    "who",
    "whole",
    "why",
    "wide",
    "wife",
    "will",
    "win",
    "window",
    "winter",
    "with",
    "within",
    "without",
    "woman",
    "women",
    "word",
    "work",
    "world",
    "would",
    "write",
    "wrong",
    "yeah",
    "year",
    "yes",
    "yet",
    "you",
    "young",
    "your",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(id: i64, heard: &str, spelling: &str) -> Rule {
        Rule {
            id,
            heard: heard.into(),
            spelling: spelling.into(),
            mode: RuleMode::Auto,
        }
    }

    #[test]
    fn common_list_is_sorted() {
        assert!(COMMON.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn whole_word_case_insensitive_keeps_saved_capitalisation() {
        let rules = [rule(1, "stefan", "Stephan")];
        assert_eq!(
            apply_text("Thanks STEFAN, and stefan's team.", &rules).unwrap(),
            "Thanks Stephan, and Stephan's team."
        );
    }

    #[test]
    fn never_replaces_inside_other_words() {
        let rules = [rule(1, "ann", "Anne")];
        assert_eq!(apply_text("Annual planning with Joanna", &rules), None);
    }

    #[test]
    fn multi_word_phrases_both_directions() {
        let rules = [
            rule(1, "tick tock", "TikTok"),
            rule(2, "push media", "PUSH Media"),
        ];
        assert_eq!(
            apply_text("Post it on tick tock. Push media agreed.", &rules).unwrap(),
            "Post it on TikTok. PUSH Media agreed."
        );
    }

    #[test]
    fn longest_rule_wins() {
        let rules = [
            rule(1, "choice", "Choyce"),
            rule(2, "choice hotels", "Choice Hotels"),
        ];
        assert_eq!(
            apply_text("the choice hotels brief", &rules).unwrap(),
            "the Choice Hotels brief"
        );
    }

    #[test]
    fn already_correct_text_is_untouched() {
        let rules = [rule(1, "tiktok", "TikTok")];
        assert_eq!(apply_text("TikTok rocks", &rules), None);
    }

    #[test]
    fn confirm_rules_are_not_auto_applied() {
        let mut r = rule(1, "humble", "Humber");
        r.mode = mode_for(&r.heard);
        assert_eq!(r.mode, RuleMode::Confirm);
        assert_eq!(apply_text("a humble start", &[r.clone()]), None);
        assert_eq!(
            find(&["a", "humble", "start"], &[r], RuleMode::Confirm).len(),
            1
        );
        assert_eq!(mode_for("Stefan"), RuleMode::Auto);
        assert_eq!(mode_for("push media"), RuleMode::Auto);
    }

    #[test]
    fn finds_the_corrected_span() {
        let c = correction("Thanks Stefan, see you.", "Thanks Stephan, see you.").unwrap();
        assert_eq!(
            (c.heard.as_str(), c.spelling.as_str(), c.common),
            ("Stefan", "Stephan", false)
        );
        let c = correction("post on tick tock now", "post on TikTok now").unwrap();
        assert_eq!(
            (c.heard.as_str(), c.spelling.as_str()),
            ("tick tock", "TikTok")
        );
        assert!(
            correction("a humble start", "a Humber start")
                .unwrap()
                .common
        );
        assert_eq!(correction("same text", "same text."), None);
        assert_eq!(correction("we go", "we really go"), None);
        assert_eq!(
            correction("one two three four five six", "a b c d e f"),
            None
        );
    }

    #[test]
    fn words_keep_timing_and_punctuation() {
        let words = vec![
            Word {
                w: "on".into(),
                s: 0.0,
                e: 0.2,
            },
            Word {
                w: "tick".into(),
                s: 0.2,
                e: 0.5,
            },
            Word {
                w: "tock.".into(),
                s: 0.5,
                e: 0.9,
            },
        ];
        let out = apply_words(&words, &[rule(1, "tick tock", "TikTok")]).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(
            out[1],
            Word {
                w: "TikTok.".into(),
                s: 0.2,
                e: 0.9
            }
        );
    }
}
