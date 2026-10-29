//! Workspace creative resources: installed guides, channel profiles, and creative optimization.

use std::fs;
use std::path::Path;

use retold_engine::retold_guide::channel::Channel;
use retold_engine::retold_guide::manifest::InputSpec;
use retold_engine::retold_guide::{Catalog, Guide};
use serde::{Deserialize, Serialize};

use crate::protocol::{Diagnostic, ErrorCode, Outcome, Payload};

pub mod audit;
pub mod beats;
pub mod choreography;
pub mod dossier;
pub mod optimizer;
pub mod prompts;
pub mod types;

pub use audit::audit_script_prose;
pub use beats::{benchmark_arc, evaluate_beats};
pub use choreography::{
    audit_soundscape, audit_storyboard, benchmark_direction, benchmark_soundscape,
};
pub use dossier::{
    CoverageContext, DEFAULT_PLAN_LIMIT, audit_bible, audit_dossier, audit_script_coverage,
    benchmark_bible, benchmark_coverage, benchmark_dossier, benchmark_plan_outline,
};
pub use optimizer::optimize_storyboard_costs;
pub use prompts::enhance_prompt;
pub use retold_engine::dossier::{Bible, Coverage, Dossier};
pub use retold_engine::research::Source;
pub use retold_engine::retold_dramaturgy::beat::{DramaticBeat, KnowledgeShift};
pub use retold_engine::retold_guide::manifest::DossierRules;
pub use retold_engine::story::{
    Delivery, Direction, Pace, ParallaxMove, Plan, SceneSound, Score, Shot, ShotMotion, Tone,
};
pub use types::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuideSummary {
    pub id: String,
    pub name: String,
    pub version: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuideInputItem {
    pub name: String,
    pub description: String,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelSummary {
    pub id: String,
    pub name: String,
    pub handle: String,
}

fn map_guide_input(name: &str, spec: &InputSpec) -> GuideInputItem {
    GuideInputItem {
        name: name.to_string(),
        description: spec.help.clone().unwrap_or_default(),
        required: spec.required,
    }
}

fn summarize_guide(guide: &Guide) -> GuideSummary {
    let manifest = guide.manifest();
    GuideSummary {
        id: manifest.id.to_string(),
        name: manifest.name.clone(),
        version: manifest.version.clone(),
        summary: manifest.summary.clone(),
    }
}

fn summarize_channel(channel: &Channel) -> ChannelSummary {
    let file = channel.file();
    ChannelSummary {
        id: file.id.to_string(),
        name: file.name.clone(),
        handle: file.handle.clone(),
    }
}

pub fn list_guides(library_root: &Path) -> Result<Payload, Diagnostic> {
    let guides_dir = library_root.join("guides");
    let catalog = Catalog::load(Some(&guides_dir)).map_err(|err| {
        Diagnostic::new(
            ErrorCode::ExecutionFailed,
            &format!("Failed to load guides: {err}"),
        )
    })?;
    let summaries: Vec<GuideSummary> = catalog
        .entries()
        .iter()
        .map(|entry| summarize_guide(&entry.guide))
        .collect();
    let total = summaries.len();
    let text = format!("Found {total} installed guide(s).");
    Ok(Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": text,
            "items": summaries,
            "total": total,
        }),
    })
}

fn build_guide_payload(loaded: &Guide) -> Payload {
    let manifest = loaded.manifest();
    let inputs: Vec<GuideInputItem> = manifest
        .inputs
        .iter()
        .map(|(input_name, spec)| map_guide_input(input_name, spec))
        .collect();
    let summary = summarize_guide(loaded);
    let text = format!(
        "Guide '{}' (v{}): {}",
        summary.name, summary.version, summary.summary
    );
    Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": text,
            "guide": summary,
            "inputs": inputs,
        }),
    }
}

pub fn show_guide(library_root: &Path, guide_id: &str) -> Result<Payload, Diagnostic> {
    let target_dir = library_root.join("guides").join(guide_id);
    if !target_dir.exists() {
        return Err(Diagnostic::new(
            ErrorCode::InputRequired,
            &format!("Guide '{guide_id}' not found."),
        ));
    }
    let loaded = Guide::from_dir(&target_dir).map_err(|err| {
        Diagnostic::new(
            ErrorCode::ExecutionFailed,
            &format!("Failed to parse guide '{guide_id}': {err}"),
        )
    })?;
    Ok(build_guide_payload(&loaded))
}

