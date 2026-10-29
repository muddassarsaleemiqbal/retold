//! Saved active tasks are historical evidence, never a queue to resume after restore.
use super::super::invalid;
use crate::library::Catalog;
use crate::protocol::Diagnostic;
use retold_engine::store::{State, inspection::MAX_INSPECTION_BYTES};
use std::io::Write;
use std::path::Path;

pub(super) fn stop(catalog: &Catalog, evidence: &Path) -> Result<(), Diagnostic> {
    for video in &catalog.videos {
        stop_state(
            &catalog.root,
            &video.layout.meta().join("state.json"),
            evidence,
        )?;
    }
    Ok(())
}

fn stop_state(root: &Path, path: &Path, evidence: &Path) -> Result<(), Diagnostic> {
    let bytes =
        retold_engine::fsx::read_bounded(path, MAX_INSPECTION_BYTES).map_err(|_| invalid())?;
    let Some(bytes) = bytes else {
        return Ok(());
    };
    let mut state: State = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    let relative = path.strip_prefix(root).map_err(|_| invalid())?;
    preserve(relative, evidence, &bytes)?;
    state.active = None;
    state.active_tasks.clear();
    let bytes = serde_json::to_vec_pretty(&state).map_err(|_| invalid())?;
    let mut file = std::fs::File::create(path).map_err(|_| invalid())?;
    file.write_all(&bytes).map_err(|_| invalid())?;
    file.sync_all().map_err(|_| invalid())
}

fn preserve(relative: &Path, evidence: &Path, bytes: &[u8]) -> Result<(), Diagnostic> {
    let original = evidence.join("original-state").join(relative);
    let parent = original.parent().ok_or_else(invalid)?;
    std::fs::create_dir_all(parent).map_err(|_| invalid())?;
    let mut file = std::fs::File::create_new(original).map_err(|_| invalid())?;
    file.write_all(bytes).map_err(|_| invalid())?;
    file.sync_all().map_err(|_| invalid())
}
