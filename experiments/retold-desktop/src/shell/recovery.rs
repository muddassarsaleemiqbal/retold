//! One background checkpoint writer coalesces typing and conditionally clears saved buffers.
mod restore;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use gpui_kit::{Context, PromptLevel, Window};
use retold_app::desktop::recovery::{self, Checkpoint, Content, Repository};

use super::Shell;

const CHECKPOINT_DELAY: Duration = Duration::from_millis(300);

#[derive(Clone)]
struct Buffer {
    library: PathBuf,
    reference: String,
    video_id: String,
    content: Content,
}

impl Buffer {
    fn capture(self) -> Result<recovery::Draft, Box<retold_app::protocol::Diagnostic>> {
        let draft =
            recovery::capture(&self.library, &self.reference, self.content).map_err(Box::new)?;
        if draft.video_id != self.video_id || draft.library != self.library {
            return Err(Box::new(retold_app::protocol::Diagnostic::new(
                retold_app::protocol::ErrorCode::PlanChanged,
                "The production identity changed while retaining text. Your editor text is kept.",
            )));
        }
        Ok(draft)
    }
}

#[derive(Default)]
pub(super) struct Recovery {
    repository: Option<Repository>,
    pub records: BTreeMap<String, Checkpoint>,
    pending: BTreeMap<String, Option<Buffer>>,
    pub loading: bool,
    pub saving: bool,
    timer: bool,
    pub error: Option<String>,
    pub blocked: BTreeMap<String, String>,
    pub closing: bool,
    pub quitting: bool,
    pub expanded: bool,
}

impl Shell {
    pub(super) fn recovery_enabled(&self) -> bool {
        self.recovery.repository.is_some()
    }
    /// Fixture hosts opt into a private recovery directory; normal startup discovers its profile.
    pub fn enable_recovery(&mut self, configuration: PathBuf, cx: &mut Context<Self>) {
        self.load_recovery(Ok(Repository::new(configuration)), cx);
    }

    pub(super) fn discover_recovery(&mut self, cx: &mut Context<Self>) {
        self.load_recovery(Repository::discover(), cx);
    }

