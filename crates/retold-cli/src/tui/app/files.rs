//! Field pickers and file interactions preserve the form beneath the modal.
use super::super::{
    files::{Browser, Decision, Task},
    page::{Choice, Entry, Modal, Page, Palette},
    runtime::Job,
    store,
};
use super::{App, Tone};
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use retold_app::protocol::Payload;
use std::path::PathBuf;

impl App {
    pub(super) fn pick_field(&mut self) {
        let Some(Page::Form(form)) = self.page() else {
            return;
        };
        let Some(field) = form.focused_field() else {
            return;
        };
        let id = field.id.clone();
        if let super::super::catalog::FieldKind::Choice(values) = &field.kind {
            let entries = field_choices(&id, values);
            self.modal = Some(Modal::Values(Palette::new(entries)));
            return;
        }
        let source = picker_source(&id);
        if let Some((command, array, key)) = source {
            let entries = self
                .store
                .contents(&store::words(command))
                .map(|value| choices(&id, &value[array], key))
                .unwrap_or_default();
            self.modal = Some(Modal::Values(Palette::new(entries)));
        } else {
            let path = form
                .value(&id)
                .map_or_else(|| PathBuf::from("."), PathBuf::from);
            self.browse(path, Some(id));
        }
    }

    pub fn browse(&mut self, path: PathBuf, field: Option<String>) {
        self.next_id = self.next_id.saturating_add(1);
        let id = self.next_id;
        self.jobs.push(Job::Local {
            id,
            task: Task::Browse(path.clone()),
        });
        self.modal = Some(Modal::Browser(Browser::new(id, path, field)));
    }

    pub(super) fn fill_field(&mut self, id: &str, value: &str) {
        let here = self.here();
        let top = self.pages[here].last_mut();
        if let Some(Page::Form(form)) = top {
            let focus = form.focus;
            *form = super::super::defaults::choose(form, (id, value), &self.defaults);
            form.focus = focus;
        }
    }

