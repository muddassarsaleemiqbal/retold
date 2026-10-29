//! Typed character review uses the engine's saved records and exact-content approval.
use std::path::Path;

use crate::protocol::{Diagnostic, ErrorCode};
use retold_engine::character_approval::{self, ReviewError};

pub use character_approval::{SheetInfo, SheetReview};

#[derive(Debug, Clone)]
pub struct CharacterDocument {
    pub reference: String,
    pub title: String,
    pub sheets: Vec<SheetInfo>,
    pub selected: Option<SheetReview>,
}

#[derive(Debug, Clone, Copy)]
pub enum Action<'a> {
    Read(Option<&'a str>),
    Approve {
        name: &'a str,
        expected_hash: &'a str,
    },
}

pub fn document(
    root: &Path,
    reference: &str,
    action: Action<'_>,
) -> Result<CharacterDocument, Diagnostic> {
    let _owner = match action {
        Action::Read(_) => None,
        Action::Approve { .. } => crate::account::activity("approve reviewed character sheet")
            .map_err(|error| crate::account::commands::diagnostic(&error))?,
    };
    let catalog = crate::discovery::scan(root)?;
    let video = catalog.resolve(reference)?;
    let selected = apply(&video.layout, action).map_err(|error| diagnostic(&error))?;
    Ok(CharacterDocument {
        reference: video.reference.clone(),
        title: video.title.clone(),
        sheets: character_approval::sheets(&video.layout).map_err(|error| diagnostic(&error))?,
        selected,
    })
}

fn apply(
    layout: &retold_engine::RunLayout,
    action: Action<'_>,
) -> Result<Option<SheetReview>, ReviewError> {
    let selected = match action {
        Action::Read(Some(name)) => Some(character_approval::review(layout, name)),
        Action::Read(None) => {
            let sheets = character_approval::sheets(layout)?;
            sheets
                .first()
                .map(|sheet| character_approval::review(layout, &sheet.name))
        }
        Action::Approve {
            name,
            expected_hash,
        } => Some(character_approval::approve_review(
            layout,
            name,
            expected_hash,
        )),
    };
    selected.transpose()
}

fn diagnostic(error: &ReviewError) -> Diagnostic {
    let code = match error {
        ReviewError::Changed => ErrorCode::PlanChanged,
        ReviewError::Invalid(_) => ErrorCode::InputRequired,
        ReviewError::Engine(_) => ErrorCode::DataInvalid,
    };
    Diagnostic::new(code, &error.to_string())
}
