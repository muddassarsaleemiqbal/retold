//! Scene publication invalidates affected image work before publishing its authoritative pointer.
use super::{Baseline, EditError, Revision, SceneSnapshot, context, snapshot, storage};
use crate::editing::{EDITING_FORMAT_VERSION, MAX_REDO, Pointer, files};
use crate::{
    RunLayout,
    assets::Storyboard,
    store::{self, Store},
};

/// A visual action is bounded independently from the complete storyboard document.
pub const MAX_ACTION_BYTES: usize = 4096;
/// A location remains a short description for one scene's image request.
pub const MAX_LOCATION_BYTES: usize = 1024;
/// A mood remains a short phrase for the visual direction.
pub const MAX_MOOD_BYTES: usize = 256;

/// Visual-only text changes preserve narration, casting and all other scene fields.
#[derive(Debug, Clone, Copy)]
pub struct ShotEdit<'a> {
    pub expected_version: &'a str,
    pub unit: usize,
    pub action: &'a str,
    pub location: &'a str,
    pub mood: &'a str,
}

/// Exact visual descendants of a proposed edit, without publishing or invalidating anything.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ChangeImpact {
    pub changed_units: Vec<usize>,
    pub reused_dependents: Vec<usize>,
    pub panel_dependents: Vec<usize>,
    pub detaches_reuse: bool,
    pub affected_stages: Vec<String>,
}

pub(super) struct Editor<'a> {
    layout: &'a RunLayout,
    store: Store,
    pointer: Pointer,
    pub(super) current: SceneSnapshot,
}

impl<'a> Editor<'a> {
    pub(super) fn open(layout: &'a RunLayout, expected: &str) -> Result<Self, EditError> {
        let store = Store::open(layout)?;
        let current = current(layout)?;
        if current.version != expected {
            return Err(EditError::SceneChanged);
        }
        if !current.editable {
            return Err(EditError::Invalid(
                "Generate a verified scene sequence before publishing scene edits.".into(),
            ));
        }
        Ok(Self {
            layout,
            store,
            pointer: storage::pointer(layout)?,
            current,
        })
    }

    pub(super) const fn require_source(&self) -> Result<(), EditError> {
        if self.current.source_changed {
            return Err(EditError::SceneSourceChanged);
        }
        Ok(())
    }

    pub(super) fn revision(
        mut self,
        board: Storyboard,
        unit: Option<usize>,
    ) -> Result<Self, EditError> {
        self.require_source()?;
        let base = self
            .pointer
            .base
            .clone()
            .ok_or_else(|| EditError::Integrity("Scene baseline is missing.".into()))?;
        let revision = Revision {
            schema: EDITING_FORMAT_VERSION,
            base,
            parent: self.pointer.head.clone(),
            created_at: jiff::Timestamp::now().to_string(),
            unit,
            board,
        };
        self.pointer.head = Some(files::save_object(
            self.layout,
            "scene-revisions",
            &revision,
        )?);
        self.pointer.redo.clear();
        self.clear_approval();
        Ok(self)
    }

    fn clear_approval(&mut self) {
        self.pointer.approved_hash = None;
        self.pointer.approved_at = None;
    }

    pub(super) fn publish(mut self) -> Result<SceneSnapshot, EditError> {
        let next = snapshot::current_board(self.layout, &self.pointer)?
            .ok_or_else(|| EditError::Integrity("Scene baseline is missing.".into()))?;
        let changed = changed_units(&self.current.board, &next)?;
        for unit in changed {
            self.store.bump_revision(&format!("image-{unit}"))?;
        }
        storage::save_pointer(self.layout, &self.pointer)?;
        current(self.layout)
    }
}

pub fn save_shot(layout: &RunLayout, edit: ShotEdit<'_>) -> Result<SceneSnapshot, EditError> {
    save_unit(layout, (edit.expected_version, edit.unit), |current| {
        edited_board(current, edit)
    })
}

/// One scene's change publishes only when its board differs and passes every direction gate.
pub(super) fn save_unit(
    layout: &RunLayout,
    (expected, unit): (&str, usize),
    edit: impl FnOnce(&SceneSnapshot) -> Result<Storyboard, EditError>,
) -> Result<SceneSnapshot, EditError> {
    let editor = Editor::open(layout, expected)?;
    editor.require_source()?;
    let board = edit(&editor.current)?;
    if store::hash(&board)? == editor.current.content_hash {
        return Ok(editor.current);
    }
    context::require_quality(layout, &board)?;
    editor.revision(board, Some(unit))?.publish()
}

fn edited_board(current: &SceneSnapshot, edit: ShotEdit<'_>) -> Result<Storyboard, EditError> {
    validate_edit(&edit)?;
    let mut board = current.board.clone();
    let shot = board
        .direction
        .shots
        .iter_mut()
        .find(|shot| shot.unit == edit.unit)
        .ok_or_else(|| EditError::Invalid("Choose an existing scene.".into()))?;
    if shot.action == edit.action && shot.location == edit.location && shot.mood == edit.mood {
        return Ok(board);
    }
    shot.action = edit.action.into();
    shot.location = edit.location.into();
    shot.mood = edit.mood.into();
    // A changed action must be drawn; keeping a recut could silently show the previous action.
    shot.reuse = None;
    Ok(board)
}

pub fn preview(layout: &RunLayout, edit: ShotEdit<'_>) -> Result<ChangeImpact, EditError> {
    let current = current(layout)?;
    require_editable(&current, edit.expected_version)?;
    let board = edited_board(&current, edit)?;
    context::require_quality(layout, &board)?;
    let changed = changed_units(&current.board, &board)?;
    let panels = super::panels::dependents(layout, (&current.board, &board), &changed)?;
    let latest = snapshot::snapshot(layout)?.ok_or(EditError::SceneChanged)?;
    if latest.version != current.version {
        return Err(EditError::SceneChanged);
    }
    Ok(impact(&current.board, changed, panels))
}

