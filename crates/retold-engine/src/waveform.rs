//! Read-only narration inspection: an edition's recorded audio, its waveform and line timings.
use std::path::Path;

use serde::Serialize;

pub use retold_providers::peaks::{Envelope, MAX_BUCKETS, MAX_ENVELOPE_BYTES};

use crate::{EngineError, RunLayout, Stage, fsx, languages::Edition, media_runtime::Asset};

const EDITIONS_BYTES: u64 = 16 * 1024 * 1024;

/// One spoken line positioned on the narration timeline, in seconds.
#[derive(Debug, Clone, Serialize)]
pub struct TimedLine {
    pub unit: usize,
    pub text: String,
    pub start: f64,
    pub end: f64,
}

/// A recorded edition: its narration asset and aligned lines; no audio is decoded yet.
#[derive(Debug, Clone, Serialize)]
pub struct Narration {
    pub edition: String,
    pub audio: Asset,
    pub duration: f64,
    pub lines: Vec<TimedLine>,
}

fn editions(layout: &RunLayout) -> Result<Vec<Edition>, EngineError> {
    let path = layout.stage(Stage::Captions).join("editions.json");
    let Some(bytes) = fsx::read_bounded(&path, EDITIONS_BYTES)? else {
        return Ok(Vec::new());
    };
    serde_json::from_slice(&bytes).map_err(|error| EngineError::Setup(error.to_string()))
}

fn timed(edition: &Edition) -> Vec<TimedLine> {
    edition
        .units
        .iter()
        .map(|unit| {
            let span = edition
                .alignment
                .units
                .iter()
                .find(|timed| timed.id == unit.id.to_string());
            TimedLine {
                unit: unit.id,
                text: unit.text.clone(),
                start: span.map_or(0.0, |span| span.start),
                end: span.map_or(0.0, |span| span.end),
            }
        })
        .collect()
}

/// Every recorded edition's narration, in saved order.
///
/// # Errors
/// Fails on unreadable or invalid edition records.
pub fn narrations(layout: &RunLayout) -> Result<Vec<Narration>, EngineError> {
    Ok(editions(layout)?
        .iter()
        .map(|edition| Narration {
            edition: crate::languages::edition_id(&edition.variant),
            audio: edition.audio.clone(),
            duration: edition.alignment.duration,
            lines: timed(edition),
        })
        .collect())
}

/// Decode bounded narration bytes whose content hash matches the recorded asset.
///
/// # Errors
/// Rejects missing, oversized, changed or undecodable audio.
pub fn envelope(audio: &Asset, path: &Path, buckets: usize) -> Result<Envelope, EngineError> {
    let limit = u64::try_from(MAX_ENVELOPE_BYTES).unwrap_or(u64::MAX);
    let bytes = fsx::read_bounded(path, limit)?
        .ok_or_else(|| EngineError::Setup("The narration audio is missing.".into()))?;
    let matches = blake3::hash(&bytes).to_hex().as_str() == audio.content_hash;
    if !matches {
        return Err(EngineError::Setup(
            "The narration audio no longer matches its recorded content hash.".into(),
        ));
    }
    retold_providers::peaks::envelope(&bytes, buckets)
        .map_err(|error| EngineError::Setup(error.to_string()))
}
