//! Enumeration of known and registered workspaces (`retold workspace list`).

use std::path::{Path, PathBuf};

use super::resolution::{read_remembered_workspace, read_workspace_toml};
use crate::account::Account;
use crate::protocol::{Outcome, Payload};

#[derive(Debug, serde::Serialize)]
struct WorkspaceListItem {
    id: String,
    name: String,
    root: PathBuf,
    active: bool,
}

fn collect_registered_items(active_root: Option<&Path>) -> Vec<WorkspaceListItem> {
    let Ok(canonical_dir) = crate::account::canonical_directory() else {
        return Vec::new();
    };
    let Ok(account) = Account::open(&canonical_dir) else {
        return Vec::new();
    };
    let Ok(workspaces) = account.registered_workspaces() else {
        return Vec::new();
    };

    workspaces
        .into_iter()
        .map(|ws| {
            let manifest = read_workspace_toml(&ws.root);
            let name = manifest.map_or_else(|| "workspace".to_string(), |item| item.name);
            let active = active_root.is_some_and(|act| act == ws.root);
            WorkspaceListItem {
                id: ws.id,
                name,
                root: ws.root,
                active,
            }
        })
        .collect()
}

fn item_from_remembered(remembered: &Path) -> WorkspaceListItem {
    let manifest = read_workspace_toml(remembered);
    let id = manifest
        .as_ref()
        .map_or_else(|| "unmanaged".to_string(), |item| item.id.clone());
    let name = manifest.map_or_else(|| "workspace".to_string(), |item| item.name);
    WorkspaceListItem {
        id,
        name,
        root: remembered.to_path_buf(),
        active: true,
    }
}

#[must_use]
pub fn list_workspaces() -> Payload {
    let remembered = read_remembered_workspace();
    let mut items = collect_registered_items(remembered.as_deref());

    if items.is_empty() {
        items.extend(
            remembered
                .as_deref()
                .filter(|dir| dir.is_dir())
                .map(item_from_remembered),
        );
    }

    let count = items.len();
    let text = format!("Found {count} workspace(s).");

    Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": text,
            "items": items,
        }),
    }
}
