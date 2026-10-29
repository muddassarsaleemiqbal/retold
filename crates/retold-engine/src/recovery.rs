//! Restored roots remain inspectable but cannot acquire write or dispatch authority.
use crate::EngineError;
use std::path::{Path, PathBuf};

pub const INACTIVE_MARKER: &str = ".retold-restore.json";

pub fn ensure_active(path: &Path) -> Result<(), EngineError> {
    let existing = existing_ancestor(path)?;
    for ancestor in existing.ancestors() {
        let marker = ancestor.join(INACTIVE_MARKER);
        match std::fs::symlink_metadata(&marker) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(EngineError::io(&marker, error)),
            Ok(_) => {
                return Err(EngineError::Setup(
                    "This workspace is inactive or retired. Execution requires explicit ownership and accounting reconciliation.".into(),
                ));
            }
        }
    }
    Ok(())
}

fn existing_ancestor(path: &Path) -> Result<PathBuf, EngineError> {
    let absolute = std::path::absolute(path).map_err(|error| EngineError::io(path, error))?;
    for ancestor in absolute.ancestors() {
        match ancestor.canonicalize() {
            Ok(existing) => return Ok(existing),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(EngineError::io(ancestor, error)),
        }
    }
    Err(EngineError::Setup("Workspace root is unavailable.".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inactive_marker_blocks_descendants_and_missing_children_without_creating_them() {
        let root = tempfile::tempdir().unwrap();
        let marker = root.path().join(INACTIVE_MARKER);
        std::fs::create_dir(&marker).unwrap();
        let child = root.path().join("future/run");
        assert!(ensure_active(&child).is_err());
        assert!(!child.exists());
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_access_cannot_hide_an_inactive_ancestor() {
        let sandbox = tempfile::tempdir().unwrap();
        let original = inactive_channel(sandbox.path());
        let alias = sandbox.path().join("alias");
        std::os::unix::fs::symlink(original, &alias).unwrap();
        assert!(ensure_active(&alias.join("run")).is_err());
    }
    #[cfg(unix)]
    fn inactive_channel(root: &Path) -> PathBuf {
        let original = root.join("restored");
        std::fs::create_dir(&original).unwrap();
        std::fs::write(original.join(INACTIVE_MARKER), b"{}").unwrap();
        original
    }
}
