//! Deterministic linguistic readability and narrative cadence analysis.
//!
//! Evaluates narration scripts using:
//! - Algorithmic English syllable counting
//! - Flesch-Kincaid Grade Level & Flesch Reading Ease
//! - Dale-Chall Readability Formula (with embedded canonical 3,000 word corpus)
//! - Sentence cadence distribution (flagging spans outside the 4–14 word optimal window)

use crate::dale_chall_words::DALE_CHALL_WORDS;
use retold_core::num;
use serde::{Deserialize, Serialize};

/// Type of cadence anomaly detected in a sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CadenceIssueKind {
    /// Sentence is under 4 words, risking choppy or disjointed pacing.
    TooShort,
    /// Sentence exceeds 14 words, risking breathless delivery and caption overflow.
    TooLong,
}

/// A flagged sentence within the script that violates the 4–14 word cadence envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CadenceSpan {
    /// 1-based sentence index in the narration script.
    pub sentence_index: usize,
    /// Word count of this sentence.
    pub word_count: usize,
    /// Text of the offending sentence.
    pub text: String,
    /// Type of pacing issue.
    pub kind: CadenceIssueKind,
}

/// Comprehensive linguistic and cadence evaluation for a script.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CadenceProfile {
    /// Total number of parsed sentences.
    pub total_sentences: usize,
    /// Total number of words.
    pub total_words: usize,
    /// Total number of syllables.
    pub total_syllables: usize,
    /// Number of difficult words not in the Dale-Chall 3,000 list.
    pub difficult_words: usize,
    /// Mean words per sentence.
    pub avg_words_per_sentence: f64,
    /// Standard deviation of sentence lengths (measures rhythmic variety).
    pub std_dev_sentence_length: f64,
    /// Flesch-Kincaid Grade Level (U.S. school grade).
    pub flesch_kincaid_grade: f64,
    /// Flesch Reading Ease score (0–100+, higher = easier to understand).
    pub flesch_reading_ease: f64,
    /// Dale-Chall readability score.
    pub dale_chall_score: f64,
    /// Sentences violating the 4–14 word narrative delivery guidelines.
    pub issues: Vec<CadenceSpan>,
}

impl CadenceProfile {
    /// Returns `true` if all sentences fall within the recommended 4–14 word cadence envelope.
    #[must_use]
    pub const fn is_balanced(&self) -> bool {
        self.issues.is_empty()
    }
}

/// Count syllables in an English word using deterministic linguistic heuristics.
#[must_use]
pub fn count_syllables(word: &str) -> usize {
    let clean: String = word
        .chars()
        .filter(|c| c.is_alphabetic())
        .flat_map(char::to_lowercase)
        .collect();

    if clean.is_empty() {
        return 0;
    }
    if clean.len() <= 3 {
        return 1;
    }

    let is_vowel = |c: char| matches!(c, 'a' | 'e' | 'i' | 'o' | 'u' | 'y');
    let chars: Vec<char> = clean.chars().collect();
    let mut syllables = 0;
    let mut prev_vowel = false;

    for &c in &chars {
        let vowel = is_vowel(c);
        if vowel && !prev_vowel {
            syllables += 1;
        }
        prev_vowel = vowel;
    }

    // Heuristic adjustments for common silent suffixes
    let len = chars.len();
    if len > 2 && chars[len - 1] == 'e' && chars[len - 2] != 'l' && chars[len - 2] != 'e' {
        // e.g. "game", "plate" (but not "bottle", "agree")
        if syllables > 1 {
            syllables -= 1;
        }
    } else if len > 3 && clean.ends_with("ed") && chars[len - 3] != 't' && chars[len - 3] != 'd' {
        // e.g. "walked", "baked" (but not "wanted", "needed")
        if syllables > 1 {
            syllables -= 1;
        }
    } else if len > 3 && clean.ends_with("es") && chars[len - 3] != 's' && chars[len - 3] != 'x' {
        // e.g. "tales" (but not "passes", "boxes")
        if syllables > 1 {
            syllables -= 1;
        }
    }

    syllables.max(1)
}