pub(super) fn require_editable(current: &SceneSnapshot, expected: &str) -> Result<(), EditError> {
    if current.version != expected {
        return Err(EditError::SceneChanged);
    }
    if current.source_changed {
        return Err(EditError::SceneSourceChanged);
    }
    if !current.editable {
        return Err(EditError::Invalid(
            "Generate a verified scene sequence before editing it.".into(),
        ));
    }
    Ok(())
}

pub(super) fn impact(
    previous: &Storyboard,
    changed: Vec<usize>,
    panel_dependents: Vec<usize>,
) -> ChangeImpact {
    let shots = &previous.direction.shots;
    let reused_dependents = shots
        .iter()
        .filter(|shot| shot.reuse.is_some_and(|unit| changed.contains(&unit)))
        .map(|shot| shot.unit)
        .collect();
    let detaches_reuse = shots
        .iter()
        .any(|shot| changed.contains(&shot.unit) && shot.reuse.is_some());
    let affected_stages = if changed.is_empty() {
        Vec::new()
    } else {
        crate::graph::Graph::stages()
            .affected("images")
            .into_iter()
            .collect()
    };
    ChangeImpact {
        changed_units: changed,
        reused_dependents,
        panel_dependents,
        detaches_reuse,
        affected_stages,
    }
}

pub fn validate_edit(edit: &ShotEdit<'_>) -> Result<(), EditError> {
    let fields = [
        (edit.action, MAX_ACTION_BYTES),
        (edit.location, MAX_LOCATION_BYTES),
        (edit.mood, MAX_MOOD_BYTES),
    ];
    let invalid = fields
        .iter()
        .any(|(text, limit)| text.trim().is_empty() || text.len() > *limit);
    if invalid {
        return Err(EditError::Invalid("Use nonempty visual direction within the action (4 KiB), location (1 KiB) and mood (256 bytes) limits.".into()));
    }
    Ok(())
}

pub(super) fn changed_units(
    previous: &Storyboard,
    next: &Storyboard,
) -> Result<Vec<usize>, EditError> {
    let mut changed = Vec::new();
    for (previous, next) in previous.direction.shots.iter().zip(&next.direction.shots) {
        if store::hash(previous)? != store::hash(next)? {
            changed.push(next.unit);
        }
    }
    Ok(changed)
}

pub fn approve(layout: &RunLayout, expected: &str) -> Result<SceneSnapshot, EditError> {
    let mut editor = Editor::open(layout, expected)?;
    editor.require_source()?;
    context::require_record(layout)?;
    context::require_quality(layout, &editor.current.board)?;
    if editor.current.approved {
        return Ok(editor.current);
    }
    let original = editor.pointer.head.is_none();
    if original {
        let board = editor.current.board.clone();
        editor = editor.revision(board, Option::default())?;
    }
    editor.pointer.approved_hash = Some(editor.current.content_hash.clone());
    editor.pointer.approved_at = Some(jiff::Timestamp::now().to_string());
    storage::save_pointer(layout, &editor.pointer)?;
    current(layout)
}

pub fn undo(layout: &RunLayout, expected: &str) -> Result<SceneSnapshot, EditError> {
    let mut editor = Editor::open(layout, expected)?;
    let history_full = editor.pointer.redo.len() >= MAX_REDO;
    if history_full {
        return Err(EditError::Invalid(
            "The scene redo history is full; save a new revision before undoing further.".into(),
        ));
    }
    let head = editor
        .pointer
        .head
        .clone()
        .ok_or_else(|| EditError::Invalid("There is no scene revision to undo.".into()))?;
    let revision = storage::revision(layout, &head)?;
    storage::require_binding(&editor.pointer, &revision)?;
    editor.pointer.head = revision.parent;
    editor.pointer.redo.push(head);
    editor.clear_approval();
    editor.publish()
}

pub fn redo(layout: &RunLayout, expected: &str) -> Result<SceneSnapshot, EditError> {
    let mut editor = Editor::open(layout, expected)?;
    editor.require_source()?;
    let next = editor
        .pointer
        .redo
        .last()
        .cloned()
        .ok_or_else(|| EditError::Invalid("There is no scene revision to redo.".into()))?;
    let revision = storage::revision(layout, &next)?;
    storage::require_binding(&editor.pointer, &revision)?;
    if revision.parent != editor.pointer.head {
        return Err(EditError::Integrity(
            "Scene redo does not follow this revision.".into(),
        ));
    }
    editor.pointer.head = Some(next);
    editor.pointer.redo.pop();
    editor.clear_approval();
    editor.publish()
}

pub(super) fn refresh_baseline(
    layout: &RunLayout,
    mut pointer: Pointer,
    board: &Storyboard,
) -> Result<(), EditError> {
    let context_hash = context::stamp(layout)?;
    if let Some(base) = &pointer.base {
        let base = storage::baseline(layout, base)?;
        if base.context_hash == context_hash && store::hash(&base.board)? == store::hash(board)? {
            return Ok(());
        }
    }
    let base = Baseline {
        schema: EDITING_FORMAT_VERSION,
        context_hash,
        board: board.clone(),
    };
    pointer.base = Some(files::save_object(layout, "scene-bases", &base)?);
    pointer.redo.clear();
    pointer.approved_hash = None;
    pointer.approved_at = None;
    storage::save_pointer(layout, &pointer)
}

pub(super) fn current(layout: &RunLayout) -> Result<SceneSnapshot, EditError> {
    snapshot::snapshot(layout)?
        .ok_or_else(|| EditError::Invalid("This video has no saved scene sequence.".into()))
}
