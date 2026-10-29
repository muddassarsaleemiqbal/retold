//! Sync a complete private staging tree, then publish it without replacing any destination.
use std::path::Path;

use crate::EngineError;

const MAX_PUBLISH_ENTRIES: usize = 20_000;
const MAX_PUBLISH_DEPTH: usize = 64;

pub fn publish_directory(staging: &Path, destination: &Path) -> Result<(), EngineError> {
    let metadata =
        std::fs::symlink_metadata(staging).map_err(|error| EngineError::io(staging, error))?;
    if !metadata.is_dir() {
        return Err(invalid("Publication requires a regular staging directory."));
    }
    SyncBudget {
        remaining: MAX_PUBLISH_ENTRIES,
    }
    .walk(staging, 0)?;
    rename_new(staging, destination)?;
    let parent = destination
        .parent()
        .ok_or_else(|| invalid("Publication destination has no parent."))?;
    sync_directory(parent).map_err(|_| invalid("The directory was published but its final sync failed; inspect the destination before retrying."))
}

struct SyncBudget {
    remaining: usize,
}

impl SyncBudget {
    fn walk(mut self, path: &Path, depth: usize) -> Result<Self, EngineError> {
        if depth > MAX_PUBLISH_DEPTH {
            return Err(invalid("Publication nesting exceeds the limit."));
        }
        self.remaining = self
            .remaining
            .checked_sub(1)
            .ok_or_else(|| invalid("Publication has too many entries."))?;
        let metadata =
            std::fs::symlink_metadata(path).map_err(|error| EngineError::io(path, error))?;
        if metadata.is_file() {
            sync_file(path)?;
            return Ok(self);
        }
        if !metadata.is_dir() {
            return Err(invalid(
                "Staging trees may contain only regular files and directories.",
            ));
        }
        for entry in std::fs::read_dir(path).map_err(|error| EngineError::io(path, error))? {
            let entry = entry.map_err(|error| EngineError::io(path, error))?;
            self = self.walk(&entry.path(), depth.saturating_add(1))?;
        }
        sync_directory(path)?;
        Ok(self)
    }
}

fn sync_file(path: &Path) -> Result<(), EngineError> {
    std::fs::File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|error| EngineError::io(path, error))
}

#[cfg(unix)]
pub fn sync_directory(path: &Path) -> Result<(), EngineError> {
    sync_file(path)
}

#[cfg(not(unix))]
pub fn sync_directory(_path: &Path) -> Result<(), EngineError> {
    Ok(())
}

#[cfg(any(target_os = "linux", target_vendor = "apple"))]
fn rename_new(staging: &Path, destination: &Path) -> Result<(), EngineError> {
    use rustix::fs::{CWD, RenameFlags, renameat_with};
    renameat_with(CWD, staging, CWD, destination, RenameFlags::NOREPLACE)
        .map_err(|error| EngineError::io(destination, error.into()))
}

#[cfg(target_os = "windows")]
fn rename_new(staging: &Path, destination: &Path) -> Result<(), EngineError> {
    std::fs::rename(staging, destination).map_err(|error| EngineError::io(destination, error))
}

#[cfg(not(any(target_os = "linux", target_vendor = "apple", target_os = "windows")))]
fn rename_new(_staging: &Path, _destination: &Path) -> Result<(), EngineError> {
    Err(invalid(
        "Atomic directory publication is unavailable on this platform.",
    ))
}

fn invalid(message: &str) -> EngineError {
    EngineError::Setup(message.into())
}
