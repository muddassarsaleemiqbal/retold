//! Version two verifies managed content and lineage; version one remains a byte-only index.
use super::{Index, creation, files::Selection, invalid};
use crate::account::{backup, types::Workspace};
use crate::protocol::Diagnostic;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Lineage {
    pub schema_version: u32,
    pub workspace: Workspace,
    pub archive_root: String,
    pub account_snapshot: String,
    pub video_ids: Vec<String>,
    pub omitted_paths: Vec<PathBuf>,
    pub execution_authorized: bool,
}

pub(super) fn verify(root: &Path, index: &Index) -> Result<bool, Diagnostic> {
    if index.schema_version == 1 {
        return Ok(false);
    }
    let lineage = read_lineage(root)?;
    validate_lineage(&lineage, index)?;
    verify_account(root, &lineage, index)?;
    verify_runs(root, &lineage, index)?;
    Ok(true)
}

pub(super) fn read_lineage(root: &Path) -> Result<Lineage, Diagnostic> {
    let path = root.join("workspace.json");
    super::regular_file(&path)?;
    let bytes = retold_engine::fsx::read_bounded(&path, super::INDEX_BYTES)
        .map_err(|_| invalid())?
        .ok_or_else(invalid)?;
    serde_json::from_slice(&bytes).map_err(|_| invalid())
}

pub(super) fn validate_lineage(lineage: &Lineage, index: &Index) -> Result<(), Diagnostic> {
    let supported = lineage.schema_version == 1
        && lineage.archive_root == "workspace"
        && lineage.account_snapshot == "account";
    let bound = lineage.workspace.id == index.workspace_id && lineage.workspace.root.is_absolute();
    if !supported || !bound || lineage.execution_authorized {
        return Err(invalid());
    }
    for path in &lineage.omitted_paths {
        super::validate_path(path.to_str().ok_or_else(invalid)?)?;
    }
    Ok(())
}

fn verify_account(root: &Path, lineage: &Lineage, index: &Index) -> Result<(), Diagnostic> {
    let path = root.join("account");
    let canonical_account = None;
    let verified = backup::verify(&path, canonical_account).map_err(|_| invalid())?;
    let registered =
        backup::contains_workspace(&path, &lineage.workspace).map_err(|_| invalid())?;
    if verified.manifest.account_id != index.account_id || !registered {
        return Err(invalid());
    }
    Ok(())
}

fn verify_runs(root: &Path, lineage: &Lineage, index: &Index) -> Result<(), Diagnostic> {
    let workspace = root.join("workspace");
    super::regular_directory(&workspace)?;
    let catalog = crate::discovery::scan(&workspace)?;
    validate_catalog(&catalog, lineage)?;
    creation::validate_runs(&catalog)?;
    let selection = Selection::scan(&catalog)?;
    verify_selection(&selection, index)
}

pub(super) fn validate_catalog(
    catalog: &crate::library::Catalog,
    lineage: &Lineage,
) -> Result<(), Diagnostic> {
    let found = unique_ids(catalog.videos.iter().map(|video| &video.id))?;
    let expected = unique_ids(lineage.video_ids.iter())?;
    let complete = catalog.issues.is_empty();
    if !complete || found != expected {
        return Err(invalid());
    }
    Ok(())
}

pub(super) fn verify_selection(selection: &Selection, index: &Index) -> Result<(), Diagnostic> {
    let managed: BTreeSet<_> = selection
        .files
        .iter()
        .map(|entry| format!("workspace/{}", entry.path))
        .chain([
            "workspace.json".into(),
            "account/backup.json".into(),
            "account/accounting.sqlite".into(),
        ])
        .collect();
    let listed: BTreeSet<_> = index.files.iter().map(|entry| entry.path.clone()).collect();
    if managed != listed {
        return Err(invalid());
    }
    Ok(())
}

fn unique_ids<'a>(
    ids: impl Iterator<Item = &'a String>,
) -> Result<BTreeSet<&'a String>, Diagnostic> {
    let mut unique = BTreeSet::new();
    for id in ids {
        if !unique.insert(id) {
            return Err(invalid());
        }
    }
    Ok(unique)
}