/// Compute Flesch-Kincaid Grade Level.
///
/// Formula: $0.39 \times \frac{W}{S} + 11.8 \times \frac{Y}{W} - 15.59$
#[must_use]
pub fn flesch_kincaid_grade_level(words: usize, sentences: usize, syllables: usize) -> f64 {
    if words == 0 || sentences == 0 {
        return 0.0;
    }
    let w = num::count_f64(words);
    let s = num::count_f64(sentences);
    let y = num::count_f64(syllables);
    (0.39 * (w / s)) + (11.8 * (y / w)) - 15.59
}

/// Compute Flesch Reading Ease score.
///
/// Formula: $206.835 - 1.015 \times \frac{W}{S} - 84.6 \times \frac{Y}{W}$
#[must_use]
pub fn flesch_reading_ease(words: usize, sentences: usize, syllables: usize) -> f64 {
    if words == 0 || sentences == 0 {
        return 100.0;
    }
    let w = num::count_f64(words);
    let s = num::count_f64(sentences);
    let y = num::count_f64(syllables);
    206.835 - (1.015 * (w / s)) - (84.6 * (y / w))
}

/// Check if a word or its common inflected form is in the Dale-Chall 3,000 easy word list.
#[must_use]
pub fn is_dale_chall_easy(word: &str) -> bool {
    let clean: String = word
        .chars()
        .filter(|c| c.is_alphabetic())
        .flat_map(char::to_lowercase)
        .collect();

    let is_stem_easy = |suffix| {
        clean
            .strip_suffix(suffix)
            .is_some_and(|stem| DALE_CHALL_WORDS.binary_search(&stem).is_ok())
    };

    clean.is_empty()
        || DALE_CHALL_WORDS.binary_search(&clean.as_str()).is_ok()
        || ["s", "ed", "ing", "ly"].into_iter().any(is_stem_easy)
}

/// Compute Dale-Chall readability score.
///
/// Formula:
/// Let $P = \frac{\text{difficult words}}{\text{words}} \times 100$.
/// $\text{Raw} = 0.1579 \times P + 0.0496 \times \frac{W}{S}$.
/// If $P > 5.0$, add $3.6365$.
#[must_use]
pub fn dale_chall_score(words: usize, sentences: usize, difficult_words: usize) -> f64 {
    if words == 0 || sentences == 0 {
        return 0.0;
    }
    let w = num::count_f64(words);
    let s = num::count_f64(sentences);
    let p = (num::count_f64(difficult_words) / w) * 100.0;
    let raw = (0.1579 * p) + (0.0496 * (w / s));
    if p > 5.0 { raw + 3.6365 } else { raw }
}

