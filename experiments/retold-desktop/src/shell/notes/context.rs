//! Context selection remains tied to displayed content; refresh never silently rebinds a draft.
use gpui_kit::{Context, Window};
use retold_app::{notes, protocol::Diagnostic};

use super::{NotePanel, Shell};

pub(in super::super) struct Observation {
    document: notes::Document,
    current: Option<notes::Target>,
    problem: Option<String>,
}

pub(in super::super) fn read(
    root: &std::path::Path,
    reference: &str,
    target: Option<&notes::Target>,
    offset: usize,
) -> Result<Observation, Box<Diagnostic>> {
    if let Some(target) = target {
        let observed =
            notes::contextual_document(root, reference, notes::ContextQuery { target, offset })?;
        Ok(Observation {
            document: observed.document,
            current: observed.current,
            problem: observed.problem,
        })
    } else {
        let query = notes::Query {
            target: Some("video".into()),
            offset,
        };
        Ok(Observation {
            document: notes::document(root, reference, &query)?,
            current: None,
            problem: None,
        })
    }
}

impl NotePanel {
    pub(in super::super) fn observe(&mut self, observed: Observation) {
        self.document = Some(observed.document);
        self.current = observed.current;
        self.target_problem = observed.problem;
        self.rebind = false;
    }
}

impl Shell {
    pub(in super::super) fn comment_script(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let target = self
            .editor
            .as_ref()
            .and_then(|editor| editor.content()?.script.sections.get(editor.section))
            .map(|section| notes::Target::script(section).map_err(|error| error.message));
        self.open_artifact_note(target, window, cx);
    }

    pub(in super::super) fn comment_scene(
        &mut self,
        frame: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = self.scenes.as_ref().and_then(|editor| {
            if frame {
                editor
                    .player
                    .read(cx)
                    .image_loaded()
                    .then(|| editor.frame_target.clone())
                    .flatten()
                    .map(Ok)
            } else {
                editor.content().map(|content| {
                    notes::Target::scene(&content.board, editor.unit).map_err(|error| error.message)
                })
            }
        });
        self.open_artifact_note(target, window, cx);
    }

    pub(in super::super) fn comment_character(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = self
            .characters
            .as_ref()
            .filter(|panel| panel.frame.is_some())
            .and_then(|panel| panel.document.as_ref()?.selected.as_ref())
            .map(|sheet| {
                notes::Target::character(&sheet.sheet.name, &sheet.sheet.content_hash)
                    .map_err(|error| error.message)
            });
        self.open_artifact_note(target, window, cx);
    }

    fn open_artifact_note(
        &mut self,
        target: Option<Result<notes::Target, String>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match target {
            Some(Ok(target)) => self.open_notes(Some(target), window, cx),
            Some(Err(error)) => {
                self.status = error;
                cx.notify();
            }
            None => {
                self.status = "Open the media before commenting on it.".into();
                cx.notify();
            }
        }
    }

    pub(in super::super) fn rebind_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.recovery_pending() {
            return;
        }
        let Some(panel) = &mut self.notes else {
            return;
        };
        if !panel.rebind {
            return;
        }
        let Some(current) = panel.current.clone() else {
            return;
        };
        if panel.target.as_ref() == Some(&current) {
            return;
        }
        panel.previous_checkpoint = panel.checkpoint.clone();
        panel.target = Some(current);
        panel.document = None;
        panel.checkpoint = notes::draft_id().ok();
        panel.id = notes::draft_id().ok();
        panel.rebind = false;
        panel.error = None;
        panel.target_problem = None;
        self.note_action(super::NoteAction::Read(0), window, cx);
    }
}
