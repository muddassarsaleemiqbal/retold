//! Authoritative manual script revisions. Generated mirrors and immutable model caches stay distinct.
mod context;
pub mod dialogue;
mod files;
mod mutation;
pub mod retake;
pub mod scenes;
mod snapshot;

use serde::{Deserialize, Serialize};

use crate::{EngineError, RunLayout, story::Script};

pub use mutation::{SectionEdit, approve, redo, save_section, undo};
pub use snapshot::{
    HistoryAvailability, HistoryCursor, HistoryEntry, HistoryPage, ScriptSnapshot, history, script,
};

const EDITING_FORMAT_VERSION: u32 = 1;
const MAX_DOCUMENT_BYTES: u64 = 2 * 1024 * 1024;
const MAX_POINTER_BYTES: u64 = 64 * 1024;
const MAX_REDO: usize = 128;
const HASH_LENGTH: usize = 64;
pub const HISTORY_PAGE_SIZE: usize = 64;

#[derive(Debug, thiserror::Error)]
pub enum EditError {
    #[error(
        "The saved scene sequence changed. Keep your draft and compare it with the latest version."
    )]
    SceneChanged,
    #[error(
        "The source, script or generated scene sequence changed. Keep scene history; undo manual scene revisions before regenerating the baseline."
    )]
    SceneSourceChanged,
    #[error("Approve the edited scenes before continuing.")]
    SceneUnapproved,
    #[error(
        "The script changed since it was opened. Your draft was not applied; reload and compare it with the saved version."
    )]
    Changed,
    #[error(
        "The source, guide or writing context changed. Keep the saved edit history and regenerate a baseline before reapplying these edits."
    )]
    SourceChanged,
    #[error("Approve the edited script before continuing.")]
    Unapproved,
    #[error("{0}")]
    Invalid(String),
    #[error("Script revision integrity failed: {0}")]
    Integrity(String),
    #[error(transparent)]
    Engine(#[from] EngineError),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pointer {
    schema: u32,
    base: Option<String>,
    head: Option<String>,
    approved_hash: Option<String>,
    approved_at: Option<String>,
    redo: Vec<String>,
}

impl Default for Pointer {
    fn default() -> Self {
        Self {
            schema: EDITING_FORMAT_VERSION,
            base: None,
            head: None,
            approved_hash: None,
            approved_at: None,
            redo: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Baseline {
    schema: u32,
    context_hash: String,
    script: Script,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Revision {
    schema: u32,
    base: String,
    parent: Option<String>,
    created_at: String,
    section: Option<String>,
    script: Script,
}

/// Hashes the authoritative pointer without mutating or repairing any editing state.
pub fn digest(layout: &RunLayout) -> Result<String, EditError> {
    let script = crate::store::hash(&files::pointer_bytes(layout)?)?;
    let scenes = scenes::pointer_digest(layout)?;
    let lines = dialogue::digest(layout)?;
    match (scenes, lines) {
        (None, None) => Ok(script),
        (Some(scenes), None) => Ok(crate::store::hash(&(script, scenes))?),
        (scenes, Some(lines)) => Ok(crate::store::hash(&(script, scenes, lines))?),
    }
}

/// Called before model work, including callers that bypass the application frontend.
pub fn ready(layout: &RunLayout) -> Result<(), EditError> {
    script_ready(layout)?;
    scenes::ready(layout)?;
    dialogue::ready(layout)
}

fn script_ready(layout: &RunLayout) -> Result<(), EditError> {
    let pointer = files::pointer(layout)?;
    let Some(head) = &pointer.head else {
        if let Some(base) = &pointer.base {
            files::baseline(layout, base)?;
        }
        return Ok(());
    };
    let revision = files::revision(layout, head)?;
    let base = files::baseline(layout, &revision.base)?;
    files::check_binding(&pointer, &revision)?;
    context::require_current(layout, &base)?;
    let content_hash = crate::store::hash(&revision.script)?;
    let approved = pointer.approved_hash.as_deref() == Some(&content_hash);
    if !approved {
        return Err(EditError::Unapproved);
    }
    context::require_quality(layout, &revision.script)
}

/// Apply approved manual content to the engine's generated baseline, never to a new source silently.
pub(crate) fn effective_script(layout: &RunLayout, generated: Script) -> Result<Script, EditError> {
    let pointer = files::pointer(layout)?;
    let Some(head) = &pointer.head else {
        mutation::refresh_baseline(layout, &pointer, &generated)?;
        return Ok(generated);
    };
    ready(layout)?;
    let revision = files::revision(layout, head)?;
    let baseline = files::baseline(layout, &revision.base)?;
    let same_baseline = crate::store::hash(&generated)? == crate::store::hash(&baseline.script)?;
    if !same_baseline {
        return Err(EditError::SourceChanged);
    }
    Ok(revision.script)
}
