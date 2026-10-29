//! Bounded, read-only saved-media pages for a single production and edition.
mod scan;
use std::path::{Path, PathBuf};

use crate::protocol::{Diagnostic, ErrorCode};

/// One page contains at most 48 media rows; scans inspect at most 4096 entries.
pub const PAGE_SIZE: usize = 48;
/// Image previews allow at most 32 MiB of compressed input.
const IMAGE_BYTES: u64 = 32 * 1024 * 1024;
/// Caption and editor previews allow at most 128 KiB of text.
const TEXT_BYTES: u64 = 128 * 1024;
const IMAGE_MANIFEST_BYTES: u64 = 2 * 1024 * 1024;

/// Supported preview categories; editor projects and captions remain downloadable files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Image,
    Audio,
    Video,
    Captions,
    Project,
}

impl Kind {
    /// Human-readable media type.
    #[must_use]
    pub const fn label(self) -> &'static str {
        ["Image", "Audio", "Video", "Captions", "Editor project"][self as usize]
    }
}

/// A regular saved file, scoped to its owning production.
#[derive(Debug, Clone)]
pub struct Item {
    pub relative: String,
    pub path: PathBuf,
    pub kind: Kind,
    pub bytes: u64,
    pub edition: Option<String>,
    identity: Identity,
    expected_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Identity {
    bytes: u64,
    modified: std::time::SystemTime,
    native: Option<(u64, u64)>,
}

fn identity(metadata: &std::fs::Metadata) -> Result<Identity, Diagnostic> {
    Ok(Identity {
        bytes: metadata.len(),
        modified: metadata.modified().map_err(invalid)?,
        native: native_identity(metadata),
    })
}

#[cfg(unix)]
fn native_identity(metadata: &std::fs::Metadata) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt as _;
    metadata
        .is_file()
        .then_some((metadata.dev(), metadata.ino()))
}

#[cfg(not(unix))]
fn native_identity(_: &std::fs::Metadata) -> Option<(u64, u64)> {
    None
}

impl Item {
    /// Resolve this selection only while its saved file identity remains unchanged.
    ///
    /// # Errors
    /// Rejects changed size/time, symlinks, moved files and nonregular files.
    pub fn validated_path(&self) -> Result<PathBuf, Diagnostic> {
        let metadata = std::fs::symlink_metadata(&self.path).map_err(invalid)?;
        let canonical = self.path.canonicalize().map_err(invalid)?;
        let observed = identity(&metadata)?;
        let unchanged = metadata.is_file() && observed == self.identity && canonical == self.path;
        if unchanged {
            return Ok(canonical);
        }
        Err(Diagnostic::new(
            ErrorCode::PlanChanged,
            "The selected saved file changed. Refresh saved media before opening it.",
        ))
    }

    /// Read a bounded image or text artifact for background preview decoding.
    ///
    /// # Errors
    /// Rejects missing, oversized and nonregular files, or a playable-stream category.
    pub fn preview_bytes(&self) -> Result<Vec<u8>, Diagnostic> {
        let path = self.validated_path()?;
        let limit = match self.kind {
            Kind::Image => IMAGE_BYTES,
            Kind::Captions | Kind::Project => TEXT_BYTES,
            _ => {
                return Err(Diagnostic::new(
                    ErrorCode::InputRequired,
                    "Audio and video use the media decoder.",
                ));
            }
        };
        let bytes = retold_engine::fsx::read_bounded(&path, limit)
            .map_err(invalid)?
            .ok_or_else(|| invalid("Saved artifact is missing"))?;
        self.validated_path()?;
        if let Some(expected) = &self.expected_hash {
            let digest = blake3::hash(&bytes).to_hex().to_string();
            if digest != *expected {
                return Err(invalid(
                    "The saved frame no longer matches its recorded content hash. Refresh scene review.",
                ));
            }
        }
        Ok(bytes)
    }
}

/// Stable, sorted rows from a bounded scan; partial scans are always disclosed.
#[derive(Debug, Clone)]
pub struct Page {
    pub reference: String,
    pub title: String,
    pub editions: Vec<String>,
    pub items: Vec<Item>,
    pub offset: usize,
    pub next_offset: Option<usize>,
    pub total: usize,
    pub partial: bool,
}

/// Filters and pagination for a bounded artifact scan.
#[derive(Debug, Clone, Default)]
pub struct Query {
    pub edition: Option<String>,
    pub offset: usize,
}

