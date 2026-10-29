//! Live attachments keep a bounded replay buffer while controls use normal confirmations.
use super::super::page::Page;
use super::super::stream::Stream;
use super::App;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use retold_app::protocol::Payload;

/// Keys that control the followed job, and the action each opens.
const CONTROLS: [(char, &str); 3] = [
    ('p', "queue pause"),
    ('r', "queue unpause"),
    ('x', "queue stop"),
];

fn control(code: KeyCode) -> Option<&'static str> {
    let KeyCode::Char(letter) = code else {
        return Option::default();
    };
    CONTROLS
        .iter()
        .find(|(key, _)| *key == letter)
        .map(|(_, action)| *action)
}

impl App {
    /// New events for a followed job; its read moves past them so they are not fetched twice.
    pub(super) fn stream_reply(&mut self, command: &[String], payload: &Payload) {
        let Payload::Result { contents, .. } = payload else {
            return;
        };
        let mut advanced = false;
        for page in self.pages.iter_mut().flatten() {
            if let Page::Reading(reading) = page
                && reading.words == command
            {
                let before = reading.sequence();
                reading.follow_stream(contents);
                advanced |= reading.sequence() != before;
            }
        }
        if advanced {
            self.store.remove(command);
        }
    }

    fn stream(&self) -> Option<&Stream> {
        match self.page() {
            Some(Page::Reading(reading)) => reading.stream.as_ref(),
            _ => Option::default(),
        }
    }

    /// Whether the open event stream takes this key instead of the result page.
    #[must_use]
    pub(super) fn stream_claims(&self, code: KeyCode) -> bool {
        let streaming = self.stream().is_some();
        streaming && (control(code).is_some() || Stream::claims(code))
    }

    /// Pause, resume or stop the job, or change how the stream is shown.
    /// Does nothing on pages without a stream, so their own keys keep working.
    pub(super) fn stream_command(&mut self, code: KeyCode) {
        let Some(Page::Reading(reading)) = self.page() else {
            return;
        };
        let streaming = reading.stream.is_some();
        if !streaming {
            return;
        }
        let id = reading.job_id();
        if let Some(action) = control(code) {
            self.open_path(action, &vec![("id".into(), id)]);
            return;
        }
        let here = self.here();
        let top = self.pages[here].last_mut();
        if let Some(Page::Reading(reading)) = top
            && let Some(stream) = &mut reading.stream
        {
            stream.view_key(code);
        }
    }

    /// Whether the stream's filter is being typed into.
    #[must_use]
    pub(super) fn stream_typing(&self) -> bool {
        let no_modal = self.modal.is_none();
        no_modal && self.stream().is_some_and(|stream| stream.filtering)
    }

    pub(super) fn stream_filter_key(&mut self, key: KeyEvent) {
        let here = self.here();
        let top = self.pages[here].last_mut();
        if let Some(Page::Reading(reading)) = top
            && let Some(stream) = &mut reading.stream
        {
            stream.filter_key(key);
        }
    }
}
