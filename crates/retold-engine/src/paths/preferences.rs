//! Serialized preference updates preserve other settings and never replace invalid input.
use std::fs::{File, OpenOptions};
use std::path::Path;

use super::{Settings, read_settings};
use crate::{EngineError, fsx};

const SETTINGS_FILENAME: &str = "settings.toml";
const LOCK: &str = ".settings.lock";

pub fn update_settings(
    config: &Path,
    update: impl FnOnce(&mut Settings),
) -> Result<(), EngineError> {
    let _owner = own_settings(config)?;
    let path = config.join(SETTINGS_FILENAME);
    let mut settings = read_settings(&path)?;
    update(&mut settings);
    let encoded =
        toml::to_string_pretty(&settings).map_err(|error| EngineError::Setup(error.to_string()))?;
    fsx::write_atomic(&path, encoded.as_bytes())
}

fn own_settings(config: &Path) -> Result<File, EngineError> {
    std::fs::create_dir_all(config).map_err(|error| EngineError::io(config, error))?;
    let path = config.join(LOCK);
    let mut options = OpenOptions::new();
    options.read(true).write(true);
    options.create(true).truncate(false);
    let owner = options
        .open(&path)
        .map_err(|error| EngineError::io(&path, error))?;
    owner
        .lock()
        .map_err(|error| EngineError::io(&path, error))?;
    Ok(owner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concurrent_updates_preserve_both_selections() {
        let directory = tempfile::tempdir().unwrap();
        let config = directory.path();
        std::thread::scope(|scope| {
            let profile = scope.spawn(|| {
                update_settings(config, |settings| {
                    settings.selected_profile = Some("studio".into());
                })
            });
            let workspace = scope.spawn(|| {
                update_settings(config, |settings| {
                    settings.selected_workspace = Some(config.join("production"));
                })
            });
            profile.join().unwrap().unwrap();
            workspace.join().unwrap().unwrap();
        });
        let saved = read_settings(&config.join(SETTINGS_FILENAME)).unwrap();
        assert_eq!(saved.selected_profile.as_deref(), Some("studio"));
        assert_eq!(saved.selected_workspace, Some(config.join("production")));
    }
}
