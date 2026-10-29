//! Presentation preferences never store run authority, credentials or worker restart instructions.
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::protocol::{Diagnostic, ErrorCode};
use retold_engine::fsx;

const PREFERENCES_FORMAT_VERSION: u32 = 1;
const MAX_PREFERENCE_BYTES: u64 = 32 * 1024;
const MAX_RECENT_WORKSPACES: usize = 8;
const MIN_ZOOM: u16 = 100;
const MAX_ZOOM: u16 = 200;
const MIN_WIDTH: u32 = 760;
const MIN_HEIGHT: u32 = 640;
const MAX_DIMENSION: u32 = 7680;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preferences {
    pub schema: u32,
    pub appearance: AppearanceSettings,
    pub zoom: u16,
    pub window: Option<WindowSize>,
    pub recent_workspaces: Vec<PathBuf>,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            schema: PREFERENCES_FORMAT_VERSION,
            appearance: AppearanceSettings::default(),
            zoom: MIN_ZOOM,
            window: None,
            recent_workspaces: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppearanceSettings {
    pub mode: Appearance,
    pub high_contrast: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindowSize {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone)]
pub enum Change {
    Appearance(AppearanceSettings),
    Zoom(u16),
    Window(WindowSize),
    Workspace(PathBuf),
}

impl Change {
    #[must_use]
    pub fn same_field(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

fn apply_change(mut preferences: Preferences, change: &Change) -> Preferences {
    match change {
        Change::Appearance(appearance) => preferences.appearance = *appearance,
        Change::Zoom(zoom) => preferences.zoom = *zoom,
        Change::Window(size) => preferences.window = Some(*size),
        Change::Workspace(root) => {
            preferences.recent_workspaces.retain(|saved| saved != root);
            preferences.recent_workspaces.insert(0, root.clone());
            preferences
                .recent_workspaces
                .truncate(MAX_RECENT_WORKSPACES);
        }
    }
    preferences
}
pub fn load() -> Result<Preferences, Diagnostic> {
    load_from(&configuration_directory()?)
}

pub fn update(changes: &[Change]) -> Result<Preferences, Diagnostic> {
    let _owner = crate::account::activity("save desktop preferences")
        .map_err(|error| crate::account::commands::diagnostic(&error))?;
    update_in(&configuration_directory()?, changes)
}

fn configuration_directory() -> Result<PathBuf, Diagnostic> {
    retold_engine::Paths::discover()
        .map(|paths| paths.config)
        .map_err(failed)
}

fn preference_path(directory: &Path) -> PathBuf {
    directory.join("desktop-preferences.json")
}

fn load_from(directory: &Path) -> Result<Preferences, Diagnostic> {
    let path = preference_path(directory);
    let Some(bytes) =
        fsx::read_bounded(&path, MAX_PREFERENCE_BYTES).map_err(|error| at_file(&path, error))?
    else {
        return Ok(Preferences::default());
    };
    let preferences: Preferences =
        serde_json::from_slice(&bytes).map_err(|error| at_file(&path, error))?;
    validate(&preferences)?;
    Ok(preferences)
}

fn at_file(path: &Path, error: impl std::fmt::Display) -> Diagnostic {
    failed(format!("{}: {error}", path.display()))
}

fn update_in(directory: &Path, changes: &[Change]) -> Result<Preferences, Diagnostic> {
    retold_engine::recovery::ensure_active(directory).map_err(failed)?;
    let _writer = writer(directory)?;
    let mut preferences = load_from(directory)?;
    for change in changes {
        preferences = apply_change(preferences, change);
    }
    validate(&preferences)?;
    fsx::write_json_atomic(&preference_path(directory), &preferences).map_err(failed)?;
    Ok(preferences)
}

fn validate(preferences: &Preferences) -> Result<(), Diagnostic> {
    let supported = preferences.schema == PREFERENCES_FORMAT_VERSION;
    let zoom = (MIN_ZOOM..=MAX_ZOOM).contains(&preferences.zoom);
    let recent = preferences.recent_workspaces.len() <= MAX_RECENT_WORKSPACES;
    let absolute = preferences
        .recent_workspaces
        .iter()
        .all(|path| path.is_absolute());
    let format_valid = supported && zoom;
    let history_valid = recent && absolute;
    if !format_valid || !history_valid {
        return Err(invalid(
            "Unsupported or invalid desktop preferences. The original file is preserved.",
        ));
    }
    if let Some(size) = preferences.window {
        validate_size(size)?;
    }
    Ok(())
}

fn validate_size(size: WindowSize) -> Result<(), Diagnostic> {
    let width = (MIN_WIDTH..=MAX_DIMENSION).contains(&size.width);
    let height = (MIN_HEIGHT..=MAX_DIMENSION).contains(&size.height);
    if !width || !height {
        return Err(invalid(
            "Saved window dimensions are outside supported bounds.",
        ));
    }
    Ok(())
}

struct Writer(std::fs::File);

impl Drop for Writer {
    fn drop(&mut self) {
        self.0.unlock().ok();
    }
}

fn writer(directory: &Path) -> Result<Writer, Diagnostic> {
    std::fs::create_dir_all(directory).map_err(failed)?;
    let path = directory.join(".desktop-preferences.lock");
    if path.is_symlink() {
        return Err(invalid(
            "The desktop preferences lock must be a regular file.",
        ));
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(false);
    let file = options.open(path).map_err(failed)?;
    file.try_lock()
        .map_err(|_| invalid("Another window is saving preferences. Retry after it finishes."))?;
    Ok(Writer(file))
}

fn failed(error: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::new(
        ErrorCode::DataInvalid,
        &format!("Desktop preferences: {error}"),
    )
}

fn invalid(message: &str) -> Diagnostic {
    Diagnostic::new(ErrorCode::InputRequired, message)
}

#[cfg(test)]
mod tests {
    use super::{
        Appearance, AppearanceSettings, Change, MAX_RECENT_WORKSPACES, load_from, preference_path,
        update_in, writer,
    };
    const LARGE_TEXT: u16 = 125;

    fn light_mode() -> AppearanceSettings {
        AppearanceSettings {
            mode: Appearance::Light,
            high_contrast: false,
        }
    }

    #[test]
    fn preferences_reject_embedded_execution_authority() {
        let root = tempfile::tempdir().unwrap();
        let path = preference_path(root.path());
        let mut document = serde_json::to_value(super::Preferences::default()).unwrap();
        document["run_allowance"] = serde_json::json!({"usd_micros": 100, "requests": 1});
        let encoded = serde_json::to_vec(&document).unwrap();
        std::fs::write(&path, &encoded).unwrap();
        assert!(load_from(root.path()).is_err());
        assert_eq!(std::fs::read(path).unwrap(), encoded);
    }

    #[test]
    fn reading_absent_preferences_creates_nothing() {
        let root = tempfile::tempdir().unwrap();
        let absent = root.path().join("not-created");
        assert_eq!(
            load_from(&absent).unwrap().appearance.mode,
            Appearance::System
        );
        assert!(!absent.exists());
    }

    #[test]
    fn updates_merge_fields_and_keep_recent_workspaces_bounded() {
        let root = tempfile::tempdir().unwrap();
        update_in(root.path(), &[Change::Appearance(light_mode())]).unwrap();
        for index in 0..=MAX_RECENT_WORKSPACES {
            update_in(
                root.path(),
                &[Change::Workspace(
                    root.path().join(format!("story-{index}")),
                )],
            )
            .unwrap();
        }
        let saved = update_in(root.path(), &[Change::Zoom(LARGE_TEXT)]).unwrap();
        assert_eq!(saved.appearance.mode, Appearance::Light);
        assert_eq!(saved.zoom, LARGE_TEXT);
        assert_eq!(saved.recent_workspaces.len(), MAX_RECENT_WORKSPACES);
        assert_eq!(
            saved.recent_workspaces[0],
            root.path().join(format!("story-{MAX_RECENT_WORKSPACES}"))
        );
    }

    #[test]
    fn invalid_existing_or_requested_preferences_preserve_the_original_bytes() {
        let root = tempfile::tempdir().unwrap();
        let path = preference_path(root.path());
        std::fs::write(&path, b"broken preferences").unwrap();
        assert!(update_in(root.path(), &[Change::Appearance(light_mode())]).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"broken preferences");
        std::fs::remove_file(&path).unwrap();
        update_in(root.path(), &[Change::Appearance(light_mode())]).unwrap();
        let original = std::fs::read(&path).unwrap();
        assert!(update_in(root.path(), &[Change::Zoom(0)]).is_err());
        assert_eq!(std::fs::read(path).unwrap(), original);
    }

    #[test]
    fn a_live_writer_blocks_other_windows_and_releases_its_lock() {
        let root = tempfile::tempdir().unwrap();
        let owner = writer(root.path()).unwrap();
        assert!(update_in(root.path(), &[Change::Appearance(light_mode())]).is_err());
        drop(owner);
        assert!(update_in(root.path(), &[Change::Appearance(light_mode())]).is_ok());
    }
}
