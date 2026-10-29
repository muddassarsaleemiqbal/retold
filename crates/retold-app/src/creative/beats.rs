//! Dramatic beat modeling, narrative tension curve, and emotional delta evaluation.

use retold_engine::retold_dramaturgy::beat::{
    DramaticBeat, KnowledgeShift, storybit_benchmark, validate_beat_gates,
};

use super::types::{BeatArcReport, BeatEvaluation, StaticBeatWarning};

pub const MIN_GATE_DELTA_E: f64 = 1.0;
pub const MIN_CLIMAX_DELTA_E: f64 = 3.0;
pub const SPARKLINE_SCALE: f64 = 2.0;
pub const SPARKLINE_MAX_CHARS: usize = 12;
pub const MAX_BEATS_FOR_AVERAGE: usize = 100_000;
pub const DEFAULT_PEAK_INDEX: usize = 0;

fn build_sparkline(delta_e: f64) -> String {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "positive scaled delta"
    )]
    let scaled = (delta_e * SPARKLINE_SCALE).round() as usize;
    let bar_length = scaled.min(SPARKLINE_MAX_CHARS);
    "█".repeat(bar_length)
}

const fn describe_knowledge_shift(shift: KnowledgeShift) -> &'static str {
    match shift {
        KnowledgeShift::None => "none",
        KnowledgeShift::Revealed => "revealed",
        KnowledgeShift::Inverted => "inverted",
    }
}

fn evaluate_single_beat(index: usize, beat: &DramaticBeat) -> BeatEvaluation {
    let delta_e = beat.delta_e();
    let is_climax = delta_e >= MIN_CLIMAX_DELTA_E;
    let sparkline = build_sparkline(delta_e);
    let shift_name = describe_knowledge_shift(beat.protagonist_knowledge);

    BeatEvaluation {
        index,
        section: beat.section.clone(),
        text: beat.text.clone(),
        knowledge_shift: shift_name.to_string(),
        power_delta: beat.power_delta,
        danger_delta: beat.danger_delta,
        delta_e,
        is_climax,
        sparkline,
    }
}

fn build_static_warning(index: usize, section: &str, delta_e: f64) -> StaticBeatWarning {
    let reason = format!(
        "ΔE = {delta_e:.2} < 1.0 indicates static filler. Increase physical peril or reveal hidden facts."
    );
    StaticBeatWarning {
        index,
        section: section.to_string(),
        delta_e,
        reason,
    }
}

fn collect_static_beats(beats: &[DramaticBeat]) -> Vec<StaticBeatWarning> {
    beats
        .iter()
        .enumerate()
        .filter_map(|(idx, beat)| {
            let delta = beat.delta_e();
            if delta < MIN_GATE_DELTA_E {
                Some(build_static_warning(idx, &beat.section, delta))
            } else {
                None
            }
        })
        .collect()
}

fn calculate_average(sum: f64, count: usize) -> f64 {
    if count == 0 {
        return 0.0;
    }
    let bounded = count.min(MAX_BEATS_FOR_AVERAGE);
    #[expect(
        clippy::cast_precision_loss,
        reason = "bounded count within integer precision"
    )]
    let divisor = bounded as f64;
    sum / divisor
}

fn find_peak_delta_and_index(evaluations: &[BeatEvaluation]) -> (f64, usize) {
    let mut peak = 0.0;
    let mut peak_idx = DEFAULT_PEAK_INDEX;
    for eval in evaluations {
        if eval.delta_e > peak {
            peak = eval.delta_e;
            peak_idx = eval.index;
        }
    }
    (peak, peak_idx)
}

fn has_reality_inversion(beats: &[DramaticBeat]) -> bool {
    beats
        .iter()
        .any(|beat| beat.protagonist_knowledge == KnowledgeShift::Inverted)
}

fn build_failed_report(
    beats: &[DramaticBeat],
    evaluations: Vec<BeatEvaluation>,
    static_beats: Vec<StaticBeatWarning>,
) -> BeatArcReport {
    let (peak_delta_e, peak_index) = find_peak_delta_and_index(&evaluations);
    let total = beats.len();
    let sum: f64 = evaluations
        .iter()
        .map(|evaluation| evaluation.delta_e)
        .sum();
    let avg = calculate_average(sum, total);
    let has_inversion = has_reality_inversion(beats);

    BeatArcReport {
        total_beats: total,
        average_delta_e: avg,
        peak_delta_e,
        peak_index,
        has_inversion,
        passes_gates: false,
        static_beats,
        beat_evaluations: evaluations,
    }
}

#[must_use]
pub fn evaluate_beats(beats: &[DramaticBeat]) -> BeatArcReport {
    let evaluations: Vec<BeatEvaluation> = beats
        .iter()
        .enumerate()
        .map(|(idx, beat)| evaluate_single_beat(idx, beat))
        .collect();

    let static_beats = collect_static_beats(beats);
    let gate_result = validate_beat_gates(beats);

    match gate_result {
        Ok(metrics) => BeatArcReport {
            total_beats: metrics.total_beats,
            average_delta_e: metrics.average_delta_e,
            peak_delta_e: metrics.peak_delta_e,
            peak_index: metrics.peak_index,
            has_inversion: metrics.has_inversion,
            passes_gates: true,
            static_beats,
            beat_evaluations: evaluations,
        },
        Err(_) => build_failed_report(beats, evaluations, static_beats),
    }
}

#[must_use]
pub fn benchmark_arc() -> BeatArcReport {
    let arc = storybit_benchmark::gold_standard_arc();
    evaluate_beats(&arc)
}
