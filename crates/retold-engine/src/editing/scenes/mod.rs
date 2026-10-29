//! Manual scene direction is distinct from generated originals and cannot authorize execution.
mod cast;
mod context;
mod mutation;
mod panels;
mod snapshot;
mod storage;

use serde::{Deserialize, Serialize};

use super::EditError;
use crate::{RunLayout, assets::Storyboard};

pub use cast::{CastEdit, MAX_CAST, PROFILE_EXPRESSIONS, preview_cast, save_cast, validate_cast};
pub use mutation::{ChangeImpact, ShotEdit, approve, preview, redo, save_shot, undo};
pub use mutation::{MAX_ACTION_BYTES, MAX_LOCATION_BYTES, MAX_MOOD_BYTES, validate_edit};
pub use snapshot::{SceneSnapshot, history, snapshot};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Baseline {
    schema: u32,
    context_hash: String,
    board: Storyboard,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Revision {
    schema: u32,
    base: String,
    parent: Option<String>,
    created_at: String,
    unit: Option<usize>,
    board: Storyboard,
}

/// Preserve legacy script fingerprints when no scene journal exists.
pub(super) fn pointer_digest(layout: &RunLayout) -> Result<Option<String>, EditError> {
    storage::pointer_bytes(layout)?
        .map(|bytes| crate::store::hash(&bytes).map_err(EditError::from))
        .transpose()
}

/// Reject unapproved, changed or corrupt manual direction before any provider can be called.
pub fn ready(layout: &RunLayout) -> Result<(), EditError> {
    let pointer = storage::pointer(layout)?;
    let Some(head) = &pointer.head else {
        if let Some(base) = &pointer.base {
            storage::baseline(layout, base)?;
        }
        return Ok(());
    };
    let revision = storage::revision(layout, head)?;
    storage::require_binding(&pointer, &revision)?;
    let base = storage::baseline(layout, &revision.base)?;
    context::require_current(layout, &base)?;
    context::require_record(layout)?;
    let hash = crate::store::hash(&revision.board)?;
    let approved = pointer.approved_hash.as_deref() == Some(&hash);
    if !approved {
        return Err(EditError::SceneUnapproved);
    }
    context::require_quality(layout, &revision.board)
}

/// Called only with the engine's freshly constructed, gated board while holding the run lock.
pub(crate) fn effective(
    layout: &RunLayout,
    generated: Storyboard,
) -> Result<Storyboard, EditError> {
    // Callers gate generated direction against the active guide. The saved guide is
    // used below only to validate manual revisions bound to that persisted source.
    let pointer = storage::pointer(layout)?;
    if let Some(head) = &pointer.head {
        ready(layout)?;
        let revision = storage::revision(layout, head)?;
        let base = storage::baseline(layout, &revision.base)?;
        if crate::store::hash(&base.board)? != crate::store::hash(&generated)? {
            return Err(EditError::SceneSourceChanged);
        }
        return Ok(revision.board);
    }
    mutation::refresh_baseline(layout, pointer, &generated)?;
    Ok(generated)
}
