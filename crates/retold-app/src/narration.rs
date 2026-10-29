//! Edition narration review: recorded audio, a bounded waveform and aligned line timings.
use std::path::Path;

use crate::desktop::media::{Item, Kind};
use crate::protocol::{Diagnostic, ErrorCode};
use retold_engine::waveform;
use serde::Serialize;

pub use waveform::{Envelope, MAX_BUCKETS, TimedLine};

#[derive(Debug, Clone, Serialize)]
pub struct Edition {
    pub edition: String,
    pub duration: f64,
    pub lines: Vec<TimedLine>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Document {
    pub video_id: String,
    pub reference: String,
    pub editions: Vec<Edition>,
}

/// A decoded waveform paired with the exact audio item it describes.
#[derive(Debug, Clone)]
pub struct Waveform {
    pub edition: String,
    pub item: Item,
    pub envelope: Envelope,
}

/// Every recorded edition of one video; reads only, decodes nothing.
pub fn document(root: &Path, reference: &str) -> Result<Document, Diagnostic> {
    let catalog = crate::discovery::scan(root)?;
    let video = catalog.resolve(reference)?;
    let editions = waveform::narrations(&video.layout)
        .map_err(invalid)?
        .into_iter()
        .map(|narration| Edition {
            edition: narration.edition,
            duration: narration.duration,
            lines: narration.lines,
        })
        .collect();
    Ok(Document {
        video_id: video.id.clone(),
        reference: video.reference.clone(),
        editions,
    })
}

/// Decode one edition's narration into at most `buckets` peaks after containment and hash checks.
pub fn waveform(
    root: &Path,
    reference: &str,
    (edition, buckets): (&str, usize),
) -> Result<Waveform, Diagnostic> {
    let catalog = crate::discovery::scan(root)?;
    let video = catalog.resolve(reference)?;
    let narration = waveform::narrations(&video.layout)
        .map_err(invalid)?
        .into_iter()
        .find(|narration| narration.edition == edition)
        .ok_or_else(|| Diagnostic::new(ErrorCode::InputRequired, "Choose a recorded edition."))?;
    let item = crate::desktop::media::recorded_asset(video, &narration.audio, Kind::Audio)?;
    let path = item.validated_path()?;
    let envelope = waveform::envelope(&narration.audio, &path, buckets).map_err(invalid)?;
    item.validated_path()?;
    Ok(Waveform {
        edition: edition.into(),
        item,
        envelope,
    })
}

fn invalid(error: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::new(ErrorCode::DataInvalid, &format!("Narration: {error}"))
}
