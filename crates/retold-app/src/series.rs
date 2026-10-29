//! Series management and registry without unverified generation.

use std::fs;
use std::path::{Path, PathBuf};

use retold_engine::Slug;
use serde::{Deserialize, Serialize};

use crate::protocol::{Diagnostic, ErrorCode, Outcome, Payload};

pub mod execution;

pub const SERIES_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeriesPart {
    pub id: String,
    pub title: String,
    pub order: u32,
    pub completed: bool,
    pub video_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeriesCharacter {
    pub name: String,
    pub look: String,
    pub first_part: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeriesManifest {
    pub schema_version: u32,
    pub id: String,
    pub work: String,
    pub kind: String,
    pub guide: String,
    pub channel: String,
    pub created: String,
    pub version: u32,
    pub parts: Vec<SeriesPart>,
    pub characters: Vec<SeriesCharacter>,
    pub budget_usd_micros: u64,
}

impl Default for SeriesManifest {
    fn default() -> Self {
        Self {
            schema_version: SERIES_SCHEMA_VERSION,
            id: String::new(),
            work: String::new(),
            kind: String::new(),
            guide: String::new(),
            channel: "default".to_string(),
            created: String::new(),
            version: 1,
            parts: Vec::new(),
            characters: Vec::new(),
            budget_usd_micros: 0,
        }
    }
}

impl SeriesManifest {
    fn apply_spec(mut self, spec: &SeriesSpec<'_>, series_id: &str) -> Self {
        self.id = series_id.to_string();
        self.work = spec.work.to_string();
        self.kind = spec.kind.to_string();
        self.guide = spec.guide.to_string();
        if let Some(chan) = spec.channel {
            self.channel = chan.to_string();
        }
        if let Some(budget) = spec.budget_micros {
            self.budget_usd_micros = budget;
        }
        self
    }
}

#[derive(Debug, Clone, Default)]
pub struct SeriesSpec<'a> {
    pub guide: &'a str,
    pub work: &'a str,
    pub kind: &'a str,
    pub channel: Option<&'a str>,
    pub budget_micros: Option<u64>,
    pub parts_count: usize,
}

fn construct_initial_manifest(series_id: &str, spec: &SeriesSpec<'_>) -> SeriesManifest {
    let mut manifest = SeriesManifest::default().apply_spec(spec, series_id);
    manifest.created = jiff::Timestamp::now().to_string();
    manifest.parts = generate_initial_parts(spec.work, spec.parts_count);
    manifest
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeriesSummary {
    pub id: String,
    pub work: String,
    pub kind: String,
    pub channel: String,
    pub part_count: usize,
    pub created: String,
}

fn generate_initial_parts(work: &str, count: usize) -> Vec<SeriesPart> {
    let effective_count = count.max(1);
    (1..=effective_count)
        .map(|index| {
            let part_id = format!("P{index:02}");
            let title = format!("{work} - Part {index}");
            let order = u32::try_from(index).unwrap_or(1);
            SeriesPart {
                id: part_id,
                title,
                order,
                completed: false,
                video_ref: None,
            }
        })
        .collect()
}

fn slugify_work(work: &str) -> Result<String, Diagnostic> {
    Slug::from_title(work)
        .map(|slug| slug.as_str().to_string())
        .map_err(|err| Diagnostic::new(ErrorCode::InputRequired, &err.to_string()))
}

fn series_root_dir(library_root: &Path) -> PathBuf {
    library_root.join("series")
}

fn series_folder_dir(library_root: &Path, series_id: &str) -> PathBuf {
    series_root_dir(library_root).join(series_id)
}

fn series_manifest_path(series_dir: &Path) -> PathBuf {
    series_dir.join("series.json")
}

fn versions_dir(series_dir: &Path) -> PathBuf {
    series_dir.join("versions")
}

fn publish_version_snapshot(
    series_dir: &Path,
    version: u32,
    manifest_bytes: &[u8],
) -> Result<(), Diagnostic> {
    let target_dir = versions_dir(series_dir);
    fs::create_dir_all(&target_dir)
        .map_err(|err| Diagnostic::new(ErrorCode::ExecutionFailed, &err.to_string()))?;
    let snapshot_file = target_dir.join(format!("v{version}.json"));
    fs::write(&snapshot_file, manifest_bytes)
        .map_err(|err| Diagnostic::new(ErrorCode::ExecutionFailed, &err.to_string()))
}

fn write_series_manifest(series_dir: &Path, manifest: &SeriesManifest) -> Result<(), Diagnostic> {
    let manifest_path = series_manifest_path(series_dir);
    let bytes = serde_json::to_vec_pretty(manifest)
        .map_err(|err| Diagnostic::new(ErrorCode::DataInvalid, &err.to_string()))?;
    fs::write(&manifest_path, &bytes)
        .map_err(|err| Diagnostic::new(ErrorCode::ExecutionFailed, &err.to_string()))?;
    publish_version_snapshot(series_dir, manifest.version, &bytes)
}

fn prepare_series_dir(library_root: &Path, series_id: &str) -> Result<PathBuf, Diagnostic> {
    let series_dir = series_folder_dir(library_root, series_id);
    if series_dir.exists() {
        return Err(Diagnostic::new(
            ErrorCode::DataInvalid,
            "A series with this name already exists.",
        ));
    }
    fs::create_dir_all(&series_dir)
        .map_err(|err| Diagnostic::new(ErrorCode::ExecutionFailed, &err.to_string()))?;
    Ok(series_dir)
}

pub fn new_series(library_root: &Path, spec: &SeriesSpec<'_>) -> Result<Payload, Diagnostic> {
    let work_name = spec.work.trim();
    if work_name.is_empty() {
        return Err(Diagnostic::new(
            ErrorCode::InputRequired,
            "Series work name is required.",
        ));
    }
    let series_id = slugify_work(spec.work)?;
    let series_dir = prepare_series_dir(library_root, &series_id)?;
    let manifest = construct_initial_manifest(&series_id, spec);
    write_series_manifest(&series_dir, &manifest)?;
    let text = format!(
        "Created series '{}' with {} part(s).",
        spec.work,
        manifest.parts.len()
    );
    Ok(Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": text,
            "series": manifest,
        }),
    })
}

