//! App state. Keys and replies change it; it never touches the terminal, disk or network. The
//! runtime asks `wants` which reads to load and `jobs` which actions to run.
mod actions;
mod bulk;
mod clipboard;
mod document;
mod files;
mod keys;
mod modals;
mod mouse;
mod onboarding;
mod operations;
mod paste;
mod providers;
pub mod review;
mod series;
mod stream;
mod workspace;

use ratatui::layout::Rect;
use retold_app::protocol::Payload;

use super::catalog::{self, Action};
use super::collection::{self, Collection, Item};
use super::dashboard;
use super::landing::{self, Landing, Tab};
use super::page::{Modal, Page};
use super::runtime::Job;
use super::section::{self, Section};
use super::store::{self, Store, Want};
use super::theme::Theme;

/// How long a notice stays on screen, in milliseconds.
const TOAST_MILLIS: u64 = 4_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Sidebar,
    Main,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Notice,
    Success,
    Failure,
}

#[derive(Debug, Clone)]
pub struct Toast {
    pub text: String,
    pub tone: Tone,
    pub until: u64,
}

#[derive(Debug, Default)]
pub struct AccountPages {
    pub attempts: Vec<String>,
    pub processes: Vec<String>,
}

/// Host programs either borrow the terminal or run as a tracked background action.
#[derive(Debug, Default)]
pub struct HostState {
    pub handoff: Option<super::handoff::Handoff>,
    pub clipboard: Option<u64>,
}

#[derive(Debug)]
pub struct App {
    pub theme: Theme,
    pub defaults: serde_json::Value,
    pub spend: super::spend_screen::Selection,
    pub account_pages: AccountPages,
    pub selected_videos: std::collections::BTreeSet<String>,
    pub section: Section,
    pub focus: Focus,
    landings: Vec<Landing>,
    /// Open pages per section, newest last.
    pages: Vec<Vec<Page>>,
    pub modal: Option<Modal>,
    pub toasts: Vec<Toast>,
    pub store: Store,
    catalog: Vec<Action>,
    /// Milliseconds since the app started; drives spinners and refresh.
    pub now: u64,
    /// The terminal size, for mouse hit-testing.
    pub area: Rect,
    pub quit: bool,
    pub host: HostState,
    pub switch_to: Option<std::path::PathBuf>,
    pub workspace: Option<std::path::PathBuf>,
    /// The onboarding step being worked through, when guided setup is running.
    setup_step: Option<usize>,
    onboarding_checked: bool,
    pub running: std::collections::BTreeSet<u64>,
    pub quit_when_idle: bool,
    pub media: MediaState,
    jobs: Vec<Job>,
    next_id: u64,
}

impl App {
    pub fn handoff_finished(
        &mut self,
        result: std::io::Result<()>,
        handoff: &super::handoff::Handoff,
    ) {
        if result.is_ok()
            && let super::handoff::Handoff::Editor {
                check: Some(words), ..
            } = handoff
        {
            self.modal = None;
            self.push(Page::Reading(super::page::Reading {
                title: "Configuration check".into(),
                words: words.clone(),
                form: None,
                scroll: 0,
                stream: None,
            }));
        }
        match result {
            Ok(()) => self.store.invalidate(),
            Err(error) => self.toast(error.to_string(), Tone::Failure),
        }
    }
    #[must_use]
    pub fn new(theme: Theme) -> Self {
        Self {
            theme,
            defaults: serde_json::json!({}),
            spend: super::spend_screen::Selection::default(),
            account_pages: AccountPages::default(),
            selected_videos: std::collections::BTreeSet::new(),
            section: Section::Home,
            focus: Focus::Main,
            landings: section::ALL.into_iter().map(Landing::new).collect(),
            pages: section::ALL.iter().map(|_| Vec::new()).collect(),
            modal: Option::default(),
            toasts: Vec::new(),
            store: Store::default(),
            catalog: catalog::actions(),
            now: 0,
            area: Rect::default(),
            quit: false,
            host: HostState::default(),
            switch_to: None,
            workspace: None,
            setup_step: Option::default(),
            onboarding_checked: false,
            running: std::collections::BTreeSet::new(),
            quit_when_idle: false,
            media: MediaState::default(),
            jobs: Vec::new(),
            next_id: 0,
        }
    }

    /// The current section's slot in `landings` and `pages`.
    const fn here(&self) -> usize {
        self.section.position()
    }

    #[must_use]
    pub fn landing(&self) -> &Landing {
        &self.landings[self.here()]
    }

    #[must_use]
    pub fn tab(&self) -> Tab {
        let tabs = landing::tabs(self.section);
        tabs.get(self.landing().tab)
            .copied()
            .unwrap_or(Tab::Actions)
    }

    #[must_use]
    pub fn page(&self) -> Option<&Page> {
        self.pages[self.here()].last()
    }

