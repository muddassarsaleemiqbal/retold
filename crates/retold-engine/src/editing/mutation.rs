//! One run lock serializes edits, approvals and pointer changes with generation.
use super::{
    Baseline, EDITING_FORMAT_VERSION, EditError, MAX_REDO, Pointer, Revision, ScriptSnapshot,
    context, files, snapshot,
};
use crate::{RunLayout, store::Store, story::Script};

#[derive(Debug, Clone, Copy)]
pub struct SectionEdit<'a> {
    pub expected_version: &'a str,
    pub section: &'a str,
    pub text: &'a str,
}

struct Editor<'a> {
    layout: &'a RunLayout,
    store: Store,
    pointer: Pointer,
    current: ScriptSnapshot,
}

impl<'a> Editor<'a> {
    fn open(layout: &'a RunLayout, expected: &str) -> Result<Self, EditError> {
        let store = Store::open(layout)?;
        let current = current_script(layout)?;
        if current.version != expected {
            return Err(EditError::Changed);
        }
        if !current.editable {
            return Err(EditError::Invalid(
                "Generate and save a verified script before editing or approving it.".into(),
            ));
        }
        Ok(Self {
            layout,
            store,
            pointer: files::pointer(layout)?,
            current,
        })
    }

    const fn require_source(&self) -> Result<(), EditError> {
        if self.current.source_changed {
            return Err(EditError::SourceChanged);
        }
        Ok(())
    }

    fn write_revision(
        mut self,
        script: Script,
        section: Option<String>,
    ) -> Result<Self, EditError> {
        self.require_source()?;
        let base = match &self.pointer.base {
            Some(base) => base.clone(),
            None => files::save_object(
                self.layout,
                "bases",
                &Baseline {
                    schema: EDITING_FORMAT_VERSION,
                    context_hash: context::stamp(self.layout)?,
                    script: self.current.script.clone(),
                },
            )?,
        };
        let revision = Revision {
            schema: EDITING_FORMAT_VERSION,
            base: base.clone(),
            parent: self.pointer.head.clone(),
            created_at: jiff::Timestamp::now().to_string(),
            section,
            script,
        };
        let id = files::save_object(self.layout, "revisions", &revision)?;
        self.pointer.base = Some(base);
        self.pointer.head = Some(id);
        self.pointer.redo.clear();
        self.clear_approval();
        Ok(self)
    }

    fn clear_approval(&mut self) {
        self.pointer.approved_hash = None;
        self.pointer.approved_at = None;
    }

    fn publish_content(mut self) -> Result<ScriptSnapshot, EditError> {
        // Invalidate first: a failed index publication must never leave new text with old deliveries.
        // A crash before pointer publication only conservatively invalidates the previous draft.
        self.store = self.store.invalidate_script_descendants()?;
        files::save_pointer(self.layout, &self.pointer)?;
        current_script(self.layout)
    }
}

pub fn save_section(
    layout: &RunLayout,
    edit: SectionEdit<'_>,
) -> Result<ScriptSnapshot, EditError> {
    let editor = Editor::open(layout, edit.expected_version)?;
    editor.require_source()?;
    let mut script = editor.current.script.clone();
    let section = script
        .sections
        .iter_mut()
        .find(|section| section.section == edit.section)
        .ok_or_else(|| EditError::Invalid("Choose an existing script section.".into()))?;
    if section.text == edit.text {
        return Ok(editor.current);
    }
    section.text = edit.text.into();
    editor
        .write_revision(script, Some(edit.section.into()))?
        .publish_content()
}

pub fn approve(layout: &RunLayout, expected: &str) -> Result<ScriptSnapshot, EditError> {
    let mut editor = Editor::open(layout, expected)?;
    editor.require_source()?;
    context::require_quality(layout, &editor.current.script)?;
    if editor.current.approved {
        return Ok(editor.current);
    }
    let original = editor.pointer.head.is_none();
    if original {
        let script = editor.current.script.clone();
        editor = editor.write_revision(script, Option::default())?;
    }
    editor.pointer.approved_hash = Some(editor.current.content_hash.clone());
    editor.pointer.approved_at = Some(jiff::Timestamp::now().to_string());
    files::save_pointer(layout, &editor.pointer)?;
    current_script(layout)
}

pub fn undo(layout: &RunLayout, expected: &str) -> Result<ScriptSnapshot, EditError> {
    let mut editor = Editor::open(layout, expected)?;
    let head = editor
        .pointer
        .head
        .clone()
        .ok_or_else(|| EditError::Invalid("There is no script revision to undo.".into()))?;
    let history_full = editor.pointer.redo.len() >= MAX_REDO;
    if history_full {
        return Err(EditError::Invalid(
            "The bounded redo stack is full. Save a new revision before further undo.".into(),
        ));
    }
    let revision = files::revision(layout, &head)?;
    files::check_binding(&editor.pointer, &revision)?;
    editor.pointer.head = revision.parent;
    editor.pointer.redo.push(head);
    editor.clear_approval();
    editor.publish_content()
}

pub fn redo(layout: &RunLayout, expected: &str) -> Result<ScriptSnapshot, EditError> {
    let mut editor = Editor::open(layout, expected)?;
    editor.require_source()?;
    let next = editor
        .pointer
        .redo
        .last()
        .cloned()
        .ok_or_else(|| EditError::Invalid("There is no script revision to redo.".into()))?;
    let revision = files::revision(layout, &next)?;
    files::check_binding(&editor.pointer, &revision)?;
    if revision.parent != editor.pointer.head {
        return Err(EditError::Integrity(
            "The redo revision does not follow the current draft.".into(),
        ));
    }
    editor.pointer.head = Some(next);
    editor.pointer.redo.pop();
    editor.clear_approval();
    editor.publish_content()
}

pub(super) fn refresh_baseline(
    layout: &RunLayout,
    pointer: &Pointer,
    generated: &Script,
) -> Result<(), EditError> {
    let Some(base_id) = &pointer.base else {
        return Ok(());
    };
    let base = files::baseline(layout, base_id)?;
    let context_hash = context::stamp(layout)?;
    let unchanged = base.context_hash == context_hash
        && crate::store::hash(&base.script)? == crate::store::hash(generated)?;
    if unchanged {
        return Ok(());
    }
    let base = Baseline {
        schema: EDITING_FORMAT_VERSION,
        context_hash,
        script: generated.clone(),
    };
    let mut refreshed = pointer.clone();
    refreshed.base = Some(files::save_object(layout, "bases", &base)?);
    refreshed.redo.clear();
    refreshed.approved_hash = None;
    refreshed.approved_at = None;
    files::save_pointer(layout, &refreshed)
}

fn current_script(layout: &RunLayout) -> Result<ScriptSnapshot, EditError> {
    snapshot::script(layout)?
        .ok_or_else(|| EditError::Invalid("This video has no saved script yet.".into()))
}
