//! Configuration profiles: reproducible named preference configurations.
//! Profiles reference IDs, channels, and settings only; they never store secrets or authority.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::protocol::{Diagnostic, ErrorCode, Outcome, Payload};

const FORBIDDEN_SECRET_KEYS: &[&str] = &[
    "secret",
    "token",
    "password",
    "api_key",
    "apikey",
    "credential",
    "private_key",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileRecord {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_channel: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_preset: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<bool>,
}

impl ProfileRecord {
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            description: None,
            default_channel: None,
            default_preset: None,
            active: None,
        }
    }

    fn to_payload(&self) -> Payload {
        let is_active = self.active.unwrap_or(false);
        let active_label = if is_active { " (active)" } else { "" };
        let text = format!("Profile '{}{active_label}'", self.name);
        Payload::Result {
            outcome: Outcome::Completed,
            contents: serde_json::json!({
                "text": text,
                "profile": self,
            }),
        }
    }
}

#[must_use]
pub fn default_config_root() -> Option<PathBuf> {
    if let Some(env_dir) = std::env::var_os("RETOLD_CONFIG_DIR") {
        return Some(PathBuf::from(env_dir));
    }
    retold_engine::Paths::discover()
        .ok()
        .map(|paths| paths.config)
}

fn resolve_root(config_root: Option<&Path>) -> Result<PathBuf, Diagnostic> {
    if let Some(path) = config_root {
        return Ok(path.to_path_buf());
    }
    default_config_root().ok_or_else(|| {
        Diagnostic::new(
            ErrorCode::InputRequired,
            "Could not determine configuration directory. Specify --root or set RETOLD_CONFIG_DIR.",
        )
    })
}

fn validate_profile_name(name: &str) -> Result<(), Diagnostic> {
    let valid = !name.is_empty()
        && name.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        });
    if valid {
        Ok(())
    } else {
        Err(Diagnostic::new(
            ErrorCode::InputRequired,
            "Profile name must contain only alphanumeric characters, dashes, and underscores.",
        ))
    }
}

fn ensure_no_secrets(content: &str) -> Result<(), Diagnostic> {
    let lower = content.to_ascii_lowercase();
    for forbidden in FORBIDDEN_SECRET_KEYS {
        if lower.contains(forbidden) {
            return Err(Diagnostic::new(
                ErrorCode::InputRequired,
                "Profiles must never contain secrets, tokens, or credentials.",
            ));
        }
    }
    Ok(())
}

fn read_selected_profile(config_root: &Path) -> Option<String> {
    let settings_file = config_root.join("settings.toml");
    let text = fs::read_to_string(settings_file).ok()?;
    let settings: retold_engine::Settings = toml::from_str(&text).ok()?;
    settings.selected_profile
}

fn write_selected_profile(config_root: &Path, name: &str) -> Result<(), Diagnostic> {
    retold_engine::paths::update_settings(config_root, |settings| {
        settings.selected_profile = Some(name.into());
    }).map_err(|_| Diagnostic::new(ErrorCode::DataInvalid,
        "Settings could not be updated. Repair invalid settings or directory permissions first."))
}

fn ensure_profiles_dir(config_root: &Path) -> Result<PathBuf, Diagnostic> {
    let profiles_dir = config_root.join("profiles");
    fs::create_dir_all(&profiles_dir).map_err(|err| {
        Diagnostic::new(
            ErrorCode::ExecutionFailed,
            &format!("Cannot create profiles directory: {err}"),
        )
    })?;
    Ok(profiles_dir)
}

fn load_profile_record(path: &Path) -> Option<ProfileRecord> {
    let stem = path.file_stem()?.to_string_lossy();
    let text = fs::read_to_string(path).ok()?;
    let mut record: ProfileRecord = toml::from_str(&text).ok()?;
    record.name = stem.into_owned();
    Some(record)
}

fn load_profile_with_active(path: &Path, active_profile: Option<&str>) -> Option<ProfileRecord> {
    let mut record = load_profile_record(path)?;
    record.active = Some(active_profile == Some(&record.name));
    Some(record)
}

