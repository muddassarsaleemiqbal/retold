//! Integration tests for creative CLI commands: audit, prompt enhancement, and cost optimization.
#![expect(clippy::unwrap_used, reason = "test assertions and fixtures")]
#![expect(clippy::expect_used, reason = "test assertions and fixtures")]

use assert_cmd::Command;
use serde_json::Value;

fn run_creative_cli(args: &[&str]) -> Value {
    let mut cmd = Command::cargo_bin("retold").unwrap();
    cmd.arg("--json").args(args);
    let output = cmd.output().expect("cli json output");
    serde_json::from_slice(&output.stdout).expect("valid json envelope")
}

fn run_creative_cli_plain(args: &[&str]) -> String {
    let mut cmd = Command::cargo_bin("retold").unwrap();
    cmd.args(args);
    let output = cmd.output().expect("cli plain output");
    String::from_utf8(output.stdout).expect("valid utf-8 text")
}

#[test]
fn creative_audit_json_reports_cliches_and_score() {
    let text_sample =
        "The detective delves into the ancient secret. He struck the flint and froze.";
    let envelope = run_creative_cli(&["creative", "audit", text_sample]);

    assert_eq!(envelope["type"], "result");
    assert_eq!(envelope["payload"]["outcome"], "completed");

    let audit = &envelope["payload"]["data"]["audit"];
    assert!(audit["script_score"].as_u64().unwrap() <= 100);
    assert_eq!(audit["sensory_verb_count"], 2);

    let cliches = audit["banned_cliches_found"].as_array().unwrap();
    assert_ne!(cliches.as_slice(), [] as [Value; 0]);
}

#[test]
fn creative_prompt_json_compiles_cinematic_tags() {
    let envelope = run_creative_cli(&[
        "creative",
        "prompt",
        "wandering ronin standing at temple entrance",
        "--target",
        "flux",
        "--lighting",
        "volumetric",
        "--optics",
        "anamorphic",
        "--grade",
        "portra",
    ]);

    assert_eq!(envelope["type"], "result");
    assert_eq!(envelope["payload"]["outcome"], "completed");

    let enhanced = &envelope["payload"]["data"]["enhanced_prompt"];
    assert_eq!(enhanced["target_model"], "flux");
    let positive = enhanced["positive_prompt"].as_str().unwrap();
    assert!(positive.contains("volumetric god rays"));
    assert!(positive.contains("35mm anamorphic prime lens"));
    assert!(positive.contains("Kodak Portra 400"));
}

#[test]
fn creative_optimize_json_handles_empty_board() {
    let envelope = run_creative_cli(&["creative", "optimize"]);

    assert_eq!(envelope["type"], "result");
    assert_eq!(envelope["payload"]["outcome"], "completed");

    let cost_opt = &envelope["payload"]["data"]["cost_optimization"];
    assert_eq!(cost_opt["total_shots"], 0);
    assert_eq!(cost_opt["fresh_generations"], 0);
}

#[test]
fn creative_audit_plain_human_output_formats_audit_card() {
    let text_sample = "The detective delves into the ancient secret. He struck the flint.";
    let output = run_creative_cli_plain(&["creative", "audit", text_sample]);

    assert!(output.contains("Script Quality Audit:"));
    assert!(output.contains("Metrics:"));
    assert!(output.contains("Banned cliche: delve"));
    assert!(output.contains("Recommendations:"));
}

#[test]
fn creative_prompt_plain_human_output_formats_prompt_card() {
    let output = run_creative_cli_plain(&[
        "creative",
        "prompt",
        "cyberpunk street vendor at night",
        "--target",
        "midjourney",
    ]);

    assert!(output.contains("Cinematic Prompt [midjourney]"));
    assert!(output.contains("Positive:"));
    assert!(output.contains("Negative:"));
    assert!(output.contains("--ar 16:9 --style raw"));
}

#[test]
fn creative_benchmark_json_returns_arc_report() {
    let envelope = run_creative_cli(&["creative", "benchmark"]);

    assert_eq!(envelope["type"], "result");
    assert_eq!(envelope["payload"]["outcome"], "completed");

    let report = &envelope["payload"]["data"]["beat_report"];
    assert_eq!(report["total_beats"], 4);
    assert_eq!(report["passes_gates"], true);
    assert_eq!(report["has_inversion"], true);
    assert!(report["peak_delta_e"].as_f64().unwrap() >= 3.0);
}

#[test]
fn creative_beats_benchmark_flag_matches_benchmark_command() {
    let envelope = run_creative_cli(&["creative", "beats", "--benchmark"]);

    assert_eq!(envelope["type"], "result");
    assert_eq!(envelope["payload"]["outcome"], "completed");

    let report = &envelope["payload"]["data"]["beat_report"];
    assert_eq!(report["total_beats"], 4);
    assert_eq!(report["passes_gates"], true);
}

