//! Dramatic beat modeling, mathematical emotional delta ($\Delta E \ge 1.0$),
//! and Storybit benchmark evaluation.

use retold_core::num;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Magnitude of protagonist revelation in a narrative beat.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[repr(u8)]
pub enum KnowledgeShift {
    /// No new revelation; status quo remains known.
    None = 0,
    /// Crucial hidden fact or motive is uncovered by the protagonist.
    Revealed = 2,
    /// Foundational truth is inverted (e.g. protector is the assassin).
    Inverted = 4,
}

impl KnowledgeShift {
    /// Numerical weight of this shift in the emotional delta formula.
    #[must_use]
    pub const fn score(self) -> f64 {
        match self {
            Self::None => 0.0,
            Self::Revealed => 2.0,
            Self::Inverted => 4.0,
        }
    }
}

/// A dramatic narrative beat capturing psychological and physical shifts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DramaticBeat {
    /// The structural section identifier (e.g., "hook", "`rising_action`", "climax").
    pub section: String,
    /// Beat narrative description or draft script prose.
    pub text: String,
    /// Magnitude of revelation for the protagonist.
    pub protagonist_knowledge: KnowledgeShift,
    /// Delta in protagonist authority/power from -5 (total defeat) to +5 (complete dominance).
    pub power_delta: i8,
    /// Delta in existential/physical threat from -5 (safe harbor) to +5 (mortal peril).
    pub danger_delta: i8,
}

impl DramaticBeat {
    /// Clamps and constructs a validated dramatic beat.
    #[must_use]
    pub fn new(
        section: impl Into<String>,
        text: impl Into<String>,
        protagonist_knowledge: KnowledgeShift,
        power_delta: i8,
        danger_delta: i8,
    ) -> Self {
        Self {
            section: section.into(),
            text: text.into(),
            protagonist_knowledge,
            power_delta: power_delta.clamp(-5, 5),
            danger_delta: danger_delta.clamp(-5, 5),
        }
    }

    /// Computes mathematical emotional delta:
    /// $\Delta E = \sqrt{(\text{power\_delta})^2 + (\text{danger\_delta})^2} + \text{protagonist\_knowledge}$.
    #[must_use]
    pub fn delta_e(&self) -> f64 {
        let p = f64::from(self.power_delta);
        let d = f64::from(self.danger_delta);
        let physical = p.hypot(d);
        physical + self.protagonist_knowledge.score()
    }
}

/// Errors raised during dramatic beat gate validation.
#[derive(Debug, Error, PartialEq)]
pub enum BeatError {
    /// The story has no dramatic beats.
    #[error("Story contains no dramatic beats")]
    EmptyStory,

    /// A beat failed the minimum narrative momentum requirement ($\Delta E \ge 1.0$).
    #[error(
        "Beat {index} in section `{section}` has ΔE = {delta_e:.2} < 1.0 (static filler without change)"
    )]
    StaticBeat {
        index: usize,
        section: String,
        delta_e: f64,
    },

    /// The story lacks an emotional climax (no beat reached $\Delta E \ge 3.0$).
    #[error("Story lacks an emotional peak; maximum ΔE was {peak:.2} < 3.0")]
    LacksClimax { peak: f64 },
}

/// Quantitative summary metrics across an entire sequence of beats.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DramaturgyMetrics {
    /// Total count of beats evaluated.
    pub total_beats: usize,
    /// Mean emotional delta per beat.
    pub average_delta_e: f64,
    /// Highest single beat emotional delta.
    pub peak_delta_e: f64,
    /// Zero-based index of the peak beat.
    pub peak_index: usize,
    /// Whether any beat triggered a profound reality inversion.
    pub has_inversion: bool,
}

