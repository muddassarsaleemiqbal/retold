//! Bounded, replayable studio events. Cursors advance only after events are retained.
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use serde_json::Value;
const MAX_EVENTS: usize = 1_000;

#[derive(Debug, Clone, Default)]
pub struct Stream {
    pub events: Vec<Value>,
    pub sequence: u64,
    pub terminal: bool,
    pub follow: bool,
    pub filter: super::input::Input,
    pub filtering: bool,
}

impl Stream {
    pub fn accept(&mut self, contents: &Value) {
        for event in contents["events"].as_array().into_iter().flatten() {
            let sequence = event["sequence"].as_u64().unwrap_or_default();
            if sequence > self.sequence {
                self.events.push(event.clone());
                self.sequence = sequence;
            }
        }
        let overflow = self.events.len().saturating_sub(MAX_EVENTS);
        self.events.drain(..overflow);
        self.terminal = contents["is_terminal"].as_bool().unwrap_or_default();
    }

    /// Keys the event view handles itself: follow, filter, and scrolling up to stop following.
    #[must_use]
    pub const fn claims(code: KeyCode) -> bool {
        matches!(code, KeyCode::Char('f' | '/'))
    }

    pub const fn view_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('f') => self.follow ^= true,
            KeyCode::Char('/') => self.filtering = true,
            KeyCode::Up | KeyCode::PageUp => self.follow = false,
            _ => {}
        }
    }

    /// Typing while filtering: Enter keeps the filter, Esc clears it.
    pub fn filter_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => self.filtering = false,
            KeyCode::Esc => {
                self.filtering = false;
                self.filter.clear();
            }
            _ => self.filter.edit(key),
        }
    }

    pub fn visible(&self) -> Vec<&Value> {
        self.events
            .iter()
            .filter(|event| {
                let text = format!("{} {}", event["stage"], event["message"]).to_lowercase();
                text.contains(&self.filter.text().to_lowercase())
            })
            .collect()
    }
}
