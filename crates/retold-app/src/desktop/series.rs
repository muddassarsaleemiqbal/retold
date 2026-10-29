//! Typed series projections for native clients over the shared series registry and planner.
use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

use crate::protocol::{Diagnostic, ErrorCode, Payload};
use crate::series::{self, SeriesManifest, SeriesSpec, SeriesSummary, execution};

/// One stage of one part's linked video, as recorded on disk.
#[derive(Debug, Clone, Deserialize)]
pub struct Cell {
    pub stage: String,
    pub decision: String,
}

/// Parts in order, each with its linked video's observed stages (none while unlinked).
#[derive(Debug, Clone)]
pub struct Matrix {
    pub manifest: SeriesManifest,
    pub progress: BTreeMap<String, Option<Vec<Cell>>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlanBlocker {
    pub message: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Authority {
    usd_micros: u64,
    requests: u64,
}

/// A reviewed subset plan; its hash is the only authority a run accepts.
#[derive(Debug, Clone, Deserialize)]
pub struct SeriesPlan {
    pub text: String,
    pub plan_hash: String,
    pub can_execute: bool,
    pub blockers: Vec<PlanBlocker>,
    authority: Authority,
    #[serde(skip)]
    pub parts: Option<String>,
    #[serde(skip)]
    pub spend: String,
}

impl SeriesPlan {
    #[must_use]
    pub const fn usd_micros(&self) -> u64 {
        self.authority.usd_micros
    }
    #[must_use]
    pub const fn requests(&self) -> u64 {
        self.authority.requests
    }
}

fn contents(payload: Payload) -> Result<serde_json::Value, Diagnostic> {
    match payload {
        Payload::Result { contents, .. } => Ok(contents),
        _ => Err(invalid("unexpected series response")),
    }
}

fn field<T: serde::de::DeserializeOwned>(
    value: &serde_json::Value,
    name: &str,
) -> Result<T, Diagnostic> {
    serde_json::from_value(value.get(name).cloned().unwrap_or_default()).map_err(invalid)
}

pub fn list(root: &Path) -> Result<Vec<SeriesSummary>, Diagnostic> {
    field(&contents(series::list_series(root)?)?, "items")
}

pub fn matrix(root: &Path, id: &str) -> Result<Matrix, Diagnostic> {
    let value = contents(series::show_series(root, id)?)?;
    Ok(Matrix {
        manifest: field(&value, "series")?,
        progress: field(&value, "parts_progress")?,
    })
}

/// Link one part to an existing video with the series' guide and channel.
pub fn link(root: &Path, id: &str, (part, video): (&str, &str)) -> Result<Matrix, Diagnostic> {
    series::link_video(root, id, (part, video))?;
    matrix(root, id)
}

/// Review a plan for the selected parts (all when `parts` is none) under a total allowance.
pub fn plan(
    root: &Path,
    id: &str,
    (parts, spend): (Option<&str>, &str),
) -> Result<SeriesPlan, Diagnostic> {
    let target = execution::Target { root, id, parts };
    let options = execution::Options {
        spend: Some(spend.into()),
        ..execution::Options::default()
    };
    let mut plan: SeriesPlan =
        serde_json::from_value(contents(execution::plan(target, &options)?)?).map_err(invalid)?;
    plan.parts = parts.map(str::to_owned);
    plan.spend = spend.into();
    Ok(plan)
}

/// Submit exactly the reviewed plan; a changed plan is rejected before any job is queued.
pub fn run(root: &Path, id: &str, reviewed: &SeriesPlan) -> Result<String, Diagnostic> {
    let target = execution::Target {
        root,
        id,
        parts: reviewed.parts.as_deref(),
    };
    let options = execution::Options {
        spend: Some(reviewed.spend.clone()),
        plan_hash: Some(reviewed.plan_hash.clone()),
        ..execution::Options::default()
    };
    field(&contents(execution::run(target, &options)?)?, "text")
}

/// A new series: its work title, guide, channel and number of parts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSeries {
    pub work: String,
    pub guide: String,
    pub channel: String,
    pub parts: usize,
}

/// The most parts one native creation may declare; more can be added later.
pub const MAX_NEW_PARTS: usize = 24;

fn require_usable(new: &NewSeries) -> Result<(), Diagnostic> {
    let named = !new.work.trim().is_empty();
    let sized = (1..=MAX_NEW_PARTS).contains(&new.parts);
    if !(named && sized) {
        return Err(Diagnostic::new(
            ErrorCode::InputRequired,
            "Name the work and choose 1 to 24 parts.",
        ));
    }
    Ok(())
}

/// Create a series registry entry; parts start unlinked and nothing is queued.
pub fn create(root: &Path, new: &NewSeries) -> Result<Matrix, Diagnostic> {
    require_usable(new)?;
    let spec = SeriesSpec {
        guide: &new.guide,
        work: new.work.trim(),
        kind: "film",
        channel: Some(&new.channel),
        budget_micros: None,
        parts_count: new.parts,
    };
    let value = contents(series::new_series(root, &spec)?)?;
    let created: SeriesManifest = field(&value, "series")?;
    matrix(root, &created.id)
}

/// A reviewed addition of parts: the series version it was computed from and the new part IDs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartAddition {
    pub expected_version: u32,
    pub count: usize,
    pub new_parts: Vec<String>,
}

/// Preview adding `count` unlinked parts; nothing is written.
pub fn review_add_parts(root: &Path, id: &str, count: usize) -> Result<PartAddition, Diagnostic> {
    let usable = (1..=MAX_NEW_PARTS).contains(&count);
    if !usable {
        return Err(Diagnostic::new(
            ErrorCode::InputRequired,
            "Add 1 to 24 parts at a time.",
        ));
    }
    let manifest = matrix(root, id)?.manifest;
    let start = manifest.parts.len();
    Ok(PartAddition {
        expected_version: manifest.version,
        count,
        new_parts: (1..=count)
            .map(|offset| format!("P{:02}", start + offset))
            .collect(),
    })
}

/// Add the reviewed parts only while the series is still at the reviewed version.
pub fn add_parts(root: &Path, id: &str, reviewed: &PartAddition) -> Result<Matrix, Diagnostic> {
    let current = matrix(root, id)?.manifest.version;
    if current != reviewed.expected_version {
        return Err(Diagnostic::new(
            ErrorCode::PlanChanged,
            "The series changed. Review the new parts again.",
        ));
    }
    series::update_series(root, id, reviewed.count)?;
    matrix(root, id)
}

fn invalid(error: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::new(ErrorCode::DataInvalid, &format!("Series: {error}"))
}
