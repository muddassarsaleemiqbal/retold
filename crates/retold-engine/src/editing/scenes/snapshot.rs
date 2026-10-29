//! Scene observations do not lock, repair or create production files.
use serde::Serialize;

use super::{EditError, context, storage};
use crate::editing::{
    HISTORY_PAGE_SIZE, HistoryAvailability, HistoryCursor, HistoryEntry, HistoryPage, Pointer,
    files,
};
use crate::{RunLayout, assets::Storyboard, store};

#[derive(Debug, Clone, Serialize)]
pub struct SceneSnapshot {
    pub board: Storyboard,
    pub version: String,
    pub content_hash: String,
    pub revision: Option<String>,
    pub approved: bool,
    pub approved_at: Option<String>,
    pub editable: bool,
    pub source_changed: bool,
    pub problems: Vec<String>,
    pub history: HistoryAvailability,
}

struct Observation {
    bytes: Option<Vec<u8>>,
    pointer: Pointer,
    source: String,
    generated: Option<Vec<u8>>,
    visual: String,
}

impl Observation {
    fn read(layout: &RunLayout) -> Result<Self, EditError> {
        let bytes = storage::pointer_bytes(layout)?;
        Ok(Self {
            pointer: files::decode_pointer(bytes.as_deref())?,
            bytes,
            source: context::stamp(layout)?,
            generated: storage::generated(layout)?,
            visual: context::visual_stamp(layout)?,
        })
    }

    fn version(&self) -> Result<String, EditError> {
        Ok(store::hash(&(
            &self.bytes,
            &self.source,
            &self.generated,
            &self.visual,
        ))?)
    }

    fn revalidate(&self, layout: &RunLayout) -> Result<(), EditError> {
        let journal_unchanged = storage::pointer_bytes(layout)? == self.bytes;
        let source_same = context::stamp(layout)? == self.source;
        let generated_same = storage::generated(layout)? == self.generated;
        let visual_same = context::visual_stamp(layout)? == self.visual;
        let content_same = source_same && generated_same;
        if !journal_unchanged || !content_same || !visual_same {
            return Err(EditError::SceneChanged);
        }
        Ok(())
    }
}

pub fn snapshot(layout: &RunLayout) -> Result<Option<SceneSnapshot>, EditError> {
    let observed = Observation::read(layout)?;
    let board = current_board(layout, &observed.pointer)?;
    let content = match board {
        Some(board) => Some(board),
        None => observed
            .generated
            .as_deref()
            .map(files::decode)
            .transpose()?,
    };
    let result = content
        .map(|board| project(layout, &observed, board))
        .transpose()?;
    observed.revalidate(layout)?;
    Ok(result)
}

pub(super) fn current_board(
    layout: &RunLayout,
    pointer: &Pointer,
) -> Result<Option<Storyboard>, EditError> {
    let Some(base) = &pointer.base else {
        return Ok(Option::default());
    };
    let base = storage::baseline(layout, base)?;
    if let Some(head) = &pointer.head {
        let revision = storage::revision(layout, head)?;
        storage::require_binding(pointer, &revision)?;
        return Ok(Some(revision.board));
    }
    Ok(Some(base.board))
}

fn project(
    layout: &RunLayout,
    observed: &Observation,
    board: Storyboard,
) -> Result<SceneSnapshot, EditError> {
    let pointer = &observed.pointer;
    let base = pointer
        .base
        .as_deref()
        .map(|id| storage::baseline(layout, id))
        .transpose()?;
    let content_hash = store::hash(&board)?;
    Ok(SceneSnapshot {
        version: observed.version()?,
        content_hash: content_hash.clone(),
        revision: pointer.head.clone(),
        approved: pointer.approved_hash.as_deref() == Some(&content_hash),
        approved_at: pointer.approved_at.clone(),
        editable: base.is_some(),
        source_changed: base.is_some_and(|base| base.context_hash != observed.source),
        problems: context::quality(layout, &board)?,
        history: HistoryAvailability {
            undo: pointer.head.is_some(),
            redo: !pointer.redo.is_empty(),
        },
        board,
    })
}

pub fn history(
    layout: &RunLayout,
    cursor: Option<&HistoryCursor>,
) -> Result<HistoryPage, EditError> {
    let pointer = storage::pointer(layout)?;
    let cursor_changed = cursor.is_some_and(|cursor| pointer.head.as_deref() != Some(&cursor.head));
    if cursor_changed {
        return Err(EditError::SceneChanged);
    }
    let next = cursor
        .map(|cursor| cursor.next.clone())
        .or_else(|| pointer.head.clone());
    let page = history_page(layout, &pointer, next)?;
    if storage::pointer(layout)?.head != pointer.head {
        return Err(EditError::SceneChanged);
    }
    Ok(page)
}

fn history_page(
    layout: &RunLayout,
    pointer: &Pointer,
    mut next: Option<String>,
) -> Result<HistoryPage, EditError> {
    let mut entries = Vec::new();
    while entries.len() < HISTORY_PAGE_SIZE {
        let Some(id) = next.take() else {
            break;
        };
        let revision = storage::revision(layout, &id)?;
        storage::require_binding(pointer, &revision)?;
        next.clone_from(&revision.parent);
        entries.push(HistoryEntry {
            revision: id,
            parent: revision.parent,
            created_at: revision.created_at,
            section: revision
                .unit
                .map(|unit| format!("Scene {}", unit.saturating_add(1))),
            content_hash: store::hash(&revision.board)?,
        });
    }
    Ok(HistoryPage {
        entries,
        next: next
            .zip(pointer.head.as_ref())
            .map(|(next, head)| HistoryCursor {
                head: head.clone(),
                next,
            }),
    })
}
