//! Actual content edits share engine locks, expected versions and immutable revisions across clients.
use std::path::Path;

use crate::protocol::{Diagnostic, ErrorCode};
use retold_engine::editing::{self, EditError};
use serde::Serialize;

pub use editing::{HistoryCursor, HistoryPage, ScriptSnapshot, SectionEdit};

#[derive(Debug, Clone, Serialize)]
pub struct ScriptDocument {
    pub video_id: String,
    pub reference: String,
    pub title: String,
    pub content: Option<ScriptSnapshot>,
    pub affected_stages: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub enum Action<'a> {
    Read,
    Save(SectionEdit<'a>),
    Approve(&'a str),
    Undo(&'a str),
    Redo(&'a str),
}

pub fn script(
    root: &Path,
    reference: &str,
    action: Action<'_>,
) -> Result<ScriptDocument, Diagnostic> {
    let _owner = match action {
        Action::Read => Option::default(),
        _ => crate::account::activity("edit saved script")
            .map_err(|error| crate::account::commands::diagnostic(&error))?,
    };
    let catalog = crate::discovery::scan(root)?;
    let video = catalog.resolve(reference)?;
    let content = apply(&video.layout, action).map_err(|error| diagnostic(&error))?;
    Ok(ScriptDocument {
        video_id: video.id.clone(),
        reference: video.reference.clone(),
        title: video.title.clone(),
        content,
        affected_stages: affected_stages(),
    })
}

fn affected_stages() -> Vec<String> {
    let mut affected = retold_engine::graph::Graph::stages().affected("script");
    affected.remove("script");
    affected.into_iter().collect()
}

fn apply(
    layout: &retold_engine::RunLayout,
    action: Action<'_>,
) -> Result<Option<ScriptSnapshot>, EditError> {
    match action {
        Action::Read => editing::script(layout),
        Action::Save(edit) => editing::save_section(layout, edit).map(Some),
        Action::Approve(expected) => editing::approve(layout, expected).map(Some),
        Action::Undo(expected) => editing::undo(layout, expected).map(Some),
        Action::Redo(expected) => editing::redo(layout, expected).map(Some),
    }
}

pub fn history(
    root: &Path,
    reference: &str,
    cursor: Option<&HistoryCursor>,
) -> Result<HistoryPage, Diagnostic> {
    let catalog = crate::discovery::scan(root)?;
    let video = catalog.resolve(reference)?;
    editing::history(&video.layout, cursor).map_err(|error| diagnostic(&error))
}

pub(crate) fn diagnostic(error: &EditError) -> Diagnostic {
    let code = match error {
        EditError::Changed
        | EditError::SourceChanged
        | EditError::SceneChanged
        | EditError::SceneSourceChanged => ErrorCode::PlanChanged,
        EditError::Unapproved | EditError::SceneUnapproved => ErrorCode::ReviewRequired,
        EditError::Invalid(_) => ErrorCode::InputRequired,
        EditError::Integrity(_) | EditError::Engine(_) => ErrorCode::DataInvalid,
    };
    Diagnostic::new(code, &error.to_string())
}
