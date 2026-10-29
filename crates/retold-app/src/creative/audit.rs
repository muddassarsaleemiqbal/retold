//! Script quality auditing, oral cadence checking, and dramaturgy gates.

use crate::creative::types::DramaturgyAuditReport;
use retold_engine::retold_dramaturgy::cadence::{BANNED_CLICHES, SENSORY_VERBS};

const SHORT_SENTENCE_MAX_WORDS: usize = 6;
const MEDIUM_SENTENCE_MAX_WORDS: usize = 14;
const MAX_MONOTONE_SENTENCE_RUN: usize = 3;
const BASE_SCRIPT_SCORE: u32 = 100;
const CLICHE_PENALTY: u32 = 15;
const RHYTHM_PENALTY: u32 = 10;
const MIN_SENSORY_VERB_COUNT: usize = 3;
const HIGH_SENSORY_THRESHOLD: usize = 5;
const SENSORY_BONUS_POINTS: u32 = 5;
const MAX_SCORE: u32 = 100;
/// Points lost for each vivid verb short of the minimum, so advice and score agree.
const SENSORY_SHORTFALL_PENALTY: u32 = 10;
const PERCENT_MULTIPLIER: f64 = 100.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SentenceClass {
    Short,
    Medium,
    Long,
}

impl SentenceClass {
    const fn from_word_count(count: usize) -> Self {
        if count <= SHORT_SENTENCE_MAX_WORDS {
            Self::Short
        } else if count <= MEDIUM_SENTENCE_MAX_WORDS {
            Self::Medium
        } else {
            Self::Long
        }
    }
}

fn find_banned_cliches(text: &str) -> Vec<String> {
    let lower_text = text.to_lowercase();
    let mut cliches_found = Vec::new();
    for cliche in BANNED_CLICHES {
        if lower_text.contains(cliche) {
            cliches_found.push((*cliche).to_string());
        }
    }
    cliches_found
}

fn clean_word(raw: &str) -> &str {
    raw.trim_matches(|char_token: char| !char_token.is_alphanumeric())
}

fn count_sensory_verbs(text: &str) -> usize {
    let lower_text = text.to_lowercase();
    lower_text
        .split_whitespace()
        .map(clean_word)
        .filter(|candidate| SENSORY_VERBS.contains(candidate))
        .count()
}

fn split_sentences(text: &str) -> Vec<&str> {
    text.split(['.', '!', '?'])
        .map(str::trim)
        .filter(|sentence| !sentence.is_empty())
        .collect()
}

fn check_rhythm_monotony(sentences: &[&str]) -> Vec<String> {
    let mut warnings = Vec::new();
    let mut current_class: Option<SentenceClass> = None;
    let mut consecutive_count: usize = 0;

    for sentence in sentences {
        let word_count = sentence.split_whitespace().count();
        let class = SentenceClass::from_word_count(word_count);

        if current_class == Some(class) {
            consecutive_count += 1;
            if consecutive_count > MAX_MONOTONE_SENTENCE_RUN {
                warnings.push(format!(
                    "Monotone rhythm detected: {consecutive_count} consecutive {class:?} sentences ({word_count} words)"
                ));
            }
        } else {
            current_class = Some(class);
            consecutive_count = 1;
        }
    }
    warnings
}

fn format_cliche_recommendations(cliches: &[String]) -> Vec<String> {
    cliches
        .iter()
        .map(|cliche| {
            format!("Replace banned AI cliche '{cliche}' with concrete sensory physical action")
        })
        .collect()
}

fn format_warning_recommendations(warnings: &[String]) -> Vec<String> {
    warnings
        .iter()
        .map(|warning| format!("Break sentence length monotony: {warning}"))
        .collect()
}

fn sensory_advice(sensory_count: usize) -> Option<String> {
    if sensory_count < MIN_SENSORY_VERB_COUNT {
        Some(
            "Use more vivid verbs (smash, strike, freeze, bleed) so it reads aloud with energy"
                .to_string(),
        )
    } else {
        None
    }
}

