//! Native video lifecycle: archive, restore from archive, copy and move to trash.
//! Every change refuses while generation owns the video; permanent deletion stays explicit CLI work.
use std::path::Path;

use crate::library::{self, Query};
use crate::protocol::{Diagnostic, ErrorCode, Payload};
use crate::video::{CopySpec, DeleteMode};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Archive,
    Unarchive,
    Copy { title: String },
    Trash,
    AddTag(String),
    RemoveTag(String),
}

/// What a change did, in words, plus the new reference for a copy.
#[derive(Debug, Clone)]
pub struct Outcome {
    pub text: String,
    pub created: Option<String>,
}

fn require_idle(root: &Path, reference: &str) -> Result<(), Diagnostic> {
    let catalog = crate::discovery::scan(root)?;
    let video = catalog.resolve(reference)?;
    retold_engine::store::Store::open(&video.layout)
        .map(drop)
        .map_err(|_| {
            Diagnostic::new(
                ErrorCode::InputRequired,
                "This video is being generated or repaired. Stop its work before changing it.",
            )
        })
}

fn contents(payload: Payload) -> Result<serde_json::Value, Diagnostic> {
    match payload {
        Payload::Result { contents, .. } => Ok(contents),
        Payload::Error { diagnostic, .. } => Err(diagnostic),
        Payload::Event(_) => Err(Diagnostic::new(
            ErrorCode::DataInvalid,
            "Unexpected lifecycle response.",
        )),
    }
}

const fn summary(change: &Change) -> &'static str {
    match change {
        Change::Archive => "Archived. It stays in the library and can be restored.",
        Change::Unarchive => "Restored from the archive.",
        Change::Copy { .. } => "Copied as a new video.",
        Change::Trash => "Moved to the workspace trash.",
        Change::AddTag(_) => "Tag added.",
        Change::RemoveTag(_) => "Tag removed.",
    }
}

fn describe(change: &Change, value: &serde_json::Value) -> Outcome {
    let created = value.get("new_id").and_then(serde_json::Value::as_str);
    Outcome {
        text: summary(change).into(),
        created: created.map(str::to_owned),
    }
}

/// Apply one reviewed lifecycle change.
pub fn apply(root: &Path, reference: &str, change: &Change) -> Result<Outcome, Diagnostic> {
    let _activity = crate::account::activity("change a video in the library")
        .map_err(|error| crate::account::commands::diagnostic(&error))?;
    require_idle(root, reference)?;
    let query = match change {
        Change::Archive => Query::Archive(reference),
        Change::Unarchive => Query::Unarchive(reference),
        Change::Copy { title } => Query::Copy(
            reference,
            CopySpec {
                title: Some(title),
                lang: None,
            },
        ),
        Change::Trash => Query::Delete(reference, DeleteMode::Trash),
        Change::AddTag(tag) => Query::Tag(reference, crate::library::TagOperation::Add(tag)),
        Change::RemoveTag(tag) => Query::Tag(reference, crate::library::TagOperation::Remove(tag)),
    };
    let value = contents(library::query(root, query))?;
    Ok(describe(change, &value))
}
