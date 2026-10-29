//! Saved views: reusable search, tag, and sort filters for library browsing.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::protocol::{Diagnostic, ErrorCode, Outcome, Payload};

const HASH_LENGTH: usize = 16;
const VIEWS_DIR_NAME: &str = "views";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemovalConsent {
    Confirmed,
    Unconfirmed,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ViewFilter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archive: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub columns: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewRecord {
    pub name: String,
    pub version: u32,
    pub filters: ViewFilter,
    pub hash: String,
}

fn validate_view_name(name: &str) -> Result<(), Diagnostic> {
    let valid = !name.is_empty()
        && name.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        });
    if valid {
        Ok(())
    } else {
        Err(Diagnostic::new(
            ErrorCode::InputRequired,
            "View name must contain only alphanumeric characters, dashes, and underscores.",
        ))
    }
}

fn compute_view_hash(content: &str) -> String {
    let digest = blake3::hash(content.as_bytes());
    digest.to_hex()[..HASH_LENGTH].to_string()
}

fn views_directory(library_root: &Path) -> PathBuf {
    library_root.join(VIEWS_DIR_NAME)
}

fn view_file_path(library_root: &Path, name: &str) -> PathBuf {
    views_directory(library_root).join(format!("{name}.json"))
}

fn read_view_record_from_text(name: &str, text: &str) -> Result<ViewRecord, Diagnostic> {
    let filters: ViewFilter = serde_json::from_str(text).map_err(|_| {
        Diagnostic::new(
            ErrorCode::DataInvalid,
            &format!("Failed to parse view '{name}' JSON file."),
        )
    })?;
    let hash = compute_view_hash(text);
    Ok(ViewRecord {
        name: name.to_string(),
        version: 1,
        filters,
        hash,
    })
}

pub fn load_view_record(library_root: &Path, name: &str) -> Result<ViewRecord, Diagnostic> {
    validate_view_name(name)?;
    let target_path = view_file_path(library_root, name);
    let text = fs::read_to_string(&target_path).map_err(|_| {
        Diagnostic::new(
            ErrorCode::RefNotFound,
            &format!("Saved view '{name}' was not found."),
        )
    })?;
    read_view_record_from_text(name, &text)
}

fn load_entry_view(entry_path: &Path) -> Option<ViewRecord> {
    let is_json = entry_path.extension().is_some_and(|ext| ext == "json");
    if !is_json || !entry_path.is_file() {
        return None;
    }
    let stem = entry_path.file_stem()?.to_string_lossy().into_owned();
    let text = fs::read_to_string(entry_path).ok()?;
    read_view_record_from_text(&stem, &text).ok()
}

fn scan_views(views_dir: &Path) -> Vec<ViewRecord> {
    let mut records = Vec::new();
    if let Ok(entries) = fs::read_dir(views_dir) {
        for entry in entries.flatten() {
            if let Some(record) = load_entry_view(&entry.path()) {
                records.push(record);
            }
        }
    }
    records.sort_by(|left, right| left.name.cmp(&right.name));
    records
}

pub fn list_views(library_root: &Path) -> Result<Payload, Diagnostic> {
    let views_dir = views_directory(library_root);
    let items = scan_views(&views_dir);
    let total = items.len();
    let text = format!("Found {total} saved view(s).");
    Ok(Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": text,
            "items": items,
            "total": total,
        }),
    })
}

fn build_view_payload(record: &ViewRecord) -> Payload {
    let text = format!("Saved view '{}' (version {})", record.name, record.version);
    Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": text,
            "view": record,
        }),
    }
}

pub fn show_view(library_root: &Path, name: &str) -> Result<Payload, Diagnostic> {
    let record = load_view_record(library_root, name)?;
    Ok(build_view_payload(&record))
}

fn write_view_file(target_path: &Path, content: &str) -> Result<(), Diagnostic> {
    let parent = target_path.parent().ok_or_else(|| {
        Diagnostic::new(ErrorCode::ExecutionFailed, "Invalid view destination path.")
    })?;
    fs::create_dir_all(parent).map_err(|error| {
        Diagnostic::new(
            ErrorCode::ExecutionFailed,
            &format!("Failed to create views directory: {error}"),
        )
    })?;
    fs::write(target_path, content).map_err(|error| {
        Diagnostic::new(
            ErrorCode::ExecutionFailed,
            &format!("Failed to write view file: {error}"),
        )
    })
}

pub fn save_view(
    library_root: &Path,
    name: &str,
    filter_file: &Path,
) -> Result<Payload, Diagnostic> {
    validate_view_name(name)?;
    let content = fs::read_to_string(filter_file).map_err(|error| {
        Diagnostic::new(
            ErrorCode::InputRequired,
            &format!(
                "Could not read filter file '{}': {error}",
                filter_file.display()
            ),
        )
    })?;
    let record = read_view_record_from_text(name, &content)?;
    save_filter(library_root, name, &record.filters)
}

/// Save a typed filter without requiring an intermediate JSON file.
pub fn save_filter(
    library_root: &Path,
    name: &str,
    filters: &ViewFilter,
) -> Result<Payload, Diagnostic> {
    validate_view_name(name)?;
    let content = serde_json::to_string(filters)
        .map_err(|_| Diagnostic::new(ErrorCode::DataInvalid, "Failed to serialize view filter."))?;
    let record = read_view_record_from_text(name, &content)?;
    let target_path = view_file_path(library_root, name);
    let canonical_json = serde_json::to_string_pretty(&record.filters)
        .map_err(|_| Diagnostic::new(ErrorCode::DataInvalid, "Failed to serialize view filter."))?;
    write_view_file(&target_path, &canonical_json)?;
    Ok(build_view_payload(&record))
}

fn ensure_view_removable(
    path: &Path,
    name: &str,
    consent: RemovalConsent,
) -> Result<(), Diagnostic> {
    if !path.exists() {
        return Err(Diagnostic::new(
            ErrorCode::RefNotFound,
            &format!("Saved view '{name}' does not exist."),
        ));
    }
    if consent == RemovalConsent::Unconfirmed {
        return Err(Diagnostic::new(
            ErrorCode::InputRequired,
            &format!("Removing view '{name}' requires confirmation. Pass --yes to confirm."),
        ));
    }
    Ok(())
}

pub fn remove_view(
    library_root: &Path,
    name: &str,
    consent: RemovalConsent,
) -> Result<Payload, Diagnostic> {
    validate_view_name(name)?;
    let target_path = view_file_path(library_root, name);
    ensure_view_removable(&target_path, name, consent)?;
    fs::remove_file(&target_path).map_err(|error| {
        Diagnostic::new(
            ErrorCode::ExecutionFailed,
            &format!("Failed to remove view file: {error}"),
        )
    })?;
    let text = format!("Removed saved view '{name}'.");
    Ok(Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": text,
            "removed": true,
            "name": name,
        }),
    })
}