fn load_series_manifest(series_dir: &Path) -> Result<SeriesManifest, Diagnostic> {
    let manifest_path = series_manifest_path(series_dir);
    let bytes = fs::read(&manifest_path)
        .map_err(|err| Diagnostic::new(ErrorCode::RefNotFound, &err.to_string()))?;
    serde_json::from_slice(&bytes)
        .map_err(|err| Diagnostic::new(ErrorCode::DataInvalid, &err.to_string()))
}

fn build_series_summary(manifest: SeriesManifest) -> SeriesSummary {
    SeriesSummary {
        id: manifest.id,
        work: manifest.work,
        kind: manifest.kind,
        channel: manifest.channel,
        part_count: manifest.parts.len(),
        created: manifest.created,
    }
}

fn collect_series_summaries(root: &Path) -> Result<Vec<SeriesSummary>, Diagnostic> {
    let entries = fs::read_dir(root)
        .map_err(|err| Diagnostic::new(ErrorCode::ExecutionFailed, &err.to_string()))?;
    let mut items = Vec::new();
    for entry in entries.flatten() {
        let is_dir = entry.file_type().is_ok_and(|kind| kind.is_dir());
        if is_dir {
            let manifest_res = load_series_manifest(&entry.path());
            if let Ok(manifest) = manifest_res {
                items.push(build_series_summary(manifest));
            }
        }
    }
    Ok(items)
}

pub fn list_series(library_root: &Path) -> Result<Payload, Diagnostic> {
    let root = series_root_dir(library_root);
    let items = if root.exists() {
        collect_series_summaries(&root)?
    } else {
        Vec::new()
    };
    let total = items.len();
    Ok(Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "items": items,
            "total": total,
        }),
    })
}

pub fn show_series(library_root: &Path, series_id: &str) -> Result<Payload, Diagnostic> {
    let series_dir = series_folder_dir(library_root, series_id);
    let manifest = load_series_manifest(&series_dir)?;
    let progress = parts_progress(library_root, &manifest);
    let manifest = refresh_completion(library_root, manifest);
    let total_parts = manifest.parts.len();
    let completed_parts = manifest.parts.iter().filter(|part| part.completed).count();
    let text = format!(
        "Series '{}': {}/{} parts completed.",
        manifest.work, completed_parts, total_parts
    );
    Ok(Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": text,
            "series": manifest,
            "parts_progress": progress,
            "completed_parts": completed_parts,
            "total_parts": total_parts,
        }),
    })
}