    /// Enter, Backspace, F4 and Esc leave this listing; other keys keep the browser open.
    pub(super) fn browser_key(&mut self, mut browser: Browser, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => return,
            KeyCode::Enter => return self.browser_enter(browser),
            KeyCode::Backspace => return self.browse_parent(browser),
            KeyCode::F(CHOOSE_FOLDER_KEY) => return self.choose_path(browser.field, browser.path),
            KeyCode::F(EDIT_KEY) => self.edit_browser_path(&browser),
            // Once a filter is being typed, letters and spaces belong to it.
            KeyCode::Char('o') if browser.query.text().is_empty() => self.open_selected(&browser),
            KeyCode::Char(' ') if browser.query.text().is_empty() => self.play_selected(&browser),
            _ => browser.press(key),
        }
        self.request_preview(&browser);
        self.modal = Some(Modal::Browser(browser));
    }

    fn browse_parent(&mut self, browser: Browser) {
        let parent = browser.path.parent().unwrap_or(&browser.path).to_path_buf();
        self.browse(parent, browser.field);
    }

    fn browser_enter(&mut self, browser: Browser) {
        match browser.decide() {
            Decision::Browse(path, field) => self.browse(path, field),
            Decision::Choose(path, field) => self.choose_path(field, path),
            Decision::Stay(browser) => self.modal = Some(Modal::Browser(*browser)),
        }
    }

    fn choose_path(&mut self, field: Option<String>, path: PathBuf) {
        if let Some(field) = field {
            self.fill_field(&field, &path.to_string_lossy());
        } else {
            self.jobs.push(Job::Local {
                id: 0,
                task: Task::Open(path),
            });
        }
    }

    pub(super) fn request_preview(&mut self, browser: &Browser) {
        if let Some(path) = browser.selected_path() {
            self.jobs.push(Job::Local {
                id: browser.id,
                task: Task::Preview(path),
            });
        }
    }

    fn open_selected(&mut self, browser: &Browser) {
        if let Some(path) = browser.selected_path() {
            self.jobs.push(Job::Local {
                id: 0,
                task: Task::Open(path),
            });
        }
    }

    fn play_selected(&mut self, browser: &Browser) {
        if let Some(path) = browser.selected_path() {
            self.jobs.push(Job::Local {
                id: 0,
                task: Task::Audio(path),
            });
        }
    }

    fn edit_browser_path(&mut self, browser: &Browser) {
        if let Some(path) = browser.selected_path() {
            self.host.handoff = Some(super::super::handoff::Handoff::Editor {
                path,
                check: browser.check.clone(),
            });
        }
    }

    pub(super) fn edit_path(&mut self) {
        let landing = self.page().is_none();
        if landing {
            self.edit_resource();
            return;
        }
        let Some(Page::Form(form)) = self.page() else {
            return;
        };
        let path = form.focused_field().and_then(|field| form.value(&field.id));
        if let Some(path) = path {
            self.host.handoff = Some(super::super::handoff::Handoff::Editor {
                path: path.into(),
                check: None,
            });
        }
    }

    /// A finished file task: the review preview, the open browser, or a failure notice.
    pub(super) fn file_reply(&mut self, job: (u64, &Task), payload: Payload) {
        let (job_id, task) = job;
        if matches!(task, Task::Clipboard(_)) {
            self.clipboard_finished(job_id, payload);
            return;
        }
        let preferences = matches!(task, Task::LoadDefaults(_) | Task::SaveDefaults(..));
        if preferences {
            self.accept_preferences(job, payload);
            return;
        }
        self.preview_reply(job, payload);
    }

    fn preview_reply(&mut self, job: (u64, &Task), payload: Payload) {
        let (job_id, task) = job;
        if matches!(task, Task::Preview(path) if job_id == u64::MAX && self.media.comparison_requested.as_ref() == Some(path))
        {
            self.media.comparison = Some(payload);
            return;
        }
        let wanted = self.media.requested.as_ref();
        let review_preview =
            matches!(task, Task::Preview(path) if job_id == 0 && wanted == Some(path));
        if review_preview {
            self.media.preview = Some(payload);
            return;
        }
        let browsing = matches!(&self.modal, Some(Modal::Browser(browser)) if browser.id == job_id);
        if let (true, Some(Modal::Browser(browser))) = (browsing, &mut self.modal) {
            browser.accept(task, payload);
            return;
        }
        if let Payload::Error { diagnostic, .. } = payload {
            self.toast(diagnostic.message, Tone::Failure);
        }
    }
}

/// F4 chooses the folder being shown; F6 opens the selection in the editor.
const CHOOSE_FOLDER_KEY: u8 = 4;
const EDIT_KEY: u8 = 6;

/// `(field, read command, array, key)`: fields whose values come from a saved list.
const PICKERS: [(&str, &str, &str, &str); 6] = [
    ("guide", "guides list", "items", "id"),
    ("channel", "channels list", "items", "id"),
    ("reference", "video list", "items", "ref"),
    ("video", "video list", "items", "ref"),
    ("voice-provider", "providers list", "providers", "id"),
    ("preset", "export-presets list", "items", "name"),
];

fn picker_source(id: &str) -> Option<(&'static str, &'static str, &'static str)> {
    PICKERS
        .iter()
        .find(|(field, ..)| *field == id)
        .map(|(_, command, array, key)| (*command, *array, *key))
}

fn choices(field: &str, list: &serde_json::Value, key: &str) -> Vec<Entry> {
    list.as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let value = item[key].as_str()?;
            let label = item["name"]
                .as_str()
                .or_else(|| item["title"].as_str())
                .unwrap_or(value);
            Some(Entry {
                label: label.into(),
                detail: value.into(),
                risk: Option::default(),
                choice: Choice::Value {
                    field: field.into(),
                    value: value.into(),
                },
            })
        })
        .collect()
}

