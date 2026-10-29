//! Read-only, bounded sheet inspection and approval of the exact pixels a person reviewed.
use std::collections::BTreeMap;

use super::{Approvals, MAX_FILE_BYTES, approvals_path, read, sheets_path};
use crate::{EngineError, RunLayout, fsx, media_runtime::Asset, store::Store};

const MAX_SHEETS: usize = 128;
const MAX_IMAGE_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct SheetInfo {
    pub name: String,
    pub content_hash: String,
    pub approved: bool,
}

#[derive(Debug, Clone)]
pub struct SheetReview {
    pub sheet: SheetInfo,
    pub bytes: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
pub enum ReviewError {
    #[error("The character sheet changed. Load the current sheet and review it before approving.")]
    Changed,
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Engine(#[from] EngineError),
}

/// Lists saved review records without creating metadata or reading every image.
pub fn sheets(layout: &RunLayout) -> Result<Vec<SheetInfo>, ReviewError> {
    let assets = assets(layout)?;
    let approvals = read::<Approvals>(&approvals_path(layout))?;
    Ok(assets
        .into_iter()
        .map(|(name, asset)| SheetInfo {
            approved: approvals.get(&name) == Some(&asset.content_hash),
            name,
            content_hash: asset.content_hash,
        })
        .collect())
}

/// Returns verified, bounded bytes so a frontend need not reopen a mutable path.
pub fn review(layout: &RunLayout, name: &str) -> Result<SheetReview, ReviewError> {
    let assets = assets(layout)?;
    let asset = assets.get(name).ok_or(ReviewError::Changed)?;
    let bytes = verified_bytes(layout, asset)?;
    let approvals = read::<Approvals>(&approvals_path(layout))?;
    Ok(SheetReview {
        sheet: SheetInfo {
            name: name.into(),
            content_hash: asset.content_hash.clone(),
            approved: approvals.get(name) == Some(&asset.content_hash),
        },
        bytes,
    })
}

fn verified_bytes(layout: &RunLayout, asset: &Asset) -> Result<Vec<u8>, ReviewError> {
    let root = layout
        .channel_dir()
        .canonicalize()
        .map_err(|error| EngineError::io(layout.channel_dir(), error))?;
    let path = asset
        .path
        .canonicalize()
        .map_err(|error| EngineError::io(&asset.path, error))?;
    if !path.starts_with(&root) {
        return Err(ReviewError::Invalid(
            "The sheet is outside this video's channel.".into(),
        ));
    }
    let bytes = fsx::read_bounded(&path, MAX_IMAGE_BYTES)?.ok_or(ReviewError::Changed)?;
    let content_hash = blake3::hash(&bytes).to_hex().to_string();
    if content_hash != asset.content_hash {
        return Err(ReviewError::Changed);
    }
    Ok(bytes)
}

/// Serializes with generation and approves only the displayed content, never a replacement.
pub fn approve_review(
    layout: &RunLayout,
    name: &str,
    expected_hash: &str,
) -> Result<SheetReview, ReviewError> {
    let _writer = Store::open(layout)?;
    let mut current = review(layout, name)?;
    if current.sheet.content_hash != expected_hash {
        return Err(ReviewError::Changed);
    }
    let mut approvals = read::<Approvals>(&approvals_path(layout))?;
    approvals.insert(name.into(), expected_hash.into());
    fsx::write_json_atomic(&approvals_path(layout), &approvals)?;
    current.sheet.approved = true;
    Ok(current)
}

fn assets(layout: &RunLayout) -> Result<BTreeMap<String, Asset>, ReviewError> {
    // The shared reader enforces MAX_FILE_BYTES even for externally edited records.
    let records = read::<BTreeMap<String, Asset>>(&sheets_path(layout))?;
    if records.len() > MAX_SHEETS {
        return Err(ReviewError::Invalid(format!(
            "Character review supports at most {MAX_SHEETS} sheets ({MAX_FILE_BYTES} record bytes)."
        )));
    }
    Ok(records)
}
