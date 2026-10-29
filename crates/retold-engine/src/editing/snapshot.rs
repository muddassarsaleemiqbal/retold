//! Read-only script/revision projections never acquire writer locks or create metadata.
use serde::{Deserialize, Serialize};

use super::{EditError, HISTORY_PAGE_SIZE, context, files};
use crate::{RunLayout, store, story::Script};

#[derive(Debug, Clone, Serialize)]
pub struct ScriptSnapshot {
    pub script: Script,
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

#[derive(Debug, Clone, Copy, Serialize)]
pub struct HistoryAvailability {
    pub undo: bool,
    pub redo: bool,
}

struct Observation {
    pointer: super::Pointer,
    journal: Option<Vec<u8>>,
    generated: Option<Vec<u8>>,
    dependencies: String,
    record: String,
}

impl Observation {
    fn read(layout: &RunLayout) -> Result<Self, EditError> {
        let journal = files::pointer_bytes(layout)?;
        Ok(Self {
            pointer: files::decode_pointer(journal.as_deref())?,
            journal,
            generated: files::generated_bytes(layout)?,
            dependencies: context::stamp(layout)?,
            record: script_record_hash(layout)?,
        })
    }

    fn revalidate(&self, layout: &RunLayout) -> Result<(), EditError> {
        let same_record = script_record_hash(layout)? == self.record;
        let same_pointer = files::pointer_bytes(layout)? == self.journal;
        let same_generated = files::generated_bytes(layout)? == self.generated;
        let same_sources = context::stamp(layout)? == self.dependencies;
        let same_documents = same_pointer && same_generated;
        if !same_record || !same_documents || !same_sources {
            return Err(EditError::Changed);
        }
        Ok(())
    }

    fn version(&self) -> Result<String, EditError> {
        Ok(store::hash(&(
            &self.journal,
            &self.generated,
            &self.dependencies,
            &self.record,
        ))?)
    }
}

pub fn script(layout: &RunLayout) -> Result<Option<ScriptSnapshot>, EditError> {
    let observed = Observation::read(layout)?;
    let content = current_content(layout, &observed)?;
    let result = content
        .map(|content| project(layout, &observed, content))
        .transpose()?;
    observed.revalidate(layout)?;
    Ok(result)
}

fn project(
    layout: &RunLayout,
    observed: &Observation,
    content: CurrentContent,
) -> Result<ScriptSnapshot, EditError> {
    let (script, editable, source_changed) = content;
    let pointer = &observed.pointer;
    let content_hash = store::hash(&script)?;
    Ok(ScriptSnapshot {
        version: observed.version()?,
        approved: pointer.approved_hash.as_deref() == Some(&content_hash),
        approved_at: pointer.approved_at.clone(),
        content_hash,
        revision: pointer.head.clone(),
        editable,
        source_changed,
        problems: context::quality(layout, &script)?,
        history: HistoryAvailability {
            undo: pointer.head.is_some(),
            redo: !pointer.redo.is_empty(),
        },
        script,
    })
}

fn script_record_hash(layout: &RunLayout) -> Result<String, EditError> {
    let state = store::read_state(layout)?;
    Ok(store::hash(&state.completed.get("script"))?)
}

type CurrentContent = (Script, bool, bool);

fn current_content(
    layout: &RunLayout,
    observed: &Observation,
) -> Result<Option<CurrentContent>, EditError> {
    let pointer = &observed.pointer;
    if let Some(base_id) = &pointer.base {
        let base = files::baseline(layout, base_id)?;
        let changed = base.context_hash != observed.dependencies;
        let script = if let Some(head) = &pointer.head {
            let revision = files::revision(layout, head)?;
            files::check_binding(pointer, &revision)?;
            revision.script
        } else {
            base.script
        };
        return Ok(Some((script, true, changed)));
    }
    let Some(bytes) = observed.generated.as_deref() else {
        return Ok(Option::default());
    };
    let script: Script = files::decode(bytes)?;
    let editable = verified_baseline(layout, &script)?;
    Ok(Some((script, editable, false)))
}

fn verified_baseline(layout: &RunLayout, script: &Script) -> Result<bool, EditError> {
    let state = store::read_state(layout)?;
    let Some(record) = state.completed.get("script") else {
        return Ok(false);
    };
    let content_hash = store::hash(script)?;
    if record.output_hash != content_hash {
        return Ok(false);
    }
    let inspected = store::inspection::inspect(layout)?;
    Ok(inspected.steps.iter().any(|step| {
        step.step == "script" && step.integrity == store::inspection::Integrity::EnvelopeVerified
    }))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryCursor {
    pub head: String,
    pub next: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HistoryEntry {
    pub revision: String,
    pub parent: Option<String>,
    pub created_at: String,
    pub section: Option<String>,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HistoryPage {
    pub entries: Vec<HistoryEntry>,
    pub next: Option<HistoryCursor>,
}

pub fn history(
    layout: &RunLayout,
    cursor: Option<&HistoryCursor>,
) -> Result<HistoryPage, EditError> {
    let pointer = files::pointer(layout)?;
    if let Some(cursor) = cursor {
        let current_head = pointer.head.as_deref() == Some(&cursor.head);
        if !current_head {
            return Err(EditError::Changed);
        }
    }
    let next = cursor
        .map(|cursor| cursor.next.clone())
        .or_else(|| pointer.head.clone());
    let page = history_page(layout, &pointer, next)?;
    let unchanged = files::pointer(layout)?.head == pointer.head;
    if !unchanged {
        return Err(EditError::Changed);
    }
    Ok(page)
}

fn history_page(
    layout: &RunLayout,
    pointer: &super::Pointer,
    mut next: Option<String>,
) -> Result<HistoryPage, EditError> {
    let mut entries = Vec::new();
    while entries.len() < HISTORY_PAGE_SIZE {
        let Some(id) = next.take() else { break };
        let revision = files::revision(layout, &id)?;
        files::check_binding(pointer, &revision)?;
        next.clone_from(&revision.parent);
        entries.push(HistoryEntry {
            revision: id,
            parent: revision.parent,
            created_at: revision.created_at,
            section: revision.section,
            content_hash: store::hash(&revision.script)?,
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
