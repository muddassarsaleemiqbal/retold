//! A retained local draft and explicit conflict resolution around shared versioned edits.
use std::path::PathBuf;

use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::{AppContext as _, Context, Entity, Subscription, Window};
use retold_app::editing::{self, Action, ScriptDocument, SectionEdit};

use super::Shell;
#[cfg(test)]
mod tests;

pub(super) struct ScriptEditor {
    pub root: PathBuf,
    pub reference: String,
    pub document: Option<ScriptDocument>,
    pub incoming: Option<ScriptDocument>,
    pub section: usize,
    pub text: Entity<TextareaState>,
    pub error: Option<String>,
    pub compare: bool,
    pub history: Option<editing::HistoryPage>,
    pub history_cursor: Option<editing::HistoryCursor>,
    pub checkpoint: Option<String>,
    /// Latest writing check for the open section: score, clichés and advice.
    pub writing: Option<super::writing::WritingCheck>,
    _subscription: Subscription,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum ScriptAction {
    Read,
    Save,
    Approve,
    Undo,
    Redo,
    History,
}

impl ScriptEditor {
    pub fn new(
        root: PathBuf,
        reference: String,
        window: &mut Window,
        cx: &mut Context<Shell>,
    ) -> Self {
        let text = cx.new(|cx| TextareaState::new(window, cx));
        let subscription = cx.subscribe(&text, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.checkpoint_script(cx);
            }
            cx.notify();
        });
        Self {
            root,
            reference,
            document: None,
            incoming: None,
            section: 0,
            text,
            error: None,
            compare: false,
            writing: None,
            history: None,
            history_cursor: None,
            checkpoint: retold_app::notes::draft_id().ok(),
            _subscription: subscription,
        }
    }

    pub fn content(&self) -> Option<&editing::ScriptSnapshot> {
        self.document.as_ref()?.content.as_ref()
    }

    pub fn saved_text(&self) -> &str {
        self.content()
            .and_then(|content| content.script.sections.get(self.section))
            .map_or("", |section| section.text.as_str())
    }

    pub fn dirty(&self, cx: &Context<Shell>) -> bool {
        self.text.read(cx).value().as_ref() != self.saved_text()
    }

    pub fn comparison_text(&self) -> String {
        let Some(incoming) = &self.incoming else {
            return self.saved_text().into();
        };
        let section = self
            .content()
            .and_then(|content| content.script.sections.get(self.section))
            .map(|section| section.section.as_str());
        let sections = incoming
            .content
            .iter()
            .flat_map(|content| content.script.sections.iter())
            .map(|section| (section.section.as_str(), section.text.as_str()));
        compared_text(sections, section)
    }

    pub fn load_text(&self, window: &mut Window, cx: &mut Context<Shell>) {
        let text = self.saved_text().to_owned();
        self.text
            .update(cx, |state, cx| state.set_value(text, window, cx));
    }

    pub fn adopt_incoming(&mut self) -> bool {
        let section = self
            .content()
            .and_then(|content| content.script.sections.get(self.section))
            .map(|section| section.section.as_str());
        let target = self
            .incoming
            .as_ref()
            .and_then(|document| document.content.as_ref())
            .and_then(|content| {
                content
                    .script
                    .sections
                    .iter()
                    .position(|saved| Some(saved.section.as_str()) == section)
            });
        let Some(target) = target else {
            self.error = Some("The latest script no longer has this section. Your text is kept; copy it before choosing another section.".into());
            return false;
        };
        self.document = self.incoming.take();
        self.section = target;
        true
    }

    fn receive(
        &mut self,
        document: ScriptDocument,
        preserve: bool,
        window: &mut Window,
        cx: &mut Context<Shell>,
    ) {
        self.error = None;
        if preserve && self.dirty(cx) {
            self.incoming = Some(document);
            self.compare = true;
            return;
        }
        self.document = Some(document);
        self.incoming = None;
        self.history = None;
        self.history_cursor = None;
        if let Some(content) = self.content() {
            self.section = self
                .section
                .min(content.script.sections.len().saturating_sub(1));
        }
        self.load_text(window, cx);
    }
}

