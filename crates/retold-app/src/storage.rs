//! Offline verification of portable directory backups. Verification grants no execution authority.
pub mod activation;
pub mod creation;
mod files;
mod publication;
pub(crate) use publication::publish as publish_directory;
pub mod restoration;
mod semantics;
pub mod usage;

use crate::protocol::{Diagnostic, ErrorCode, Outcome, Payload};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Component, Path};

const INDEX_FILE: &str = "index.json";
const INDEX_BYTES: u64 = 4 * 1024 * 1024;
const FILE_LIMIT: usize = 10_000;
const MAX_TOTAL_BYTES: u64 = 1024 * 1024 * 1024;
const BUFFER_BYTES: usize = 64 * 1024;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Index {
    pub schema_version: u32,
    pub format: String,
    pub workspace_id: String,
    pub account_id: String,
    pub files: Vec<Entry>,
    pub omissions: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub path: String,
    pub bytes: u64,
    pub blake3: String,
}

#[derive(Debug, Serialize)]
pub struct Verified {
    pub index: Index,
    pub total_bytes: u64,
    pub managed_workspace_verified: bool,
    pub execution_authorized: bool,
}

#[must_use]
pub fn respond(path: &Path) -> Payload {
    match verify(path) {
        Ok(verified) => Payload::Result {
            outcome: Outcome::Completed,
            contents: serde_json::json!({
                "text": "Portable backup index and file checksums verified offline. Execution remains unauthorized.",
                "backup_verified": verified,
            }),
        },
        Err(error) => error.into_payload(),
    }
}

pub fn verify(root: &Path) -> Result<Verified, Diagnostic> {
    regular_directory(root)?;
    let index = read_index(root)?;
    validate_header(&index)?;
    let total_bytes = verify_entries(root, &index.files)?;
    verify_inventory(root, &index.files)?;
    let managed_workspace_verified = semantics::verify(root, &index)?;
    Ok(Verified {
        managed_workspace_verified,
        index,
        total_bytes,
        execution_authorized: false,
    })
}

fn read_index(root: &Path) -> Result<Index, Diagnostic> {
    let path = root.join(INDEX_FILE);
    regular_file(&path)?;
    let bytes = retold_engine::fsx::read_bounded(&path, INDEX_BYTES)
        .map_err(|_| invalid())?
        .ok_or_else(invalid)?;
    serde_json::from_slice(&bytes).map_err(|_| invalid())
}

fn validate_header(index: &Index) -> Result<(), Diagnostic> {
    let supported =
        matches!(index.schema_version, 1 | 2) && index.format == "retold-workspace-backup";
    let identified = identified(&index.workspace_id) && identified(&index.account_id);
    let bounded = index.files.len() <= FILE_LIMIT;
    if !supported || !identified || !bounded {
        return Err(invalid());
    }
    Ok(())
}

fn verify_entries(root: &Path, entries: &[Entry]) -> Result<u64, Diagnostic> {
    let mut seen = BTreeSet::new();
    let mut total = 0_u64;
    for entry in entries {
        validate_path(&entry.path)?;
        if !seen.insert(&entry.path) {
            return Err(invalid());
        }
        total = total.checked_add(entry.bytes).ok_or_else(invalid)?;
        if total > MAX_TOTAL_BYTES {
            return Err(invalid());
        }
        verify_entry(root, entry)?;
    }
    Ok(total)
}

fn validate_path(value: &str) -> Result<(), Diagnostic> {
    let forbidden = value.is_empty() || value.contains(['\\', ':']) || value == INDEX_FILE;
    let safe = Path::new(value)
        .components()
        .all(|part| matches!(part, Component::Normal(_)));
    let clean = value
        .split('/')
        .all(|part| !part.is_empty() && part != "." && part != "..");
    if forbidden || !safe || !clean {
        return Err(invalid());
    }
    Ok(())
}

