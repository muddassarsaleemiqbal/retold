//! Semantic scene direction, read-only impact review and expected-version publication across clients.
use std::path::{Path, PathBuf};

use crate::protocol::{Diagnostic, ErrorCode};
use retold_engine::editing::scenes;
use serde::{Deserialize, Serialize};

pub use crate::editing::{HistoryCursor, HistoryPage};
pub use scenes::{ChangeImpact, SceneSnapshot, ShotEdit};
pub use scenes::{MAX_ACTION_BYTES, MAX_LOCATION_BYTES, MAX_MOOD_BYTES};

#[derive(Debug, Clone, Serialize)]
pub struct Document {
    pub video_id: String,
    pub reference: String,
    pub title: String,
    pub content: Option<SceneSnapshot>,
}

#[derive(Debug, Clone, Copy)]
pub struct FrameSelection<'a> {
    pub unit: usize,
    pub expected_version: &'a str,
}

/// A frame lookup cannot pair a newly generated sequence with an older scene description.
pub fn frame(
    root: &Path,
    reference: &str,
    selection: FrameSelection<'_>,
) -> Result<Option<crate::desktop::media::Item>, Diagnostic> {
    require_frame_version(root, reference, selection.expected_version)?;
    let frame = crate::desktop::media::scene_frame(root, reference, selection.unit)?;
    require_frame_version(root, reference, selection.expected_version)?;
    Ok(frame)
}