impl Shell {
    pub(super) fn open_script(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_scenes = false;
        let (Some(snapshot), Some(reference)) = (&self.snapshot, &self.selected_video) else {
            return;
        };
        if let Some(editor) = &self.editor {
            if &editor.reference == reference {
                self.show_script = true;
                return;
            }
            if editor.dirty(cx) {
                self.status =
                    "Save or discard your script changes before opening another video.".into();
                return;
            }
        }
        self.editor = Some(ScriptEditor::new(
            snapshot.context.library.clone(),
            reference.clone(),
            window,
            cx,
        ));
        self.show_script = true;
        self.script_action(ScriptAction::Read, window, cx);
    }

    pub(super) fn script_action(
        &mut self,
        action: ScriptAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        let Some(editor) = &self.editor else { return };
        if !matches!(
            action,
            ScriptAction::Read | ScriptAction::Save | ScriptAction::History
        ) && editor.dirty(cx)
        {
            self.status = "Save or discard your changes before changing the saved script.".into();
            return;
        }
        let root = editor.root.clone();
        let reference = editor.reference.clone();
        let version = editor
            .content()
            .map_or_else(String::new, |content| content.version.clone());
        let section = editor
            .content()
            .and_then(|content| content.script.sections.get(editor.section))
            .map_or_else(String::new, |section| section.section.clone());
        let text = editor.text.read(cx).value().to_string();
        let cursor = editor.history_cursor.clone();
        self.busy = true;
        self.review = None;
        if let Some(delivery) = &mut self.delivery {
            delivery.review = None;
        }
        self.status = "Reading or saving the script…".into();
        let work = cx.background_executor().spawn(async move {
            if matches!(action, ScriptAction::History) {
                return editing::history(&root, &reference, cursor.as_ref())
                    .map(ScriptReply::History);
            }
            let request = match action {
                ScriptAction::Read | ScriptAction::History => Action::Read,
                ScriptAction::Save => Action::Save(SectionEdit {
                    expected_version: &version,
                    section: &section,
                    text: &text,
                }),
                ScriptAction::Approve => Action::Approve(&version),
                ScriptAction::Undo => Action::Undo(&version),
                ScriptAction::Redo => Action::Redo(&version),
            };
            editing::script(&root, &reference, request).map(ScriptReply::Document)
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = work.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                if let Some(editor) = &mut this.editor {
                    match result {
                        Ok(ScriptReply::Document(document)) => {
                            editor.receive(
                                document,
                                matches!(action, ScriptAction::Read),
                                window,
                                cx,
                            );
                            this.status = if editor.incoming.is_some() {
                                "Your text is kept."
                            } else if editor.content().is_some_and(|content| content.approved) {
                                "Script approved."
                            } else {
                                "Script refreshed. Approve it before the next run."
                            }
                            .into();
                        }
                        Ok(ScriptReply::History(page)) => {
                            editor.history_cursor = page.next.clone();
                            editor.history = Some(page);
                            this.status = "Saved script revisions loaded.".into();
                        }
                        Err(error) => {
                            editor.error = Some(error.message);
                            this.status =
                                "The script request could not be completed. Your text is kept."
                                    .into();
                        }
                    }
                }
                this.checkpoint_script(cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}

enum ScriptReply {
    Document(ScriptDocument),
    History(editing::HistoryPage),
}

fn compared_text<'a>(
    sections: impl IntoIterator<Item = (&'a str, &'a str)>,
    section: Option<&str>,
) -> String {
    let matching = sections
        .into_iter()
        .find(|(name, _)| Some(*name) == section);
    matching.map_or_else(
        || "The latest script has no matching section. Your local text is kept.".into(),
        |(_, text)| text.to_owned(),
    )
}
