//! Reconstructs the pinned backup view without writing a temporary archive or trusting live state.
use super::super::{
    Entry, Index, creation,
    files::Selection,
    invalid,
    restoration::Inactive,
    semantics::{self, Lineage},
};
use super::{Receipt, read_json};
use crate::{
    account::{activation::Transfer, backup},
    library::Catalog,
    protocol::Diagnostic,
};
use retold_engine::{
    recovery::INACTIVE_MARKER,
    store::{State, inspection::MAX_INSPECTION_BYTES},
};
use std::path::{Path, PathBuf};

pub(super) const RECOVERY: &str = ".retold-recovery";
pub(super) const RECEIPT_FILE: &str = "activation.json";

pub(super) struct Evidence {
    pub root: PathBuf,
    pub catalog: Catalog,
    pub inactive: Inactive,
    pub transfer: Transfer,
    pub fully_inactive: bool,
}

impl Evidence {
    pub fn load(root: &Path) -> Result<Self, Diagnostic> {
        super::super::regular_directory(root)?;
        let root = root.canonicalize().map_err(|_| invalid())?;
        let evidence = root.join(RECOVERY);
        let inactive = inactive(&root, &evidence)?;
        let index = verified_index(&evidence, &inactive)?;
        let lineage = verify_lineage(&evidence, &index)?;
        let catalog = verify_catalog(&root, &lineage, &index)?;
        let fully_inactive = verify_markers(&root, &catalog, &inactive)?;
        verify_disk_inventory(&root, &catalog, &index)?;
        let transfer = Transfer {
            original: lineage.workspace,
            target: root.clone(),
            snapshot: evidence.join("account"),
            index_blake3: inactive.recovery_index_blake3.clone(),
        };
        Ok(Self {
            root,
            catalog,
            inactive,
            transfer,
            fully_inactive,
        })
    }
}

fn verify_lineage(evidence: &Path, index: &Index) -> Result<Lineage, Diagnostic> {
    let lineage = semantics::read_lineage(evidence)?;
    semantics::validate_lineage(&lineage, index)?;
    verify_account(evidence, index)?;
    Ok(lineage)
}

fn verify_catalog(root: &Path, lineage: &Lineage, index: &Index) -> Result<Catalog, Diagnostic> {
    let catalog = crate::discovery::scan(root)?;
    semantics::validate_catalog(&catalog, lineage)?;
    creation::validate_runs(&catalog)?;
    let selection = Selection::scan(&catalog)?;
    semantics::verify_selection(&selection, index)?;
    verify_bytes(root, index)?;
    verify_states(root, index)?;
    Ok(catalog)
}

fn inactive(root: &Path, evidence: &Path) -> Result<Inactive, Diagnostic> {
    let marker = root.join(INACTIVE_MARKER);
    let marker_exists = marker.try_exists().map_err(|_| invalid())?;
    let restored: Inactive = if marker_exists {
        read_json(&marker)?
    } else {
        let stored_receipt: Receipt = read_json(&evidence.join(RECEIPT_FILE))?;
        stored_receipt.inactive
    };
    let valid = restored.schema_version == 1
        && !restored.execution_authorized
        && restored.queues_stopped
        && !restored.accounting_installed;
    if !valid {
        return Err(invalid());
    }
    Ok(restored)
}

fn verified_index(evidence: &Path, inactive: &Inactive) -> Result<Index, Diagnostic> {
    let index = super::super::read_index(evidence)?;
    super::super::validate_header(&index)?;
    let path = evidence.join("index.json");
    let bytes = super::super::regular_file(&path)?;
    let digest = super::super::digest(&path, bytes)?;
    let bound =
        index.workspace_id == inactive.workspace_id && index.account_id == inactive.account_id;
    if index.schema_version != 2 || !bound || digest != inactive.recovery_index_blake3 {
        return Err(invalid());
    }
    Ok(index)
}

fn verify_account(evidence: &Path, index: &Index) -> Result<(), Diagnostic> {
    let lineage = semantics::read_lineage(evidence)?;
    let path = evidence.join("account");
    let canonical = None;
    let financial = backup::verify(&path, canonical).map_err(|_| invalid())?;
    let bound = backup::contains_workspace(&path, &lineage.workspace).map_err(|_| invalid())?;
    if !bound || financial.manifest.account_id != index.account_id {
        return Err(invalid());
    }
    Ok(())
}

fn map_entry_path(path: &str) -> String {
    let Some(relative) = path.strip_prefix("workspace/") else {
        return format!("{RECOVERY}/{path}");
    };
    if relative.ends_with("/.retold/state.json") {
        format!("{RECOVERY}/original-state/{relative}")
    } else {
        relative.into()
    }
}