fn load_entry_channel(entry_path: &Path) -> Option<ChannelSummary> {
    let is_directory = entry_path.is_dir();
    let manifest_exists = entry_path.join("channel.toml").exists();
    if !is_directory || !manifest_exists {
        return None;
    }
    Channel::load(entry_path)
        .ok()
        .map(|loaded| summarize_channel(&loaded))
}

fn collect_channel_summaries(channels_dir: &Path) -> Vec<ChannelSummary> {
    let mut channels = Vec::new();
    if let Ok(entries) = fs::read_dir(channels_dir) {
        for entry in entries.flatten() {
            if let Some(summary) = load_entry_channel(&entry.path()) {
                channels.push(summary);
            }
        }
    }
    channels
}

pub fn list_channels(library_root: &Path) -> Result<Payload, Diagnostic> {
    let channels_dir = library_root.join("channels");
    let channels = collect_channel_summaries(&channels_dir);
    let total = channels.len();
    let text = format!("Found {total} installed channel(s).");
    Ok(Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": text,
            "items": channels,
            "total": total,
        }),
    })
}

fn build_channel_payload(loaded: &Channel) -> Payload {
    let summary = summarize_channel(loaded);
    let file = loaded.file();
    let voice_languages: Vec<String> = file
        .voices
        .keys()
        .map(std::string::ToString::to_string)
        .collect();
    let text = format!(
        "Channel '{}' (@{}): {} voice language(s).",
        summary.name,
        summary.handle,
        voice_languages.len()
    );
    Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": text,
            "channel": summary,
            "voices": voice_languages,
        }),
    }
}

pub fn show_channel(library_root: &Path, channel_id: &str) -> Result<Payload, Diagnostic> {
    let target_dir = library_root.join("channels").join(channel_id);
    if !target_dir.exists() {
        return Err(Diagnostic::new(
            ErrorCode::InputRequired,
            &format!("Channel '{channel_id}' not found."),
        ));
    }
    let loaded = Channel::load(&target_dir).map_err(|err| {
        Diagnostic::new(
            ErrorCode::ExecutionFailed,
            &format!("Failed to parse channel '{channel_id}': {err}"),
        )
    })?;
    Ok(build_channel_payload(&loaded))
}

#[must_use]
pub fn query_audit(text: &str) -> Payload {
    let report = audit_script_prose(text);
    let summary_text = format!(
        "Script quality score: {}/100 ({} cliches detected, {} sensory verbs, {:.1}% density).",
        report.script_score,
        report.banned_cliches_found.len(),
        report.sensory_verb_count,
        report.sensory_verb_density_pct
    );

    Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": summary_text,
            "audit": report,
        }),
    }
}

#[must_use]
pub fn query_prompt_enhance(base_prompt: &str, options: &PromptEnhanceOptions) -> Payload {
    let enhanced = enhance_prompt(base_prompt, options);
    let summary_text = format!(
        "Enhanced prompt for {:?} ({} estimated tokens).",
        enhanced.target_model, enhanced.estimated_tokens
    );

    Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": summary_text,
            "enhanced_prompt": enhanced,
        }),
    }
}

#[must_use]
pub fn query_cost_optimize(shots: &[Shot]) -> Payload {
    let summary = optimize_storyboard_costs(shots);
    let summary_text = format!(
        "Cost optimization: {} shots analyzed, saved {} ({:.1}% savings) via {} frame re-uses and {} parallax moves.",
        summary.total_shots,
        summary.savings_usd,
        summary.savings_percentage,
        summary.reused_frames,
        summary.parallax_upgrades
    );

    Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": summary_text,
            "cost_optimization": summary,
        }),
    }
}

#[must_use]
pub fn query_beats(beats: &[DramaticBeat]) -> Payload {
    let report = evaluate_beats(beats);
    let summary_text = format!(
        "Dramaturgy arc evaluation: {} beats, avg ΔE = {:.2}, peak ΔE = {:.2} (Beat {}). Gates pass: {}.",
        report.total_beats,
        report.average_delta_e,
        report.peak_delta_e,
        report.peak_index + 1,
        report.passes_gates
    );

    Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": summary_text,
            "beat_report": report,
        }),
    }
}

