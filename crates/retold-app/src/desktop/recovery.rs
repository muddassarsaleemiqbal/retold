//! Local editing checkpoints carry text and comparison context, never execution authority.
mod storage;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::protocol::{Diagnostic, ErrorCode};

/// Large pastes stay in the editor without displacing the last durable checkpoint.
pub const SCRIPT_TEXT_LIMIT: usize = 128 * 1024;
/// Review comments share the append API's byte ceiling.
pub const NOTE_TEXT_LIMIT: usize = 8192;

/// A retained editing buffer and the immutable production identity it belongs to.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Draft {
    pub library: PathBuf,
    pub reference: String,
    pub video_id: String,
    pub title: String,
    pub content: Content,
}

/// Only text editing can be recovered. Approval and financial authority are excluded.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Content {
    Script {
        section: String,
        source_version: String,
        text: String,
    },
    Note {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<crate::notes::Target>,
        append_id: String,
        source_version: String,
        text: String,
    },
    Scene {
        draft: crate::scenes::Draft,
        /// The unsaved cast of the same scene; absent in older checkpoints.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cast: Option<Vec<crate::scenes::CastPose>>,
    },
    Line {
        draft: crate::dialogue::Draft,
    },
}

impl Content {
    #[must_use]
    pub fn text(&self) -> &str {
        match self {
            Self::Script { text, .. } | Self::Note { text, .. } => text,
            Self::Scene { draft, .. } => &draft.action,
            Self::Line { draft } => &draft.text,
        }
    }

    #[must_use]
    pub fn copy_text(&self) -> String {
        match self {
            Self::Scene { draft, .. } => format!(
                "Action:\n{}\n\nLocation:\n{}\n\nMood:\n{}",
                draft.action, draft.location, draft.mood
            ),
            _ => self.text().to_owned(),
        }
    }
}

/// The version protects a retained draft from another window's edits or stale cleanup.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Checkpoint {
    pub id: String,
    pub version: String,
    pub updated_at: String,
    pub draft: Draft,
}

/// A small installation-local store. Reads never create, repair or activate data.
#[derive(Debug, Clone)]
pub struct Repository {
    directory: PathBuf,
}

impl Repository {
    #[must_use]
    pub const fn new(configuration: PathBuf) -> Self {
        Self {
            directory: configuration,
        }
    }

    /// Resolve the active installation without opening any workspace or starting a worker.
    pub fn discover() -> Result<Self, Diagnostic> {
        retold_engine::Paths::discover()
            .map(|paths| Self::new(paths.config))
            .map_err(storage::failed)
    }

    /// Read at most 32 checkpoints with an 8 MiB total inspection ceiling.
    pub fn list(&self) -> Result<Vec<Checkpoint>, Diagnostic> {
        Ok(storage::read(&self.directory)?.entries)
    }

    /// Save using the last acknowledged checkpoint version, or `None` for a new draft.
    pub fn put(
        &self,
        id: &str,
        expected: Option<&str>,
        draft: Draft,
    ) -> Result<Checkpoint, Diagnostic> {
        storage::put(
            &self.directory,
            storage::Update {
                id,
                expected,
                draft,
            },
        )
    }

    /// Remove only the acknowledged version; a newer draft is never discarded by stale cleanup.
    pub fn remove(&self, id: &str, expected: &str) -> Result<(), Diagnostic> {
        storage::remove(&self.directory, id, expected)
    }
}

/// Capture a draft's actual video identity. This does not mutate production metadata.
pub fn capture(root: &Path, reference: &str, content: Content) -> Result<Draft, Diagnostic> {
    let library = root.canonicalize().map_err(storage::failed)?;
    let catalog = crate::discovery::scan(&library)?;
    let video = catalog.resolve(reference)?;
    let draft = Draft {
        library,
        reference: video.reference.clone(),
        video_id: video.id.clone(),
        title: video.title.clone(),
        content,
    };
    storage::validate_draft(&draft)?;
    Ok(draft)
}

/// Restoration refuses a moved/replaced production; retained text remains available for recovery.
pub fn validate_target(draft: &Draft) -> Result<(), Diagnostic> {
    storage::validate_draft(draft)?;
    let current = capture(&draft.library, &draft.reference, draft.content.clone())?;
    if current.video_id != draft.video_id || current.library != draft.library {
        return Err(invalid(
            "This retained draft belongs to a different production. Its text was kept.",
        ));
    }
    Ok(())
}

fn invalid(message: &str) -> Diagnostic {
    Diagnostic::new(ErrorCode::InputRequired, message)
}