    fn load_recovery(
        &mut self,
        repository: Result<Repository, retold_app::protocol::Diagnostic>,
        cx: &mut Context<Self>,
    ) {
        if self.recovery.loading || self.recovery.saving {
            return;
        }
        let repository = match repository {
            Ok(repository) => repository,
            Err(error) => {
                self.recovery.error = Some(error.message);
                return;
            }
        };
        self.recovery.repository = Some(repository.clone());
        self.recovery.loading = true;
        let work = cx
            .background_executor()
            .spawn(async move { repository.list() });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                this.recovery.loading = false;
                match result {
                    Ok(records) => {
                        this.recovery.records = records
                            .into_iter()
                            .map(|entry| (entry.id.clone(), entry))
                            .collect();
                        this.recovery.error = None;
                        this.checkpoint_script(cx);
                        this.checkpoint_note(cx);
                        this.checkpoint_scene(cx);
                    }
                    Err(error) => this.recovery.error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn checkpoint_script(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = &self.editor else {
            return;
        };
        let Some(id) = &editor.checkpoint else {
            return;
        };
        if editor.dirty(cx) && editor.text.read(cx).value().len() > recovery::SCRIPT_TEXT_LIMIT {
            self.recovery.blocked.insert(id.clone(), "Script text exceeds the 128 KiB retention limit. The previous checkpoint is kept. Shorten the text or copy it before closing.".into());
            return;
        }
        self.recovery.blocked.remove(id);
        let buffer = if editor.dirty(cx) {
            let Some(content) = editor.content() else {
                return;
            };
            let Some(section) = content.script.sections.get(editor.section) else {
                return;
            };
            Some(Buffer {
                library: editor.root.clone(),
                reference: editor.reference.clone(),
                video_id: editor
                    .document
                    .as_ref()
                    .map_or_else(String::new, |document| document.video_id.clone()),
                content: Content::Script {
                    section: section.section.clone(),
                    source_version: content.version.clone(),
                    text: editor.text.read(cx).value().to_string(),
                },
            })
        } else {
            None
        };
        self.queue_checkpoint(id.clone(), buffer, cx);
    }

    pub(super) fn checkpoint_note(&mut self, cx: &mut Context<Self>) {
        let Some(panel) = &self.notes else {
            return;
        };
        let Some(id) = &panel.checkpoint else {
            return;
        };
        if panel.dirty(cx) && panel.text.read(cx).value().len() > recovery::NOTE_TEXT_LIMIT {
            self.recovery.blocked.insert(id.clone(), "Review text exceeds the 8 KiB retention limit. The previous checkpoint is kept. Shorten the text or copy it before closing.".into());
            return;
        }
        self.recovery.blocked.remove(id);
        let buffer = if panel.dirty(cx) {
            let (Some(document), Some(append_id)) = (&panel.document, &panel.id) else {
                return;
            };
            Some(Buffer {
                library: panel.library.clone(),
                reference: panel.reference.clone(),
                video_id: document.video_id.clone(),
                content: Content::Note {
                    target: panel.target.clone(),
                    append_id: append_id.clone(),
                    source_version: document.version.clone(),
                    text: panel.text.read(cx).value().to_string(),
                },
            })
        } else {
            None
        };
        let previous = buffer
            .is_none()
            .then(|| panel.previous_checkpoint.clone())
            .flatten();
        self.queue_checkpoint(id.clone(), buffer, cx);
        if let Some(previous) = previous {
            self.queue_checkpoint(previous, None, cx);
        }
    }

    pub(super) fn checkpoint_scene(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = &self.scenes else {
            return;
        };
        let Some(id) = &editor.checkpoint else {
            return;
        };
        if editor.dirty(cx)
            && let Some(error) = editor.bounds_error(cx)
        {
            self.recovery.blocked.insert(id.clone(), error.into());
            return;
        }
        self.recovery.blocked.remove(id);
        let buffer = if editor.dirty(cx) {
            let (Some(draft), Some(document)) = (editor.draft(cx), &editor.document) else {
                return;
            };
            Some(Buffer {
                library: editor.library.clone(),
                reference: editor.reference.clone(),
                video_id: document.video_id.clone(),
                content: Content::Scene {
                    draft,
                    cast: editor.cast.clone().filter(|_| editor.cast_dirty()),
                },
            })
        } else {
            None
        };
        self.queue_checkpoint(id.clone(), buffer, cx);
    }

    pub(super) fn checkpoint_line(&mut self, cx: &mut Context<Self>) {
        let Some(panel) = &self.lines else {
            return;
        };
        let Some(id) = panel.checkpoint.clone() else {
            return;
        };
        let buffer = if panel.dirty(cx) {
            let (Some(draft), Some(document)) = (panel.draft(cx), &panel.document) else {
                return;
            };
            Some(Buffer {
                library: panel.library.clone(),
                reference: panel.reference.clone(),
                video_id: document.video_id.clone(),
                content: Content::Line { draft },
            })
        } else {
            None
        };
        self.queue_checkpoint(id, buffer, cx);
    }

    fn queue_checkpoint(&mut self, id: String, buffer: Option<Buffer>, cx: &mut Context<Self>) {
        if self.recovery.repository.is_none() || self.recovery.closing {
            return;
        }
        self.recovery.pending.insert(id, buffer);
        if self.recovery.timer {
            return;
        }
        self.recovery.timer = true;
        let timer = cx.background_executor().timer(CHECKPOINT_DELAY);
        cx.spawn(async move |this, cx| {
            timer.await;
            let _ = this.update(cx, |this, cx| {
                this.recovery.timer = false;
                this.flush_checkpoints(cx);
            });
        })
        .detach();
    }

    pub(super) fn flush_checkpoints(&mut self, cx: &mut Context<Self>) {
        if self.recovery.saving || self.recovery.loading {
            return;
        }
        let Some(repository) = self.recovery.repository.clone() else {
            return;
        };
        let Some((id, buffer)) = self.recovery.pending.pop_first() else {
            if self.recovery.closing {
                self.finish_recovery_quit(cx);
            } else if self.recovery.quitting {
                self.recovery.quitting = false;
                self.finish_quit(cx);
            }
            return;
        };
        let previous = self.recovery.records.get(&id).cloned();
        let retry = (id.clone(), buffer.clone());
        self.recovery.saving = true;
        let work = cx.background_executor().spawn(async move {
            if let Some(buffer) = buffer {
                let draft = buffer.capture()?;
                repository
                    .put(
                        &id,
                        previous.as_ref().map(|entry| entry.version.as_str()),
                        draft,
                    )
                    .map(Some)
                    .map_err(Box::new)
            } else {
                if let Some(previous) = previous {
                    repository
                        .remove(&id, &previous.version)
                        .map_err(Box::new)?;
                }
                Ok(None)
            }
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                this.recovery.saving = false;
                match result {
                    Ok(Some(saved)) => {
                        let prior = this
                            .notes
                            .as_mut()
                            .filter(|panel| panel.checkpoint.as_ref() == Some(&saved.id))
                            .and_then(|panel| panel.previous_checkpoint.take());
                        this.recovery.records.insert(saved.id.clone(), saved);
                        if let Some(prior) = prior {
                            this.recovery.pending.insert(prior, None);
                        }
                    }
                    Ok(None) => {
                        this.recovery.records.remove(&retry.0);
                    }
                    Err(error) => {
                        this.recovery.error = Some(error.message);
                        this.recovery.pending.entry(retry.0).or_insert(retry.1);
                        this.recovery.closing = false;
                        this.recovery.quitting = false;
                        cx.notify();
                        return;
                    }
                }
                this.recovery.error = None;
                this.flush_checkpoints(cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn discard_checkpoint(&mut self, id: String, cx: &mut Context<Self>) {
        self.recovery.pending.insert(id, None);
        self.flush_checkpoints(cx);
    }

    pub(super) fn refresh_retained_drafts(&mut self, cx: &mut Context<Self>) {
        if self.recovery.saving || self.recovery.loading {
            return;
        }
        let Some(repository) = self.recovery.repository.clone() else {
            return;
        };
        self.recovery.pending.clear();
        self.recovery.blocked.clear();
        if let Some(editor) = &mut self.scenes
            && editor.dirty(cx)
        {
            editor.checkpoint = retold_app::notes::draft_id().ok();
        }
        if let Some(editor) = &mut self.editor
            && editor.dirty(cx)
        {
            editor.checkpoint = retold_app::notes::draft_id().ok();
        }
        if let Some(panel) = &mut self.notes
            && panel.dirty(cx)
        {
            panel.checkpoint = retold_app::notes::draft_id().ok();
        }
        self.load_recovery(Ok(repository), cx);
    }

    pub(super) fn wait_checkpoints_before_quit(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.recovery.blocked.is_empty() {
            self.status = "Keep this window open: editing text exceeds its retention limit. Shorten or copy it, or explicitly discard it before closing.".into();
            return true;
        }
        if self.recovery.saving || !self.recovery.pending.is_empty() {
            self.recovery.quitting = true;
            self.flush_checkpoints(cx);
            return true;
        }
        false
    }

    pub(super) fn confirm_discard_checkpoint(
        &mut self,
        id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.prompting || self.recovery_pending() {
            return;
        }
        self.prompting = true;
        let answer = window.prompt(
            PromptLevel::Warning,
            "Discard this retained draft?",
            Some("Only this retained editing text will be removed. Saved production work is kept."),
            &["Keep draft", "Discard draft"],
            cx,
        );
        cx.spawn(async move |this, cx| {
            let answer = answer.await;
            let _ = this.update(cx, |this, cx| {
                this.prompting = false;
                if matches!(answer, Ok(1)) {
                    this.discard_checkpoint(id, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn discard_editing_for_quit(&mut self, cx: &mut Context<Self>) {
        for id in [
            self.scenes
                .as_ref()
                .and_then(|editor| editor.checkpoint.clone()),
            self.editor
                .as_ref()
                .and_then(|editor| editor.checkpoint.clone()),
            self.notes
                .as_ref()
                .and_then(|panel| panel.checkpoint.clone()),
        ]
        .into_iter()
        .flatten()
        {
            self.recovery.pending.insert(id, None);
        }
        self.recovery.closing = true;
        if self.recovery.repository.is_none() {
            self.finish_recovery_quit(cx);
        } else {
            self.flush_checkpoints(cx);
        }
    }

    fn finish_recovery_quit(&mut self, cx: &mut Context<Self>) {
        self.providers.form = None;
        self.editor = None;
        self.scenes = None;
        self.show_scenes = false;
        self.notes = None;
        self.show_notes = false;
        self.draft = None;
        self.import = None;
        self.new_root = None;
        self.recovery.blocked.clear();
        self.finish_quit(cx);
    }

    pub fn recovery_pending(&self) -> bool {
        self.recovery.loading
            || self.recovery.saving
            || self.recovery.timer
            || !self.recovery.pending.is_empty()
    }

    pub(super) fn retained_checkpoints(&self) -> impl Iterator<Item = &Checkpoint> {
        self.recovery.records.values().filter(|entry| {
            let script = self
                .editor
                .as_ref()
                .and_then(|editor| editor.checkpoint.as_deref());
            let note = self
                .notes
                .as_ref()
                .and_then(|panel| panel.checkpoint.as_deref());
            let scene = self
                .scenes
                .as_ref()
                .and_then(|editor| editor.checkpoint.as_deref());
            Some(entry.id.as_str()) != script
                && Some(entry.id.as_str()) != note
                && Some(entry.id.as_str()) != scene
        })
    }
}
