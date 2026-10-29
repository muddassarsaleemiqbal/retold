//! Oral cadence, breath-paced sentence constraints, sensory verb density,
//! and AI cliché filtering.

use retold_core::num;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Banned AI clichés that deaden narration and signal artificial prose.
pub const BANNED_CLICHES: &[&str] = &[
    "delve",
    "delving",
    "delves into",
    "dive into",
    "testament",
    "a testament to",
    "tapestry",
    "rich tapestry",
    "beacon",
    "beacon of hope",
    "little did they know",
    "it is important to remember",
    "in conclusion",
    "seamlessly",
    "nestled",
    "bustling",
    "serves as a reminder",
    "not just a, but a",
    "game changer",
    "unwavering",
];

/// Curated high-impact physical sensory verbs rooted in Anglo-Saxon oral traditions.
pub const SENSORY_VERBS: &[&str] = &[
    "smash",
    "smashed",
    "lock",
    "locked",
    "break",
    "broke",
    "broken",
    "freeze",
    "froze",
    "frozen",
    "strike",
    "struck",
    "bleed",
    "bled",
    "burn",
    "burned",
    "burnt",
    "grasp",
    "grasped",
    "tear",
    "tore",
    "torn",
    "crush",
    "crushed",
    "slam",
    "slammed",
    "rip",
    "ripped",
    "snap",
    "snapped",
    "sprint",
    "sprinted",
    "shudder",
    "shuddered",
    "scream",
    "screamed",
    "clutch",
    "clutched",
    "plunge",
    "plunged",
    "grip",
    "gripped",
    "strike",
    "slash",
    "slashed",
    "shove",
    "shoved",
    "drag",
    "dragged",
    "choke",
    "choked",
    "hurl",
    "hurled",
];

/// A violation of oral cadence or dramatic prose standards.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CadenceViolation {
    /// Sentence index (0-based).
    pub sentence_index: usize,
    /// Offending sentence excerpt.
    pub sentence: String,
    /// Category of the violation.
    pub category: CadenceCategory,
    /// Explanatory guidance for human operator or repair prompt.
    pub guidance: String,
}

/// Category of cadence failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum CadenceCategory {
    /// Sentence has fewer than 4 words (staccato fragment).
    TooShort,
    /// Sentence has more than 14 words (exceeds human breath group).
    TooLong,
    /// Contains a banned AI cliché.
    BannedCliche,
    /// Sentence lacks vivid physical sensory verbs or concrete nouns.
    LacksSensoryAction,
    /// Conclusion lacks an open moral dilemma or poignant paradox.
    MoralizingConclusion,
}

/// Prose audit results containing cadence scores and violations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CadenceReport {
    /// Total sentences checked.
    pub total_sentences: usize,
    /// Ratio of sensory verbs per 100 words.
    pub sensory_verb_density: f64,
    /// List of violations found.
    pub violations: Vec<CadenceViolation>,
    /// Whether the script passes all hard gates.
    pub is_clean: bool,
}

/// Evaluates prose for oral delivery, breath cadence, and vivid imagery.
#[derive(Debug, Default, Clone, Copy)]
pub struct CadenceEngine;

impl CadenceEngine {
    /// Creates a new cadence engine.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Audits a sequence of sentences for oral performance constraints.
    #[must_use]
    pub fn audit_sentences(&self, sentences: &[String]) -> CadenceReport {
        let mut violations = Vec::new();
        let mut total_words = 0usize;
        let mut sensory_count = 0usize;

        for (idx, sentence) in sentences.iter().enumerate() {
            let words: Vec<&str> = sentence.split_whitespace().collect();
            let count = words.len();
            total_words += count;

            Self::check_length_bounds(idx, sentence, count, &mut violations);
            Self::check_banned_cliches(idx, sentence, &mut violations);
            sensory_count += Self::count_sensory_words(&words);
        }

        Self::check_epilogue_dilemma(sentences, &mut violations);

        let density = Self::calculate_density(sensory_count, total_words);
        let is_clean = violations.is_empty();

        CadenceReport {
            total_sentences: sentences.len(),
            sensory_verb_density: density,
            violations,
            is_clean,
        }
    }