#[must_use]
pub fn query_benchmark() -> Payload {
    let report = benchmark_arc();
    let summary_text = format!(
        "Storybit benchmark arc: {} beats, avg ΔE = {:.2}, peak ΔE = {:.2} (Beat {}). Reality inversion: {}.",
        report.total_beats,
        report.average_delta_e,
        report.peak_delta_e,
        report.peak_index + 1,
        report.has_inversion
    );

    Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": summary_text,
            "beat_report": report,
        }),
    }
}

#[must_use]
pub fn query_dossier(dossier: &Dossier, source: &Source, rules: &DossierRules) -> Payload {
    let report = audit_dossier(dossier, source, rules);
    let summary_text = format!(
        "Dossier audit for '{}' ({}): {} beats ({} must), {} iconic elements. Passes gates: {}.",
        report.work_title,
        report.work_year,
        report.total_beats,
        report.must_beats,
        report.iconic_count,
        report.passes_gates
    );

    Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": summary_text,
            "dossier_report": report,
        }),
    }
}

#[must_use]
pub fn query_bible(bible: &Bible, plan_limit: u32) -> Payload {
    let report = audit_bible(bible, plan_limit);
    let summary_text = format!(
        "Character bible audit: {} profiled characters. Passes gates: {}.",
        report.total_characters, report.passes_gates
    );

    Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": summary_text,
            "bible_report": report,
        }),
    }
}

#[must_use]
pub fn query_coverage(context: &CoverageContext<'_>) -> Payload {
    let report = audit_script_coverage(context);
    let summary_text = format!(
        "Script adaptation coverage: {}/{} beats covered, {}/{} iconic items covered. Passes: {}.",
        report.covered_beats,
        report.total_outline_beats,
        report.covered_iconic,
        context.plan.iconic.len(),
        report.passes_gates
    );

    Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": summary_text,
            "coverage_report": report,
        }),
    }
}

#[must_use]
pub fn query_benchmark_dossier() -> Payload {
    let (dossier, source, rules) = benchmark_dossier();
    query_dossier(&dossier, &source, &rules)
}

#[must_use]
pub fn query_benchmark_bible() -> Payload {
    let bible = benchmark_bible();
    query_bible(&bible, DEFAULT_PLAN_LIMIT)
}

#[must_use]
pub fn query_benchmark_coverage() -> Payload {
    let (coverage, plan, dossier, narration) = benchmark_coverage();
    let context = CoverageContext {
        coverage: &coverage,
        plan: &plan,
        dossier: &dossier,
        narration,
    };
    query_coverage(&context)
}

#[must_use]
pub fn query_storyboard(direction: &Direction, plan: &Plan) -> Payload {
    let report = audit_storyboard(direction, plan);
    let summary_text = format!(
        "Storyboard audit: {} total shots ({} dynamic motion, {} static). Parallax coverage: {:.1}%. Passes: {}.",
        report.total_shots,
        report.dynamic_motion_shots,
        report.static_shots,
        report.parallax_coverage_pct,
        report.passes_gates
    );

    Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": summary_text,
            "storyboard_report": report,
        }),
    }
}

#[must_use]
pub fn query_soundscape(
    sounds: &[SceneSound],
    scores: &[Score],
    deliveries: &[Delivery],
) -> Payload {
    let report = audit_soundscape(sounds, scores, deliveries);
    let summary_text = format!(
        "Soundscape audit: {} narrative units, {} stings, {} silences. Dominant mood: {}. Passes: {}.",
        report.total_units,
        report.stings_count,
        report.silence_moments,
        report.dominant_mood,
        report.passes_gates
    );

    Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": summary_text,
            "soundscape_report": report,
        }),
    }
}

#[must_use]
pub fn query_benchmark_storyboard() -> Payload {
    let (direction, plan) = benchmark_direction();
    query_storyboard(&direction, &plan)
}

#[must_use]
pub fn query_benchmark_soundscape() -> Payload {
    let (sounds, scores, deliveries) = benchmark_soundscape();
    query_soundscape(&sounds, &scores, &deliveries)
}
