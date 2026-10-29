//! Read-only guide suggestions from explicitly selected saved runs. No provider or media work.
use crate::{
    EngineError, RunMeta,
    languages::Edition,
    run::GuideRef,
    store::{self, State},
};
use retold_guide::Guide;
use retold_parallax::render::{Fallback, Rendered};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    path::{Path, PathBuf},
};

#[derive(Debug, Serialize)]
pub struct Report {
    pub schema: u32,
    pub scope: &'static str,
    pub runs: Vec<RunAssessment>,
    pub proposals: Vec<Proposal>,
    pub conclusion: &'static str,
}
#[derive(Debug, Serialize)]
pub struct RunAssessment {
    pub run: PathBuf,
    pub run_id: String,
    pub guide: GuideRef,
    pub channel_hash: String,
    pub quality: String,
    pub observations: Vec<Observation>,
    pub unavailable: Vec<String>,
}
#[derive(Debug, Serialize)]
pub struct Observation {
    pub kind: &'static str,
    pub subject: String,
    pub evidence: PathBuf,
    pub status: &'static str,
    pub detail: Value,
}
#[derive(Debug, Serialize)]
pub struct Proposal {
    pub guide: GuideRef,
    pub channel_hash: String,
    pub quality: String,
    pub variant: String,
    pub independent_runs: usize,
    pub compared_inputs: usize,
    pub field: &'static str,
    pub before: f64,
    pub after: f64,
    pub diff: String,
    pub evidence: Vec<PathBuf>,
    pub tradeoff: &'static str,
}
struct Loaded {
    assessment: RunAssessment,
    guide: Guide,
    source_key: String,
    // Hold a shared lock while inspecting the run, without creating or updating any file.
    _lock: File,
}

