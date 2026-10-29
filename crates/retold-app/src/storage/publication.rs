//! Staging owns temporary data; only a complete directory can become the destination.
use crate::protocol::{Diagnostic, ErrorCode};
use std::path::{Path, PathBuf};

pub(super) fn destination(output: &Path, root: &Path) -> Result<PathBuf, Diagnostic> {
    let name = output.file_name().ok_or_else(invalid_output)?;
    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let parent = parent.canonicalize().map_err(|_| invalid_output())?;
    let output = parent.join(name);
    let exists = std::fs::symlink_metadata(&output).is_ok();
    if exists || output.starts_with(root) {
        return Err(invalid_output());
    }
    Ok(output)
}

pub(super) fn staging(output: &Path) -> Result<tempfile::TempDir, Diagnostic> {
    let parent = output.parent().ok_or_else(invalid_output)?;
    tempfile::Builder::new()
        .prefix(".retold-backup-")
        .tempdir_in(parent)
        .map_err(|_| failed())
}

pub fn publish(staging: &Path, output: &Path) -> Result<(), Diagnostic> {
    retold_engine::fsx::publish_directory(staging, output).map_err(|_| failed())
}

pub(super) fn sync_directory(path: &Path) -> Result<(), Diagnostic> {
    retold_engine::fsx::sync_directory(path).map_err(|_| failed())
}

fn invalid_output() -> Diagnostic {
    Diagnostic::new(
        ErrorCode::InputRequired,
        "Choose a new backup folder outside the workspace, with an existing parent. Backups never overwrite.",
    )
}

fn failed() -> Diagnostic {
    Diagnostic::new(
        ErrorCode::ExecutionFailed,
        "Backup publication failed. Inspect the destination before retrying; a complete backup may exist if the final directory sync failed.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destination_created_after_planning_is_never_replaced() {
        let sandbox = tempfile::tempdir().unwrap();
        let destination = sandbox.path().join("backup");
        let staged = staging(&destination).unwrap();
        std::fs::write(staged.path().join("index.json"), b"staged").unwrap();
        std::fs::create_dir(&destination).unwrap();
        assert!(publish(staged.path(), &destination).is_err());
        assert!(!destination.join("index.json").exists());
    }
}
