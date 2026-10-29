//! Direct controls for saved rows use the shared confirmed command path.
use super::super::{collection, files::Task, landing::Tab, runtime::Job};
use super::{App, Tone};
use ratatui::crossterm::event::KeyCode;
use std::path::{Component, Path};

impl App {
    #[must_use]
    pub(super) fn operation_for(&self, code: KeyCode) -> Option<KeyCode> {
        let claimed = match self.tab() {
            Tab::Collection(collection)
                if collection == &collection::ATTEMPTS || collection == &collection::PROCESSES =>
            {
                matches!(code, KeyCode::Char('>' | '<'))
            }
            Tab::Collection(collection) if collection == &collection::JOBS => {
                matches!(code, KeyCode::Char('p' | 'r' | 'x' | 'J' | 'K'))
            }
            Tab::Collection(collection)
                if collection == &collection::MUSIC || collection == &collection::EFFECTS =>
            {
                code == KeyCode::Char(' ')
            }
            _ => bool::default(),
        };
        claimed.then_some(code)
    }

    pub(super) fn operation_key(&mut self, code: KeyCode) {
        if matches!(self.tab(), Tab::Collection(collection) if collection == &collection::ATTEMPTS || collection == &collection::PROCESSES)
        {
            self.account_page_key(code);
        } else if matches!(self.tab(), Tab::Collection(collection) if collection == &collection::JOBS)
        {
            self.queue_key(code);
        } else {
            self.audio_key();
        }
    }

    fn account_page_key(&mut self, code: KeyCode) {
        let Tab::Collection(collection) = self.tab() else {
            return;
        };
        let words = self.collection_words(collection);
        let next = self.store.contents(&words).and_then(|contents| {
            let page = if collection == &collection::ATTEMPTS {
                &contents["account_attempts"]
            } else {
                contents
            };
            page["next_after"].as_str().map(str::to_owned)
        });
        let pages = if collection == &collection::ATTEMPTS {
            &mut self.account_pages.attempts
        } else {
            &mut self.account_pages.processes
        };
        if code == KeyCode::Char('<') {
            pages.pop();
        } else if let Some(next) = next {
            pages.push(next);
        }
        let here = self.here();
        self.landings[here].select(0);
    }

    fn queue_key(&mut self, code: KeyCode) {
        let command = match code {
            KeyCode::Char('p') => "queue pause",
            KeyCode::Char('r') => "queue unpause",
            KeyCode::Char('x') => "queue stop",
            _ => {
                self.reorder_job(code);
                return;
            }
        };
        if let Some(item) = self.selected_item() {
            self.open_path(command, &vec![("id".into(), item.key)]);
        }
    }

    fn reorder_job(&mut self, code: KeyCode) {
        let rows = self.visible_items();
        let selected = self.landing().selected();
        let pair = match code {
            KeyCode::Char('K') => selected.checked_sub(1).map(|before| (selected, before)),
            _ => selected.checked_add(1).map(|next| (next, selected)),
        };
        let Some((moving, before)) =
            pair.and_then(|(moving, before)| rows.get(moving).zip(rows.get(before)))
        else {
            return;
        };
        let pending = [moving, before]
            .iter()
            .all(|item| item.raw["status"] == "queued");
        if !pending {
            self.toast("Only queued jobs can be reordered.".into(), Tone::Notice);
            return;
        }
        let fill = vec![
            ("id".into(), moving.key.clone()),
            ("before".into(), before.key.clone()),
        ];
        self.open_path("queue move", &fill);
    }

    fn audio_key(&mut self) {
        let Tab::Collection(collection) = self.tab() else {
            return;
        };
        let Some(item) = self.selected_item() else {
            return;
        };
        let Some(path) = self.audio_path(collection, &item.raw) else {
            self.toast(
                "This track has no safe saved file path. Refresh the library.".into(),
                Tone::Notice,
            );
            return;
        };
        self.jobs.push(Job::Local {
            id: 0,
            task: Task::Audio(path),
        });
    }

    fn audio_path(
        &self,
        collection: &collection::Collection,
        item: &serde_json::Value,
    ) -> Option<std::path::PathBuf> {
        let contents = self
            .store
            .contents(&super::super::store::words(collection.list))?;
        let directory = contents["directory"].as_str()?;
        let name = Path::new(item["file_name"].as_str()?);
        let mut parts = name.components();
        let safe = matches!(parts.next(), Some(Component::Normal(_))) && parts.next().is_none();
        safe.then(|| Path::new(directory).join(name))
    }
}