pub fn assess(paths: &[PathBuf]) -> Result<Report, EngineError> {
    if paths.is_empty() {
        return Err(invalid(
            Path::new("."),
            "select at least one saved run directory",
        ));
    }
    let mut loaded = Vec::new();
    let mut ids = BTreeSet::new();
    let mut directories = BTreeSet::new();
    for path in paths {
        let run = std::fs::canonicalize(path).map_err(|e| EngineError::io(path, e))?;
        if !directories.insert(run.clone()) {
            return Err(invalid(&run, "the same run was selected twice"));
        }
        let item = load(&run)?;
        if !ids.insert(item.assessment.run_id.clone()) {
            return Err(invalid(
                &run,
                "duplicate run identity; copied runs are not independent evidence",
            ));
        }
        loaded.push(item);
    }
    loaded.sort_by(|a, b| a.assessment.run.cmp(&b.assessment.run));
    let proposals = propose(&loaded)?;
    Ok(Report {
        schema: 1,
        scope: "Local quality evidence only; no audience-performance claims, provider calls or guide writes",
        conclusion: if proposals.is_empty() {
            "No guide change justified by the selected evidence; observations may need manual review or more independent runs."
        } else {
            "Candidate diffs need human review and representative validation; no guide was changed."
        },
        proposals,
        runs: loaded.into_iter().map(|r| r.assessment).collect(),
    })
}
fn load(run: &Path) -> Result<Loaded, EngineError> {
    let meta_dir = run.join(".retold");
    let lock_path = meta_dir.join("lock");
    let lock = File::open(&lock_path).map_err(|e| EngineError::io(&lock_path, e))?;
    lock.try_lock_shared()
        .map_err(|_| invalid(run, "run is busy; retry after it stops"))?;
    let meta: RunMeta = read(&meta_dir.join("run.json"))?;
    let state: State = read(&meta_dir.join("state.json"))?;
    if meta.schema != crate::run::RUN_SCHEMA || state.schema != 1 {
        return Err(invalid(run, "unsupported run/state schema"));
    }
    if state.active.is_some() || !state.active_tasks.is_empty() || state.error.is_some() {
        return Err(invalid(
            run,
            "run is unfinished or stopped with an error; resume it before assessment",
        ));
    }
    let guide =
        Guide::from_dir(&meta_dir.join("guide")).map_err(|e| invalid(run, &e.to_string()))?;
    if guide.content_hash() != meta.guide.hash
        || guide.id() != meta.guide.id
        || guide.manifest().version != meta.guide.version
    {
        return Err(invalid(
            run,
            "guide snapshot differs from the recorded identity",
        ));
    }
    let channel_path = meta_dir.join("channel.toml");
    let channel = std::fs::read(&channel_path).map_err(|e| EngineError::io(&channel_path, e))?;
    let source_key = store::hash(&meta.inputs)?;
    let mut result = RunAssessment {
        run: run.to_owned(),
        run_id: meta.id,
        guide: meta.guide,
        channel_hash: blake3::hash(&channel).to_hex().to_string(),
        quality: meta.quality.to_string(),
        observations: vec![],
        unavailable: vec![],
    };
    motion(&mut result, &guide, &state)?;
    writing(&mut result)?;
    timing(&mut result, &guide)?;
    Ok(Loaded {
        assessment: result,
        guide,
        source_key,
        _lock: lock,
    })
}
fn motion(result: &mut RunAssessment, guide: &Guide, state: &State) -> Result<(), EngineError> {
    let files = files(&result.run.join("work/parallax-shots"))?;
    let mut count = 0;
    for path in files.into_iter().filter(|p| {
        p.file_name()
            .is_some_and(|n| n.to_string_lossy().ends_with(".audit.json"))
    }) {
        let value: Value = read(&path)?;
        let rendered: Rendered =
            serde_json::from_value(value.clone()).map_err(|e| invalid(&path, &e.to_string()))?;
        let step = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_suffix(".audit.json"))
            .ok_or_else(|| invalid(&path, "invalid shot audit filename"))?;
        let variant = step
            .strip_prefix("shots-")
            .and_then(|s| s.split_once('-'))
            .map(|(_, v)| v)
            .ok_or_else(|| invalid(&path, "invalid shot identifier"))?;
        let settings = &guide.manifest().visuals.parallax;
        let expected = match rendered.fallback {
            Fallback::Requested => settings.strength,
            Fallback::Gentler => settings.strength * 0.5,
            Fallback::Flat => 0.0,
        };
        let current = state
            .completed
            .get(step)
            .is_some_and(|record| store::hash(&value).is_ok_and(|h| h == record.output_hash));
        let bounded = value["audit"].get("fill_radius_ratio").is_some()
            && value["audit"].get("framing_scale").is_some()
            && retold_parallax::render::Quality::default().passes(rendered.audit);
        let verified = current
            && bounded
            && settings.enabled
            && rendered.applied_strength.is_finite()
            && (rendered.applied_strength - expected).abs() < 1e-9;
        result.observations.push(Observation {
            kind:"parallax", subject:step.into(), evidence:path.clone(),
            status:if verified {"verified"} else {"unverified_or_legacy"},
            detail:json!({"variant":variant,"requested_strength":settings.strength,"applied_strength":rendered.applied_strength,"fallback":rendered.fallback,"audit":rendered.audit}),
        });
        count += 1;
    }
    if count == 0 {
        result.unavailable.push(
            "No parallax audit evidence; absent or disabled QA is unknown, not a pass.".into(),
        );
    }
    Ok(())
}
fn writing(result: &mut RunAssessment) -> Result<(), EngineError> {
    let mut count = 0;
    for path in files(&result.run.join("work/02-script"))?
        .into_iter()
        .filter(|p| {
            p.file_name().is_some_and(|n| {
                let n = n.to_string_lossy();
                n.starts_with("checks-") && n.ends_with(".json")
            })
        })
    {
        let value: Value = read(&path)?;
        let gates = value
            .get("gates")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid(&path, "missing writing gate results"))?;
        for gate in gates {
            let id = gate
                .get("gate")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid(&path, "missing gate id"))?;
            let message = gate
                .get("message")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid(&path, "missing gate message"))?;
            result.observations.push(Observation {kind:"writing",subject:id.into(),evidence:path.clone(),status:"review_only",detail:json!({"message":message,"note":"Historical repair pass; not proof of a current guide defect. No quality gate should be weakened."})});
        }
        count += 1;
    }
    if count == 0 {
        result
            .unavailable
            .push("No saved writing checks; writing quality is unknown.".into());
    }
    Ok(())
}
fn timing(result: &mut RunAssessment, guide: &Guide) -> Result<(), EngineError> {
    let path = result.run.join("work/07-captions/editions.json");
    if !path.try_exists().map_err(|e| EngineError::io(&path, e))? {
        result
            .unavailable
            .push("No saved language editions; narration timing is unknown.".into());
        return Ok(());
    }
    let editions: Vec<Edition> = read(&path)?;
    if editions.is_empty() {
        result
            .unavailable
            .push("No language editions recorded; narration timing is unknown.".into());
    }
    for edition in editions {
        let seconds = edition.alignment.duration;
        if !seconds.is_finite() || seconds <= 0.0 {
            return Err(invalid(&path, "invalid aligned speech duration"));
        }
        let m = guide.manifest();
        let range = m
            .length
            .get(&edition.variant.narration)
            .and_then(|r| r.seconds)
            .unwrap_or(m.format.target_seconds);
        let total = seconds + m.edit.ending.seconds;
        result.observations.push(Observation {kind:"timing",subject:edition.variant.to_string(),evidence:path.clone(),status:"review_only",detail:json!({"narration_seconds":seconds,"with_ending_seconds":total,"target_seconds":range,"within_target":(range[0]..=range[1]).contains(&total),"alignment_matched":edition.alignment.matched,"note":"Saved post-fit timing; cannot establish original pacing or justify changing targets by itself."})});
    }
    Ok(())
}