fn fallback_recommendation() -> Vec<String> {
    vec!["Prose satisfies oral cadence and dramaturgy gates.".to_string()]
}

fn collect_recommendations(
    cliches: &[String],
    warnings: &[String],
    sensory_count: usize,
) -> Vec<String> {
    let mut items = format_cliche_recommendations(cliches);
    items.extend(format_warning_recommendations(warnings));
    if let Some(advice) = sensory_advice(sensory_count) {
        items.push(advice);
    }
    items
}

fn build_recommendations(
    cliches: &[String],
    warnings: &[String],
    sensory_count: usize,
) -> Vec<String> {
    let collected = collect_recommendations(cliches, warnings, sensory_count);
    if collected.is_empty() {
        fallback_recommendation()
    } else {
        collected
    }
}

fn calculate_score(cliche_count: usize, warning_count: usize, sensory_count: usize) -> u32 {
    let mut score = BASE_SCRIPT_SCORE;
    let cliche_u32 = u32::try_from(cliche_count).unwrap_or(u32::MAX);
    let warning_u32 = u32::try_from(warning_count).unwrap_or(u32::MAX);
    let cliche_deduction = cliche_u32.saturating_mul(CLICHE_PENALTY);
    let rhythm_deduction = warning_u32.saturating_mul(RHYTHM_PENALTY);

    score = score.saturating_sub(cliche_deduction);
    score = score.saturating_sub(rhythm_deduction);

    let shortfall = MIN_SENSORY_VERB_COUNT.saturating_sub(sensory_count);
    let shortfall_u32 = u32::try_from(shortfall).unwrap_or(u32::MAX);
    score = score.saturating_sub(shortfall_u32.saturating_mul(SENSORY_SHORTFALL_PENALTY));

    if sensory_count >= HIGH_SENSORY_THRESHOLD {
        score = (score + SENSORY_BONUS_POINTS).min(MAX_SCORE);
    }
    score
}

#[expect(clippy::cast_precision_loss, reason = "word count averages")]
fn compute_word_metrics(text: &str, sentences: &[&str]) -> (usize, usize, f64) {
    let total_words = text.split_whitespace().count();
    let sentence_count = sentences.len().max(1);
    let words_per_sentence_avg = (total_words as f64) / (sentence_count as f64);
    (total_words, sentence_count, words_per_sentence_avg)
}

#[expect(clippy::cast_precision_loss, reason = "sensory density percentage")]
fn compute_sensory_metrics(text: &str, total_words: usize) -> (usize, f64) {
    let sensory_verb_count = count_sensory_verbs(text);
    let density_pct = if total_words > 0 {
        ((sensory_verb_count as f64) / (total_words as f64)) * PERCENT_MULTIPLIER
    } else {
        0.0
    };
    (sensory_verb_count, density_pct)
}

#[must_use]
pub fn audit_script_prose(text: &str) -> DramaturgyAuditReport {
    let sentences = split_sentences(text);
    let (total_words, sentence_count, words_per_sentence_avg) =
        compute_word_metrics(text, &sentences);
    let (sensory_verb_count, sensory_verb_density_pct) = compute_sensory_metrics(text, total_words);

    let banned_cliches_found = find_banned_cliches(text);
    let rhythm_warnings = check_rhythm_monotony(&sentences);
    let recommendations =
        build_recommendations(&banned_cliches_found, &rhythm_warnings, sensory_verb_count);
    let script_score = calculate_score(
        banned_cliches_found.len(),
        rhythm_warnings.len(),
        sensory_verb_count,
    );

    DramaturgyAuditReport {
        script_score,
        total_words,
        sentence_count,
        words_per_sentence_avg,
        sensory_verb_count,
        sensory_verb_density_pct,
        banned_cliches_found,
        rhythm_warnings,
        recommendations,
    }
}
