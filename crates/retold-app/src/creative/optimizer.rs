//! Storyboard cost optimization, frame re-use, and parallax camera animation analysis.

use crate::creative::types::{
    CostOptimizationSummary, DEFAULT_IMAGE_COST_MICROS, OptimizationStrategy, PARALLAX_COST_MICROS,
    ShotOptimization,
};
use retold_engine::story::Shot;

const PERCENT_MULTIPLIER: f64 = 100.0;
const MICROS_PER_DOLLAR: f64 = 1_000_000.0;

#[expect(clippy::cast_precision_loss, reason = "display dollar amounts")]
fn format_usd(micros: i64) -> String {
    let dollars = (micros as f64) / MICROS_PER_DOLLAR;
    format!("${dollars:.2}")
}

fn check_reuse_optimization(shot_index: usize, current_shot: &Shot) -> Option<ShotOptimization> {
    current_shot.reuse.map(|source_index| ShotOptimization {
        shot_index,
        strategy: OptimizationStrategy::FrameReuse {
            source_shot: source_index,
        },
        original_cost_micros: DEFAULT_IMAGE_COST_MICROS,
        optimized_cost_micros: 0,
        savings_micros: DEFAULT_IMAGE_COST_MICROS,
        explanation: format!("Recuts frame from shot {source_index} at $0 cost"),
    })
}

fn is_parallax_candidate(current: &Shot, previous: &Shot) -> bool {
    let same_setting = !current.setting.is_empty() && current.setting == previous.setting;
    let same_poses = current.poses.len() == previous.poses.len();
    same_setting && same_poses
}

const fn select_parallax_motion(shot: &Shot) -> &'static str {
    if shot.hero { "push_in" } else { "orbit" }
}

fn check_parallax_optimization(
    shot_index: usize,
    current_shot: &Shot,
    previous_shot: &Shot,
) -> Option<ShotOptimization> {
    if !is_parallax_candidate(current_shot, previous_shot) {
        return None;
    }
    let motion = select_parallax_motion(current_shot);
    let savings = DEFAULT_IMAGE_COST_MICROS - PARALLAX_COST_MICROS;
    Some(ShotOptimization {
        shot_index,
        strategy: OptimizationStrategy::ParallaxMotion {
            motion_name: motion.to_string(),
        },
        original_cost_micros: DEFAULT_IMAGE_COST_MICROS,
        optimized_cost_micros: PARALLAX_COST_MICROS,
        savings_micros: savings,
        explanation: format!("Upgrades shared scene with 2.5D parallax {motion} animation"),
    })
}

fn fresh_generation_optimization(shot_index: usize) -> ShotOptimization {
    ShotOptimization {
        shot_index,
        strategy: OptimizationStrategy::FreshGeneration,
        original_cost_micros: DEFAULT_IMAGE_COST_MICROS,
        optimized_cost_micros: DEFAULT_IMAGE_COST_MICROS,
        savings_micros: 0,
        explanation: "Key hero moment requiring fresh diffusion generation".to_string(),
    }
}

fn analyze_shot(
    shot_index: usize,
    current_shot: &Shot,
    previous_shot: Option<&Shot>,
) -> ShotOptimization {
    if let Some(reuse_opt) = check_reuse_optimization(shot_index, current_shot) {
        return reuse_opt;
    }
    if let Some(parallax_opt) =
        previous_shot.and_then(|prev| check_parallax_optimization(shot_index, current_shot, prev))
    {
        return parallax_opt;
    }
    fresh_generation_optimization(shot_index)
}

struct Tally {
    fresh: usize,
    reused: usize,
    parallax: usize,
    original_micros: i64,
    optimized_micros: i64,
}

fn tally_optimizations(optimizations: &[ShotOptimization]) -> Tally {
    let mut tally = Tally {
        fresh: 0,
        reused: 0,
        parallax: 0,
        original_micros: 0,
        optimized_micros: 0,
    };
    for item in optimizations {
        tally.original_micros = tally
            .original_micros
            .saturating_add(item.original_cost_micros);
        tally.optimized_micros = tally
            .optimized_micros
            .saturating_add(item.optimized_cost_micros);
        match item.strategy {
            OptimizationStrategy::FreshGeneration => tally.fresh += 1,
            OptimizationStrategy::FrameReuse { .. } => tally.reused += 1,
            OptimizationStrategy::ParallaxMotion { .. } => tally.parallax += 1,
        }
    }
    tally
}

#[expect(clippy::cast_precision_loss, reason = "savings percentage calculation")]
fn compute_cost_summary(optimizations: Vec<ShotOptimization>) -> CostOptimizationSummary {
    let tally = tally_optimizations(&optimizations);
    let total_shots = optimizations.len();
    let savings_micros = tally.original_micros.saturating_sub(tally.optimized_micros);
    let savings_pct = if tally.original_micros > 0 {
        ((savings_micros as f64) / (tally.original_micros as f64)) * PERCENT_MULTIPLIER
    } else {
        0.0
    };

    CostOptimizationSummary {
        total_shots,
        fresh_generations: tally.fresh,
        reused_frames: tally.reused,
        parallax_upgrades: tally.parallax,
        original_cost_usd: format_usd(tally.original_micros),
        optimized_cost_usd: format_usd(tally.optimized_micros),
        savings_usd: format_usd(savings_micros),
        savings_percentage: savings_pct,
        optimizations,
    }
}

#[must_use]
pub fn optimize_storyboard_costs(shots: &[Shot]) -> CostOptimizationSummary {
    let mut optimizations = Vec::with_capacity(shots.len());
    let mut previous_shot: Option<&Shot> = None;

    for (index, shot) in shots.iter().enumerate() {
        let opt = analyze_shot(index, shot, previous_shot);
        optimizations.push(opt);
        previous_shot = Some(shot);
    }

    compute_cost_summary(optimizations)
}
