//! User audio library management (music and SFX) with licence and cue checks.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::protocol::{Diagnostic, ErrorCode, Outcome, Payload};

/// Moods the director scores with; music coverage is reported against them.
pub use retold_engine::story::SCORE_MOODS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioCategory {
    Music,
    Sfx,
}

impl AudioCategory {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Music => "music",
            Self::Sfx => "sfx",
        }
    }

    #[must_use]
    pub fn parse(category_name: &str) -> Option<Self> {
        match category_name {
            "music" => Some(Self::Music),
            "sfx" => Some(Self::Sfx),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioLicense {
    pub license_type: String,
    pub attribution: Option<String>,
    pub commercial_use: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioItem {
    pub id: String,
    pub category: AudioCategory,
    pub title: String,
    pub file_name: String,
    /// Measured from the file at import; `None` when the format cannot be read here.
    #[serde(default)]
    pub duration_seconds: Option<f64>,
    pub tags: Vec<String>,
    pub license: AudioLicense,
    pub verified: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AudioStore {
    pub items: BTreeMap<String, AudioItem>,
}

#[derive(Debug, Clone)]
pub struct AudioImportSpec<'a> {
    pub category: AudioCategory,
    pub source_path: &'a Path,
    pub title: Option<&'a str>,
    pub license_type: Option<&'a str>,
    pub attribution: Option<&'a str>,
    pub commercial_use: bool,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct AudioTagSpec<'a> {
    pub category: AudioCategory,
    pub id: &'a str,
    pub tags: Vec<String>,
}

fn audio_dir(library_root: &Path, category: AudioCategory) -> PathBuf {
    library_root.join("audio").join(category.as_str())
}

fn store_file_path(library_root: &Path, category: AudioCategory) -> PathBuf {
    let file_name = format!("{}.json", category.as_str());
    library_root.join("audio").join(file_name)
}

fn load_store(library_root: &Path, category: AudioCategory) -> AudioStore {
    let path = store_file_path(library_root, category);
    let Ok(bytes) = fs::read(&path) else {
        return AudioStore::default();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

fn persist_store(
    library_root: &Path,
    category: AudioCategory,
    store: &AudioStore,
) -> Result<(), Diagnostic> {
    let path = store_file_path(library_root, category);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let json = serde_json::to_string_pretty(store)
        .map_err(|err| Diagnostic::new(ErrorCode::DataInvalid, &err.to_string()))?;
    fs::write(&path, json)
        .map_err(|err| Diagnostic::new(ErrorCode::ExecutionFailed, &err.to_string()))
}

fn resolve_file_name(source: &Path) -> Result<String, Diagnostic> {
    let name_opt = source.file_name().and_then(|name| name.to_str());
    match name_opt {
        Some(name_str) => Ok(name_str.to_string()),
        None => Err(Diagnostic::new(
            ErrorCode::InputRequired,
            "Invalid source audio file path.",
        )),
    }
}

fn derive_audio_id(file_name: &str) -> String {
    let stem = file_name
        .rsplit_once('.')
        .map_or(file_name, |(stem, _)| stem);
    stem.to_lowercase().replace(' ', "-")
}

fn copy_source_file(source: &Path, destination: &Path) -> Result<(), Diagnostic> {
    if let Some(parent) = destination.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::copy(source, destination)
        .map(|_| ())
        .map_err(|err| Diagnostic::new(ErrorCode::ExecutionFailed, &err.to_string()))
}

fn build_license(spec: &AudioImportSpec<'_>) -> AudioLicense {
    let default_license = "unknown";
    let license_type = spec.license_type.unwrap_or(default_license).to_string();
    let attribution = spec.attribution.map(ToString::to_string);
    AudioLicense {
        license_type,
        attribution,
        commercial_use: spec.commercial_use,
    }
}

fn record_imported_item(mut store: AudioStore, item_id: String, item: AudioItem) -> AudioStore {
    store.items.insert(item_id, item);
    store
}

fn construct_item(
    file_name: &str,
    spec: &AudioImportSpec<'_>,
    duration_seconds: Option<f64>,
) -> AudioItem {
    let item_id = derive_audio_id(file_name);
    let license = build_license(spec);
    let title = spec
        .title
        .map_or_else(|| item_id.clone(), ToString::to_string);
    AudioItem {
        id: item_id,
        category: spec.category,
        title,
        file_name: file_name.to_string(),
        duration_seconds,
        tags: spec.tags.clone(),
        license,
        verified: true,
    }
}

pub fn import(library_root: &Path, spec: &AudioImportSpec<'_>) -> Result<Payload, Diagnostic> {
    let source_exists = spec.source_path.exists();
    if !source_exists {
        return Err(Diagnostic::new(
            ErrorCode::InputRequired,
            "Source audio file does not exist.",
        ));
    }
    let file_name = resolve_file_name(spec.source_path)?;
    let target_dir = audio_dir(library_root, spec.category);
    let target_path = target_dir.join(&file_name);
    copy_source_file(spec.source_path, &target_path)?;

    let item = construct_item(&file_name, spec, wav_seconds(&target_path));
    let store = load_store(library_root, spec.category);
    let updated_store = record_imported_item(store, item.id.clone(), item.clone());
    persist_store(library_root, spec.category, &updated_store)?;

    let text = format!(
        "Imported {} asset '{}'.",
        spec.category.as_str(),
        item.title
    );
    Ok(build_item_payload(&text, &item))
}

fn build_item_payload(text: &str, item: &AudioItem) -> Payload {
    Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": text,
            "item": item,
        }),
    }
}

pub fn list(library_root: &Path, category: AudioCategory) -> Result<Payload, Diagnostic> {
    let store = load_store(library_root, category);
    let items: Vec<AudioItem> = store.items.into_values().collect();
    let total = items.len();
    Ok(Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "category": category.as_str(),
            "directory": audio_dir(library_root, category),
            "items": items,
            "total": total,
        }),
    })
}

