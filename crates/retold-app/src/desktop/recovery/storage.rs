//! Bounded, atomically published checkpoints with compare-and-swap updates across windows.
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use retold_engine::fsx;
use serde::{Deserialize, Serialize};

use super::{Checkpoint, Content, Diagnostic, Draft, NOTE_TEXT_LIMIT, SCRIPT_TEXT_LIMIT, invalid};

const FORMAT_VERSION: u32 = 1;
const MAX_BYTES: u64 = 8 * 1024 * 1024;
const MAX_DRAFTS: usize = 32;
const MAX_LABEL_BYTES: usize = 1024;
const ID_LENGTH: usize = 64;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Store {
    schema: u32,
    pub entries: Vec<Checkpoint>,
}

fn path(directory: &Path) -> PathBuf {
    directory.join("desktop-drafts.json")
}

pub(super) fn read(directory: &Path) -> Result<Store, Diagnostic> {
    let Some(bytes) = fsx::read_bounded(&path(directory), MAX_BYTES).map_err(failed)? else {
        return Ok(Store {
            schema: FORMAT_VERSION,
            entries: Vec::new(),
        });
    };
    let store: Store = serde_json::from_slice(&bytes).map_err(failed)?;
    validate(&store)?;
    Ok(store)
}

fn validate(store: &Store) -> Result<(), Diagnostic> {
    let unsupported = store.schema != FORMAT_VERSION;
    let oversized = store.entries.len() > MAX_DRAFTS;
    if unsupported || oversized {
        return Err(invalid(
            "Unsupported or oversized retained-draft file. The original was kept.",
        ));
    }
    let mut ids = BTreeSet::new();
    for entry in &store.entries {
        validate_entry(entry)?;
        if !ids.insert(&entry.id) {
            return Err(invalid("Duplicate retained draft identifier."));
        }
    }
    Ok(())
}

fn validate_entry(entry: &Checkpoint) -> Result<(), Diagnostic> {
    if !identifier(&entry.id) {
        return Err(invalid("Invalid retained draft identifier."));
    }
    validate_draft(&entry.draft)?;
    entry
        .updated_at
        .parse::<jiff::Timestamp>()
        .map_err(failed)?;
    let checksum = version(&entry.draft, &entry.updated_at)?;
    if entry.version != checksum {
        return Err(invalid(
            "The retained draft checksum changed. The original was kept.",
        ));
    }
    Ok(())
}

pub(super) fn validate_draft(draft: &Draft) -> Result<(), Diagnostic> {
    let labels = [&draft.reference, &draft.video_id, &draft.title];
    let scope =
        draft.library.is_absolute() && labels.iter().all(|label| bounded(label, MAX_LABEL_BYTES));
    let content = valid_content(&draft.content);
    if !scope || !content {
        return Err(invalid(
            "The retained draft has invalid context or exceeds its text limit (script 128 KiB; note 8 KiB). Keep the text in the editor.",
        ));
    }
    Ok(())
}

fn valid_content(content: &Content) -> bool {
    match content {
        Content::Script {
            section,
            source_version,
            text,
        } => {
            bounded(section, MAX_LABEL_BYTES)
                && identifier(source_version)
                && text.len() <= SCRIPT_TEXT_LIMIT
        }
        Content::Note {
            target,
            append_id,
            source_version,
            text,
        } => {
            target
                .as_ref()
                .is_none_or(|target| target.validate().is_ok())
                && identifier(append_id)
                && identifier(source_version)
                && bounded(text, NOTE_TEXT_LIMIT)
        }
        Content::Scene { draft, cast } => valid_scene(draft, cast.as_deref()),
        Content::Line { draft } => valid_line(draft),
    }
}

fn valid_line(draft: &crate::dialogue::Draft) -> bool {
    identifier(&draft.expected_version)
        && bounded(&draft.language, MAX_LABEL_BYTES)
        && draft.text.len() <= crate::dialogue::MAX_LINE_BYTES
}

fn valid_scene(draft: &crate::scenes::Draft, cast: Option<&[crate::scenes::CastPose]>) -> bool {
    is_bounded_scene(draft) && cast.is_none_or(|cast| cast.len() <= crate::scenes::MAX_CAST)
}

fn bounded(text: &str, maximum: usize) -> bool {
    !text.trim().is_empty() && text.len() <= maximum
}

fn is_bounded_scene(draft: &crate::scenes::Draft) -> bool {
    let version = identifier(&draft.expected_version);
    let action = draft.action.len() <= crate::scenes::MAX_ACTION_BYTES;
    let location = draft.location.len() <= crate::scenes::MAX_LOCATION_BYTES;
    let mood = draft.mood.len() <= crate::scenes::MAX_MOOD_BYTES;
    version && action && location && mood
}