fn mapped(entry: &Entry) -> Entry {
    Entry {
        path: map_entry_path(&entry.path),
        bytes: entry.bytes,
        blake3: entry.blake3.clone(),
    }
}

fn verify_bytes(root: &Path, index: &Index) -> Result<(), Diagnostic> {
    let entries: Vec<_> = index.files.iter().map(mapped).collect();
    super::super::verify_entries(root, &entries).map(|_| ())
}

fn state_relpath(entry: &Entry) -> Option<&str> {
    let relative = entry.path.strip_prefix("workspace/")?;
    relative
        .ends_with("/.retold/state.json")
        .then_some(relative)
}

fn compare_state(root: &Path, entry: &Entry, relative: &str) -> Result<(), Diagnostic> {
    let mapped_entry = mapped(entry);
    let mut expected = read_state(&root.join(mapped_entry.path))?;
    expected.active = None;
    expected.active_tasks.clear();
    let actual = read_state(&root.join(relative))?;
    let expected_hash = retold_engine::store::hash(&expected).map_err(|_| invalid())?;
    let actual_hash = retold_engine::store::hash(&actual).map_err(|_| invalid())?;
    if actual_hash != expected_hash {
        return Err(invalid());
    }
    Ok(())
}

fn verify_state_entry(root: &Path, entry: &Entry) -> Result<(), Diagnostic> {
    if let Some(relative) = state_relpath(entry) {
        compare_state(root, entry, relative)?;
    }
    Ok(())
}

fn verify_states(root: &Path, index: &Index) -> Result<(), Diagnostic> {
    for entry in &index.files {
        verify_state_entry(root, entry)?;
    }
    Ok(())
}

fn read_state(path: &Path) -> Result<State, Diagnostic> {
    let bytes = retold_engine::fsx::read_bounded(path, MAX_INSPECTION_BYTES)
        .map_err(|_| invalid())?
        .ok_or_else(invalid)?;
    serde_json::from_slice(&bytes).map_err(|_| invalid())
}

fn verify_markers(root: &Path, catalog: &Catalog, inactive: &Inactive) -> Result<bool, Diagnostic> {
    let paths = std::iter::once(root.to_path_buf())
        .chain(catalog.videos.iter().map(|video| video.directory.clone()));
    let mut complete = true;
    for directory in paths {
        let path = directory.join(INACTIVE_MARKER);
        let exists = path.try_exists().map_err(|_| invalid())?;
        if exists {
            let actual: Inactive = read_json(&path)?;
            if actual != *inactive {
                return Err(invalid());
            }
        } else {
            complete = false;
        }
    }
    Ok(complete)
}

fn metadata_entries(root: &Path, catalog: &Catalog) -> Result<Vec<Entry>, Diagnostic> {
    let mut entries = Vec::new();
    let mut paths = vec![
        root.join(RECOVERY).join("index.json"),
        root.join(RECOVERY).join(RECEIPT_FILE),
        root.join(INACTIVE_MARKER),
    ];
    let video_markers = catalog
        .videos
        .iter()
        .map(|video| video.directory.join(INACTIVE_MARKER));
    paths.extend(video_markers);
    for path in paths {
        let exists = path.try_exists().map_err(|_| invalid())?;
        if exists {
            let relative = path.strip_prefix(root).map_err(|_| invalid())?;
            let relative_str = relative.to_str().ok_or_else(invalid)?;
            entries.push(Entry {
                path: relative_str.into(),
                bytes: 0,
                blake3: String::new(),
            });
        }
    }
    Ok(entries)
}

fn selection_inventory(selection: &Selection, index: &Index) -> Vec<Entry> {
    let mut entries: Vec<_> = selection
        .files
        .iter()
        .map(|entry| Entry {
            path: entry.path.clone(),
            bytes: entry.bytes,
            blake3: entry.blake3.clone(),
        })
        .collect();
    let snapshot_files = index
        .files
        .iter()
        .filter(|entry| {
            !entry.path.starts_with("workspace/") || entry.path.ends_with("/.retold/state.json")
        })
        .map(mapped);
    entries.extend(snapshot_files);
    entries
}

fn verify_disk_inventory(root: &Path, catalog: &Catalog, index: &Index) -> Result<(), Diagnostic> {
    let selection = Selection::scan(catalog)?;
    let mut entries = selection_inventory(&selection, index);
    let metadata = metadata_entries(root, catalog)?;
    entries.extend(metadata);
    super::super::verify_tree(root, &entries)
}
