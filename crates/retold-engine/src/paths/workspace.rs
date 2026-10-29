//! Shared workspace format and confined path resolution for every application entry point.
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::Settings;
use crate::{EngineError, Paths, fsx};

pub const SCHEMA: u64 = 1;
pub const FILENAME: &str = "workspace.toml";
pub const CONFIG: &str = "config";
pub const LIBRARY: &str = "library";
const MAX_MANIFEST_BYTES: u64 = 64 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u64,
    pub id: String,
    pub name: String,
    pub library_path: String,
    #[serde(default = "default_config")]
    pub config_path: String,
}

fn default_config() -> String {
    CONFIG.into()
}

impl Manifest {
    #[must_use]
    pub fn new(id: String, name: String) -> Self {
        Self {
            schema_version: SCHEMA,
            id,
            name,
            library_path: LIBRARY.into(),
            config_path: CONFIG.into(),
        }
    }

    pub fn load(root: &Path) -> Result<Self, EngineError> {
        let path = root.join(FILENAME);
        let bytes = fsx::read_bounded(&path, MAX_MANIFEST_BYTES)?
            .ok_or_else(|| invalid("Workspace manifest is missing."))?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| invalid("Workspace manifest must be UTF-8."))?;
        let manifest: Self = toml::from_str(text).map_err(|_| EngineError::Config {
            path,
            message: "Invalid workspace manifest. Check its schema and required fields.".into(),
        })?;
        manifest.validate()?;
        Ok(manifest)
    }

    fn validate(&self) -> Result<(), EngineError> {
        let safe_id = !self.id.is_empty()
            && self.id.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
            });
        let named = !self.name.trim().is_empty();
        if self.schema_version != SCHEMA || !safe_id || !named {
            return Err(invalid("Workspace schema, identity or name is invalid."));
        }
        Ok(())
    }
}

fn invalid(message: &str) -> EngineError {
    EngineError::Setup(message.into())
}

/// Checks every existing ancestor, so a symlink cannot redirect a not-yet-created child.
pub fn inside(root: &Path, relative: &str) -> Result<PathBuf, EngineError> {
    let relative = relative_path(relative)?;
    let root = root
        .canonicalize()
        .map_err(|error| EngineError::io(root, error))?;
    let target = root.join(relative);
    let existing = target
        .ancestors()
        .take_while(|path| *path != root)
        .find(|ancestor| std::fs::symlink_metadata(ancestor).is_ok());
    existing.map_or_else(
        || Ok(target.clone()),
        |ancestor| confined_target(&target, ancestor, &root),
    )
}

fn relative_path(relative: &str) -> Result<&Path, EngineError> {
    let relative = Path::new(relative);
    let valid = !relative.as_os_str().is_empty()
        && relative
            .components()
            .all(|part| matches!(part, Component::Normal(_)));
    if !valid {
        return Err(invalid(
            "Workspace paths must be nonempty relative paths without traversal.",
        ));
    }
    Ok(relative)
}

fn confined_target(target: &Path, ancestor: &Path, root: &Path) -> Result<PathBuf, EngineError> {
    let resolved = ancestor
        .canonicalize()
        .map_err(|error| EngineError::io(ancestor, error))?;
    let confined = resolved.starts_with(root);
    if !confined {
        return Err(invalid("Workspace paths must stay within their workspace."));
    }
    let remaining = target
        .strip_prefix(ancestor)
        .map_err(|_| invalid("Workspace path could not be resolved."))?;
    let no_suffix = remaining.as_os_str().is_empty();
    Ok(if no_suffix {
        resolved
    } else {
        resolved.join(remaining)
    })
}

#[must_use]
pub fn selected_root(settings: &Settings) -> Option<PathBuf> {
    if let Some(root) = std::env::var_os("RETOLD_WORKSPACE") {
        return Some(PathBuf::from(root));
    }
    if std::env::var_os("RETOLD_LIBRARY").is_some() {
        return None;
    }
    let current = std::env::current_dir().ok();
    current
        .as_deref()
        .and_then(|path| {
            path.ancestors()
                .find(|parent| parent.join(FILENAME).is_file())
        })
        .map(Path::to_path_buf)
        .or_else(|| settings.selected_workspace.clone())
}

/// An explicit managed root or its library resolves the same config and library pair.
pub fn for_library(root: &Path) -> Result<Paths, EngineError> {
    let absolute = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    for parent in absolute.ancestors() {
        let has_manifest = parent.join(FILENAME).exists();
        if has_manifest {
            let paths = Paths::from_workspace(parent)?;
            if parent == absolute || paths.library == absolute {
                return Ok(paths);
            }
            break;
        }
    }
    Ok(Paths {
        config: super::config_directory()?,
        library: absolute,
    })
}
