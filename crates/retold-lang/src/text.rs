//! Sentences, words and speech time, for any supported language.

use retold_core::num;
use retold_core::{LanguageTag, WritingScript};

/// Sentence-ending marks across scripts: Latin, Devanagari danda, Urdu full stop and question
/// mark, Arabic question mark, Japanese full stop and exclamation/question marks, and ellipsis.
const TERMINATORS: &[char] = &['.', '!', '?', '…', '।', '॥', '۔', '؟', '。', '！', '？'];

/// Closing quotes and brackets that may follow a terminator.
const CLOSERS: &[char] = &['\"', '”', '’', '\'', ')', '»', '」', '』'];

/// Latin abbreviations that end in a full stop but do not end a sentence.
const ABBREVIATIONS: &[&str] = &[
    "Dr.", "Mr.", "Mrs.", "Ms.", "St.", "Jr.", "Sr.", "Prof.", "Capt.", "Lt.", "Sgt.", "Gen.",
    "Col.", "Mt.", "vs.", "No.",
];

/// Words of a text, split on whitespace (for whitespace-separated scripts).
pub fn words(text: &str) -> impl Iterator<Item = &str> {
    text.split_whitespace()
}

#[must_use]
pub fn word_count(text: &str) -> usize {
    words(text).count()
}

/// Splits narration into sentences. A sentence ends at a terminator followed by a space
/// (or immediate terminator for CJK scripts). In scripts with case, the next sentence must
/// also start with a capital, digit or quote.
#[must_use]
pub fn sentences(text: &str, script: WritingScript) -> Vec<String> {
    if matches!(
        script,
        WritingScript::Jpan | WritingScript::Hans | WritingScript::Hant
    ) {
        return sentences_cjk(text);
    }
    let words: Vec<&str> = words(text).collect();
    let mut out: Vec<String> = Vec::new();
    let mut cur: Vec<&str> = Vec::new();
    for (i, w) in words.iter().enumerate() {
        cur.push(w);
        let next = words.get(i + 1);
        if ends_sentence(w)
            && next.is_some_and(|n| starts_sentence(n, script))
            && !is_abbreviation(w)
        {
            out.push(cur.join(" "));
            cur.clear();
        }
    }
    if !cur.is_empty() {
        out.push(cur.join(" "));
    }
    out
}

fn sentences_cjk(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for c in text.chars() {
        cur.push(c);
        if matches!(c, '。' | '！' | '？' | '\n') {
            let trimmed = cur.trim().to_string();
            if !trimmed.is_empty() {
                out.push(trimmed);
            }
            cur.clear();
        }
    }
    let trimmed = cur.trim().to_string();
    if !trimmed.is_empty() {
        out.push(trimmed);
    }
    out
}

fn ends_sentence(word: &str) -> bool {
    word.trim_end_matches(CLOSERS).ends_with(TERMINATORS)
}

fn starts_sentence(word: &str, script: WritingScript) -> bool {
    if !script.has_case() {
        return true;
    }
    word.trim_start_matches(['\"', '“', '‘', '\'', '(', '«'])
        .chars()
        .next()
        .is_some_and(|c| c.is_uppercase() || c.is_numeric())
}

fn is_abbreviation(word: &str) -> bool {
    ABBREVIATIONS.contains(&word)
}

/// Whether a word ends a phrase (a caption or subtitle may break after it).
#[must_use]
pub fn ends_phrase(word: &str) -> bool {
    let w = word.trim_end_matches(CLOSERS);
    w.ends_with(TERMINATORS)
        || w.ends_with([
            ',', ';', ':', '،', '—', '、', '؛', '\u{3001}', '\u{060c}', '\u{061b}',
        ])
}

/// Average speaking rate for narration, in words per second.
#[must_use]
pub fn words_per_second(lang: &LanguageTag) -> f64 {
    const SPEAKING_RATES: &[(&[&str], f64)] = &[
        (&["hi", "ur", "pa", "mr", "ne", "bn"], 2.9),
        (&["es", "it", "pt", "fr"], 2.8),
        (&["de", "ru", "uk", "tr", "fi"], 2.2),
        (&["ja", "zh"], 2.5),
        (&["ar"], 2.4),
    ];
    let lang_code = lang.language();
    for (languages, rate_val) in SPEAKING_RATES {
        if languages.contains(&lang_code) {
            return *rate_val;
        }
    }
    2.6
}

/// Estimated speech time for `text`, in seconds, counting pauses at punctuation.
#[must_use]
pub fn speech_seconds(text: &str, lang: &LanguageTag) -> f64 {
    let profile = crate::profile::profile_for(lang);
    let tokens = profile.tokenize_subtitles(text);
    let count = tokens.len();
    let mut seconds = num::count_f64(count) / words_per_second(lang);
    for w in &tokens {
        let w = w.trim_end_matches(CLOSERS);
        if w.ends_with(TERMINATORS) {
            seconds += 0.25;
        } else if ends_phrase(w) {
            seconds += 0.1;
        }
    }
    seconds
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_sentences_respect_titles_and_ellipses() {
        let s = sentences(
            "Dr. Nicky sees him. Wait... then he runs! \"Who?\" she asks. 1922 ends it.",
            WritingScript::Latn,
        );
        assert_eq!(
            s,
            vec![
                "Dr. Nicky sees him.",
                "Wait... then he runs!",
                "\"Who?\" she asks.",
                "1922 ends it.",
            ]
        );
    }

    #[test]
    fn hindi_and_urdu_sentences() {
        let h = sentences("वह भागा। फिर रुका! क्या हुआ?", WritingScript::Deva);
        assert_eq!(h, vec!["वह भागा।", "फिर रुका!", "क्या हुआ?"]);

        let u = sentences("وہ بھاگا۔ پھر رکا! کیا ہوا؟", WritingScript::Arab);
        assert_eq!(u, vec!["وہ بھاگا۔", "پھر رکا!", "کیا ہوا؟"]);
    }

    #[test]
    fn japanese_sentences() {
        let j = sentences("昔々ある所に。おじいさんがいました！", WritingScript::Jpan);
        assert_eq!(j, vec!["昔々ある所に。", "おじいさんがいました！"]);
    }
}
