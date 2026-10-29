//! Resolution of workspace paths and remembered defaults.

use std::fs;
use std::path::{Path, PathBuf};

use super::types::{
    DEFAULT_CONFIG_DIR_NAME, DEFAULT_LIBRARY_DIR_NAME, WORKSPACE_TOML_FILENAME, WorkspacePaths,
    WorkspaceToml,
};
use crate::protocol::{Diagnostic, ErrorCode};

const RETOLD_APP_QUALIFIER: &str = "";
const RETOLD_APP_ORG: &str = "";
const RETOLD_APP_NAME: &str = "retold";

#[must_use]
pub fn global_config_dir() -> Option<PathBuf> {
    if let Some(env_dir) = std::env::var_os("RETOLD_CONFIG_DIR") {
        return Some(PathBuf::from(env_dir));
    }
    directories::ProjectDirs::from(RETOLD_APP_QUALIFIER, RETOLD_APP_ORG, RETOLD_APP_NAME)
        .map(|dirs| dirs.config_dir().to_path_buf())
}

#[must_use]
pub fn global_settings_file() -> Option<PathBuf> {
    global_config_dir().map(|dir| dir.join("settings.toml"))
}

#[must_use]
pub fn read_remembered_workspace() -> Option<PathBuf> {
    let settings_path = global_settings_file()?;
    let text = fs::read_to_string(settings_path).ok()?;
    let table: toml::Table = toml::from_str(&text).ok()?;
    let value = table.get("selected_workspace")?.as_str()?;
    Some(PathBuf::from(value))
}

fn candidate_from_env() -> Option<PathBuf> {
    std::env::var_os("RETOLD_WORKSPACE").map(PathBuf::from)
}

fn candidate_from_cwd() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    let marker = cwd.join(WORKSPACE_TOML_FILENAME);
    marker.is_file().then_some(cwd)
}

fn candidate_from_legacy() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|dirs| dirs.home_dir().join("Retold"))
}

#[must_use]
pub fn resolve_workspace_root(explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = explicit {
        return Some(path.to_path_buf());
    }
    candidate_from_env()
        .or_else(candidate_from_cwd)
        .or_else(read_remembered_workspace)
        .or_else(candidate_from_legacy)
}

#[must_use]
pub fn read_workspace_toml(root: &Path) -> Option<WorkspaceToml> {
    WorkspaceToml::load(root).ok()
}

fn project_dirs() -> Result<directories::ProjectDirs, Diagnostic> {
    directories::ProjectDirs::from(RETOLD_APP_QUALIFIER, RETOLD_APP_ORG, RETOLD_APP_NAME)
        .ok_or_else(|| {
            Diagnostic::new(
                ErrorCode::ExecutionFailed,
                "Could not determine system application directories.",
            )
        })
}

fn resolve_state_path(dirs: &directories::ProjectDirs, id: &str) -> PathBuf {
    dirs.data_local_dir().join("workspaces").join(id)
}

fn resolve_cache_path(dirs: &directories::ProjectDirs, id: &str) -> PathBuf {
    dirs.cache_dir().join("workspaces").join(id)
}

fn canonical_or_raw(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| {
        std::env::current_dir().map_or_else(|_| path.to_path_buf(), |current| current.join(path))
    })
}

fn content_paths(root: &Path) -> Result<(PathBuf, PathBuf), Diagnostic> {
    let has_manifest = root.join(WORKSPACE_TOML_FILENAME).exists();
    if has_manifest {
        managed_content_paths(root)
    } else {
        Ok(default_content_paths(root))
    }
}

fn managed_content_paths(root: &Path) -> Result<(PathBuf, PathBuf), Diagnostic> {
    retold_engine::Paths::from_workspace(root)
        .map(|paths| (paths.config, paths.library))
        .map_err(|error| Diagnostic::new(ErrorCode::DataInvalid, &error.to_string()))
}

fn default_content_paths(root: &Path) -> (PathBuf, PathBuf) {
    (
        root.join(DEFAULT_CONFIG_DIR_NAME),
        root.join(DEFAULT_LIBRARY_DIR_NAME),
    )
}

pub fn derive_workspace_paths(root: &Path, id: &str) -> Result<WorkspacePaths, Diagnostic> {
    let dirs = project_dirs()?;
    let canonical = canonical_or_raw(root);
    let (config, library) = content_paths(&canonical)?;
    let state = resolve_state_path(&dirs, id);
    let cache = resolve_cache_path(&dirs, id);

    Ok(WorkspacePaths {
        root: canonical,
        config,
        library,
        state,
        cache,
    })
}
