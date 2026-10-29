//! Legacy comments stay readable; new appends publish one bounded, atomic journal.
use super::{Append, Diagnostic, Document, ErrorCode, PAGE_SIZE, Query, ReviewNote, Video};

/// The journal is bounded independently of legacy reviews and their unrelated history.
const MAX_BYTES: u64 = 2 * 1024 * 1024;
const MAX_NOTES: usize = 2000;
const FORMAT_VERSION: u32 = 1;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Journal {
    schema: u32,
    pub entries: Vec<ReviewNote>,
}

impl Default for Journal {
    fn default() -> Self {
        Self {
            schema: FORMAT_VERSION,
            entries: Vec::new(),
        }
    }
}

pub(super) struct Snapshot {
    current: Option<Vec<u8>>,
    legacy: Option<Vec<u8>>,
    journal: Journal,
    comments: Vec<ReviewNote>,
    identity: (String, String),
}

impl Snapshot {
    pub fn read(video: &Video) -> Result<Self, Diagnostic> {
        let current = read(&path(video))?;
        let legacy = read(&video.directory.join("meta/reviews.json"))?;
        let journal = decode(current.as_deref())?;
        let mut comments = legacy_notes(legacy.as_deref())?;
        comments.extend(journal.entries.iter().cloned());
        Ok(Self {
            current,
            legacy,
            journal,
            comments,
            identity: (video.id.clone(), location_stamp(&video.directory)),
        })
    }

    pub fn revalidate(&self, video: &Video) -> Result<(), Diagnostic> {
        validate_manifest(video)?;
        let current = read(&path(video))?;
        let legacy = read(&video.directory.join("meta/reviews.json"))?;
        if current != self.current || legacy != self.legacy {
            return Err(changed());
        }
        Ok(())
    }

    pub fn version(&self) -> Result<String, Diagnostic> {
        retold_engine::store::hash(&(&self.identity, &self.current, &self.legacy)).map_err(invalid)
    }

    pub fn require_version(&self, expected: &str) -> Result<(), Diagnostic> {
        if self.version()? != expected {
            return Err(changed());
        }
        Ok(())
    }

    pub fn already_saved(&self, request: &Append<'_>) -> Result<bool, Diagnostic> {
        let id = format!("note-{}", request.id);
        let existing = self.journal.entries.iter().find(|note| note.id == id);
        let Some(existing) = existing else {
            return Ok(false);
        };
        if existing.item == request.item && existing.text == request.text {
            return Ok(true);
        }
        Err(Diagnostic::new(
            ErrorCode::InputRequired,
            "This comment identifier already belongs to different text. Start a new note.",
        ))
    }

    pub fn saved_note(&self, id: &str) -> Result<ReviewNote, Diagnostic> {
        let id = format!("note-{id}");
        self.journal
            .entries
            .iter()
            .find(|note| note.id == id)
            .cloned()
            .ok_or_else(|| {
                invalid("The saved comment is no longer in the journal. Refresh before retrying.")
            })
    }

    pub fn save(self, video: &Video, request: Append<'_>) -> Result<(), Diagnostic> {
        self.revalidate(video)?;
        self.journal.append(request)?.publish(video)
    }

    pub fn document(&self, video: &Video, query: &Query) -> Result<Document, Diagnostic> {
        let mut notes = self.selected(query);
        notes.sort_by_cached_key(|note| {
            (
                std::cmp::Reverse(note.created_at.parse::<jiff::Timestamp>().ok()),
                note.id.clone(),
            )
        });
        let total = notes.len();
        let offset = query
            .offset
            .min(total.saturating_sub(1) / PAGE_SIZE * PAGE_SIZE);
        let next = offset.saturating_add(PAGE_SIZE);
        Ok(Document {
            video_id: video.id.clone(),
            reference: video.reference.clone(),
            title: video.title.clone(),
            version: self.version()?,
            notes: notes.into_iter().skip(offset).take(PAGE_SIZE).collect(),
            total,
            offset,
            next_offset: (next < total).then_some(next),
        })
    }
    fn selected(&self, query: &Query) -> Vec<ReviewNote> {
        self.comments
            .iter()
            .filter(|note| {
                query
                    .target
                    .as_ref()
                    .is_none_or(|target| target == &note.item)
            })
            .cloned()
            .collect()
    }
}

