//! Tests for creative quality auditing, prompt expansion, and storyboard cost optimization.

use retold_app::creative;
use retold_app::creative::types::{
    CameraOptics, CinematicLighting, ColorGrading, ModelTarget, OptimizationStrategy,
    PromptEnhanceOptions,
};
use retold_engine::story::Shot;

const DEFAULT_FRAME_FOCUS: [f64; 2] = [0.5, 0.5];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeroStatus {
    Hero,
    Standard,
}

impl HeroStatus {
    const fn is_hero(self) -> bool {
        matches!(self, Self::Hero)
    }
}

#[test]
fn script_audit_detects_cliches_and_sensory_verbs() {
    let sample = "The hero delves into the rich tapestry of the ancient city. He smashed the lock and froze in the dark.";
    let report = creative::audit_script_prose(sample);

    assert!(report.banned_cliches_found.len() >= 2);
    assert!(report.banned_cliches_found.contains(&"delve".to_string()));
    assert!(
        report
            .banned_cliches_found
            .contains(&"rich tapestry".to_string())
    );
    assert_eq!(report.sensory_verb_count, 3);
    assert!(report.script_score < 100);
    assert_ne!(report.recommendations, [] as [std::string::String; 0]);
}

#[test]
fn script_audit_clean_prose_gets_high_score() {
    let clean = "The iron gate broke under the heavy hammer. Cold rain struck his face. He torn the wet letter and dashed into the fog.";
    let report = creative::audit_script_prose(clean);

    assert_eq!(report.banned_cliches_found.len(), 0);
    assert!(report.sensory_verb_count >= 3);
    assert_eq!(report.script_score, 100);
}

#[test]
fn prompt_enhancement_compiles_cinematic_elements() {
    let options = PromptEnhanceOptions {
        target: ModelTarget::Flux,
        lighting: CinematicLighting::VolumetricGodRays,
        optics: CameraOptics::Anamorphic35mm,
        color_grade: ColorGrading::TealAndOrange,
    };
    let enhanced = creative::enhance_prompt("cyberpunk detective standing in alleyway", &options);

    assert_eq!(enhanced.target_model, ModelTarget::Flux);
    assert!(enhanced.positive_prompt.contains("volumetric god rays"));
    assert!(
        enhanced
            .positive_prompt
            .contains("35mm anamorphic prime lens")
    );
    assert!(enhanced.positive_prompt.contains("teal and orange"));
    assert!(
        enhanced
            .positive_prompt
            .contains("8k UHD photographic masterpiece")
    );
    assert_ne!(enhanced.negative_prompt, "");
    assert!(enhanced.estimated_tokens > 20);
}

#[test]
fn prompt_enhancement_midjourney_appends_flags() {
    let options = PromptEnhanceOptions {
        target: ModelTarget::Midjourney,
        lighting: CinematicLighting::Chiaroscuro,
        optics: CameraOptics::Prime50mm,
        color_grade: ColorGrading::KodakPortra400,
    };
    let enhanced = creative::enhance_prompt("portrait of a wandering knight", &options);

    assert_eq!(enhanced.target_model, ModelTarget::Midjourney);
    assert!(enhanced.positive_prompt.contains("--ar 16:9 --style raw"));
}

fn make_fresh_shot(unit: usize, setting: &str, status: HeroStatus) -> Shot {
    Shot {
        unit,
        shot: "wide".into(),
        location: "exterior".into(),
        action: "standing".into(),
        mood: "calm".into(),
        characters: Vec::new(),
        focus: DEFAULT_FRAME_FOCUS,
        motion: None,
        parallax_move: None,
        hero: status.is_hero(),
        poses: Vec::new(),
        setting: setting.into(),
        continuity: "daylight".into(),
        reuse: None,
        sound: None,
        score: None,
        delivery: None,
        props: Vec::new(),
    }
}

fn make_reused_shot(unit: usize, setting: &str, source: usize) -> Shot {
    Shot {
        unit,
        shot: "wide".into(),
        location: "exterior".into(),
        action: "standing".into(),
        mood: "calm".into(),
        characters: Vec::new(),
        focus: DEFAULT_FRAME_FOCUS,
        motion: None,
        parallax_move: None,
        hero: false,
        poses: Vec::new(),
        setting: setting.into(),
        continuity: "daylight".into(),
        reuse: Some(source),
        sound: None,
        score: None,
        delivery: None,
        props: Vec::new(),
    }
}

#[test]
fn cost_optimizer_identifies_reuse_and_parallax() {
    let shot0 = make_fresh_shot(0, "courtyard", HeroStatus::Hero);
    let shot1 = make_fresh_shot(1, "courtyard", HeroStatus::Standard);
    let shot2 = make_reused_shot(2, "courtyard", 0);

    let test_sequence = vec![shot0, shot1, shot2];
    let summary = creative::optimize_storyboard_costs(&test_sequence);

    assert_eq!(summary.total_shots, 3);
    assert_eq!(summary.fresh_generations, 1);
    assert_eq!(summary.parallax_upgrades, 1);
    assert_eq!(summary.reused_frames, 1);
    assert!(summary.savings_percentage > 50.0);

    matches!(
        summary.optimizations[0].strategy,
        OptimizationStrategy::FreshGeneration
    );
    matches!(
        summary.optimizations[1].strategy,
        OptimizationStrategy::ParallaxMotion { .. }
    );
    matches!(
        summary.optimizations[2].strategy,
        OptimizationStrategy::FrameReuse { source_shot: 0 }
    );
}

