//! A single pending clipboard write is tracked like other background actions.
use super::super::files::Task;
use super::{App, Tone};
use retold_app::protocol::Payload;

impl App {
    pub(super) fn copy_command(&mut self) {
        let copying = self.host.clipboard.is_some();
        if copying {
            self.toast(
                "A clipboard copy is already in progress.".into(),
                Tone::Notice,
            );
            return;
        }
        let task = Task::Clipboard(self.command());
        self.queue_local_action(task);
        self.host.clipboard = Some(self.next_id);
    }

    pub(super) fn clipboard_finished(&mut self, id: u64, payload: Payload) {
        if self.host.clipboard != Some(id) {
            return;
        }
        self.host.clipboard = None;
        self.finish_local_action(id);
        match payload {
            Payload::Error { diagnostic, .. } => self.toast(diagnostic.message, Tone::Failure),
            _ => self.toast("Command copied to clipboard.".into(), Tone::Success),
        }
    }
}
