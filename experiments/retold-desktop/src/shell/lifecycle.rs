//! Native quit prompts protect unsaved setup without cancelling accepted production work.
use gpui_kit::{Context, PromptLevel, Window};

use super::Shell;

impl Shell {
    pub(super) fn has_unsaved_setup(&self, cx: &Context<Self>) -> bool {
        self.providers.form.is_some()
            || self.draft.is_some()
            || self.import.is_some()
            || self.new_root.is_some()
            || self.editor.as_ref().is_some_and(|editor| editor.dirty(cx))
            || self.notes.as_ref().is_some_and(|panel| panel.dirty(cx))
            || self.scenes.as_ref().is_some_and(|editor| editor.dirty(cx))
    }

    pub fn request_quit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.has_unsaved_setup(cx) {
            self.remember_window_size(window, cx);
            self.finish_quit(cx);
            return;
        }
        if self.prompting || self.recovery.closing || self.recovery.quitting {
            return;
        }
        self.prompting = true;
        self.remember_window_size(window, cx);
        let retain = self.can_retain_editing();
        let answer = if retain {
            window.prompt(PromptLevel::Warning, "Close Studio with editing drafts?", Some("Keep script, scene direction and review-note text on this device for later recovery, or discard it. Saved work and accepted requests are kept. Closing Studio leaves its background worker running."), &["Keep editing", "Keep drafts and quit", "Discard drafts and quit"], cx)
        } else {
            window.prompt(PromptLevel::Warning, "Discard unsaved changes?", Some("Your unsaved provider, script, scene direction, review note, video or workspace changes will be discarded. Saved work and accepted requests are kept. Closing Studio leaves its background worker running."), &["Keep editing", "Discard and quit"], cx)
        };
        cx.spawn(async move |this, cx| {
            let answer = answer.await;
            let _ = this.update(cx, |this, cx| {
                this.prompting = false;
                if retain && matches!(answer, Ok(1)) {
                    this.checkpoint_script(cx);
                    this.checkpoint_note(cx);
                    this.checkpoint_scene(cx);
                    this.finish_quit(cx);
                } else if matches!(answer, Ok(1)) || (retain && matches!(answer, Ok(2))) {
                    this.discard_editing_for_quit(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn can_retain_editing(&self) -> bool {
        self.recovery_enabled()
            && self.providers.form.is_none()
            && self.draft.is_none()
            && self.import.is_none()
            && self.new_root.is_none()
    }
}
