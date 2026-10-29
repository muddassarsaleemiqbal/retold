//! Export presets: reusable rendering specifications for local posting and delivery packages.

use std::fs;
use std::path::{Path, PathBuf};

use blake3::Hasher;
use serde::{Deserialize, Serialize};

use crate::protocol::{Diagnostic, ErrorCode, Outcome, Payload};

const HASH_LENGTH: usize = 16;
pub const VALID_ASPECTS: &[&str] = &["9:16", "16:9", "1:1"];
pub const VALID_CONTAINERS: &[&str] = &["mp4", "mov", "mkv"];
pub const VALID_VIDEO_CODECS: &[&str] = &["h264", "h265", "av1", "prores"];
pub const VALID_AUDIO_CODECS: &[&str] = &["aac", "pcm", "mp3", "opus"];
pub const VALID_CAPTION_MODES: &[&str] = &["burned", "subtitles", "none"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemovalConsent {
    Confirmed,
    Unconfirmed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetFile {
    pub aspects: Vec<String>,
    pub container: String,
    pub video_codec: String,
    pub audio_codec: String,
    pub caption_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetRecord {
    pub name: String,
    pub version: u32,
    pub aspects: Vec<String>,
    pub container: String,
    pub video_codec: String,
    pub audio_codec: String,
    pub caption_mode: String,
    pub hash: String,
}

impl PresetFile {
    fn to_record(&self, name: &str, hash: String) -> PresetRecord {
        PresetRecord {
            name: name.to_string(),
            version: 1,
            aspects: self.aspects.clone(),
            container: self.container.clone(),
            video_codec: self.video_codec.clone(),
            audio_codec: self.audio_codec.clone(),
            caption_mode: self.caption_mode.clone(),
            hash,
        }
    }
}

fn validate_aspects(aspects: &[String]) -> Result<(), Diagnostic> {
    if aspects.is_empty() {
        return Err(Diagnostic::new(
            ErrorCode::InputRequired,
            "Preset must specify at least one target aspect ratio.",
        ));
    }
    for aspect in aspects {
        let is_valid = VALID_ASPECTS.contains(&aspect.as_str());
        if !is_valid {
            return Err(Diagnostic::new(
                ErrorCode::InputRequired,
                &format!("Unsupported aspect '{aspect}'. Supported: 9:16, 16:9, 1:1."),
            ));
        }
    }
    Ok(())
}

fn validate_container(container: &str) -> Result<(), Diagnostic> {
    let is_valid = VALID_CONTAINERS.contains(&container);
    if is_valid {
        Ok(())
    } else {
        Err(Diagnostic::new(
            ErrorCode::InputRequired,
            &format!("Unsupported container '{container}'."),
        ))
    }
}

fn validate_video_codec(codec: &str) -> Result<(), Diagnostic> {
    let is_valid = VALID_VIDEO_CODECS.contains(&codec);
    if is_valid {
        Ok(())
    } else {
        Err(Diagnostic::new(
            ErrorCode::InputRequired,
            &format!("Unsupported video_codec '{codec}'."),
        ))
    }
}

fn validate_audio_codec(codec: &str) -> Result<(), Diagnostic> {
    let is_valid = VALID_AUDIO_CODECS.contains(&codec);
    if is_valid {
        Ok(())
    } else {
        Err(Diagnostic::new(
            ErrorCode::InputRequired,
            &format!("Unsupported audio_codec '{codec}'."),
        ))
    }
}

fn validate_caption_mode(mode: &str) -> Result<(), Diagnostic> {
    let is_valid = VALID_CAPTION_MODES.contains(&mode);
    if is_valid {
        Ok(())
    } else {
        Err(Diagnostic::new(
            ErrorCode::InputRequired,
            &format!("Unsupported caption_mode '{mode}'."),
        ))
    }
}

fn validate_codecs(file: &PresetFile) -> Result<(), Diagnostic> {
    validate_container(&file.container)?;
    validate_video_codec(&file.video_codec)?;
    validate_audio_codec(&file.audio_codec)?;
    validate_caption_mode(&file.caption_mode)
}

fn validate_preset(file: &PresetFile) -> Result<(), Diagnostic> {
    validate_aspects(&file.aspects)?;
    validate_codecs(file)
}

fn compute_preset_hash(content: &str) -> String {
    let mut hasher = Hasher::new();
    hasher.update(content.as_bytes());
    let hex = hasher.finalize().to_hex();
    hex[..HASH_LENGTH].to_string()
}

fn load_preset_file(path: &Path) -> Option<PresetRecord> {
    let stem = path.file_stem()?.to_string_lossy().into_owned();
    let text = fs::read_to_string(path).ok()?;
    let file: PresetFile = serde_json::from_str(&text).ok()?;
    let hash = compute_preset_hash(&text);
    Some(file.to_record(&stem, hash))
}

fn load_entry_preset(entry_path: &Path) -> Option<PresetRecord> {
    let is_json = entry_path.extension().is_some_and(|ext| ext == "json");
    let is_file = entry_path.is_file();
    if !is_file || !is_json {
        return None;
    }
    load_preset_file(entry_path)
}

fn scan_presets(presets_dir: &Path) -> Vec<PresetRecord> {
    let mut records = Vec::new();
    if let Ok(entries) = fs::read_dir(presets_dir) {
        for entry in entries.flatten() {
            if let Some(record) = load_entry_preset(&entry.path()) {
                records.push(record);
            }
        }
    }
    records.sort_by(|left, right| left.name.cmp(&right.name));
    records
}

pub fn list_presets(library_root: &Path) -> Result<Payload, Diagnostic> {
    let presets_dir = library_root.join("presets");
    let items = scan_presets(&presets_dir);
    let total = items.len();
    let text = format!("Found {total} export preset(s).");
    Ok(Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": text,
            "items": items,
            "total": total,
        }),
    })
}

