//! Edition lines: per-language reviewed overrides with expected-version publication across clients.
use std::path::{Path, PathBuf};

use crate::protocol::{Diagnostic, ErrorCode};
use retold_engine::editing::dialogue;
use serde::{Deserialize, Serialize};

pub use dialogue::{DialogueSnapshot, Line, LineImpact, MAX_LINE_BYTES};

#[derive(Debug, Clone, Serialize)]
pub struct Document {
    pub video_id: String,
    pub reference: String,
    pub languages: Vec<String>,
    pub content: Option<DialogueSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Draft {
    pub expected_version: String,
    pub language: String,
    pub unit: usize,
    pub text: String,
}

impl Draft {
    pub fn validate(&self) -> Result<(), Diagnostic> {
        dialogue::validate_line(&self.edit()).map_err(|error| crate::editing::diagnostic(&error))
    }
    fn edit(&self) -> dialogue::LineEdit<'_> {
        dialogue::LineEdit {
            expected_version: &self.expected_version,
            language: &self.language,
            unit: self.unit,
            text: &self.text,
        }
    }
}

/// Opaque line review; grants neither approval nor provider dispatch.
#[derive(Debug, Clone, Serialize)]
pub struct ReviewedLine {
    library: PathBuf,
    video_id: String,
    reference: String,
    hash: String,
    pub draft: Draft,
    pub impact: LineImpact,
}

/// Read one edition; with no language, the first transcreated edition is shown.
pub fn document(
    root: &Path,
    reference: &str,
    language: Option<&str>,
) -> Result<Document, Diagnostic> {
    let catalog = crate::discovery::scan(root)?;
    let video = catalog.resolve(reference)?;
    let languages =
        dialogue::languages(&video.layout).map_err(|error| crate::editing::diagnostic(&error))?;
    let chosen = language
        .map(str::to_owned)
        .or_else(|| languages.first().cloned());
    let content = chosen
        .map(|language| dialogue::snapshot(&video.layout, &language))
        .transpose()
        .map_err(|error| crate::editing::diagnostic(&error))?
        .flatten();
    Ok(Document {
        video_id: video.id.clone(),
        reference: video.reference.clone(),
        languages,
        content,
    })
}

pub fn review(root: &Path, reference: &str, draft: Draft) -> Result<ReviewedLine, Diagnostic> {
    draft.validate()?;
    let library = root.canonicalize().map_err(invalid)?;
    let catalog = crate::discovery::scan(&library)?;
    let video = catalog.resolve(reference)?;
    let impact = dialogue::preview(&video.layout, draft.edit())
        .map_err(|error| crate::editing::diagnostic(&error))?;
    let hash = fingerprint(&library, video, &draft)?;
    Ok(ReviewedLine {
        library,
        video_id: video.id.clone(),
        reference: video.reference.clone(),
        hash,
        draft,
        impact,
    })
}

pub fn save(review: &ReviewedLine) -> Result<Document, Diagnostic> {
    let _activity = crate::account::activity("save reviewed edition line")
        .map_err(|error| crate::account::commands::diagnostic(&error))?;
    let library = review.library.canonicalize().map_err(invalid)?;
    let catalog = crate::discovery::scan(&library)?;
    let video = catalog.resolve(&review.reference)?;
    let same_identity = library == review.library && video.id == review.video_id;
    if !same_identity || fingerprint(&library, video, &review.draft)? != review.hash {
        return Err(Diagnostic::new(
            ErrorCode::PlanChanged,
            "Line review changed. Keep your text and review its impact again.",
        ));
    }
    dialogue::save_line(&video.layout, review.draft.edit())
        .map_err(|error| crate::editing::diagnostic(&error))?;
    document(&library, &review.reference, Some(&review.draft.language))
}

/// Approve one edition at the exact `(language, expected_version)` that was reviewed.
pub fn approve(
    root: &Path,
    reference: &str,
    (language, expected): (&str, &str),
) -> Result<Document, Diagnostic> {
    let _activity = crate::account::activity("approve edition lines")
        .map_err(|error| crate::account::commands::diagnostic(&error))?;
    let catalog = crate::discovery::scan(root)?;
    let video = catalog.resolve(reference)?;
    dialogue::approve(&video.layout, language, expected)
        .map_err(|error| crate::editing::diagnostic(&error))?;
    document(root, reference, Some(language))
}

fn fingerprint(
    library: &Path,
    video: &crate::library::Video,
    draft: &Draft,
) -> Result<String, Diagnostic> {
    retold_engine::store::hash(&(
        "line",
        library.as_os_str().as_encoded_bytes(),
        &video.id,
        &video.reference,
        draft,
    ))
    .map_err(invalid)
}

fn invalid(error: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::new(ErrorCode::DataInvalid, &format!("Edition lines: {error}"))
}