    fn calculate_density(sensory_count: usize, total_words: usize) -> f64 {
        if total_words == 0 {
            0.0
        } else {
            (num::count_f64(sensory_count) / num::count_f64(total_words)) * 100.0
        }
    }

    fn check_length_bounds(
        idx: usize,
        sentence: &str,
        word_count: usize,
        violations: &mut Vec<CadenceViolation>,
    ) {
        if word_count < 4 {
            violations.push(CadenceViolation {
                sentence_index: idx,
                sentence: sentence.to_string(),
                category: CadenceCategory::TooShort,
                guidance: format!(
                    "Sentence has {word_count} words; oral delivery requires 4 to 14 words per breath unit."
                ),
            });
        } else if word_count > 14 {
            violations.push(CadenceViolation {
                sentence_index: idx,
                sentence: sentence.to_string(),
                category: CadenceCategory::TooLong,
                guidance: format!(
                    "Sentence has {word_count} words; break into shorter breath units of 4 to 14 words."
                ),
            });
        }
    }

    fn check_banned_cliches(idx: usize, sentence: &str, violations: &mut Vec<CadenceViolation>) {
        let lower = sentence.to_lowercase();
        for &cliche in BANNED_CLICHES {
            if lower.contains(cliche) {
                violations.push(CadenceViolation {
                    sentence_index: idx,
                    sentence: sentence.to_string(),
                    category: CadenceCategory::BannedCliche,
                    guidance: format!("Remove artificial AI cliché `{cliche}`."),
                });
            }
        }
    }

    fn check_epilogue_dilemma(sentences: &[String], violations: &mut Vec<CadenceViolation>) {
        if let Some(last) = sentences.last() {
            let lower = last.to_lowercase();
            let is_moralizing = lower.contains("in conclusion")
                || lower.contains("serves as a reminder")
                || lower.contains("we must remember")
                || lower.contains("teaches us that");

            let has_dilemma_ending = last.ends_with('?')
                || lower.contains("or")
                || lower.contains("choice")
                || lower.contains("remain")
                || lower.contains("cost");

            if is_moralizing || !has_dilemma_ending {
                let sentence_index = sentences.len().saturating_sub(1);
                violations.push(CadenceViolation {
                    sentence_index,
                    sentence: last.clone(),
                    category: CadenceCategory::MoralizingConclusion,
                    guidance: "Conclude with an unresolved moral question or poignant dilemma rather than a tidy summary.".to_string(),
                });
            }
        }
    }

    fn count_sensory_words(words: &[&str]) -> usize {
        words
            .iter()
            .map(|w| w.trim_matches(|c: char| !c.is_alphabetic()).to_lowercase())
            .filter(|w| SENSORY_VERBS.contains(&w.as_str()))
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breath_cadence_detects_out_of_bound_sentences() {
        let engine = CadenceEngine::new();
        let sentences = vec![
            "He ran.".into(), // 2 words -> TooShort (<4)
            "The commander slammed his fist against the rusted iron console.".into(), // 10 words -> OK
            "When the soldiers marched across the burning valley they saw the citadel falling while the sky turned black as ash.".into(), // 20 words -> TooLong (>14)
        ];
        let report = engine.audit_sentences(&sentences);
        assert!(!report.is_clean);
        assert_eq!(report.violations.len(), 3); // 2 bounds + 1 moralizing end
    }

    #[test]
    fn banned_cliches_are_flagged() {
        let engine = CadenceEngine::new();
        let sentences = vec!["Let us delve into the rich tapestry of ancient history?".into()];
        let report = engine.audit_sentences(&sentences);
        assert!(!report.is_clean);
        let cliches = report
            .violations
            .iter()
            .filter(|v| v.category == CadenceCategory::BannedCliche)
            .count();
        assert!(cliches >= 2); // "delve" and "tapestry"
    }

    #[test]
    fn sensory_verb_density_is_computed() {
        let engine = CadenceEngine::new();
        let sentences = vec!["He smashed the lock and ripped the iron door?".into()];
        let report = engine.audit_sentences(&sentences);
        assert!(report.sensory_verb_density > 15.0);
    }
}
