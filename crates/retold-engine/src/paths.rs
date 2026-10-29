//! Where Retold keeps its settings and the video library.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::EngineError;

mod preferences;
pub mod workspace;
pub use preferences::update_settings;

/// App folders. Override with `RETOLD_CONFIG_DIR` and `RETOLD_LIBRARY` (tests, portable
/// setups); the library can also be set in `settings.toml`.
#[derive(Debug, Clone)]
pub struct Paths {
    pub config: PathBuf,
    pub library: PathBuf,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    /// Library root; `~/Retold` when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub library: Option<PathBuf>,
    /// Channel picked first in the wizard.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_channel: Option<String>,
    /// Active profile name in `profiles/<name>.toml`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_profile: Option<String>,
    /// Remembered managed workspace; does not grant execution authority.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_workspace: Option<PathBuf>,
}

impl Paths {
    pub fn discover() -> Result<Self, EngineError> {
        let home = directories::BaseDirs::new()
            .map(|b| b.home_dir().to_owned())
            .ok_or_else(|| EngineError::Setup("could not find your home folder".into()))?;
        let config = config_directory()?;
        let settings = read_settings(&config.join("settings.toml"))?;
        if let Some(root) = workspace::selected_root(&settings) {
            return Self::from_workspace(&root);
        }
        let library = match std::env::var_os("RETOLD_LIBRARY") {
            Some(p) => PathBuf::from(p),
            None => settings
                .library
                .map_or_else(|| home.join("Retold"), |p| expand_home(&p, &home)),
        };
        Ok(Self { config, library })
    }

    /// Resolves a validated workspace without changing process environment or global settings.
    pub fn from_workspace(root: &Path) -> Result<Self, EngineError> {
        let manifest = workspace::Manifest::load(root)?;
        let paths = Self {
            config: workspace::inside(root, &manifest.config_path)?,
            library: workspace::inside(root, &manifest.library_path)?,
        };
        let config_inside_library = paths.config.starts_with(&paths.library);
        let library_inside_config = paths.library.starts_with(&paths.config);
        if config_inside_library || library_inside_config {
            return Err(EngineError::Setup(
                "Workspace config and library paths must be separate.".into(),
            ));
        }
        Ok(paths)
    }

    #[must_use]
    pub fn guides(&self) -> PathBuf {
        self.config.join("guides")
    }
    #[must_use]
    pub fn channels(&self) -> PathBuf {
        self.config.join("channels")
    }
    #[must_use]
    pub fn profiles(&self) -> PathBuf {
        self.config.join("profiles")
    }
    #[must_use]
    pub fn providers_file(&self) -> PathBuf {
        self.config.join("providers.toml")
    }
    #[must_use]
    pub fn settings_file(&self) -> PathBuf {
        self.config.join("settings.toml")
    }
    #[must_use]
    pub fn answers_file(&self) -> PathBuf {
        self.config.join("answers.toml")
    }
    #[must_use]
    pub fn schemas(&self) -> PathBuf {
        self.config.join("schemas")
    }
    /// Secrets file (`OPENROUTER_API_KEY=…`), read in addition to the environment.
    #[must_use]
    pub fn env_file(&self) -> PathBuf {
        self.config.join(".env")
    }

    /// Whether the library sits in a folder the system empties (macOS clears `/tmp` on
    /// restart), which would silently lose every run, image and paid narration.
    #[must_use]
    pub fn library_is_temporary(&self) -> bool {
        is_temporary(&self.library)
    }

    pub fn settings(&self) -> Result<Settings, EngineError> {
        read_settings(&self.settings_file())
    }
}

/// Installation preferences live outside individual workspaces.
pub fn config_directory() -> Result<PathBuf, EngineError> {
    std::env::var_os("RETOLD_CONFIG_DIR")
        .map(PathBuf::from)
        .map_or_else(
            || {
                directories::ProjectDirs::from("", "", "retold")
                    .map(|directories| directories.config_dir().to_owned())
                    .ok_or_else(|| EngineError::Setup("could not find the settings folder".into()))
            },
            Ok,
        )
}

fn read_settings(path: &Path) -> Result<Settings, EngineError> {
    const MAX_SETTINGS_BYTES: u64 = 64 * 1024;
    match crate::fsx::read_bounded(path, MAX_SETTINGS_BYTES)? {
        Some(bytes) => std::str::from_utf8(&bytes)
            .ok()
            .and_then(|text| toml::from_str(text).ok())
            .ok_or_else(|| EngineError::Config {
                path: path.to_owned(),
                message: "Settings must contain supported fields with valid values.".into(),
            }),
        None => Ok(Settings::default()),
    }
}

/// `/tmp`, `/private/tmp`, `/var/tmp` and the per-user temp folder (`$TMPDIR`).
#[must_use]
pub fn is_temporary(path: &Path) -> bool {
    let mut roots = vec![
        PathBuf::from("/tmp"),
        PathBuf::from("/private/tmp"),
        PathBuf::from("/var/tmp"),
        PathBuf::from("/private/var/tmp"),
        PathBuf::from("/var/folders"),
        PathBuf::from("/private/var/folders"),
    ];
    roots.push(std::env::temp_dir());
    roots.iter().any(|root| path.starts_with(root))
}

fn expand_home(p: &Path, home: &Path) -> PathBuf {
    p.strip_prefix("~")
        .map_or_else(|_| p.to_owned(), |rest| home.join(rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temporary_folders_are_recognised() {
        assert!(is_temporary(Path::new("/tmp/retold/library")));
        assert!(is_temporary(Path::new("/private/tmp/x")));
        assert!(is_temporary(&std::env::temp_dir().join("lib")));
        assert!(!is_temporary(Path::new("/Users/me/Retold")));
    }
}
