//! Restoring text rechecks production identity and presents fresh saved content for comparison.
use gpui_kit::{Context, Window};
use retold_app::desktop::{
    self,
    recovery::{self, Checkpoint, Content},
};
use retold_app::{dialogue, editing, scenes};

use super::super::{Destination, Shell, editing::ScriptEditor, notes::NotePanel};

enum Restored {
    Script(editing::ScriptDocument),
    Note(super::super::notes::context::Observation),
    Scene(Box<scenes::Document>),
    Line(Box<dialogue::Document>),
}

impl Shell {
    pub(in super::super) fn restore_checkpoint(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy || self.control_busy || self.has_unsaved_setup(cx) || self.recovery_pending() {
            return;
        }
        let Some(checkpoint) = self.recovery.records.get(id).cloned() else {
            return;
        };
        self.busy = true;
        let control_generation = self.control_generation;
        let work = cx.background_executor().spawn(async move {
            recovery::validate_target(&checkpoint.draft)?;
            let snapshot = desktop::open_workspace(&checkpoint.draft.library)?;
            let draft = &checkpoint.draft;
            let content = match &draft.content {
                Content::Scene { .. } => {
                    scenes::document(&draft.library, &draft.reference, scenes::Action::Read)
                        .map(Box::new)
                        .map(Restored::Scene)
                }
                Content::Line { draft: line } => {
                    dialogue::document(&draft.library, &draft.reference, Some(&line.language))
                        .map(Box::new)
                        .map(Restored::Line)
                }
                Content::Script { .. } => {
                    editing::script(&draft.library, &draft.reference, editing::Action::Read)
                        .map(Restored::Script)
                }
                Content::Note { target, .. } => super::super::notes::context::read(
                    &draft.library,
                    &draft.reference,
                    target.as_ref(),
                    0,
                )
                .map(Restored::Note)
                .map_err(|error| *error),
            }?;
            recovery::validate_target(&checkpoint.draft)?;
            Ok::<_, retold_app::protocol::Diagnostic>((checkpoint, snapshot, content))
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = work.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                match result {
                    Ok((checkpoint, snapshot, content)) => {
                        if this.control_generation != control_generation {
                            this.status = "Queue controls changed while restoring. The retained draft is kept; restore it after the control request finishes.".into();
                            cx.notify();
                            return;
                        }
                        this.prepare_recovered_workspace(&snapshot);
                        let restored = match content {
                            Restored::Script(document) => this.restore_script(&checkpoint, document, window, cx),
                            Restored::Scene(document) => this.restore_scene(&checkpoint, *document, window, cx),
                            Restored::Note(document) => { this.restore_note(&checkpoint, document, window, cx); true }
                            Restored::Line(document) => this.restore_line(&checkpoint, *document, window, cx),
                        };
                        if restored {
                            this.clear_media(cx);
                            this.characters = None;
                            this.delivery = None;
                            this.review = None;
                            this.snapshot = Some(snapshot);
                            this.selected_video = Some(checkpoint.draft.reference);
                            this.destination = Destination::Videos;
                            this.show_lab = false;
                            if this.show_scenes { this.load_scene_frame(cx); }
                            this.status = "Retained text restored. Review it against saved work before saving.".into();
                        }
                    }
                    Err(error) => this.recovery.error = Some(error.message),
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }

    fn restore_line(
        &mut self,
        checkpoint: &Checkpoint,
        current: dialogue::Document,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Content::Line { draft } = &checkpoint.draft.content else {
            return false;
        };
        let exists = current
            .content
            .as_ref()
            .is_some_and(|content| content.lines.iter().any(|line| line.unit == draft.unit));
        if current.video_id != checkpoint.draft.video_id || !exists {
            self.recovery.error = Some("The saved edition no longer has this production and line. The retained text is kept and can be copied.".into());
            return false;
        }
        let mut panel = crate::shell::lines::LinesPanel::new(
            checkpoint.draft.library.clone(),
            checkpoint.draft.reference.clone(),
            window,
            cx,
        );
        panel.unit = draft.unit;
        panel.checkpoint = Some(checkpoint.id.clone());
        panel.document = Some(current);
        let text = draft.text.clone();
        panel
            .text
            .update(cx, |state, cx| state.set_value(text, window, cx));
        self.lines = Some(panel);
        true
    }

    fn prepare_recovered_workspace(&mut self, snapshot: &desktop::WorkspaceSnapshot) {
        self.catalog_page = None;
        self.catalog_back.clear();
        self.catalog_options = Default::default();
        self.activity_options = Default::default();
        self.observed_job = None;
        self.last_stop = None;
        self.editor = None;
        self.scenes = None;
        self.show_scenes = false;
        self.notes = None;
        self.refresh_error = None;
        if self
            .providers
            .snapshot
            .as_ref()
            .is_some_and(|providers| providers.root != snapshot.context.config)
        {
            self.providers = crate::shell::providers::ProviderPanel::new();
        }
        self.budget = crate::shell::budget::BudgetPanel::new();
    }

    fn restore_scene(
        &mut self,
        checkpoint: &Checkpoint,
        current: scenes::Document,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Content::Scene { draft, cast } = &checkpoint.draft.content else {
            return false;
        };
        let exists = current
            .content
            .as_ref()
            .is_some_and(|content| content.board.units.iter().any(|unit| unit.id == draft.unit));
        if current.video_id != checkpoint.draft.video_id || !exists {
            self.recovery.error = Some("The saved sequence no longer has this production and scene. The retained direction is kept and can be copied.".into());
            return false;
        }
        let mut editor = crate::shell::scenes::SceneEditor::new(
            checkpoint.draft.library.clone(),
            checkpoint.draft.reference.clone(),
            window,
            cx,
        );
        editor.unit = draft.unit;
        editor.cast.clone_from(cast);
        editor.checkpoint = Some(checkpoint.id.clone());
        editor.document = Some(current.clone());
        if let Some(content) = editor
            .document
            .as_mut()
            .and_then(|document| document.content.as_mut())
        {
            content.version.clone_from(&draft.expected_version);
        }
        editor.incoming = Some(current);
        editor.action.update(cx, |state, cx| {
            state.set_value(draft.action.clone(), window, cx)
        });
        editor.location.update(cx, |state, cx| {
            state.set_value(draft.location.clone(), window, cx)
        });
        editor.mood.update(cx, |state, cx| {
            state.set_value(draft.mood.clone(), window, cx)
        });
        self.scenes = Some(editor);
        self.show_scenes = true;
        self.show_notes = false;
        self.show_script = false;
        true
    }

    fn restore_script(
        &mut self,
        checkpoint: &Checkpoint,
        current: editing::ScriptDocument,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Content::Script {
            section,
            source_version,
            text,
        } = &checkpoint.draft.content
        else {
            return false;
        };
        if current.video_id != checkpoint.draft.video_id {
            self.recovery.error = Some(
                "This script belongs to a different production. The retained draft is kept.".into(),
            );
            return false;
        }
        let index = current.content.as_ref().and_then(|content| {
            content
                .script
                .sections
                .iter()
                .position(|saved| saved.section == *section)
        });
        let Some(index) = index else {
            self.recovery.error = Some(format!(
                "The saved script no longer has section {section}. The retained draft is kept; restore the matching script before retrying."
            ));
            return false;
        };
        let mut editor = ScriptEditor::new(
            checkpoint.draft.library.clone(),
            checkpoint.draft.reference.clone(),
            window,
            cx,
        );
        editor.section = index;
        editor.checkpoint = Some(checkpoint.id.clone());
        editor.document = Some(current.clone());
        if let Some(content) = editor
            .document
            .as_mut()
            .and_then(|document| document.content.as_mut())
        {
            content.version.clone_from(source_version);
        }
        editor.incoming = Some(current);
        editor.compare = true;
        editor
            .text
            .update(cx, |state, cx| state.set_value(text.clone(), window, cx));
        self.editor = Some(editor);
        self.show_notes = false;
        self.show_script = true;
        true
    }

    fn restore_note(
        &mut self,
        checkpoint: &Checkpoint,
        observed: super::super::notes::context::Observation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Content::Note {
            target,
            append_id,
            source_version,
            text,
        } = &checkpoint.draft.content
        else {
            return;
        };
        let mut panel = NotePanel::new(
            checkpoint.draft.library.clone(),
            checkpoint.draft.reference.clone(),
            window,
            cx,
        );
        panel.observe(observed);
        panel.target = target.clone();
        if let Some(document) = &mut panel.document {
            document.version.clone_from(source_version);
        }
        panel.checkpoint = Some(checkpoint.id.clone());
        panel.id = Some(append_id.clone());
        panel.restoring_text = Some(text.clone());
        panel.composing = true;
        panel
            .text
            .update(cx, |state, cx| state.set_value(text.clone(), window, cx));
        self.notes = Some(panel);
        self.show_notes = true;
        self.show_script = false;
    }
}
