//! Results of read commands, keyed by their words. A screen names the commands it shows; the
//! store says which are missing or out of date. After a change everything is marked stale and
//! reloads in the background while the old values stay on screen, so nothing flickers.
use std::collections::{HashMap, HashSet};

use retold_app::protocol::Payload;
use serde_json::Value;

/// Live screens refresh at most this often, in milliseconds.
pub const REFRESH_MILLIS: u64 = 5_000;

#[derive(Debug, Clone, Default)]
struct Slot {
    payload: Option<Payload>,
    loading: bool,
    stale: bool,
    loaded_at: u64,
}

#[derive(Debug, Clone, Default)]
pub struct Store {
    slots: HashMap<Vec<String>, Slot>,
}

/// A command a screen shows; `live` ones also refresh on a timer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Want {
    pub words: Vec<String>,
    pub live: bool,
    pub interval: u64,
}

impl Want {
    #[must_use]
    pub fn once(command: &str) -> Self {
        Self {
            words: words(command),
            live: false,
            interval: REFRESH_MILLIS,
        }
    }

    #[must_use]
    pub fn live(command: &str) -> Self {
        Self {
            words: words(command),
            live: true,
            interval: REFRESH_MILLIS,
        }
    }

    #[must_use]
    pub const fn of(words: Vec<String>) -> Self {
        Self {
            words,
            live: false,
            interval: REFRESH_MILLIS,
        }
    }
}

/// What a screen can draw for one command.
#[derive(Debug, Clone, Copy)]
pub enum View<'a> {
    /// Nothing has arrived yet.
    Loading,
    /// The command answered; `refreshing` while a newer answer is on its way.
    Ready {
        contents: &'a Value,
        refreshing: bool,
    },
    /// The command was refused or failed; shown with its fix.
    Failed(&'a Payload),
}

impl Store {
    pub fn remove(&mut self, words: &[String]) {
        let loading = self.slots.get(words).is_some_and(|slot| slot.loading);
        if !loading {
            self.slots.remove(words);
            return;
        }
        let slot = self.slots.get_mut(words);
        if let Some(slot) = slot {
            slot.payload = None;
            slot.stale = true;
        }
    }
    /// Which of `wanted` should be requested now: never loaded, stale, or live and old.
    #[must_use]
    pub fn due(&self, wanted: &[Want], now: u64) -> Vec<Vec<String>> {
        let mut seen = HashSet::new();
        wanted
            .iter()
            .filter(|want| self.is_due(want, now))
            .filter(|want| seen.insert(&want.words))
            .map(|want| want.words.clone())
            .collect()
    }

    fn is_due(&self, want: &Want, now: u64) -> bool {
        let Some(slot) = self.slots.get(&want.words) else {
            return true;
        };
        let age = now.saturating_sub(slot.loaded_at);
        let expired = want.live && age >= want.interval;
        let missing = slot.payload.is_none();
        !slot.loading && (missing || slot.stale || expired)
    }

    /// Records that these reads have been sent, so they are not sent twice.
    pub fn mark_loading(&mut self, sent: &[Vec<String>]) {
        for words in sent {
            let slot = self.slots.entry(words.clone()).or_default();
            slot.loading = true;
            slot.stale = false;
        }
    }

    pub fn fill(&mut self, words: Vec<String>, payload: Payload, now: u64) {
        let slot = self.slots.entry(words).or_default();
        slot.loading = false;
        if slot.stale {
            return;
        }
        slot.payload = Some(payload);
        slot.loaded_at = now;
    }

    /// After any change, reload everything on screen without blanking it.
    pub fn invalidate(&mut self) {
        for slot in self.slots.values_mut() {
            slot.stale = true;
        }
    }

    #[must_use]
    pub fn view(&self, words: &[String]) -> View<'_> {
        let Some(slot) = self.slots.get(words) else {
            return View::Loading;
        };
        match &slot.payload {
            None => View::Loading,
            Some(Payload::Result { contents, .. }) => View::Ready {
                contents,
                refreshing: slot.loading || slot.stale,
            },
            Some(payload) => View::Failed(payload),
        }
    }

    /// The contents when the command succeeded, for screens that only need values.
    #[must_use]
    pub fn contents(&self, words: &[String]) -> Option<&Value> {
        match self.view(words) {
            View::Ready { contents, .. } => Some(contents),
            View::Loading | View::Failed(_) => None,
        }
    }

    /// Review gates require a completed read that has not since been invalidated.
    #[must_use]
    pub fn current_contents(&self, words: &[String]) -> Option<&Value> {
        match self.view(words) {
            View::Ready {
                contents,
                refreshing: false,
            } => Some(contents),
            _ => None,
        }
    }

    #[must_use]
    pub fn busy(&self) -> bool {
        self.slots.values().any(|slot| slot.loading)
    }
}

/// `["video", "list"]` from `"video list"`; screens write commands as plain text.
#[must_use]
pub fn words(command: &str) -> Vec<String> {
    command.split_whitespace().map(str::to_owned).collect()
}

#[cfg(test)]
#[path = "tests/cache.rs"]
mod tests;