impl App {
    fn edit_resource(&mut self) {
        let super::super::landing::Tab::Collection(collection) = self.tab() else {
            return;
        };
        let kind = match collection.name {
            "Guides" => "guides",
            "Channels" => "channels",
            _ => return,
        };
        let Some(item) = self.selected_item() else {
            return;
        };
        self.next_id = self.next_id.saturating_add(1);
        let id = self.next_id;
        let mut browser = Browser::new(id, PathBuf::new(), Option::default());
        browser.check = Some(vec![kind.into(), "check".into(), item.key.clone()]);
        self.jobs.push(Job::Local {
            id,
            task: Task::Resource {
                root: self.workspace.clone(),
                kind: kind.into(),
                id: item.key,
            },
        });
        self.modal = Some(Modal::Browser(browser));
    }
}

fn field_choice(field: &str, value: &str) -> Entry {
    Entry {
        label: value.into(),
        detail: "Use this option".into(),
        risk: None,
        choice: Choice::Value {
            field: field.into(),
            value: value.into(),
        },
    }
}

fn field_choices(field: &str, values: &[String]) -> Vec<Entry> {
    values
        .iter()
        .map(|value| field_choice(field, value))
        .collect()
}

impl App {
    pub fn load_defaults(&mut self) {
        self.jobs.push(Job::Local {
            id: 0,
            task: Task::LoadDefaults(self.workspace.clone()),
        });
    }
    pub(super) fn remember_defaults(&mut self, id: u64) {
        let form = self.pages.iter().flatten().find_map(|page| match page {
            Page::Outcome(outcome) if outcome.id == id => {
                super::super::defaults::successful_form(outcome)
            }
            _ => None,
        });
        let Some(form) = form else {
            return;
        };
        self.defaults = super::super::defaults::remember(&self.defaults, &form);
        self.save_defaults();
    }
    fn save_defaults(&mut self) {
        let task = Task::SaveDefaults(self.workspace.clone(), self.defaults.to_string());
        self.queue_local_action(task);
    }
}

impl App {
    fn accept_preferences(&mut self, job: (u64, &Task), payload: Payload) {
        let (id, task) = job;
        if matches!(task, Task::LoadDefaults(_)) {
            self.load_preferences(payload);
            return;
        }
        if matches!(task, Task::SaveDefaults(..)) {
            self.finish_local_action(id);
        }
        if let Payload::Error { diagnostic, .. } = payload {
            self.toast(diagnostic.message, Tone::Failure);
        }
    }

    fn load_preferences(&mut self, payload: Payload) {
        match payload {
            Payload::Result { contents, .. } => {
                let empty = self
                    .defaults
                    .as_object()
                    .is_none_or(serde_json::Map::is_empty);
                if empty {
                    self.defaults = contents["defaults"].clone();
                    self.hydrate_drafts();
                }
            }
            Payload::Error { diagnostic, .. } => self.toast(diagnostic.message, Tone::Failure),
            Payload::Event(_) => {}
        }
    }

    fn hydrate_drafts(&mut self) {
        for page in self.pages.iter_mut().flatten() {
            if let Page::Form(form) = page {
                let mut restored = super::super::defaults::fill(form.clone(), &self.defaults);
                restored.focus = form.focus;
                if restored.entries != form.entries {
                    restored.problem = None;
                }
                *form = restored;
            }
        }
    }
}

impl App {
    pub(super) fn queue_local_action(&mut self, task: Task) {
        self.next_id = self.next_id.saturating_add(1);
        let id = self.next_id;
        self.running.insert(id);
        self.jobs.push(Job::Local { id, task });
    }

    pub(super) fn finish_local_action(&mut self, id: u64) {
        self.running.remove(&id);
        let quitting = self.quit_when_idle && self.running.is_empty();
        if quitting {
            self.quit = true;
        }
    }
}