fn build_preset_payload(record: &PresetRecord) -> Payload {
    let text = format!(
        "Preset '{}': {} [{}], video: {}, audio: {}, captions: {}",
        record.name,
        record.aspects.join(", "),
        record.container,
        record.video_codec,
        record.audio_codec,
        record.caption_mode,
    );
    Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": text,
            "preset": record,
        }),
    }
}

pub fn find_preset_record(library_root: &Path, name: &str) -> Result<PresetRecord, Diagnostic> {
    let context = crate::workspace::context::Context::resolve(Some(library_root))?;
    let file_name = format!("{name}.json");
    let preset_path = context.config.join("presets").join(file_name);
    if !preset_path.exists() {
        return Err(Diagnostic::new(
            ErrorCode::RefNotFound,
            &format!("Export preset '{name}' not found."),
        ));
    }
    load_preset_file(&preset_path).ok_or_else(|| {
        Diagnostic::new(
            ErrorCode::DataInvalid,
            &format!("Failed to parse export preset '{name}'."),
        )
    })
}

pub fn show_preset(library_root: &Path, name: &str) -> Result<Payload, Diagnostic> {
    let record = find_preset_record(library_root, name)?;
    Ok(build_preset_payload(&record))
}

fn write_preset_file(dest_path: &Path, file: &PresetFile) -> Result<String, Diagnostic> {
    let pretty = serde_json::to_string_pretty(file)
        .map_err(|err| Diagnostic::new(ErrorCode::ExecutionFailed, &err.to_string()))?;
    fs::write(dest_path, &pretty)
        .map_err(|err| Diagnostic::new(ErrorCode::ExecutionFailed, &err.to_string()))?;
    Ok(compute_preset_hash(&pretty))
}

fn load_source_preset(source_file: &Path) -> Result<PresetFile, Diagnostic> {
    let text = fs::read_to_string(source_file).map_err(|err| {
        Diagnostic::new(
            ErrorCode::InputRequired,
            &format!("Cannot read preset source file: {err}"),
        )
    })?;
    serde_json::from_str(&text).map_err(|err| {
        Diagnostic::new(
            ErrorCode::InputRequired,
            &format!("Invalid preset JSON: {err}"),
        )
    })
}

fn ensure_presets_dir(library_root: &Path) -> Result<PathBuf, Diagnostic> {
    let presets_dir = library_root.join("presets");
    fs::create_dir_all(&presets_dir).map_err(|err| {
        Diagnostic::new(
            ErrorCode::ExecutionFailed,
            &format!("Cannot create presets directory: {err}"),
        )
    })?;
    Ok(presets_dir)
}

fn persist_preset(
    library_root: &Path,
    name: &str,
    file: &PresetFile,
) -> Result<PresetRecord, Diagnostic> {
    let presets_dir = ensure_presets_dir(library_root)?;
    let file_name = format!("{name}.json");
    let dest_path = presets_dir.join(file_name);
    let hash = write_preset_file(&dest_path, file)?;
    Ok(file.to_record(name, hash))
}

pub fn create_preset(
    library_root: &Path,
    name: &str,
    source_file: &Path,
) -> Result<Payload, Diagnostic> {
    let file = load_source_preset(source_file)?;
    create_preset_value(library_root, name, &file)
}

/// Register a typed preset through the same validation and persistence as file imports.
pub fn create_preset_value(
    library_root: &Path,
    name: &str,
    file: &PresetFile,
) -> Result<Payload, Diagnostic> {
    let plain = |letter: char| letter.is_ascii_alphanumeric() || matches!(letter, '-' | '_');
    let valid = !name.is_empty() && name.chars().all(plain);
    if !valid {
        return Err(Diagnostic::new(
            ErrorCode::InputRequired,
            "Use letters, digits, dashes or underscores for the preset name.",
        ));
    }
    validate_preset(file)?;
    let record = persist_preset(library_root, name, file)?;
    let text = format!("Export preset '{}' created successfully.", record.name);
    Ok(Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": text,
            "preset": record,
        }),
    })
}

pub fn remove_preset(
    library_root: &Path,
    name: &str,
    consent: RemovalConsent,
) -> Result<Payload, Diagnostic> {
    if consent == RemovalConsent::Unconfirmed {
        return Err(Diagnostic::new(
            ErrorCode::InputRequired,
            &format!("Confirmation required. Pass --yes to remove preset '{name}'."),
        ));
    }
    let file_name = format!("{name}.json");
    let target = library_root.join("presets").join(file_name);
    if !target.exists() {
        return Err(Diagnostic::new(
            ErrorCode::RefNotFound,
            &format!("Export preset '{name}' not found."),
        ));
    }
    fs::remove_file(&target)
        .map_err(|err| Diagnostic::new(ErrorCode::ExecutionFailed, &err.to_string()))?;
    let text = format!("Export preset '{name}' was removed.");
    Ok(Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": text,
            "removed": name,
        }),
    })
}
