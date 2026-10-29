//! A person approves character sheets by content hash; production past the character gate
//! waits until every current sheet is approved, so a regenerated sheet needs a fresh look.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::media_runtime::Asset;
use crate::{EngineError, RunLayout, Stage, fsx};

mod inspection;
pub use inspection::{ReviewError, SheetInfo, SheetReview, approve_review, review, sheets};

/// The sheets offered for review, written when they are made.
const SHEETS_FILE: &str = "review.json";
/// Character name → the content hash a person approved.
const APPROVALS_FILE: &str = "approved.json";
const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;

type Approvals = BTreeMap<String, String>;

/// Which sheets to approve: all current ones, or the one saved at this path.
#[derive(Debug, Clone, Copy)]
pub enum Selection<'a> {
    All,
    Sheet(&'a Path),
}

/// Records approval of the selected current sheets; returns the approved character names.
///
/// # Errors
/// When no sheets have been made yet, the selected sheet is not one of them, or the
/// approval record cannot be written.
pub fn approve(layout: &RunLayout, selection: Selection<'_>) -> Result<Vec<String>, EngineError> {
    let sheets = read::<BTreeMap<String, Asset>>(&sheets_path(layout))?;
    let chosen: Vec<_> = sheets
        .iter()
        .filter(|(_, asset)| selection.includes(&asset.path))
        .collect();
    if chosen.is_empty() {
        return Err(EngineError::Setup(
            "No matching character sheet to approve. Continue the video to its character review first."
                .into(),
        ));
    }
    let mut approvals = read::<Approvals>(&approvals_path(layout))?;
    approvals.extend(
        chosen
            .iter()
            .map(|(name, asset)| ((*name).clone(), asset.content_hash.clone())),
    );
    fsx::write_json_atomic(&approvals_path(layout), &approvals)?;
    Ok(chosen.into_iter().map(|(name, _)| name.clone()).collect())
}

/// Whether every one of `sheets` is approved exactly as it is now.
///
/// # Errors
/// When the approval record exists but cannot be read.
pub fn approved(layout: &RunLayout, sheets: &BTreeMap<String, Asset>) -> Result<bool, EngineError> {
    let approvals = read::<Approvals>(&approvals_path(layout))?;
    Ok(sheets
        .iter()
        .all(|(name, asset)| approvals.get(name) == Some(&asset.content_hash)))
}

/// Whether `path` lies in the character sheets stage, where approval applies.
#[must_use]
pub fn is_sheet(layout: &RunLayout, path: &Path) -> bool {
    path.starts_with(layout.stage(Stage::Characters))
}

impl Selection<'_> {
    fn includes(self, sheet: &Path) -> bool {
        match self {
            Self::All => true,
            Self::Sheet(path) => same_file(path, sheet),
        }
    }
}

/// Saved paths may be recorded through a symlinked temporary directory.
fn same_file(left: &Path, right: &Path) -> bool {
    let canonical = |path: &Path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    canonical(left) == canonical(right)
}

fn sheets_path(layout: &RunLayout) -> PathBuf {
    layout.stage(Stage::Characters).join(SHEETS_FILE)
}

fn approvals_path(layout: &RunLayout) -> PathBuf {
    layout.stage(Stage::Characters).join(APPROVALS_FILE)
}

fn read<T: serde::de::DeserializeOwned + Default>(path: &Path) -> Result<T, EngineError> {
    let Some(bytes) = fsx::read_bounded(path, MAX_FILE_BYTES)? else {
        return Ok(T::default());
    };
    serde_json::from_slice(&bytes).map_err(|error| EngineError::Config {
        path: path.to_path_buf(),
        message: error.to_string(),
    })
}
