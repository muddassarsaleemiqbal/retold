//! Windows socket addresses live in the per-user studio state directory; there is no alias.
use std::path::{Path, PathBuf};

use super::{StudioError, server::SOCKET_NAME};

pub(super) fn prepare(state: &Path) -> Result<PathBuf, StudioError> {
    std::fs::create_dir_all(state)?;
    Ok(state.join(SOCKET_NAME))
}

pub(super) fn address(state: &Path) -> Result<PathBuf, StudioError> {
    Ok(state.join(SOCKET_NAME))
}

pub(super) fn cleanup(_state: &Path) {}