    /// The section's actions, narrowed by the Actions tab filter.
    #[must_use]
    pub fn section_actions(&self) -> Vec<(usize, &Action)> {
        let filter = match self.tab() {
            Tab::Actions => self.landing().filter().to_lowercase(),
            _ => String::new(),
        };
        let needles: Vec<&str> = filter.split_whitespace().collect();
        let mentions = |action: &Action| {
            let haystack = format!("{} {} {}", action.command(), action.title, action.about);
            let lowered = haystack.to_lowercase();
            needles.iter().all(|needle| lowered.contains(needle))
        };
        self.catalog
            .iter()
            .enumerate()
            .filter(|(_, action)| action.section == self.section && mentions(action))
            .collect()
    }

    #[must_use]
    pub fn selected_action(&self) -> Option<&Action> {
        let selected = self.landing().selected();
        let actions = self.section_actions();
        actions.get(selected).map(|(_, action)| *action)
    }

    /// Items of the current collection tab after the filter.
    #[must_use]
    pub fn visible_items(&self) -> Vec<Item> {
        let Tab::Collection(collection) = self.tab() else {
            return Vec::new();
        };
        let words = self.collection_words(collection);
        let all = self
            .store
            .contents(&words)
            .map(|contents| collection::items(collection, contents))
            .unwrap_or_default();
        let filter = self.landing().filter();
        let unfiltered = filter.trim().is_empty();
        if unfiltered {
            return all;
        }
        let matching = collection::matching(&all, filter);
        matching.into_iter().cloned().collect()
    }

    #[must_use]
    pub fn selected_item(&self) -> Option<Item> {
        let selected = self.landing().selected();
        self.visible_items().into_iter().nth(selected)
    }

    #[must_use]
    pub fn dashboard_rows(&self) -> Vec<dashboard::Row> {
        dashboard::rows(&self.store)
    }

    /// Rows in the current tab, for moving the selection.
    #[must_use]
    pub fn row_count(&self) -> usize {
        match self.tab() {
            Tab::Dashboard => count(&self.dashboard_rows()),
            Tab::Collection(_) => count(&self.visible_items()),
            Tab::Actions => count(&self.section_actions()),
            Tab::Spend | Tab::Report(_) => 0,
        }
    }
}

/// What the footer and the runtime need.
impl App {
    /// The command for whatever is selected, shown in the footer.
    #[must_use]
    pub fn command(&self) -> String {
        if let Some(Modal::Route(editor)) = &self.modal {
            return super::form::command_line(&editor.words());
        }
        if let Some(Modal::Preview(preview)) = &self.modal {
            return preview.form.command_line();
        }
        if let Some(Modal::Confirm(form)) = &self.modal {
            return form.command_line();
        }
        if let Some(Modal::Plan(review)) = &self.modal {
            return self
                .store
                .current_contents(&review.words)
                .and_then(|contents| review.executable(contents))
                .unwrap_or_else(|| review.form.clone())
                .command_line();
        }
        match self.page() {
            Some(Page::Form(form)) => form.command_line(),
            Some(Page::Reading(reading)) => super::form::command_line(&reading.words),
            Some(Page::Outcome(outcome)) => super::form::command_line(&outcome.words),
            Some(Page::Video(video)) => super::form::command_line(&video.words()),
            None => self.landing_command(),
        }
    }

    fn landing_command(&self) -> String {
        match self.tab() {
            Tab::Dashboard => "retold home".into(),
            Tab::Spend => super::form::command_line(&self.spend.words()),
            Tab::Report(report) => format!("retold {}", report.command),
            Tab::Collection(collection) => self.selected_item().map_or_else(
                || format!("retold {}", collection.list),
                |item| detail_command(collection, &item),
            ),
            Tab::Actions => self.selected_action().map_or_else(
                || "retold --help".into(),
                |action| format!("retold {}", action.command()),
            ),
        }
    }

    /// Every read the visible screen needs; the header's are always included.
    #[must_use]
    pub fn wants(&self) -> Vec<Want> {
        let mut wants = self.modal_wants();
        if let Some(page) = self.page() {
            wants.extend(page_wants(page));
            return wants;
        }
        match self.tab() {
            Tab::Spend => wants.push(self.spend.want()),
            Tab::Report(report) => wants.push(Want::once(report.command)),
            Tab::Collection(collection) => wants.extend(self.collection_wants(collection)),
            Tab::Dashboard | Tab::Actions => {}
        }
        wants
    }

    fn modal_wants(&self) -> Vec<Want> {
        let mut wanted = dashboard::wants();
        let words = match &self.modal {
            Some(Modal::Preview(preview)) => Some(&preview.words),
            Some(Modal::Plan(review)) => Some(&review.words),
            _ => None,
        };
        if let Some(words) = words {
            wanted.push(Want::of(words.clone()));
        }
        wanted
    }

    pub fn collection_words(&self, collection: &Collection) -> Vec<String> {
        let mut words = store::words(collection.list);
        let after = if collection == &collection::ATTEMPTS {
            self.account_pages.attempts.last()
        } else if collection == &collection::PROCESSES {
            self.account_pages.processes.last()
        } else {
            None
        };
        if let Some(after) = after {
            words.extend(["--after".into(), after.clone()]);
        }
        words
    }

