//! Core types and structures for workspaces.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const DEFAULT_CONFIG_DIR_NAME: &str = "config";
pub const DEFAULT_LIBRARY_DIR_NAME: &str = "library";
pub const WORKSPACE_TOML_FILENAME: &str = "workspace.toml";

pub use retold_engine::paths::workspace::Manifest as WorkspaceToml;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspacePaths {
    pub root: PathBuf,
    pub config: PathBuf,
    pub library: PathBuf,
    pub state: PathBuf,
    pub cache: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceDescriptor {
    pub id: String,
    pub name: String,
    pub paths: WorkspacePaths,
    pub registered: bool,
    pub active: bool,
}