/// Validates that every beat satisfies the $\Delta E \ge 1.0$ gate and the story peaks effectively.
///
/// # Errors
/// Returns [`BeatError::EmptyStory`] if no beats are provided,
/// [`BeatError::StaticBeat`] if any beat has $\Delta E < 1.0$,
/// or [`BeatError::LacksClimax`] if the peak $\Delta E < 3.0$.
pub fn validate_beat_gates(beats: &[DramaticBeat]) -> Result<DramaturgyMetrics, BeatError> {
    if beats.is_empty() {
        return Err(BeatError::EmptyStory);
    }

    let mut sum_delta = 0.0;
    let mut peak_delta = 0.0;
    let mut peak_idx = 0;
    let mut has_inversion = false;

    for (i, beat) in beats.iter().enumerate() {
        let de = beat.delta_e();
        if de < 1.0 {
            return Err(BeatError::StaticBeat {
                index: i,
                section: beat.section.clone(),
                delta_e: de,
            });
        }
        if beat.protagonist_knowledge == KnowledgeShift::Inverted {
            has_inversion = true;
        }
        sum_delta += de;
        if de > peak_delta {
            peak_delta = de;
            peak_idx = i;
        }
    }

    if peak_delta < 3.0 {
        return Err(BeatError::LacksClimax { peak: peak_delta });
    }

    let avg = sum_delta / num::count_f64(beats.len());

    Ok(DramaturgyMetrics {
        total_beats: beats.len(),
        average_delta_e: avg,
        peak_delta_e: peak_delta,
        peak_index: peak_idx,
        has_inversion,
    })
}

/// Standardized Storybit Benchmark fixtures for verifying narrative engines.
pub mod storybit_benchmark {
    use super::{DramaticBeat, KnowledgeShift};
    /// A gold-standard dramatic arc with clear escalation, betrayal inversion, and resolution.
    #[must_use]
    pub fn gold_standard_arc() -> Vec<DramaticBeat> {
        vec![
            DramaticBeat::new(
                "hook",
                "The courier discovers the sealed royal dispatch contains his own death warrant.",
                KnowledgeShift::Revealed,
                -1,
                3,
            ),
            DramaticBeat::new(
                "rising_action",
                "He flees through the aqueducts while mercenary hounds track his scent.",
                KnowledgeShift::None,
                -2,
                4,
            ),
            DramaticBeat::new(
                "climax",
                "Cornered at the precipice, he realizes the commander hunting him is his long-lost brother.",
                KnowledgeShift::Inverted,
                -3,
                5,
            ),
            DramaticBeat::new(
                "resolution",
                "Together they burn the dispatch and turn back toward the palace gates.",
                KnowledgeShift::None,
                4,
                -2,
            ),
        ]
    }

    /// A defective static arc containing idle summaries that fail the $\Delta E \ge 1.0$ gate.
    #[must_use]
    pub fn static_flawed_arc() -> Vec<DramaticBeat> {
        vec![
            DramaticBeat::new(
                "hook",
                "The city wakes up as the morning sun rises over the rooftops.",
                KnowledgeShift::None,
                0,
                0,
            ),
            DramaticBeat::new(
                "exposition",
                "People walk through the marketplace buying vegetables and bread.",
                KnowledgeShift::None,
                0,
                0,
            ),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_beat_fails_delta_e_threshold() {
        let beat = DramaticBeat::new("intro", "The weather was mild.", KnowledgeShift::None, 0, 0);
        assert!(beat.delta_e() < 1.0);
    }

    #[test]
    fn physical_danger_shift_calculates_correctly() {
        // power_delta = 3, danger_delta = 4 -> hypot(3, 4) = 5.0; KnowledgeShift::None = 0
        let beat = DramaticBeat::new("test", "A sudden ambush.", KnowledgeShift::None, 3, 4);
        assert!((beat.delta_e() - 5.0).abs() < 1e-6);
    }

    #[test]
    fn revelation_boosts_delta_e() {
        // power_delta = 0, danger_delta = 0, KnowledgeShift::Revealed = 2.0
        let beat = DramaticBeat::new(
            "reveal",
            "He found the key.",
            KnowledgeShift::Revealed,
            0,
            0,
        );
        assert!((beat.delta_e() - 2.0).abs() < 1e-6);
    }

    #[test]
    fn gold_standard_arc_passes_validation() {
        let arc = storybit_benchmark::gold_standard_arc();
        let res = validate_beat_gates(&arc);
        assert!(res.is_ok());
        let metrics = res.unwrap();
        assert_eq!(metrics.total_beats, 4);
        assert!(metrics.has_inversion);
        assert!(metrics.peak_delta_e >= 5.0);
    }

    #[test]
    fn static_flawed_arc_is_rejected_with_exact_gate_error() {
        let arc = storybit_benchmark::static_flawed_arc();
        let err = validate_beat_gates(&arc).unwrap_err();
        assert!(matches!(err, BeatError::StaticBeat { index: 0, .. }));
    }
}