    fn collection_wants(&self, collection: &'static Collection) -> Vec<Want> {
        let mut wanted = collection.wants(self.selected_item().as_ref());
        if let Some(primary) = wanted.first_mut() {
            primary.words = self.collection_words(collection);
        }
        wanted
    }

    /// Actions waiting to be sent; `clear_jobs` once they are.
    #[must_use]
    pub fn jobs(&self) -> Vec<Job> {
        self.jobs.clone()
    }

    pub fn clear_jobs(&mut self) {
        self.jobs.clear();
    }

    pub fn receive(&mut self, job: Job, payload: Payload) {
        match job {
            Job::Query(words) => {
                self.store.fill(words.clone(), payload.clone(), self.now);
                self.stream_reply(&words, &payload);
            }
            Job::Action { id, .. } => self.finish(id, &payload),
            Job::Local { id, task } => self.file_reply((id, &task), payload),
        }
        self.maybe_setup();
        self.refresh_media();
        self.refresh_comparison();
    }

    fn finish(&mut self, id: u64, payload: &Payload) {
        self.running.remove(&id);
        let action = self.outcome_action(id);
        self.fill_outcome(id, payload);
        let succeeded = matches!(payload, Payload::Result { .. });
        let title = action.as_ref().map(Action::label).unwrap_or_default();
        let command = action.as_ref().map(Action::command).unwrap_or_default();
        self.announce(&title, payload);
        self.store.invalidate();
        if succeeded {
            self.remember_defaults(id);
            self.follow_switch(&command, payload);
            self.setup_finished(&command);
        }
        let idle = self.running.is_empty();
        if self.quit_when_idle && idle {
            self.quit = true;
        }
    }

    /// The action whose result page waits for reply `id`.
    fn outcome_action(&self, id: u64) -> Option<Action> {
        self.pages.iter().flatten().find_map(|page| match page {
            Page::Outcome(outcome) if outcome.id == id => Some(outcome.form.action.clone()),
            _ => Option::default(),
        })
    }

    fn fill_outcome(&mut self, id: u64, payload: &Payload) {
        for page in self.pages.iter_mut().flatten() {
            if let Page::Outcome(outcome) = page
                && outcome.id == id
            {
                outcome.payload = Some(payload.clone());
            }
        }
    }

    fn announce(&mut self, title: &str, payload: &Payload) {
        let succeeded = matches!(payload, Payload::Result { .. });
        let (text, tone) = if succeeded {
            (format!("{title} finished."), Tone::Success)
        } else {
            (format!("{title} did not complete."), Tone::Failure)
        };
        self.toast(text, tone);
    }

    /// After a successful `workspace switch`, the loop reopens everything on the new root.
    fn follow_switch(&mut self, command: &str, payload: &Payload) {
        let switched = command == "workspace switch";
        if let (true, Payload::Result { contents, .. }) = (switched, payload) {
            self.switch_to = contents["workspace"]["root"].as_str().map(Into::into);
        }
    }

    pub fn tick(&mut self, now: u64) {
        self.now = now;
        self.toasts.retain(|toast| toast.until > now);
    }

    fn toast(&mut self, text: String, tone: Tone) {
        self.toasts.push(Toast {
            text,
            tone,
            until: self.now.saturating_add(TOAST_MILLIS),
        });
    }
}

const fn count<T>(rows: &[T]) -> usize {
    rows.len()
}

fn reading_wants(reading: &super::page::Reading) -> Vec<Want> {
    let provider = reading.provider_id().is_some();
    let streaming = reading
        .stream
        .as_ref()
        .is_some_and(|stream| !stream.terminal);
    let primary = Want {
        words: reading.words.clone(),
        live: provider || streaming,
        interval: if provider {
            store::REFRESH_MILLIS
        } else {
            STREAM_MILLIS
        },
    };
    let health = provider.then(|| Want::live("providers health"));
    std::iter::once(primary).chain(health).collect()
}

fn page_wants(page: &Page) -> Vec<Want> {
    match page {
        Page::Reading(reading) => reading_wants(reading),
        Page::Video(video) => {
            let mut wanted = vec![Want::of(video.words())];
            wanted.extend(review::wants(&video.reference));
            wanted
        }
        Page::Form(form) if form.action.command() == "creative prompt" && form.complete() => {
            vec![Want::of(form.arguments())]
        }
        Page::Form(_) | Page::Outcome(_) => Vec::new(),
    }
}

const STREAM_MILLIS: u64 = 250;

pub use collection::detail_words;

fn detail_command(collection: &Collection, item: &Item) -> String {
    collection.detail.map_or_else(
        || format!("retold {}", collection.list),
        |command| super::form::command_line(&detail_words(command, item)),
    )
}

#[derive(Debug, Default)]
pub struct MediaState {
    pub preview: Option<Payload>,
    pub requested: Option<std::path::PathBuf>,
    pub comparison: Option<Payload>,
    pub comparison_requested: Option<std::path::PathBuf>,
}