/// Analyze a complete narration script for readability, cadence distribution, and pacing anomalies.
#[must_use]
pub fn analyze_script(text: &str) -> CadenceProfile {
    let raw_sentences = split_sentences(text);
    if raw_sentences.is_empty() {
        return CadenceProfile {
            total_sentences: 0,
            total_words: 0,
            total_syllables: 0,
            difficult_words: 0,
            avg_words_per_sentence: 0.0,
            std_dev_sentence_length: 0.0,
            flesch_kincaid_grade: 0.0,
            flesch_reading_ease: 100.0,
            dale_chall_score: 0.0,
            issues: Vec::new(),
        };
    }

    let mut total_words = 0;
    let mut total_syllables = 0;
    let mut difficult_words = 0;
    let mut lengths = Vec::with_capacity(raw_sentences.len());
    let mut issues = Vec::new();

    for (idx, sentence) in raw_sentences.iter().enumerate() {
        let words: Vec<&str> = sentence
            .split_whitespace()
            .filter(|w| w.chars().any(char::is_alphabetic))
            .collect();

        let word_count = words.len();
        lengths.push(num::count_f64(word_count));
        total_words += word_count;

        for &w in &words {
            total_syllables += count_syllables(w);
            if !is_dale_chall_easy(w) {
                difficult_words += 1;
            }
        }

        if word_count < 4 {
            issues.push(CadenceSpan {
                sentence_index: idx + 1,
                word_count,
                text: (*sentence).clone(),
                kind: CadenceIssueKind::TooShort,
            });
        } else if word_count > 14 {
            issues.push(CadenceSpan {
                sentence_index: idx + 1,
                word_count,
                text: (*sentence).clone(),
                kind: CadenceIssueKind::TooLong,
            });
        }
    }

    let total_sentences = raw_sentences.len();
    let avg_words = if total_sentences == 0 {
        0.0
    } else {
        num::count_f64(total_words) / num::count_f64(total_sentences)
    };

    let variance = if total_sentences <= 1 {
        0.0
    } else {
        let sum_sq: f64 = lengths.iter().map(|&len| (len - avg_words).powi(2)).sum();
        sum_sq / num::count_f64(total_sentences)
    };
    let std_dev = variance.sqrt();

    let fk_grade = flesch_kincaid_grade_level(total_words, total_sentences, total_syllables);
    let fre = flesch_reading_ease(total_words, total_sentences, total_syllables);
    let dc = dale_chall_score(total_words, total_sentences, difficult_words);

    CadenceProfile {
        total_sentences,
        total_words,
        total_syllables,
        difficult_words,
        avg_words_per_sentence: avg_words,
        std_dev_sentence_length: std_dev,
        flesch_kincaid_grade: fk_grade,
        flesch_reading_ease: fre,
        dale_chall_score: dc,
        issues,
    }
}

fn split_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut current = String::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        for c in trimmed.chars() {
            current.push(c);
            if matches!(c, '.' | '!' | '?') {
                let s = current.trim();
                if !s.is_empty() {
                    sentences.push(s.to_string());
                }
                current.clear();
            }
        }

        let s = current.trim();
        if !s.is_empty() {
            sentences.push(s.to_string());
            current.clear();
        }
    }

    sentences
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_syllables_accurately() {
        assert_eq!(count_syllables("the"), 1);
        assert_eq!(count_syllables("plot"), 1);
        assert_eq!(count_syllables("action"), 2);
        assert_eq!(count_syllables("cinema"), 3);
        assert_eq!(count_syllables("cinematography"), 6);
        assert_eq!(count_syllables("game"), 1); // silent e
        assert_eq!(count_syllables("walked"), 1); // silent ed
    }

    #[test]
    fn easy_words_recognized() {
        assert!(is_dale_chall_easy("house"));
        assert!(is_dale_chall_easy("running")); // inflected
        assert!(is_dale_chall_easy("played")); // inflected
        assert!(!is_dale_chall_easy("transubstantiation"));
        assert!(!is_dale_chall_easy("cinematography"));
    }

    #[test]
    fn flags_cadence_anomalies() {
        let script = "He ran. \
            The sudden explosion shattered every glass window across the entire downtown metropolitan boulevard without warning. \
            She waited in the darkness for dawn to break.";

        let profile = analyze_script(script);
        assert_eq!(profile.total_sentences, 3);
        assert_eq!(profile.issues.len(), 2);
        assert_eq!(profile.issues[0].kind, CadenceIssueKind::TooShort);
        assert_eq!(profile.issues[1].kind, CadenceIssueKind::TooLong);
        assert!(!profile.is_balanced());
    }

    #[test]
    fn balanced_script_passes_cleanly() {
        let script = "The sun sets over the quiet harbor. \
            A solitary bell rings across the cold water. \
            Night arrives with a sudden autumn chill.";

        let profile = analyze_script(script);
        assert!(profile.is_balanced());
        assert_eq!(profile.issues.len(), 0);
        assert!(profile.flesch_reading_ease > 50.0);
    }
}