fn path(video: &Video) -> std::path::PathBuf {
    video.layout.meta().join("review-notes.json")
}

fn location_stamp(path: &std::path::Path) -> String {
    blake3::hash(path.as_os_str().as_encoded_bytes())
        .to_hex()
        .to_string()
}

fn validate_manifest(video: &Video) -> Result<(), Diagnostic> {
    let bytes = read(&video.layout.run_file())?;
    let observed = bytes
        .map(|bytes| retold_engine::store::hash(&bytes).map_err(invalid))
        .transpose()?;
    let expected = Some(video.metadata_hash.clone());
    if observed != expected {
        return Err(changed());
    }
    Ok(())
}

fn read(path: &std::path::Path) -> Result<Option<Vec<u8>>, Diagnostic> {
    retold_engine::fsx::read_bounded(path, MAX_BYTES).map_err(invalid)
}

fn decode(bytes: Option<&[u8]>) -> Result<Journal, Diagnostic> {
    let Some(bytes) = bytes else {
        return Ok(Journal::default());
    };
    let journal: Journal = serde_json::from_slice(bytes).map_err(invalid)?;
    journal.validate()
}

fn legacy_notes(bytes: Option<&[u8]>) -> Result<Vec<ReviewNote>, Diagnostic> {
    let Some(bytes) = bytes else {
        return Ok(Vec::new());
    };
    let store: crate::review::ReviewStore = serde_json::from_slice(bytes).map_err(invalid)?;
    Ok(store.notes.into_values().flatten().collect())
}

pub(super) fn journal(video: &Video) -> Result<Journal, Diagnostic> {
    decode(read(&path(video))?.as_deref())
}

pub(super) fn invalid(error: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::new(
        ErrorCode::DataInvalid,
        &format!("Review notes could not be read or saved: {error}"),
    )
}

fn changed() -> Diagnostic {
    Diagnostic::new(
        ErrorCode::PlanChanged,
        "Review notes changed. Your text was kept; refresh the comments before saving again.",
    )
}

impl Journal {
    fn validate(self) -> Result<Self, Diagnostic> {
        let supported = self.schema == FORMAT_VERSION && self.entries.len() <= MAX_NOTES;
        if !supported {
            return Err(invalid("Unsupported review journal schema or entry count."));
        }
        let mut identifiers = std::collections::BTreeSet::new();
        for note in &self.entries {
            validate_note(note)?;
            let unique = identifiers.insert(&note.id);
            if !unique {
                return Err(invalid(
                    "Duplicate comment identifiers in the review journal.",
                ));
            }
        }
        Ok(self)
    }

    fn append(mut self, request: Append<'_>) -> Result<Self, Diagnostic> {
        let full = self.entries.len() >= MAX_NOTES;
        if full {
            return Err(invalid(
                "The review journal is full; existing comments were preserved.",
            ));
        }
        self.entries.push(new_note(request));
        Ok(self)
    }

    fn publish(self, video: &Video) -> Result<(), Diagnostic> {
        let bytes = serde_json::to_vec_pretty(&self).map_err(invalid)?;
        let oversized = bytes.len() as u64 > MAX_BYTES;
        if oversized {
            return Err(invalid(
                "The review journal reached its 2 MiB limit; existing comments were preserved.",
            ));
        }
        retold_engine::fsx::write_atomic(&path(video), &bytes).map_err(invalid)
    }
}

fn validate_note(note: &ReviewNote) -> Result<(), Diagnostic> {
    let id = note
        .id
        .strip_prefix("note-")
        .ok_or_else(|| invalid("Invalid comment identifier in the review journal."))?;
    let input = Append {
        expected_version: "",
        id,
        item: &note.item,
        text: &note.text,
    };
    input.validate().map_err(|error| invalid(error.message))?;
    note.created_at
        .parse::<jiff::Timestamp>()
        .map_err(invalid)?;
    Ok(())
}

fn new_note(request: Append<'_>) -> ReviewNote {
    ReviewNote {
        id: format!("note-{}", request.id),
        item: request.item.into(),
        text: request.text.into(),
        created_at: jiff::Timestamp::now().to_string(),
    }
}