#[test]
fn payload_queries_return_completed_outcomes() {
    let audit_res = creative::query_audit("The quick brown fox jumped over the lazy dog.");
    assert_eq!(audit_res.exit_code(), 0);

    let options = PromptEnhanceOptions::default();
    let prompt_res = creative::query_prompt_enhance("serene mountain landscape", &options);
    assert_eq!(prompt_res.exit_code(), 0);

    let cost_res = creative::query_cost_optimize(&[]);
    assert_eq!(cost_res.exit_code(), 0);
}

#[test]
fn dramaturgy_benchmark_arc_passes_all_gates() {
    let report = creative::benchmark_arc();
    assert_eq!(report.total_beats, 4);
    assert!(report.passes_gates);
    assert!(report.average_delta_e > 2.0);
    assert!(report.peak_delta_e >= 3.0);
    assert!(report.has_inversion);
    assert_eq!(report.static_beats.len(), 0);
    assert_eq!(report.beat_evaluations.len(), 4);
}

#[test]
fn dramaturgy_static_beats_are_flagged() {
    let unshifted = creative::KnowledgeShift::None;
    let static_beat = creative::DramaticBeat::new(
        "filler",
        "Protagonist drinks a cup of coffee and watches the wall.",
        unshifted,
        0,
        0,
    );
    let report = creative::evaluate_beats(&[static_beat]);
    assert!(!report.passes_gates);
    assert_eq!(report.static_beats.len(), 1);
    assert_eq!(report.static_beats[0].delta_e, 0.0);
    assert!(report.static_beats[0].reason.contains("static filler"));
}

#[test]
fn query_beats_and_benchmark_payloads_return_completed() {
    let benchmark_payload = creative::query_benchmark();
    assert_eq!(benchmark_payload.exit_code(), 0);

    let beats_payload = creative::query_beats(&[]);
    assert_eq!(beats_payload.exit_code(), 0);
}

#[test]
fn dossier_benchmark_audit_passes_all_gates() {
    let (dossier, source, rules) = creative::benchmark_dossier();
    let report = creative::audit_dossier(&dossier, &source, &rules);

    assert_eq!(report.work_title, "The Count of Monte Cristo");
    assert_eq!(report.total_beats, 4);
    assert_eq!(report.must_beats, 4);
    assert_eq!(report.iconic_count, 2);
    assert!(report.passes_gates);
    assert_eq!(report.gate_errors.len(), 0);
}

#[test]
fn bible_benchmark_audit_passes_gates() {
    let bible = creative::benchmark_bible();
    let report = creative::audit_bible(&bible, creative::DEFAULT_PLAN_LIMIT);

    assert_eq!(report.total_characters, 2);
    assert!(report.passes_gates);
    assert_eq!(report.characters[0].name, "Edmond Dantes");
    assert_eq!(report.characters[0].role, "protagonist");
    assert!(report.characters[0].emotions.len() >= 3);
}

#[test]
fn coverage_benchmark_audit_evaluates_adaptation() {
    let (coverage, plan, dossier, narration) = creative::benchmark_coverage();
    let context = creative::CoverageContext {
        coverage: &coverage,
        plan: &plan,
        dossier: &dossier,
        narration,
    };

    let report = creative::audit_script_coverage(&context);
    assert_eq!(report.total_outline_beats, 4);
    assert_eq!(report.covered_beats, 4);
    assert_eq!(report.covered_iconic, 2);
    assert!(report.passes_gates);
}

#[test]
fn query_dossier_bible_coverage_payloads_return_completed() {
    let (dossier, source, rules) = creative::benchmark_dossier();
    let dossier_payload = creative::query_dossier(&dossier, &source, &rules);
    assert_eq!(dossier_payload.exit_code(), 0);

    let bible = creative::benchmark_bible();
    let bible_payload = creative::query_bible(&bible, creative::DEFAULT_PLAN_LIMIT);
    assert_eq!(bible_payload.exit_code(), 0);
}

#[test]
fn storyboard_benchmark_audit_evaluates_camera_choreography() {
    let (direction, plan) = creative::benchmark_direction();
    let report = creative::audit_storyboard(&direction, &plan);

    assert_eq!(report.total_shots, 4);
    assert_eq!(report.dynamic_motion_shots, 4);
    assert_eq!(report.static_shots, 0);
    assert_eq!(report.parallax_coverage_pct, 100.0);
    assert_eq!(report.locations.len(), 4);
    assert!(report.passes_gates);
    assert_eq!(report.recommendations.len(), 0);
    assert_eq!(report.shots[0].motion_type, "parallax push-in");
}

#[test]
fn soundscape_benchmark_audit_evaluates_audio_score_and_delivery() {
    let (sounds, scores, deliveries) = creative::benchmark_soundscape();
    let report = creative::audit_soundscape(&sounds, &scores, &deliveries);

    assert_eq!(report.total_units, 3);
    assert_eq!(report.stings_count, 2);
    assert_eq!(report.silence_moments, 0);
    assert!(report.passes_gates);
    assert_eq!(
        report.units[0].ambience,
        "wind whistling through prison iron bars and distant crashing waves"
    );
    assert_eq!(report.units[0].pace, "fast");
    assert_eq!(report.units[0].tone, "tense");
}

#[test]
fn query_storyboard_and_soundscape_payloads_return_completed() {
    let storyboard_payload = creative::query_benchmark_storyboard();
    assert_eq!(storyboard_payload.exit_code(), 0);

    let soundscape_payload = creative::query_benchmark_soundscape();
    assert_eq!(soundscape_payload.exit_code(), 0);
}

#[test]
fn script_audit_without_vivid_verbs_loses_points() {
    let flat =
        "A stranger visits the woman at night. She is waiting for dawn. Can she live to see it?";
    let report = creative::audit_script_prose(flat);

    assert_eq!(report.banned_cliches_found.len(), 0);
    assert!(report.sensory_verb_count < 3);
    assert!(report.script_score <= 80);
}
