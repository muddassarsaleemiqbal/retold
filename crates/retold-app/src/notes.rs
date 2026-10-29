//! Review comments are append-only, revision checked and independent of production approval.
mod storage;
mod target;
pub use target::{Subject, Target};

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::library::Video;
use crate::protocol::{Diagnostic, ErrorCode};
pub use crate::review::ReviewNote;

/// Comment pages remain small even when a video has a long review history.
pub const PAGE_SIZE: usize = 64;
const MAX_TEXT_BYTES: usize = 8192;
const MAX_TARGET_BYTES: usize = 512;
const ID_LENGTH: usize = 64;
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// A context filter and page position; an absent target includes all review comments.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    pub target: Option<String>,
    pub offset: usize,
}

/// Read-only comments include the exact version needed for an append.
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
pub struct Document {
    pub video_id: String,
    pub reference: String,
    pub title: String,
    pub version: String,
    pub notes: Vec<ReviewNote>,
    pub total: usize,
    pub offset: usize,
    pub next_offset: Option<usize>,
}

/// The stable identifier makes a retry safe after an acknowledged write loses its response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Append<'a> {
    pub expected_version: &'a str,
    pub id: &'a str,
    pub item: &'a str,
    pub text: &'a str,
}

/// The durable saved comment and refreshed page acknowledge the same append identifier.
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
pub struct Saved {
    pub note: ReviewNote,
    pub document: Document,
}

/// Historical comments remain readable when the artifact is replaced or unavailable.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ContextualDocument {
    pub document: Document,
    pub target: Target,
    pub current: Option<Target>,
    pub problem: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct ContextQuery<'a> {
    pub target: &'a Target,
    pub offset: usize,
}
#[derive(Debug, Clone, Copy)]
pub struct ContextAppend<'a> {
    pub target: &'a Target,
    pub request: Append<'a>,
}
#[derive(Clone, Copy)]
struct Submission<'a> {
    request: Append<'a>,
    target: Option<&'a Target>,
}

pub fn contextual_document(
    root: &Path,
    reference: &str,
    context: ContextQuery<'_>,
) -> Result<ContextualDocument, Diagnostic> {
    let target = context.target;
    let query = Query {
        target: Some(target.key()?),
        offset: context.offset,
    };
    let catalog = crate::discovery::scan(root)?;
    let video = catalog.resolve(reference)?;
    let snapshot = storage::Snapshot::read(video)?;
    let document = snapshot.document(video, &query)?;
    let (current, problem) = context_status(root, video, target);
    snapshot.revalidate(video)?;
    Ok(ContextualDocument {
        document,
        target: target.clone(),
        current,
        problem,
    })
}

fn context_status(root: &Path, video: &Video, target: &Target) -> (Option<Target>, Option<String>) {
    let observed = target::current(root, video, target);
    match observed {
        Ok(current) => {
            let problem = (current != *target).then(|| "This changed since you reviewed it. These comments belong to the earlier version.".into());
            (Some(current), problem)
        }
        Err(error) => (None, Some(error.message)),
    }
}

pub fn append_contextual(
    root: &Path,
    reference: &str,
    context: ContextAppend<'_>,
) -> Result<Saved, Diagnostic> {
    let ContextAppend { target, request } = context;
    let matches = request.item == target.key()?;
    if !matches {
        return Err(Diagnostic::new(
            ErrorCode::InputRequired,
            "The note does not match its reviewed artifact.",
        ));
    }
    submit(
        root,
        reference,
        Submission {
            request,
            target: Some(target),
        },
    )
}

/// Generate an offline comment identifier without creating any metadata.
///
/// # Errors
/// Propagates a serialization failure while hashing the timestamp/process/sequence tuple.
pub fn draft_id() -> Result<String, Diagnostic> {
    let identity = (
        jiff::Timestamp::now(),
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed),
    );
    retold_engine::store::hash(&identity).map_err(storage::invalid)
}