#[test]
fn creative_benchmark_plain_human_output_formats_tension_arc() {
    let output = run_creative_cli_plain(&["creative", "benchmark"]);

    assert!(output.contains("Dramatic Tension Arc (ΔE per Beat):"));
    assert!(output.contains("Total Beats:       4"));
    assert!(output.contains("Peak Tension:"));
    assert!(output.contains("Gate Status:       PASSED"));
}

#[test]
fn creative_dossier_json_reports_structure_and_gates() {
    let envelope = run_creative_cli(&["creative", "dossier"]);

    assert_eq!(envelope["type"], "result");
    assert_eq!(envelope["payload"]["outcome"], "completed");

    let report = &envelope["payload"]["data"]["dossier_report"];
    assert_eq!(report["work_title"], "The Count of Monte Cristo");
    assert_eq!(report["total_beats"], 4);
    assert_eq!(report["must_beats"], 4);
    assert_eq!(report["passes_gates"], true);
}

#[test]
fn creative_bible_json_reports_characters() {
    let envelope = run_creative_cli(&["creative", "bible"]);

    assert_eq!(envelope["type"], "result");
    assert_eq!(envelope["payload"]["outcome"], "completed");

    let report = &envelope["payload"]["data"]["bible_report"];
    assert_eq!(report["total_characters"], 2);
    assert_eq!(report["passes_gates"], true);
    assert_eq!(report["characters"][0]["name"], "Edmond Dantes");
}

#[test]
fn creative_coverage_json_reports_realization() {
    let envelope = run_creative_cli(&["creative", "coverage"]);

    assert_eq!(envelope["type"], "result");
    assert_eq!(envelope["payload"]["outcome"], "completed");

    let report = &envelope["payload"]["data"]["coverage_report"];
    assert_eq!(report["total_outline_beats"], 4);
    assert_eq!(report["covered_beats"], 4);
    assert_eq!(report["passes_gates"], true);
}

#[test]
fn creative_dossier_plain_human_card() {
    let output = run_creative_cli_plain(&["creative", "dossier"]);

    assert!(output.contains("Research Dossier: The Count of Monte Cristo"));
    assert!(output.contains("Chronological Beats: 4"));
    assert!(output.contains("Gates Status:        PASSED"));
}

#[test]
fn creative_bible_plain_human_card() {
    let output = run_creative_cli_plain(&["creative", "bible"]);

    assert!(output.contains("Character Bible (2 characters, Status: PASSED):"));
    assert!(output.contains("[protagonist] Edmond Dantes"));
    assert!(output.contains("[antagonist] Fernand Mondego"));
    assert!(output.contains("Visual Look:"));
}

#[test]
fn creative_coverage_plain_human_card() {
    let output = run_creative_cli_plain(&["creative", "coverage"]);

    assert!(output.contains("Script Adaptation Coverage:"));
    assert!(output.contains("Beats Realized:   4/4"));
    assert!(output.contains("Gate Status:      PASSED"));
}

#[test]
fn creative_storyboard_json_reports_choreography() {
    let envelope = run_creative_cli(&["creative", "storyboard"]);

    assert_eq!(envelope["type"], "result");
    assert_eq!(envelope["payload"]["outcome"], "completed");

    let report = &envelope["payload"]["data"]["storyboard_report"];
    assert_eq!(report["total_shots"], 4);
    assert_eq!(report["dynamic_motion_shots"], 4);
    assert_eq!(report["passes_gates"], true);
}

#[test]
fn creative_storyboard_plain_human_card() {
    let output = run_creative_cli_plain(&["creative", "storyboard"]);

    assert!(output.contains("Storyboard Choreography (4 shots, Status: PASSED):"));
    assert!(output.contains("Dynamic Motion:   4 shots"));
    assert!(output.contains("Panavision C-Series"));
}

#[test]
fn creative_audio_json_reports_soundscape() {
    let envelope = run_creative_cli(&["creative", "audio"]);

    assert_eq!(envelope["type"], "result");
    assert_eq!(envelope["payload"]["outcome"], "completed");

    let report = &envelope["payload"]["data"]["soundscape_report"];
    assert_eq!(report["total_units"], 3);
    assert_eq!(report["dominant_mood"], "mysterious");
    assert_eq!(report["passes_gates"], true);
}

#[test]
fn creative_audio_plain_human_card() {
    let output = run_creative_cli_plain(&["creative", "audio"]);

    assert!(output.contains("Audio Soundscape & Score (3 units, Status: PASSED):"));
    assert!(output.contains("Dominant Mood:    mysterious"));
    assert!(output.contains("Score Accents:    2 stings"));
}
