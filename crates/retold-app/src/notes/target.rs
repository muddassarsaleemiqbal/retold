//! Comment targets identify immutable reviewed content rather than a mutable screen selection.
use serde::{Deserialize, Serialize};

const MAX_LABEL_BYTES: usize = 256;
const MAX_UNIT: usize = 1_000_000;

use super::{Diagnostic, ErrorCode, Video, bounded_text, storage, valid_identifier};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Subject {
    Script { section: String },
    Scene { unit: usize },
    Frame { unit: usize },
    Character { name: String },
}

/// Fields are private so callers construct a target from the content they actually displayed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Target {
    subject: Subject,
    content_hash: String,
}

impl Target {
    pub fn script(section: &retold_engine::story::SectionText) -> Result<Self, Diagnostic> {
        Self::new(
            Subject::Script {
                section: section.section.clone(),
            },
            retold_engine::store::hash(section).map_err(storage::invalid)?,
        )
    }

    pub fn scene(
        board: &retold_engine::assets::Storyboard,
        unit: usize,
    ) -> Result<Self, Diagnostic> {
        let narration = board.units.iter().find(|entry| entry.id == unit);
        let shot = board
            .direction
            .shots
            .iter()
            .find(|entry| entry.unit == unit);
        let (Some(narration), Some(shot)) = (narration, shot) else {
            return Err(missing());
        };
        Self::new(
            Subject::Scene { unit },
            retold_engine::store::hash(&(narration, shot)).map_err(storage::invalid)?,
        )
    }

    pub fn frame(unit: usize, bytes: &[u8]) -> Result<Self, Diagnostic> {
        Self::new(
            Subject::Frame { unit },
            blake3::hash(bytes).to_hex().to_string(),
        )
    }

    pub fn character(name: &str, content_hash: &str) -> Result<Self, Diagnostic> {
        Self::new(
            Subject::Character { name: name.into() },
            content_hash.into(),
        )
    }

    fn new(subject: Subject, content_hash: String) -> Result<Self, Diagnostic> {
        let target = Self {
            subject,
            content_hash,
        };
        target.validate()?;
        Ok(target)
    }

    pub fn validate(&self) -> Result<(), Diagnostic> {
        let valid_subject = match &self.subject {
            Subject::Script { section } => bounded_text(section, MAX_LABEL_BYTES),
            Subject::Character { name } => bounded_text(name, MAX_LABEL_BYTES),
            Subject::Scene { unit } | Subject::Frame { unit } => *unit <= MAX_UNIT,
        };
        if !valid_subject || !valid_identifier(&self.content_hash) {
            return Err(Diagnostic::new(
                ErrorCode::InputRequired,
                "This note lost track of what it was about. Your text is kept; open the item again to continue.",
            ));
        }
        Ok(())
    }

    /// Fixed-size journal keys preserve arbitrary Unicode names without ambiguous delimiters.
    pub fn key(&self) -> Result<String, Diagnostic> {
        self.validate()?;
        retold_engine::store::hash(self)
            .map(|hash| format!("artifact-{hash}"))
            .map_err(storage::invalid)
    }

    #[must_use]
    pub fn label(&self) -> String {
        match &self.subject {
            Subject::Script { section } => format!("Script · {section}"),
            Subject::Scene { unit } => format!("Scene {} · direction", unit.saturating_add(1)),
            Subject::Frame { unit } => format!("Scene {} · recorded frame", unit.saturating_add(1)),
            Subject::Character { name } => format!("Character · {name}"),
        }
    }

    #[must_use]
    pub fn content_hash(&self) -> &str {
        &self.content_hash
    }
}

pub(super) fn current(
    root: &std::path::Path,
    video: &Video,
    target: &Target,
) -> Result<Target, Diagnostic> {
    target.validate()?;
    match &target.subject {
        Subject::Script { section } => current_script(video, section),
        Subject::Scene { unit } => {
            let snapshot = retold_engine::editing::scenes::snapshot(&video.layout)
                .map_err(|error| crate::editing::diagnostic(&error))?
                .ok_or_else(missing)?;
            Target::scene(&snapshot.board, *unit)
        }
        Subject::Frame { unit } => {
            let item = crate::desktop::media::scene_frame(root, &video.reference, *unit)?
                .ok_or_else(missing)?;
            Target::frame(*unit, &item.preview_bytes()?)
        }
        Subject::Character { name } => {
            let sheet = retold_engine::character_approval::review(&video.layout, name)
                .map_err(storage::invalid)?;
            Target::character(name, &sheet.sheet.content_hash)
        }
    }
}

fn current_script(video: &Video, section: &str) -> Result<Target, Diagnostic> {
    let snapshot = retold_engine::editing::script(&video.layout)
        .map_err(|error| crate::editing::diagnostic(&error))?
        .ok_or_else(missing)?;
    Target::script(
        snapshot
            .script
            .sections
            .iter()
            .find(|entry| entry.section == section)
            .ok_or_else(missing)?,
    )
}

pub(super) fn require_current(
    root: &std::path::Path,
    video: &Video,
    target: &Target,
) -> Result<(), Diagnostic> {
    if current(root, video, target)? != *target {
        return Err(Diagnostic::new(
            ErrorCode::PlanChanged,
            "This changed since you reviewed it. Your note stays on the version you reviewed; open the current version to start a new note.",
        ));
    }
    Ok(())
}

fn missing() -> Diagnostic {
    Diagnostic::new(
        ErrorCode::InputRequired,
        "The version you reviewed is no longer available. Its earlier comments and your text are kept.",
    )
}