/// Read both the legacy comment store and the append-only journal without migration writes.
///
/// # Errors
/// Rejects malformed, oversized or concurrently changed journals and unknown productions.
pub fn document(root: &Path, reference: &str, query: &Query) -> Result<Document, Diagnostic> {
    let catalog = crate::discovery::scan(root)?;
    let video = catalog.resolve(reference)?;
    let snapshot = storage::Snapshot::read(video)?;
    let result = snapshot.document(video, query)?;
    snapshot.revalidate(video)?;
    Ok(result)
}

/// Append one comment under the generation lock; comments never approve or start work.
///
/// # Errors
/// Rejects stale versions, reused identifiers with different content, invalid inputs and active
/// generation or inactive roots. Retrying an already saved identical request is idempotent.
pub fn append(root: &Path, reference: &str, request: Append<'_>) -> Result<Saved, Diagnostic> {
    let reserved = request.item.starts_with("artifact-");
    if reserved {
        return Err(Diagnostic::new(
            ErrorCode::InputRequired,
            "Open the artifact and use its version-bound comment action.",
        ));
    }
    submit(
        root,
        reference,
        Submission {
            request,
            target: Option::default(),
        },
    )
}

fn submit(root: &Path, reference: &str, submission: Submission<'_>) -> Result<Saved, Diagnostic> {
    let request = submission.request;
    request.validate()?;
    let catalog = crate::discovery::scan(root)?;
    let video = catalog.resolve(reference)?;
    let previous = storage::Snapshot::read(video)?;
    if previous.already_saved(&request)? {
        previous.revalidate(video)?;
        return acknowledge(video, &previous, request);
    }
    let _activity = crate::account::activity("save review note")
        .map_err(|error| crate::account::commands::diagnostic(&error))?;
    let _writer = acquire_writer(video)?;
    append_locked(root, video, submission)
}

fn acquire_writer(video: &Video) -> Result<retold_engine::store::Store, Diagnostic> {
    retold_engine::store::Store::open(&video.layout).map_err(|error| match error {
        retold_engine::EngineError::Locked { .. } => Diagnostic::new(ErrorCode::InputRequired, "This video is being changed by another task. Keep your note; refresh and save after that task finishes."),
        other => storage::invalid(other),
    })
}

fn append_locked(
    root: &Path,
    video: &Video,
    submission: Submission<'_>,
) -> Result<Saved, Diagnostic> {
    let Submission { request, target } = submission;
    let snapshot = storage::Snapshot::read(video)?;
    if !snapshot.already_saved(&request)? {
        snapshot.require_version(request.expected_version)?;
        if let Some(target) = target {
            target::require_current(root, video, target)?;
        }
        snapshot.save(video, request)?;
    }
    let saved = storage::Snapshot::read(video)?;
    acknowledge(video, &saved, request)
}

fn acknowledge(
    video: &Video,
    saved: &storage::Snapshot,
    request: Append<'_>,
) -> Result<Saved, Diagnostic> {
    let query = Query {
        target: Some(request.item.into()),
        offset: 0,
    };
    Ok(Saved {
        note: saved.saved_note(request.id)?,
        document: saved.document(video, &query)?,
    })
}

impl Append<'_> {
    fn validate(&self) -> Result<(), Diagnostic> {
        let valid_id = valid_identifier(self.id);
        let target = bounded_text(self.item, MAX_TARGET_BYTES);
        let text = bounded_text(self.text, MAX_TEXT_BYTES);
        if valid_id && target && text {
            return Ok(());
        }
        Err(Diagnostic::new(
            ErrorCode::InputRequired,
            "Use a valid comment identifier, a target of at most 512 bytes and a nonempty note of at most 8 KiB.",
        ))
    }
}

/// Legacy review/history projections include journal comments without overwriting their store.
pub(crate) fn merge_saved(
    video: &Video,
    mut store: crate::review::ReviewStore,
) -> Result<crate::review::ReviewStore, Diagnostic> {
    for note in storage::journal(video)?.entries {
        let group = store.notes.entry(note.item.clone()).or_default();
        group.push(note);
    }
    Ok(store)
}

fn valid_identifier(id: &str) -> bool {
    id.len() == ID_LENGTH && id.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn bounded_text(text: &str, limit: usize) -> bool {
    !text.trim().is_empty() && text.len() <= limit
}