fn append_new_parts(
    mut parts: Vec<SeriesPart>,
    work: &str,
    additional_count: usize,
) -> Vec<SeriesPart> {
    let current_count = parts.len();
    for offset in 1..=additional_count {
        let next_index = current_count + offset;
        let part_id = format!("P{next_index:02}");
        let title = format!("{work} - Part {next_index}");
        let order = u32::try_from(next_index).unwrap_or(u32::MAX);
        parts.push(SeriesPart {
            id: part_id,
            title,
            order,
            completed: false,
            video_ref: None,
        });
    }
    parts
}

pub fn update_series(
    library_root: &Path,
    series_id: &str,
    additional_parts: usize,
) -> Result<Payload, Diagnostic> {
    let series_dir = series_folder_dir(library_root, series_id);
    let mut manifest = load_series_manifest(&series_dir)?;
    if additional_parts > 0 {
        manifest.parts = append_new_parts(manifest.parts, &manifest.work, additional_parts);
    }
    manifest.version = manifest.version.saturating_add(1);
    write_series_manifest(&series_dir, &manifest)?;
    let text = format!(
        "Updated series '{}' to version {}.",
        manifest.work, manifest.version
    );
    Ok(Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": text,
            "series": manifest,
        }),
    })
}

fn filter_parts<'a>(parts: &'a [SeriesPart], selector: Option<&str>) -> Vec<&'a SeriesPart> {
    let Some(filter_str) = selector else {
        return parts.iter().collect();
    };
    let target_ids: Vec<&str> = filter_str.split(',').map(str::trim).collect();
    parts
        .iter()
        .filter(|part| target_ids.contains(&part.id.as_str()))
        .collect()
}

/// Link an existing draft; binding never grants generation authority.
pub fn link_video(root: &Path, id: &str, link: (&str, &str)) -> Result<Payload, Diagnostic> {
    let (part_id, reference) = link;
    let directory = series_folder_dir(root, id);
    let mut manifest = load_series_manifest(&directory)?;
    let catalog = crate::discovery::scan(root)?;
    let video = catalog.resolve(reference)?;
    if video.guide != manifest.guide || video.channel != manifest.channel {
        return Err(Diagnostic::new(
            ErrorCode::InputRequired,
            "Video guide and channel must match the series.",
        ));
    }
    let part = manifest
        .parts
        .iter_mut()
        .find(|part| part.id == part_id)
        .ok_or_else(|| Diagnostic::new(ErrorCode::InputRequired, "Choose an existing part ID."))?;
    part.video_ref = Some(video.reference.clone());
    part.completed = false;
    manifest.version = manifest.version.saturating_add(1);
    write_series_manifest(&directory, &manifest)?;
    Ok(Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({"text":format!("Linked {part_id} to {}.",video.reference),"series":manifest}),
    })
}

fn parts_progress(root: &Path, manifest: &SeriesManifest) -> serde_json::Value {
    let catalog = crate::discovery::scan(root);
    let mut progress = serde_json::Map::new();
    for part in &manifest.parts {
        let evidence = part.video_ref.as_deref().and_then(|reference| {
            let catalog = catalog.as_ref().ok()?;
            let snapshot = crate::video::snapshot(catalog, reference).ok()?;
            serde_json::to_value(crate::video::stages(&snapshot.recorded)).ok()
        });
        progress.insert(part.id.clone(), evidence.unwrap_or(serde_json::Value::Null));
    }
    serde_json::Value::Object(progress)
}

/// Completion is observed from the linked run, never from a queue receipt or stale registry flag.
fn refresh_completion(root: &Path, mut manifest: SeriesManifest) -> SeriesManifest {
    let catalog = crate::discovery::scan(root);
    for part in &mut manifest.parts {
        part.completed = part.video_ref.as_deref().is_some_and(|reference| {
            catalog
                .as_ref()
                .is_ok_and(|catalog| saved_ready(catalog, reference).unwrap_or(false))
        });
    }
    manifest
}

fn saved_ready(catalog: &crate::library::Catalog, reference: &str) -> Result<bool, Diagnostic> {
    let snapshot = crate::video::snapshot(catalog, reference)?;
    let verified = snapshot.recorded.steps.iter().any(|step| {
        step.step == "timeline"
            && step.integrity == retold_engine::store::inspection::Integrity::EnvelopeVerified
    });
    let deliveries = retold_engine::session::prepared_deliveries(&snapshot.video.layout)
        .map_err(|error| Diagnostic::new(ErrorCode::DataInvalid, &error.to_string()))?;
    Ok(verified && !deliveries.is_empty())
}