type GroupKey = (String, String, String, String);
fn propose(loaded: &[Loaded]) -> Result<Vec<Proposal>, EngineError> {
    let mut groups: BTreeMap<GroupKey, Vec<&Loaded>> = BTreeMap::new();
    for run in loaded {
        let variants: BTreeSet<_> = run
            .assessment
            .observations
            .iter()
            .filter(|o| o.kind == "parallax")
            .filter_map(|o| o.detail["variant"].as_str())
            .collect();
        for variant in variants {
            groups
                .entry((
                    run.assessment.guide.hash.clone(),
                    run.assessment.channel_hash.clone(),
                    run.assessment.quality.clone(),
                    variant.into(),
                ))
                .or_default()
                .push(run);
        }
    }
    let mut proposals = vec![];
    for ((_, channel_hash, quality, variant), runs) in groups {
        let all_sources: BTreeSet<_> = runs.iter().map(|r| &r.source_key).collect();
        let mut independent = BTreeSet::new();
        let mut evidence = vec![];
        for run in &runs {
            let rows: Vec<_> = run
                .assessment
                .observations
                .iter()
                .filter(|o| o.kind == "parallax" && o.detail["variant"] == variant)
                .collect();
            let recovered = rows
                .iter()
                .filter(|o| o.detail["fallback"] != "requested")
                .count();
            let gentler = rows.iter().any(|o| o.detail["fallback"] == "gentler");
            if rows.len() >= 2
                && rows.iter().all(|o| o.status == "verified")
                && recovered * 2 >= rows.len()
                && gentler
                && independent.insert(run.source_key.clone())
            {
                evidence.extend(rows.iter().map(|o| o.evidence.clone()));
            }
        }
        if independent.len() < 3 || independent.len() * 2 < all_sources.len() {
            continue;
        }
        let first = runs[0];
        let before = first.guide.manifest().visuals.parallax.strength;
        let after = before * 0.5;
        if let Some(diff) = strength_diff(&first.guide, after)? {
            proposals.push(Proposal {guide:first.assessment.guide.clone(),channel_hash,quality,variant,independent_runs:independent.len(),compared_inputs:all_sources.len(),field:"visuals.parallax.strength",before,after,diff,evidence,tradeoff:"Less depth motion. Validate representative scenes, review the exact diff, and create a new guide version before adopting it; existing runs stay unchanged."});
        }
    }
    Ok(proposals)
}
fn strength_diff(guide: &Guide, after: f64) -> Result<Option<String>, EngineError> {
    let text = guide
        .file("guide.toml")
        .ok_or_else(|| invalid(Path::new("guide.toml"), "missing guide manifest"))?;
    let mut in_table = false;
    let mut lines: Vec<_> = text.lines().map(str::to_owned).collect();
    for i in 0..lines.len() {
        let trimmed = lines[i].trim();
        if trimmed.starts_with('[') {
            in_table = trimmed.split('#').next().unwrap_or("").trim() == "[visuals.parallax]";
            continue;
        }
        if !in_table {
            continue;
        }
        if let Some((key, _)) = trimmed.split_once('=')
            && key.trim() == "strength"
        {
            let old = lines[i].clone();
            let indent_len = old.len().saturating_sub(old.trim_start().len());
            let indent = &old[..indent_len];
            let comment = old
                .split_once('#')
                .map_or(String::new(), |(_, s)| format!(" #{s}"));
            lines[i] = format!("{indent}strength = {after}{comment}");
            let mut candidate = guide.files().clone();
            candidate.insert("guide.toml".into(), format!("{}\n", lines.join("\n")));
            Guide::from_files(retold_guide::Origin::Inline, candidate)
                .map_err(|e| invalid(Path::new("guide.toml"), &e.to_string()))?;
            return Ok(Some(format!(
                "--- guide.toml\n+++ guide.toml (candidate only)\n@@ -{},1 +{},1 @@\n-{old}\n+{}\n",
                i + 1,
                i + 1,
                lines[i]
            )));
        }
    }
    Ok(None) // Inline-table manifests still get observations, without a guessed text patch.
}
fn read<T: DeserializeOwned>(path: &Path) -> Result<T, EngineError> {
    let bytes = std::fs::read(path).map_err(|e| EngineError::io(path, e))?;
    serde_json::from_slice(&bytes).map_err(|e| invalid(path, &e.to_string()))
}
fn files(path: &Path) -> Result<Vec<PathBuf>, EngineError> {
    let entries = match std::fs::read_dir(path) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(EngineError::io(path, e)),
    };
    let mut paths = vec![];
    for entry in entries {
        let entry = entry.map_err(|e| EngineError::io(path, e))?;
        if entry
            .file_type()
            .map_err(|e| EngineError::io(&entry.path(), e))?
            .is_file()
        {
            paths.push(entry.path());
        }
    }
    paths.sort();
    Ok(paths)
}
fn invalid(path: &Path, message: &str) -> EngineError {
    EngineError::Config {
        path: path.to_owned(),
        message: message.into(),
    }
}