impl Query {
    fn accepts(&self, item: &Item) -> bool {
        self.edition.as_deref().is_none_or(|selected| {
            item.edition
                .as_deref()
                .is_none_or(|value| value == selected)
        })
    }

    fn validate(&self, editions: &[String]) -> Result<(), Diagnostic> {
        let valid = self
            .edition
            .as_ref()
            .is_none_or(|selected| editions.contains(selected));
        if valid {
            return Ok(());
        }
        Err(Diagnostic::new(
            ErrorCode::InputRequired,
            "Choose an edition saved in this video.",
        ))
    }
}

/// List saved artifacts without opening the queue, decoding files, or hashing the library.
///
/// # Errors
/// Rejects unknown productions, unsupported editions and unreadable production roots.
pub fn page(root: &Path, reference: &str, query: &Query) -> Result<Page, Diagnostic> {
    let catalog = crate::discovery::scan(root)?;
    let video = catalog.resolve(reference)?;
    query.validate(&video.editions)?;
    let (mut items, partial) = scan::collect(video.layout.video_dir(), &video.editions)?;
    items.retain(|item| query.accepts(item));
    items.sort_by(|left, right| left.relative.cmp(&right.relative));
    Ok(paginate(video, (items, partial), query.offset))
}

/// Resolve the frame actually recorded for a scene, including a panel or local recut.
pub fn scene_frame(root: &Path, reference: &str, unit: usize) -> Result<Option<Item>, Diagnostic> {
    let catalog = crate::discovery::scan(root)?;
    let video = catalog.resolve(reference)?;
    let path = video
        .layout
        .stage(retold_engine::Stage::Images)
        .join("images.json");
    let Some(bytes) =
        retold_engine::fsx::read_bounded(&path, IMAGE_MANIFEST_BYTES).map_err(invalid)?
    else {
        return Ok(Option::default());
    };
    let images: retold_engine::assets::Images = serde_json::from_slice(&bytes).map_err(invalid)?;
    let Some(frame) = images.frames.into_iter().find(|frame| frame.unit == unit) else {
        return Ok(Option::default());
    };
    let item = recorded_asset(video, &frame.image, Kind::Image)?;
    let unchanged = retold_engine::fsx::read_bounded(&path, IMAGE_MANIFEST_BYTES)
        .map_err(invalid)?
        == Some(bytes);
    if !unchanged {
        return Err(invalid(
            "Saved scene frames changed during inspection. Refresh scene review.",
        ));
    }
    Ok(Some(item))
}

/// A recorded asset contained by its production, bound to its recorded content hash.
pub(crate) fn recorded_asset(
    video: &crate::library::Video,
    asset: &retold_engine::media_runtime::Asset,
    kind: Kind,
) -> Result<Item, Diagnostic> {
    let metadata = std::fs::symlink_metadata(&asset.path).map_err(invalid)?;
    let root = video.layout.video_dir().canonicalize().map_err(invalid)?;
    let path = asset.path.canonicalize().map_err(invalid)?;
    let contained = path.starts_with(&root) && metadata.is_file();
    if !contained {
        return Err(invalid(
            "The saved media does not belong to this production. It was not opened.",
        ));
    }
    let relative = path
        .strip_prefix(&root)
        .ok()
        .and_then(Path::to_str)
        .ok_or_else(|| invalid("The media path is not supported by native review."))?;
    let identity = identity(&metadata)?;
    Ok(Item {
        relative: relative.into(),
        path,
        kind,
        bytes: identity.bytes,
        edition: None,
        identity,
        expected_hash: Some(asset.content_hash.clone()),
    })
}

fn paginate(video: &crate::library::Video, collected: (Vec<Item>, bool), offset: usize) -> Page {
    let (items, partial) = collected;
    let total = items.len();
    let offset = offset.min(total.saturating_sub(1) / PAGE_SIZE * PAGE_SIZE);
    let next = offset.saturating_add(PAGE_SIZE);
    Page {
        reference: video.reference.clone(),
        title: video.title.clone(),
        editions: video.editions.clone(),
        items: items.into_iter().skip(offset).take(PAGE_SIZE).collect(),
        offset,
        next_offset: (next < total).then_some(next),
        total,
        partial,
    }
}

fn invalid(error: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::new(
        ErrorCode::DataInvalid,
        &format!("Saved media could not be read: {error}"),
    )
}