fn identifier(text: &str) -> bool {
    text.len() == ID_LENGTH && text.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn version(draft: &Draft, timestamp: &str) -> Result<String, Diagnostic> {
    retold_engine::store::hash(&(draft, timestamp)).map_err(failed)
}

pub(super) fn put(directory: &Path, request: Update<'_>) -> Result<Checkpoint, Diagnostic> {
    let Update {
        id,
        expected,
        draft,
    } = request;
    validate_draft(&draft)?;
    if !identifier(id) {
        return Err(invalid("Invalid retained draft identifier."));
    }
    let _activity = activity()?;
    let _writer = writer(directory)?;
    let store = read(directory)?;
    let (store, entry) = revise(
        store,
        Update {
            id,
            expected,
            draft,
        },
    )?;
    save(directory, &store)?;
    Ok(entry)
}

fn revise(store: Store, request: Update<'_>) -> Result<(Store, Checkpoint), Diagnostic> {
    let Update {
        id,
        expected,
        draft,
    } = request;
    let previous = store.entries.iter().find(|entry| entry.id == id);
    if let Some(previous) = previous
        && previous.draft == draft
    {
        let entry = previous.clone();
        return Ok((store, entry));
    }
    require_version(previous, expected)?;
    if let Some(previous) = previous {
        require_target(&previous.draft, &draft)?;
    }
    let entry = checkpoint(id, draft)?;
    Ok((replace(store, entry.clone()), entry))
}

fn checkpoint(id: &str, draft: Draft) -> Result<Checkpoint, Diagnostic> {
    let updated_at = jiff::Timestamp::now().to_string();
    Ok(Checkpoint {
        id: id.into(),
        version: version(&draft, &updated_at)?,
        updated_at,
        draft,
    })
}

fn replace(mut store: Store, entry: Checkpoint) -> Store {
    store.entries.retain(|previous| previous.id != entry.id);
    store.entries.insert(0, entry);
    store
}

pub(super) struct Update<'a> {
    pub id: &'a str,
    pub expected: Option<&'a str>,
    pub draft: Draft,
}

fn changed_target() -> Diagnostic {
    invalid("A retained draft cannot change its production or editing target.")
}

fn require_same_target(previous: &Content, next: &Content) -> Result<(), Diagnostic> {
    let matched = match (previous, next) {
        (
            Content::Script {
                section: before, ..
            },
            Content::Script { section: after, .. },
        ) => before == after,
        (Content::Note { target: before, .. }, Content::Note { target: after, .. }) => {
            before == after
        }
        (Content::Scene { draft: before, .. }, Content::Scene { draft: after, .. }) => {
            before.unit == after.unit
        }
        (Content::Line { draft: before }, Content::Line { draft: after }) => {
            before.language == after.language && before.unit == after.unit
        }
        _ => false,
    };
    matched.then_some(()).ok_or_else(changed_target)
}

fn require_target(previous: &Draft, next: &Draft) -> Result<(), Diagnostic> {
    let identity = previous.library == next.library
        && previous.reference == next.reference
        && previous.video_id == next.video_id;
    if !identity {
        return Err(changed_target());
    }
    require_same_target(&previous.content, &next.content)
}

fn require_version(
    previous: Option<&Checkpoint>,
    expected: Option<&str>,
) -> Result<(), Diagnostic> {
    let observed = previous.map(|entry| entry.version.as_str());
    if observed != expected {
        return Err(invalid(
            "Another window changed this retained draft. Your editor text was kept; refresh retained drafts before resolving it.",
        ));
    }
    Ok(())
}

pub(super) fn remove(directory: &Path, id: &str, expected: &str) -> Result<(), Diagnostic> {
    if !identifier(id) {
        return Err(invalid("Invalid retained draft identifier."));
    }
    let _activity = activity()?;
    let _writer = writer(directory)?;
    let mut store = read(directory)?;
    let previous = store.entries.iter().find(|entry| entry.id == id);
    if previous.is_none() {
        return Ok(());
    }
    require_version(previous, Some(expected))?;
    store.entries.retain(|entry| entry.id != id);
    save(directory, &store)
}

fn save(directory: &Path, store: &Store) -> Result<(), Diagnostic> {
    validate(store)?;
    let bytes = serde_json::to_vec_pretty(store).map_err(failed)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_BYTES {
        return Err(invalid(
            "Retained drafts exceed the storage limit. Discard an older draft before retrying.",
        ));
    }
    fsx::write_atomic(&path(directory), &bytes).map_err(failed)
}

fn activity() -> Result<Option<crate::account::Account>, Diagnostic> {
    crate::account::activity("retain desktop editing draft")
        .map_err(|error| crate::account::commands::diagnostic(&error))
}

struct Writer(std::fs::File);

impl Drop for Writer {
    fn drop(&mut self) {
        self.0.unlock().ok();
    }
}

fn writer(directory: &Path) -> Result<Writer, Diagnostic> {
    retold_engine::recovery::ensure_active(directory).map_err(failed)?;
    std::fs::create_dir_all(directory).map_err(failed)?;
    let path = directory.join(".desktop-drafts.lock");
    if path.is_symlink() {
        return Err(invalid("The retained draft lock must be a regular file."));
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(false);
    let file = options.open(path).map_err(failed)?;
    file.try_lock().map_err(|_| {
        invalid("Another window is retaining a draft. Your text is kept; retry shortly.")
    })?;
    Ok(Writer(file))
}

pub(super) fn failed(error: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::new(
        crate::protocol::ErrorCode::DataInvalid,
        &format!("Retained drafts: {error}"),
    )
}
