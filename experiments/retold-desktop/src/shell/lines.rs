//! Edition lines: one language and one line at a time, reviewed before publication.
use std::path::PathBuf;

use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::{AppContext as _, Context, Entity, Subscription, Window};
use retold_app::dialogue::{self, Document, Draft, ReviewedLine};

use super::Shell;

pub(super) struct LinesPanel {
    pub library: PathBuf,
    pub reference: String,
    pub document: Option<Document>,
    pub unit: usize,
    pub text: Entity<TextareaState>,
    pub review: Option<ReviewedLine>,
    pub error: Option<String>,
    pub checkpoint: Option<String>,
    _subscription: Option<Subscription>,
}

#[derive(Clone)]
pub(super) enum LineAction {
    Read(Option<String>),
    Review,
    Save,
    Approve,
}

enum Reply {
    Document(Box<Document>),
    Review(Box<ReviewedLine>),
}

impl LinesPanel {
    pub(super) fn new(
        library: PathBuf,
        reference: String,
        window: &mut Window,
        cx: &mut Context<Shell>,
    ) -> Self {
        let text = cx.new(|cx| TextareaState::new(window, cx));
        let subscription = cx.subscribe(&text, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                if let Some(panel) = &mut this.lines {
                    panel.review = None;
                }
                this.checkpoint_line(cx);
            }
        });
        Self {
            library,
            reference,
            document: None,
            unit: 0,
            text,
            review: None,
            error: None,
            checkpoint: retold_app::notes::draft_id().ok(),
            _subscription: Some(subscription),
        }
    }

    pub(super) fn content(&self) -> Option<&dialogue::DialogueSnapshot> {
        self.document.as_ref()?.content.as_ref()
    }

    pub(super) fn line(&self) -> Option<&dialogue::Line> {
        self.content()?
            .lines
            .iter()
            .find(|line| line.unit == self.unit)
    }

    pub(super) fn dirty(&self, cx: &Context<Shell>) -> bool {
        self.line()
            .is_some_and(|line| line.text != self.text.read(cx).value().as_ref())
    }

    pub(super) fn draft(&self, cx: &Context<Shell>) -> Option<Draft> {
        let content = self.content()?;
        Some(Draft {
            expected_version: content.version.clone(),
            language: content.language.clone(),
            unit: self.unit,
            text: self.text.read(cx).value().to_string(),
        })
    }

    pub(super) fn load_text(&self, window: &mut Window, cx: &mut Context<Shell>) {
        let text = self
            .line()
            .map(|line| line.text.clone())
            .unwrap_or_default();
        self.text
            .update(cx, |state, cx| state.set_value(text, window, cx));
    }
}

impl Shell {
    pub(super) fn begin_lines(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let (Some(snapshot), Some(reference)) = (&self.snapshot, &self.selected_video) else {
            return;
        };
        let library = snapshot.context.library.clone();
        let reference = reference.clone();
        self.lines = Some(LinesPanel::new(library, reference, window, cx));
        self.line_action(LineAction::Read(None), window, cx);
    }

    pub(super) fn select_line(&mut self, unit: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = &mut self.lines else {
            return;
        };
        if self.busy || panel.dirty(cx) {
            return;
        }
        panel.unit = unit;
        panel.review = None;
        panel.load_text(window, cx);
        cx.notify();
    }

    pub(super) fn line_action(
        &mut self,
        action: LineAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        let Some(panel) = &mut self.lines else {
            return;
        };
        let (library, reference) = (panel.library.clone(), panel.reference.clone());
        let draft = panel.draft(cx);
        let review = panel.review.clone();
        let language = panel.content().map(|content| content.language.clone());
        let version = panel.content().map(|content| content.version.clone());
        panel.error = None;
        self.busy = true;
        self.status = "Reading or saving edition lines…".into();
        let scope = (library.clone(), reference.clone());
        let request = action.clone();
        let work = cx.background_executor().spawn(async move {
            let missing = || {
                retold_app::protocol::Diagnostic::new(
                    retold_app::protocol::ErrorCode::InputRequired,
                    "Choose an edition line first.",
                )
            };
            match request {
                LineAction::Read(chosen) => {
                    let wanted = chosen.or(language);
                    dialogue::document(&library, &reference, wanted.as_deref())
                        .map(|document| Reply::Document(Box::new(document)))
                }
                LineAction::Review => {
                    dialogue::review(&library, &reference, draft.ok_or_else(missing)?)
                        .map(|review| Reply::Review(Box::new(review)))
                }
                LineAction::Save => dialogue::save(&review.ok_or_else(missing)?)
                    .map(|document| Reply::Document(Box::new(document))),
                LineAction::Approve => {
                    let (Some(language), Some(version)) = (language, version) else {
                        return Err(missing());
                    };
                    dialogue::approve(&library, &reference, (&language, &version))
                        .map(|document| Reply::Document(Box::new(document)))
                }
            }
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = work.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                let Some(panel) = &mut this.lines else {
                    return;
                };
                if panel.library != scope.0 || panel.reference != scope.1 {
                    return;
                }
                match result {
                    Ok(Reply::Document(document)) => {
                        let language_changed = panel.content().map(|c| &c.language)
                            != document.content.as_ref().map(|c| &c.language);
                        panel.document = Some(*document);
                        panel.review = None;
                        if language_changed {
                            panel.unit = panel
                                .content()
                                .and_then(|c| c.lines.first())
                                .map_or(0, |line| line.unit);
                        }
                        panel.load_text(window, cx);
                        this.status = match panel.content() {
                            Some(content) if !content.approved => {
                                "Edited lines need approval before production.".into()
                            }
                            Some(_) => "Edition lines loaded.".into(),
                            None => "Only the original-language narration exists.".into(),
                        };
                    }
                    Ok(Reply::Review(review)) => {
                        if panel.draft(cx).as_ref() == Some(&review.draft) {
                            panel.review = Some(*review);
                        }
                        this.status =
                            "Review the rebuilt stages before saving. Nothing was published."
                                .into();
                    }
                    Err(error) => {
                        panel.error = Some(error.message);
                        this.status = "The line request failed. Your text was kept.".into();
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
