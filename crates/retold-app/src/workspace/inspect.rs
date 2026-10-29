//! Workspace inspection logic for `show`.

use std::path::Path;

use super::resolution::{
    derive_workspace_paths, read_remembered_workspace, read_workspace_toml, resolve_workspace_root,
};
use super::types::{WorkspaceDescriptor, WorkspaceToml};
use crate::account::Account;
use crate::protocol::{Diagnostic, ErrorCode, Outcome, Payload};

pub fn inspect_workspace(root: &Path) -> Result<WorkspaceDescriptor, Diagnostic> {
    let manifest = read_workspace_toml(root);
    let id = manifest
        .as_ref()
        .map_or_else(|| "unmanaged".to_string(), |item| item.id.clone());
    let name = manifest
        .as_ref()
        .map_or_else(|| "workspace".to_string(), |item| item.name.clone());

    let paths = derive_workspace_paths(root, &id)?;
    let registered = crate::account::canonical_directory()
        .ok()
        .and_then(|dir| Account::open(&dir).ok())
        .and_then(|acc| acc.workspace_at(root).ok().flatten())
        .is_some();
    let active = read_remembered_workspace()
        .and_then(|rem| rem.canonicalize().ok())
        .is_some_and(|rem_can| {
            root.canonicalize()
                .is_ok_and(|root_can| rem_can == root_can)
        });

    Ok(WorkspaceDescriptor {
        id,
        name,
        paths,
        registered,
        active,
    })
}

#[must_use]
pub fn show_context(context: &super::context::Context) -> Payload {
    let manifest = read_workspace_toml(&context.root);
    match inspect_workspace(&context.root) {
        Ok(mut descriptor) => {
            descriptor.paths.config.clone_from(&context.config);
            descriptor.paths.library.clone_from(&context.library);
            build_show_payload(&descriptor, manifest.as_ref())
        }
        Err(diagnostic) => diagnostic.into_payload(),
    }
}

fn build_workspace_json(
    desc: &WorkspaceDescriptor,
    manifest: Option<&WorkspaceToml>,
) -> serde_json::Value {
    serde_json::json!({
        "id": desc.id,
        "name": desc.name,
        "manifest_present": manifest.is_some(),
        "paths": {
            "root": desc.paths.root,
            "config": desc.paths.config,
            "library": desc.paths.library,
            "state": desc.paths.state,
            "cache": desc.paths.cache,
        },
        "registered": desc.registered,
        "active": desc.active,
    })
}

fn build_show_payload(desc: &WorkspaceDescriptor, manifest: Option<&WorkspaceToml>) -> Payload {
    let reg_label = if desc.registered {
        "registered"
    } else {
        "unregistered"
    };
    let active_label = if desc.active { ", active default" } else { "" };
    let text = format!(
        "Workspace '{}' [{}] ({reg_label}{active_label}) at '{}'.",
        desc.name,
        desc.id,
        desc.paths.root.display()
    );

    Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": text,
            "workspace": build_workspace_json(desc, manifest),
        }),
    }
}

#[must_use]
pub fn show_workspace(explicit_path: Option<&Path>) -> Payload {
    let Some(resolved_root) = resolve_workspace_root(explicit_path) else {
        return Diagnostic::new(
            ErrorCode::InputRequired,
            "Could not resolve workspace. Run 'retold workspace init <path>' or specify --workspace.",
        )
        .into_payload();
    };

    let manifest = read_workspace_toml(&resolved_root);
    match inspect_workspace(&resolved_root) {
        Ok(desc) => build_show_payload(&desc, manifest.as_ref()),
        Err(diagnostic) => diagnostic.into_payload(),
    }
}