fn require_frame_version(root: &Path, reference: &str, expected: &str) -> Result<(), Diagnostic> {
    let current = document(root, reference, Action::Read)?;
    let matches = current
        .content
        .as_ref()
        .is_some_and(|content| content.version == expected);
    if !matches {
        return Err(Diagnostic::new(
            ErrorCode::PlanChanged,
            "The scene sequence changed. Check the saved sequence before opening its frame.",
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
pub enum Action<'a> {
    Read,
    Approve(&'a str),
    Undo(&'a str),
    Redo(&'a str),
}

/// An owned local draft; its exact values are part of the reviewed save fingerprint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Draft {
    pub expected_version: String,
    pub unit: usize,
    pub action: String,
    pub location: String,
    pub mood: String,
}

impl Draft {
    pub fn validate_text(&self) -> Result<(), Diagnostic> {
        scenes::validate_edit(&self.edit()).map_err(|error| crate::editing::diagnostic(&error))
    }
    fn edit(&self) -> ShotEdit<'_> {
        ShotEdit {
            expected_version: &self.expected_version,
            unit: self.unit,
            action: &self.action,
            location: &self.location,
            mood: &self.mood,
        }
    }
}

/// Opaque local save review. It grants neither provider dispatch nor content approval.
#[derive(Debug, Clone, Serialize)]
pub struct ReviewedChange {
    library: PathBuf,
    video_id: String,
    reference: String,
    hash: String,
    pub draft: Draft,
    pub impact: ChangeImpact,
}

pub fn document(root: &Path, reference: &str, action: Action<'_>) -> Result<Document, Diagnostic> {
    let _activity = match action {
        Action::Read => Option::default(),
        _ => crate::account::activity("change saved scene sequence")
            .map_err(|error| crate::account::commands::diagnostic(&error))?,
    };
    let catalog = crate::discovery::scan(root)?;
    let video = catalog.resolve(reference)?;
    let content =
        apply(&video.layout, action).map_err(|error| crate::editing::diagnostic(&error))?;
    Ok(project(video, content))
}

fn apply(
    layout: &retold_engine::RunLayout,
    action: Action<'_>,
) -> Result<Option<SceneSnapshot>, retold_engine::editing::EditError> {
    match action {
        Action::Read => scenes::snapshot(layout),
        Action::Approve(version) => scenes::approve(layout, version).map(Some),
        Action::Undo(version) => scenes::undo(layout, version).map(Some),
        Action::Redo(version) => scenes::redo(layout, version).map(Some),
    }
}

fn project(video: &crate::library::Video, content: Option<SceneSnapshot>) -> Document {
    Document {
        video_id: video.id.clone(),
        reference: video.reference.clone(),
        title: video.title.clone(),
        content,
    }
}

/// Inspect exact affected scenes and visual descendants without acquiring a writer or queue.
pub fn review(root: &Path, reference: &str, draft: Draft) -> Result<ReviewedChange, Diagnostic> {
    draft.validate_text()?;
    let library = root.canonicalize().map_err(invalid)?;
    let catalog = crate::discovery::scan(&library)?;
    let video = catalog.resolve(reference)?;
    let impact = scenes::preview(&video.layout, draft.edit())
        .map_err(|error| crate::editing::diagnostic(&error))?;
    let hash = fingerprint(&library, video, &draft)?;
    Ok(ReviewedChange {
        library,
        video_id: video.id.clone(),
        reference: video.reference.clone(),
        hash,
        draft,
        impact,
    })
}

/// Publish only the values in the retained review, with both identity and revision revalidation.
pub fn save(review: &ReviewedChange) -> Result<Document, Diagnostic> {
    let _activity = crate::account::activity("save reviewed scene direction")
        .map_err(|error| crate::account::commands::diagnostic(&error))?;
    let library = review.library.canonicalize().map_err(invalid)?;
    let catalog = crate::discovery::scan(&library)?;
    let video = catalog.resolve(&review.reference)?;
    let same_identity = library == review.library && video.id == review.video_id;
    if !same_identity || fingerprint(&library, video, &review.draft)? != review.hash {
        return Err(Diagnostic::new(
            ErrorCode::PlanChanged,
            "Scene review changed. Keep your text and review its impact again.",
        ));
    }
    let content = scenes::save_shot(&video.layout, review.draft.edit())
        .map_err(|error| crate::editing::diagnostic(&error))?;
    Ok(project(video, Some(content)))
}

fn fingerprint(
    library: &Path,
    video: &crate::library::Video,
    draft: &Draft,
) -> Result<String, Diagnostic> {
    retold_engine::store::hash(&(
        library.as_os_str().as_encoded_bytes(),
        &video.id,
        &video.reference,
        draft,
    ))
    .map_err(invalid)
}

pub fn history(
    root: &Path,
    reference: &str,
    cursor: Option<&HistoryCursor>,
) -> Result<HistoryPage, Diagnostic> {
    let catalog = crate::discovery::scan(root)?;
    let video = catalog.resolve(reference)?;
    scenes::history(&video.layout, cursor).map_err(|error| crate::editing::diagnostic(&error))
}

fn invalid(error: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::new(ErrorCode::DataInvalid, &format!("Scene direction: {error}"))
}

pub use retold_engine::story::CastPose;
pub use scenes::{CastEdit, MAX_CAST, PROFILE_EXPRESSIONS};

/// An owned cast draft: who appears in one scene and the expression each one shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CastDraft {
    pub expected_version: String,
    pub unit: usize,
    pub cast: Vec<CastPose>,
}

impl CastDraft {
    pub fn validate(&self) -> Result<(), Diagnostic> {
        scenes::validate_cast(&self.edit()).map_err(|error| crate::editing::diagnostic(&error))
    }
    fn edit(&self) -> CastEdit<'_> {
        CastEdit {
            expected_version: &self.expected_version,
            unit: self.unit,
            cast: &self.cast,
        }
    }
}

/// Opaque cast review bound to library, video identity and the exact proposed cast.
#[derive(Debug, Clone, Serialize)]
pub struct ReviewedCast {
    library: PathBuf,
    video_id: String,
    reference: String,
    hash: String,
    pub draft: CastDraft,
    pub impact: ChangeImpact,
}

/// Read-only impact of recasting one scene; acquires no writer and creates no jobs.
pub fn review_cast(
    root: &Path,
    reference: &str,
    draft: CastDraft,
) -> Result<ReviewedCast, Diagnostic> {
    draft.validate()?;
    let library = root.canonicalize().map_err(invalid)?;
    let catalog = crate::discovery::scan(&library)?;
    let video = catalog.resolve(reference)?;
    let impact = scenes::preview_cast(&video.layout, draft.edit())
        .map_err(|error| crate::editing::diagnostic(&error))?;
    let hash = cast_fingerprint(&library, video, &draft)?;
    Ok(ReviewedCast {
        library,
        video_id: video.id.clone(),
        reference: video.reference.clone(),
        hash,
        draft,
        impact,
    })
}

/// Publish exactly the reviewed cast after revalidating identity and revision.
pub fn save_cast(review: &ReviewedCast) -> Result<Document, Diagnostic> {
    let _activity = crate::account::activity("save reviewed scene cast")
        .map_err(|error| crate::account::commands::diagnostic(&error))?;
    let library = review.library.canonicalize().map_err(invalid)?;
    let catalog = crate::discovery::scan(&library)?;
    let video = catalog.resolve(&review.reference)?;
    let same_identity = library == review.library && video.id == review.video_id;
    if !same_identity || cast_fingerprint(&library, video, &review.draft)? != review.hash {
        return Err(Diagnostic::new(
            ErrorCode::PlanChanged,
            "Cast review changed. Keep your selection and review its impact again.",
        ));
    }
    let content = scenes::save_cast(&video.layout, review.draft.edit())
        .map_err(|error| crate::editing::diagnostic(&error))?;
    Ok(project(video, Some(content)))
}

fn cast_fingerprint(
    library: &Path,
    video: &crate::library::Video,
    draft: &CastDraft,
) -> Result<String, Diagnostic> {
    retold_engine::store::hash(&(
        "cast",
        library.as_os_str().as_encoded_bytes(),
        &video.id,
        &video.reference,
        draft,
    ))
    .map_err(invalid)
}
