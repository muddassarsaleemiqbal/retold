//! Typed native projections of saved materials and licensed music/effects.
use std::path::Path;

use crate::assets::{self, AssetDetail, AssetSummary, AssetsQuerySpec};
use crate::audio::{self, AudioCategory, AudioItem};
use crate::protocol::{Diagnostic, ErrorCode, Payload};

/// A browsing page never needs more rows than this.
pub const MATERIAL_LIMIT: usize = 200;

fn contents(payload: Payload) -> Result<serde_json::Value, Diagnostic> {
    match payload {
        Payload::Result { contents, .. } => Ok(contents),
        _ => Err(invalid("unexpected response")),
    }
}

fn field<T: serde::de::DeserializeOwned>(
    value: &serde_json::Value,
    name: &str,
) -> Result<T, Diagnostic> {
    serde_json::from_value(value.get(name).cloned().unwrap_or_default()).map_err(invalid)
}

/// Saved materials of one video, at most [`MATERIAL_LIMIT`].
pub fn list(root: &Path, video: &str) -> Result<Vec<AssetSummary>, Diagnostic> {
    let spec = AssetsQuerySpec {
        video: Some(video),
        ..AssetsQuerySpec::default()
    };
    let mut items: Vec<AssetSummary> =
        field(&contents(assets::list_assets(root, &spec)?)?, "items")?;
    items.truncate(MATERIAL_LIMIT);
    Ok(items)
}

/// Provenance, take and every usage that a change to this material would affect.
pub fn detail(root: &Path, asset: &str) -> Result<AssetDetail, Diagnostic> {
    field(&contents(assets::show_asset(root, asset)?)?, "asset")
}

/// Re-hash the saved file against its recorded checksum.
pub fn verify(root: &Path, asset: &str) -> Result<bool, Diagnostic> {
    match assets::verify_asset(root, asset) {
        Ok(payload) => field(&contents(payload)?, "verified"),
        Err(error) if error.code == ErrorCode::DataInvalid => Ok(false),
        Err(error) => Err(error),
    }
}

/// Imported music and sound effects with licence and verification state.
pub fn audio(root: &Path) -> Result<Vec<AudioItem>, Diagnostic> {
    let mut items = Vec::new();
    for category in [AudioCategory::Music, AudioCategory::Sfx] {
        let found: Vec<AudioItem> = field(&contents(audio::list(root, category)?)?, "items")?;
        items.extend(found);
    }
    Ok(items)
}

/// What the user declares about an audio file they import: its category and licence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioLicenceDeclaration {
    pub category: AudioCategory,
    pub licence: String,
    pub commercial_use: bool,
}

/// Copy a licensed music or effect file into the library with its declared licence.
pub fn import_audio(
    root: &Path,
    source: &Path,
    declared: &AudioLicenceDeclaration,
) -> Result<Vec<AudioItem>, Diagnostic> {
    let licence = declared.licence.trim();
    if licence.is_empty() {
        return Err(Diagnostic::new(
            ErrorCode::InputRequired,
            "Name the licence, such as CC-BY-4.0 or Purchased.",
        ));
    }
    let spec = audio::AudioImportSpec {
        category: declared.category,
        source_path: source,
        title: Option::default(),
        license_type: Some(licence),
        attribution: Option::default(),
        commercial_use: declared.commercial_use,
        tags: Vec::new(),
    };
    audio::import(root, &spec)?;
    self::audio(root)
}

fn invalid(error: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::new(ErrorCode::DataInvalid, &format!("Materials: {error}"))
}
