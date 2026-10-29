//! Bounded source review freezes exactly the text selected for an offline draft.
use std::io::Write;
use std::path::{Path, PathBuf};

use super::{CreatedDraft, MAX_SOURCE_BYTES, Options, input_error};
use crate::protocol::Diagnostic;

/// Read-only source text shown before saving a draft.
#[derive(Debug, Clone)]
pub struct SourcePreview {
    pub path: PathBuf,
    pub text: String,
    pub bytes: usize,
    pub truncated: bool,
    digest: String,
}

const PREVIEW_CHARACTERS: usize = 12_000;

/// Inspect a bounded UTF-8 source without creating a video or changing the source.
///
/// # Errors
/// Rejects missing, empty, nonregular, oversized and non-UTF-8 sources.
pub fn preview_source(path: &Path) -> Result<SourcePreview, Diagnostic> {
    let source = read_source(path)?;
    Ok(summarize(path, &source))
}

fn summarize(path: &Path, source: &str) -> SourcePreview {
    SourcePreview {
        path: path.to_owned(),
        text: excerpt(source),
        bytes: source.len(),
        truncated: shortened(source),
        digest: digest(source),
    }
}

fn digest(source: &str) -> String {
    blake3::hash(source.as_bytes()).to_hex().to_string()
}

/// Save only the reviewed source, using a private snapshot during engine publication.
///
/// # Errors
/// Rejects changed source choices or bytes and propagates ordinary draft validation failures.
pub fn save_draft_from_source(
    root: &Path,
    options: &Options,
    preview: &SourcePreview,
) -> Result<CreatedDraft, Diagnostic> {
    let selected = options.source_file.as_deref();
    let reviewed = Some(preview.path.as_path());
    if selected != reviewed {
        return Err(input_error(
            "The source selection changed. Preview it again before saving.",
        ));
    }
    let source = read_source(&preview.path)?;
    let digest = blake3::hash(source.as_bytes()).to_hex().to_string();
    if digest != preview.digest {
        return Err(input_error(
            "The source file changed. Preview its current text before saving.",
        ));
    }
    save_snapshot(root, options, &source)
}

fn save_snapshot(root: &Path, options: &Options, source: &str) -> Result<CreatedDraft, Diagnostic> {
    let mut captured = tempfile::NamedTempFile::new().map_err(super::engine_error)?;
    captured
        .write_all(source.as_bytes())
        .map_err(super::engine_error)?;
    captured.as_file().sync_all().map_err(super::engine_error)?;
    let options = Options {
        source_file: Some(captured.path().to_owned()),
        ..options.clone()
    };
    super::save_draft(root, &options)
}

pub(super) fn read_source(path: &Path) -> Result<String, Diagnostic> {
    let bytes = retold_engine::fsx::read_bounded(path, MAX_SOURCE_BYTES)
        .map_err(|_| input_error("Source must be a readable regular UTF-8 file of at most 2 MB."))?
        .ok_or_else(|| input_error("Source file does not exist."))?;
    let text = String::from_utf8(bytes).map_err(|_| input_error("Source file must be UTF-8."))?;
    let empty = text.trim().is_empty();
    if empty {
        return Err(input_error("Source file must not be empty."));
    }
    Ok(text)
}

fn excerpt(source: &str) -> String {
    source.chars().take(PREVIEW_CHARACTERS).collect()
}

fn shortened(source: &str) -> bool {
    source.chars().count() > PREVIEW_CHARACTERS
}