fn verify_entry(root: &Path, entry: &Entry) -> Result<(), Diagnostic> {
    let mut path = root.to_path_buf();
    let components: Vec<_> = Path::new(&entry.path).components().collect();
    let (last, parents) = components.split_last().ok_or_else(invalid)?;
    for parent in parents {
        path.push(parent);
        regular_directory(&path)?;
    }
    path.push(last);
    let length = regular_file(&path)?;
    if length != entry.bytes {
        return Err(invalid());
    }
    let digest = digest(&path, entry.bytes)?;
    if digest != entry.blake3 {
        return Err(invalid());
    }
    Ok(())
}

fn digest(path: &Path, expected: u64) -> Result<String, Diagnostic> {
    let file = std::fs::File::open(path).map_err(|_| invalid())?;
    let mut reader = file.take(expected.saturating_add(1));
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0_u8; BUFFER_BYTES];
    let mut consumed = 0_u64;
    loop {
        let count = reader.read(&mut buffer).map_err(|_| invalid())?;
        if count == 0 {
            break;
        }
        consumed += count as u64;
        hasher.update(&buffer[..count]);
    }
    if consumed != expected {
        return Err(invalid());
    }
    Ok(hasher.finalize().to_hex().to_string())
}

fn regular_directory(path: &Path) -> Result<(), Diagnostic> {
    let metadata = std::fs::symlink_metadata(path).map_err(|_| invalid())?;
    if !metadata.is_dir() {
        return Err(invalid());
    }
    Ok(())
}

fn regular_file(path: &Path) -> Result<u64, Diagnostic> {
    let metadata = std::fs::symlink_metadata(path).map_err(|_| invalid())?;
    if !metadata.is_file() {
        return Err(invalid());
    }
    Ok(metadata.len())
}

fn invalid() -> Diagnostic {
    Diagnostic::new(
        ErrorCode::DataInvalid,
        "Portable backup failed integrity checks: inspect its version, lineage, paths, sizes and checksums.",
    )
}

fn verify_inventory(root: &Path, entries: &[Entry]) -> Result<(), Diagnostic> {
    Inventory::new(root, entries).verify()
}

struct Inventory {
    expected: BTreeSet<std::path::PathBuf>,
    pending: Vec<std::path::PathBuf>,
    remaining: usize,
}

impl Inventory {
    fn new(root: &Path, entries: &[Entry]) -> Self {
        let mut expected: BTreeSet<_> =
            entries.iter().map(|entry| root.join(&entry.path)).collect();
        expected.insert(root.join(INDEX_FILE));
        Self {
            expected,
            pending: vec![root.to_path_buf()],
            remaining: FILE_LIMIT,
        }
    }

    fn verify(mut self) -> Result<(), Diagnostic> {
        loop {
            let next = self.pending.pop();
            let Some(directory) = next else {
                break;
            };
            let children = std::fs::read_dir(directory).map_err(|_| invalid())?;
            for child in children {
                self.remaining = self.remaining.checked_sub(1).ok_or_else(invalid)?;
                self = self.inspect(&child.map_err(|_| invalid())?)?;
            }
        }
        let complete = self.expected.is_empty();
        if !complete {
            return Err(invalid());
        }
        Ok(())
    }

    fn inspect(mut self, child: &std::fs::DirEntry) -> Result<Self, Diagnostic> {
        let path = child.path();
        let kind = child.file_type().map_err(|_| invalid())?;
        if kind.is_dir() {
            self.pending.push(path);
        } else {
            let regular = kind.is_file();
            let listed = self.expected.remove(&path);
            if !regular || !listed {
                return Err(invalid());
            }
        }
        Ok(self)
    }
}

fn identified(value: &str) -> bool {
    !value.trim().is_empty()
}

fn verify_tree(root: &Path, entries: &[Entry]) -> Result<(), Diagnostic> {
    let mut inventory = Inventory::new(root, entries);
    inventory.expected.remove(&root.join(INDEX_FILE));
    inventory.verify()
}
