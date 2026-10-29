//! Typed, presentation-free workspace snapshots. Opening never starts workers or repairs data.
pub mod account_admin;
pub mod admin;
pub mod authoring;
pub mod backups;
pub mod budget;
pub mod characters;
mod choices;
pub mod destinations;
pub mod lifecycle;
pub mod materials;
pub mod media;
pub mod onboarding;
pub mod preferences;
pub mod providers;
pub mod readiness;
pub mod recovery;
mod run;
pub mod series;
pub mod views;
pub use choices::{DraftChoices, DraftField, GuideChoice, draft_choices};
pub use run::{
    DeliveryEdition, ExportRequest, ReviewedExport, ReviewedRun, delivery_editions, review_export,
    review_run,
};

use std::path::{Path, PathBuf};

use crate::library::{CatalogIssue, Video};
use crate::protocol::{Diagnostic, ErrorCode};
use crate::queue::{DispatchState, JobPage, ListOptions, QueueStore};
use crate::workspace::context::Context;

pub const VIDEO_PAGE_LIMIT: usize = 200;

/// Human-facing edition labels share the language parser used by generation and captions.
#[must_use]
pub fn edition_label(specification: &str) -> String {
    let Ok(variant) = specification.parse::<retold_engine::Variant>() else {
        return specification.into();
    };
    let narration = variant.narration.display_name();
    let captions: Vec<_> = variant
        .captions
        .iter()
        .filter(|track| track.lang != variant.narration)
        .map(|track| track.lang.display_name())
        .collect();
    if captions.is_empty() {
        narration
    } else {
        format!("{narration}; {} captions", captions.join(", "))
    }
}

#[derive(Debug, Clone)]
pub struct WorkspaceSnapshot {
    pub context: Context,
    pub videos: Vec<Video>,
    pub total_videos: usize,
    pub issues: Vec<CatalogIssue>,
    pub activity: Result<ActivitySnapshot, Diagnostic>,
    pub choices: Result<DraftChoices, Diagnostic>,
}

#[derive(Debug, Clone)]
pub struct ActivitySnapshot {
    pub state_directory: PathBuf,
    pub dispatch: DispatchState,
    pub page: JobPage,
    pub worker: crate::studio::StudioStatus,
}

pub fn open_workspace(root: &Path) -> Result<WorkspaceSnapshot, Diagnostic> {
    let root = root.canonicalize().map_err(|_| invalid_root())?;
    if !root.is_dir() {
        return Err(invalid_root());
    }
    let context = creation_context(&root)?;
    let mut catalog = crate::discovery::scan(&context.library)?;
    catalog.videos.sort_by(|left, right| {
        right
            .created
            .cmp(&left.created)
            .then_with(|| left.id.cmp(&right.id))
    });
    let total_videos = catalog.videos.len();
    catalog.videos.truncate(VIDEO_PAGE_LIMIT);
    Ok(WorkspaceSnapshot {
        activity: activity(&context.library, &ListOptions::default()),
        choices: draft_choices(&context.config),
        context,
        videos: catalog.videos,
        total_videos,
        issues: catalog.issues,
    })
}

fn creation_context(root: &Path) -> Result<Context, Diagnostic> {
    let mut context = Context::resolve(Some(root))?;
    let paths =
        retold_engine::paths::workspace::for_library(&context.library).map_err(snapshot_error)?;
    context.config = paths.config;
    Ok(context)
}

pub fn activity(library: &Path, options: &ListOptions) -> Result<ActivitySnapshot, Diagnostic> {
    let state_directory = queue_state(library)?;
    let database = state_directory.join(crate::studio::server::QUEUE_DB_NAME);
    let exists = database.try_exists().map_err(snapshot_error)?;
    if !exists {
        return Ok(ActivitySnapshot {
            worker: crate::studio::query_status(&state_directory),
            state_directory,
            dispatch: DispatchState::default(),
            page: JobPage {
                jobs: Vec::new(),
                total: 0,
                next_cursor: None,
            },
        });
    }
    let queue = QueueStore::open_read_only(&database).map_err(snapshot_error)?;
    Ok(ActivitySnapshot {
        worker: crate::studio::query_status(&state_directory),
        state_directory,
        dispatch: queue.dispatch_state().map_err(snapshot_error)?,
        page: queue.list_page(options).map_err(snapshot_error)?,
    })
}

/// Native workspace controls cannot silently target a shared `RETOLD_STATE` override.
pub fn queue_state(library: &Path) -> Result<PathBuf, Diagnostic> {
    let resolved = crate::studio::resolve_workspace_state(Some(library));
    let expected = library.join(".retold/studio");
    let resolved = canonical_queue_path(&resolved)?;
    let expected = canonical_queue_path(&expected)?;
    let library = canonical_queue_path(library)?;
    let private = expected.starts_with(&library);
    if resolved != expected || !private {
        return Err(Diagnostic::new(
            ErrorCode::InputRequired,
            "RETOLD_STATE points outside this workspace's private queue. Unset the override before using native queue or run controls.",
        ));
    }
    Ok(resolved)
}

fn canonical_queue_path(path: &Path) -> Result<PathBuf, Diagnostic> {
    match path.canonicalize() {
        Ok(path) => Ok(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let parent = path.parent().ok_or_else(invalid_root)?;
            let name = path.file_name().ok_or_else(invalid_root)?;
            Ok(canonical_queue_path(parent)?.join(name))
        }
        Err(error) => Err(snapshot_error(error)),
    }
}

fn invalid_root() -> Diagnostic {
    Diagnostic::new(
        ErrorCode::InputRequired,
        "Choose an existing, readable workspace directory.",
    )
}

fn snapshot_error(error: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::new(ErrorCode::DataInvalid, &error.to_string())
}