fn scan_profiles(profiles_dir: &Path, active_profile: Option<&str>) -> Vec<ProfileRecord> {
    let mut records = Vec::new();
    if let Ok(entries) = fs::read_dir(profiles_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let is_toml = path.extension().is_some_and(|ext| ext == "toml");
            if path.is_file() && is_toml {
                records.extend(load_profile_with_active(&path, active_profile));
            }
        }
    }
    records.sort_by(|left, right| left.name.cmp(&right.name));
    records
}

pub fn list_profiles(config_root: Option<&Path>) -> Result<Payload, Diagnostic> {
    let root = resolve_root(config_root)?;
    let profiles_dir = root.join("profiles");
    let active = read_selected_profile(&root);
    let items = scan_profiles(&profiles_dir, active.as_deref());
    let total = items.len();
    let text = format!("Found {total} configuration profile(s).");
    Ok(Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": text,
            "items": items,
            "total": total,
            "active_profile": active,
        }),
    })
}

pub fn show_profile(config_root: Option<&Path>, name: &str) -> Result<Payload, Diagnostic> {
    validate_profile_name(name)?;
    let root = resolve_root(config_root)?;
    let profile_file = root.join("profiles").join(format!("{name}.toml"));
    if !profile_file.exists() {
        return Err(Diagnostic::new(
            ErrorCode::RefNotFound,
            &format!("Configuration profile '{name}' not found."),
        ));
    }
    let active = read_selected_profile(&root);
    let record = load_profile_with_active(&profile_file, active.as_deref()).ok_or_else(|| {
        Diagnostic::new(
            ErrorCode::DataInvalid,
            &format!("Failed to parse profile '{name}'."),
        )
    })?;
    Ok(record.to_payload())
}

fn default_profile_content(name: &str) -> String {
    format!("name = \"{name}\"\ndescription = \"Profile for {name}\"\n")
}

fn read_profile_source(source_file: Option<&Path>, name: &str) -> Result<String, Diagnostic> {
    match source_file {
        Some(file) => {
            let content = fs::read_to_string(file).map_err(|err| {
                Diagnostic::new(
                    ErrorCode::InputRequired,
                    &format!("Cannot read profile source file: {err}"),
                )
            })?;
            ensure_no_secrets(&content)?;
            let _: ProfileRecord = toml::from_str(&content).map_err(|err| {
                Diagnostic::new(
                    ErrorCode::InputRequired,
                    &format!("Invalid profile TOML: {err}"),
                )
            })?;
            Ok(content)
        }
        None => Ok(default_profile_content(name)),
    }
}

pub fn create_profile(
    config_root: Option<&Path>,
    name: &str,
    source_file: Option<&Path>,
) -> Result<Payload, Diagnostic> {
    validate_profile_name(name)?;
    let root = resolve_root(config_root)?;
    let profiles_dir = ensure_profiles_dir(&root)?;
    let target_file = profiles_dir.join(format!("{name}.toml"));
    if target_file.exists() {
        return Err(Diagnostic::new(
            ErrorCode::ExecutionFailed,
            &format!("Profile '{name}' already exists."),
        ));
    }
    let content = read_profile_source(source_file, name)?;
    fs::write(&target_file, &content).map_err(|err| {
        Diagnostic::new(
            ErrorCode::ExecutionFailed,
            &format!("Failed to write profile: {err}"),
        )
    })?;
    let record = load_profile_record(&target_file).unwrap_or_else(|| ProfileRecord::new(name));
    let text = format!("Configuration profile '{name}' created successfully.");
    Ok(Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": text,
            "profile": record,
        }),
    })
}

pub fn use_profile(config_root: Option<&Path>, name: &str) -> Result<Payload, Diagnostic> {
    validate_profile_name(name)?;
    let root = resolve_root(config_root)?;
    let profile_file = root.join("profiles").join(format!("{name}.toml"));
    if !profile_file.exists() {
        return Err(Diagnostic::new(
            ErrorCode::RefNotFound,
            &format!("Configuration profile '{name}' not found."),
        ));
    }
    write_selected_profile(&root, name)?;
    let text = format!("Configuration profile '{name}' is now active.");
    Ok(Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": text,
            "active_profile": name,
        }),
    })
}
