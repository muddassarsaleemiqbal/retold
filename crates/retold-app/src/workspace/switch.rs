//! Workspace switching and remembered default workspace persistence.

use std::path::{Path, PathBuf};

use super::resolution::global_config_dir;
use super::types::WorkspaceToml;
use crate::protocol::{Diagnostic, ErrorCode, Outcome, Payload};

fn canonical_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

pub fn save_remembered_workspace(workspace_root: &Path) -> Result<(), Diagnostic> {
    let config = global_config_dir().ok_or_else(|| {
        Diagnostic::new(
            ErrorCode::ExecutionFailed,
            "Could not determine the installation settings directory.",
        )
    })?;
    let canonical = canonical_path(workspace_root);
    retold_engine::paths::update_settings(&config, |settings| {
        settings.selected_workspace = Some(canonical);
    }).map_err(|_| Diagnostic::new(ErrorCode::DataInvalid,
        "Settings could not be updated. Repair invalid settings or directory permissions before switching."))
}

fn build_switch_payload(name: &str, canonical: &Path, manifest: Option<&WorkspaceToml>) -> Payload {
    let text = format!(
        "Switched default workspace to '{name}' at '{}'.",
        canonical.display()
    );

    Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": text,
            "workspace": {
                "name": name,
                "root": canonical,
                "manifest_present": manifest.is_some(),
            }
        }),
    }
}

#[must_use]
pub fn switch_workspace(target: &Path) -> Payload {
    if !target.is_dir() {
        return Diagnostic::new(
            ErrorCode::InputRequired,
            "Target path is not an existing directory.",
        )
        .into_payload();
    }

    let manifest = match WorkspaceToml::load(target) {
        Ok(manifest) => Some(manifest),
        Err(error) => {
            return Diagnostic::new(ErrorCode::DataInvalid, &error.to_string()).into_payload();
        }
    };
    if let Err(error) = retold_engine::Paths::from_workspace(target) {
        return Diagnostic::new(ErrorCode::DataInvalid, &error.to_string()).into_payload();
    }
    let name = manifest
        .as_ref()
        .map_or("workspace", |manifest_item| manifest_item.name.as_str());

    if let Err(diagnostic) = save_remembered_workspace(target) {
        return diagnostic.into_payload();
    }

    let canonical = canonical_path(target);
    build_switch_payload(name, &canonical, manifest.as_ref())
}
