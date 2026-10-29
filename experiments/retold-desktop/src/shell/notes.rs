//! A note draft survives navigation and failed writes; saved identifiers make retries safe.
pub(super) mod context;
use std::path::PathBuf;

use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::{AppContext as _, Context, Entity, Subscription, Window};
use retold_app::notes::{self, Document};

use super::{Destination, Shell};

pub(super) struct NotePanel {
    pub library: PathBuf,
    pub reference: String,
    pub document: Option<Document>,
    pub target: Option<notes::Target>,
    pub current: Option<notes::Target>,
    pub target_problem: Option<String>,
    pub rebind: bool,
    pub text: Entity<TextareaState>,
    pub id: Option<String>,
    pub error: Option<String>,
    pub composing: bool,
    pub checkpoint: Option<String>,
    pub previous_checkpoint: Option<String>,
    pub restoring_text: Option<String>,
    _subscription: Subscription,
}

#[derive(Clone, Copy)]
pub(super) enum NoteAction {
    Read(usize),
    Save,
}

enum Input {
    Read(usize),
    Save {
        version: String,
        id: String,
        text: String,
    },
}

enum Reply {
    Read(context::Observation),
    Saved(Box<notes::Saved>, context::Observation),
}

impl NotePanel {
    pub(super) fn new(
        library: PathBuf,
        reference: String,
        window: &mut Window,
        cx: &mut Context<Shell>,
    ) -> Self {
        let text = cx.new(|cx| TextareaState::new(window, cx));
        let subscription = cx.subscribe(&text, |this, source, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change)
                && let Some(panel) = &mut this.notes
            {
                if source.entity_id() != panel.text.entity_id() {
                    return;
                }
                let restored = panel
                    .restoring_text
                    .take()
                    .is_some_and(|text| panel.text.read(cx).value().as_ref() == text);
                if !restored {
                    match notes::draft_id() {
                        Ok(id) => panel.id = Some(id),
                        Err(error) => {
                            panel.id = None;
                            panel.error = Some(error.message);
                        }
                    }
                }
                this.checkpoint_note(cx);
                cx.notify();
            }
        });
        Self {
            library,
            reference,
            document: None,
            target: None,
            current: None,
            target_problem: None,
            rebind: false,
            text,
            id: notes::draft_id().ok(),
            error: None,
            composing: false,
            checkpoint: notes::draft_id().ok(),
            previous_checkpoint: None,
            restoring_text: None,
            _subscription: subscription,
        }
    }

    pub(super) fn dirty(&self, cx: &Context<Shell>) -> bool {
        !self.text.read(cx).value().trim().is_empty()
    }
}

impl Shell {
    pub(super) fn begin_notes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_notes(None, window, cx);
    }

    pub(super) fn open_notes(
        &mut self,
        target: Option<notes::Target>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        if self.editor.as_ref().is_some_and(|editor| editor.dirty(cx)) {
            self.status = "Save or discard your script changes before opening video notes.".into();
            self.show_script = true;
            cx.notify();
            return;
        }
        if self.scenes.as_ref().is_some_and(|editor| editor.dirty(cx)) {
            self.status = "Save or discard scene changes before opening comments.".into();
            self.show_scenes = true;
            cx.notify();
            return;
        }
        let (Some(snapshot), Some(reference)) = (&self.snapshot, &self.selected_video) else {
            return;
        };
        let same = self.notes.as_ref().is_some_and(|panel| {
            panel.library == snapshot.context.library
                && panel.reference == *reference
                && panel.target == target
        });
        if !same && self.notes.as_ref().is_some_and(|panel| panel.dirty(cx)) {
            self.return_to_notes(cx);
            self.status = "Your previous video note is kept. Save or clear it before opening another note draft.".into();
            return;
        }
        if !same {
            self.notes = Some(NotePanel::new(
                snapshot.context.library.clone(),
                reference.clone(),
                window,
                cx,
            ));
        }
        if let Some(panel) = &mut self.notes {
            panel.target = target;
        }
        self.clear_media(cx);
        self.characters = None;
        self.delivery = None;
        self.show_script = false;
        self.show_scenes = false;
        self.show_notes = true;
        self.note_action(NoteAction::Read(0), window, cx);
    }

    pub(super) fn note_action(
        &mut self,
        action: NoteAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        let Some(panel) = &mut self.notes else {
            return;
        };
        let input = match action {
            NoteAction::Read(offset) => Input::Read(offset),
            NoteAction::Save => {
                let (Some(document), Some(id)) = (&panel.document, &panel.id) else {
                    return;
                };
                Input::Save {
                    version: document.version.clone(),
                    id: id.clone(),
                    text: panel.text.read(cx).value().to_string(),
                }
            }
        };
        let library = panel.library.clone();
        let reference = panel.reference.clone();
        let target = panel.target.clone();
        let scope = (
            library.clone(),
            reference.clone(),
            panel.text.entity_id(),
            target.clone(),
        );
        panel.error = None;
        self.busy = true;
        self.status = if matches!(&input, Input::Save { .. }) {
            "Saving review note…"
        } else {
            "Loading video notes…"
        }
        .into();
        let task = cx.background_executor().spawn(async move {
            match input {
                Input::Save { version, id, text } => {
                    let item = match &target {
                        Some(target) => target.key()?,
                        None => "video".into(),
                    };
                    let request = notes::Append {
                        expected_version: &version,
                        id: &id,
                        item: &item,
                        text: &text,
                    };
                    let saved = match &target {
                        Some(target) => notes::append_contextual(
                            &library,
                            &reference,
                            notes::ContextAppend { target, request },
                        ),
                        None => notes::append(&library, &reference, request),
                    }?;
                    let observed = context::read(&library, &reference, target.as_ref(), 0)?;
                    Ok(Reply::Saved(Box::new(saved), observed))
                }
                Input::Read(offset) => {
                    context::read(&library, &reference, target.as_ref(), offset).map(Reply::Read)
                }
            }
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                let Some(panel) = &mut this.notes else {
                    return;
                };
                if panel.library != scope.0
                    || panel.reference != scope.1
                    || panel.text.entity_id() != scope.2
                    || panel.target != scope.3
                {
                    return;
                }
                match result {
                    Ok(Reply::Read(observed)) => {
                        panel.observe(observed);
                        this.status = "Video notes refreshed. Your draft was kept.".into();
                    }
                    Ok(Reply::Saved(saved, observed)) => {
                        let same_id = panel
                            .id
                            .as_ref()
                            .is_some_and(|id| saved.note.id == format!("note-{id}"));
                        let same_text = panel.text.read(cx).value().as_ref() == saved.note.text;
                        panel.observe(observed);
                        if same_id && same_text {
                            panel.composing = false;
                            panel
                                .text
                                .update(cx, |state, cx| state.set_value("", window, cx));
                            this.status = "Review note saved.".into();
                        } else {
                            this.status =
                                "The earlier note was saved. Your current draft was kept.".into();
                        }
                    }
                    Err(error) => {
                        panel.error = Some(error.message);
                        this.status = "Your text was kept. See the note above to continue.".into();
                    }
                }
                this.checkpoint_note(cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn return_to_notes(&mut self, cx: &mut Context<Self>) {
        let Some(panel) = &self.notes else {
            return;
        };
        self.selected_video = Some(panel.reference.clone());
        self.destination = Destination::Videos;
        self.show_notes = true;
        cx.notify();
    }
}
