//! Crash-safe file writes: write to a temp file beside the target, flush, then rename. A
//! crash leaves either the old file or the new one, never half of one.

use std::io::Write;
use std::path::Path;

use crate::error::EngineError;
mod publication;
pub use publication::{publish_directory, sync_directory};

/// Read a regular file with a hard byte bound. Never creates or repairs anything.
pub fn read_bounded(path: &Path, ceiling: u64) -> Result<Option<Vec<u8>>, EngineError> {
    use std::io::Read;
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Option::default()),
        Err(error) => return Err(EngineError::io(path, error)),
    };
    if !metadata.is_file() || metadata.len() > ceiling {
        return Err(EngineError::Setup(
            "Expected a bounded regular file.".into(),
        ));
    }
    let file = std::fs::File::open(path).map_err(|error| EngineError::io(path, error))?;
    let mut bytes = Vec::new();
    file.take(ceiling.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| EngineError::io(path, error))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > ceiling {
        return Err(EngineError::Setup(
            "File grew beyond the inspection limit.".into(),
        ));
    }
    Ok(Some(bytes))
}

pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), EngineError> {
    let parent = path
        .parent()
        .ok_or_else(|| EngineError::Setup(format!("{} has no parent folder", path.display())))?;
    std::fs::create_dir_all(parent).map_err(|e| EngineError::io(parent, e))?;
    let mut tmp = tempfile::Builder::new()
        .prefix(".tmp-")
        .tempfile_in(parent)
        .map_err(|e| EngineError::io(parent, e))?;
    tmp.write_all(bytes).map_err(|e| EngineError::io(path, e))?;
    tmp.as_file()
        .sync_all()
        .map_err(|e| EngineError::io(path, e))?;
    tmp.persist(path)
        .map_err(|e| EngineError::io(path, e.error))?;
    #[cfg(unix)]
    std::fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|e| EngineError::io(parent, e))?;
    Ok(())
}

pub fn write_json_atomic<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), EngineError> {
    let mut bytes = serde_json::to_vec_pretty(value)
        .map_err(|e| EngineError::Setup(format!("encoding {}: {e}", path.display())))?;
    bytes.push(b'\n');
    write_atomic(path, &bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_leaves_no_staging_files() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("a/b/state.json");
        write_atomic(&p, b"one").unwrap();
        write_atomic(&p, b"two").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "two");
        let names: Vec<_> = std::fs::read_dir(p.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names.len(), 1, "{names:?}");
    }
}