fn apply_tags(
    mut store: AudioStore,
    id: &str,
    new_tags: &[String],
) -> Result<(AudioStore, AudioItem), Diagnostic> {
    let item_entry = store.items.get_mut(id);
    let Some(item) = item_entry else {
        return Err(Diagnostic::new(
            ErrorCode::RefNotFound,
            "Audio item not found.",
        ));
    };
    for tag in new_tags {
        let tag_missing = !item.tags.contains(tag);
        if tag_missing {
            item.tags.push(tag.clone());
        }
    }
    let cloned_item = item.clone();
    Ok((store, cloned_item))
}

pub fn tag(library_root: &Path, spec: &AudioTagSpec<'_>) -> Result<Payload, Diagnostic> {
    let store = load_store(library_root, spec.category);
    let (updated_store, item) = apply_tags(store, spec.id, &spec.tags)?;
    persist_store(library_root, spec.category, &updated_store)?;
    let text = format!("Updated tags for '{}'.", item.title);
    Ok(build_item_payload(&text, &item))
}

fn single_item_licence(store: &AudioStore, target_id: &str) -> Result<Payload, Diagnostic> {
    let item_opt = store.items.get(target_id);
    let Some(item) = item_opt else {
        return Err(Diagnostic::new(
            ErrorCode::RefNotFound,
            "Audio item not found.",
        ));
    };
    let text = format!(
        "Licence for '{}': {}.",
        item.title, item.license.license_type
    );
    Ok(build_item_payload(&text, item))
}

pub fn licence(
    library_root: &Path,
    category: AudioCategory,
    target_id: Option<&str>,
) -> Result<Payload, Diagnostic> {
    let store = load_store(library_root, category);
    if let Some(id) = target_id {
        return single_item_licence(&store, id);
    }
    let items: Vec<AudioItem> = store.items.into_values().collect();
    let unverified_count = items
        .iter()
        .filter(|item| !item.license.commercial_use)
        .count();
    Ok(Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "category": category.as_str(),
            "items": items,
            "non_commercial_count": unverified_count,
        }),
    })
}

fn verify_audio_items(
    library_root: &Path,
    category: AudioCategory,
    mut store: AudioStore,
) -> (AudioStore, usize) {
    let target_dir = audio_dir(library_root, category);
    let mut verified_count: usize = 0;
    for item in store.items.values_mut() {
        let file_path = target_dir.join(&item.file_name);
        let file_exists = file_path.exists();
        item.verified = file_exists;
        if file_exists {
            verified_count = verified_count.saturating_add(1);
        }
    }
    (store, verified_count)
}

fn check_single_item(store: &AudioStore, target_id: &str) -> Result<Payload, Diagnostic> {
    let item_opt = store.items.get(target_id);
    let Some(item) = item_opt else {
        return Err(Diagnostic::new(
            ErrorCode::RefNotFound,
            "Audio item not found.",
        ));
    };
    let text = format!(
        "Audio check for '{}': verified={}.",
        item.title, item.verified
    );
    Ok(build_item_payload(&text, item))
}

fn summarize_check(category: AudioCategory, verified_count: usize, total: usize) -> Payload {
    let text = format!(
        "Verified {verified_count}/{total} {category_name} files on disk.",
        category_name = category.as_str()
    );
    Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": text,
            "category": category.as_str(),
            "verified_count": verified_count,
            "total": total,
        }),
    }
}

pub fn check(
    library_root: &Path,
    category: AudioCategory,
    target_id: Option<&str>,
) -> Result<Payload, Diagnostic> {
    let store = load_store(library_root, category);
    let (updated_store, verified_count) = verify_audio_items(library_root, category, store);
    persist_store(library_root, category, &updated_store)?;

    if let Some(id) = target_id {
        return check_single_item(&updated_store, id);
    }
    let total = updated_store.items.len();
    Ok(summarize_check(category, verified_count, total))
}

/// Duration of a PCM or float WAV file; other formats are not measured here.
fn wav_seconds(path: &Path) -> Option<f64> {
    let bytes = fs::read(path).ok()?;
    let wave = retold_engine::retold_voice::audio::Wave::decode(&bytes).ok()?;
    let frames = u32::try_from(wave.samples.len()).ok()?;
    let rate = Some(wave.rate).filter(|rate| *rate > 0)?;
    Some(f64::from(frames) / f64::from(rate))
}
