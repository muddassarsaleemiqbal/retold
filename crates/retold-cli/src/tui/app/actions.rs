//! Opening things: rows, items, videos and actions. Reads with everything filled in show at once;
//! anything that changes state goes through its form and a confirmation first.
use super::super::catalog::{Action, Risk};
use super::super::collection::{Collection, Opening};
use super::super::dashboard::Target;
use super::super::form::Form;
use super::super::landing::Tab;
use super::super::page::{Modal, Outcome, Page, Reading, VideoPage};
use super::super::runtime::Job;
use super::super::section::Section;
use super::App;

/// `(argument id, value)` pairs filled into a form when it opens.
pub type Fill = Vec<(String, String)>;

impl App {
    /// Enter on the landing: open the row's video, action or item menu.
    pub(super) fn activate(&mut self) {
        match self.tab() {
            Tab::Dashboard => self.activate_dashboard(),
            Tab::Collection(collection) => self.activate_item(collection),
            Tab::Actions => self.activate_action(),
            Tab::Spend | Tab::Report(_) => {}
        }
    }

    fn activate_action(&mut self) {
        let selected = self.landing().selected();
        let actions = self.section_actions();
        let index = actions.get(selected).map(|(index, _)| *index);
        if let Some(index) = index {
            self.open_action(index, &Fill::new());
        }
    }

    fn activate_dashboard(&mut self) {
        let selected = self.landing().selected();
        let Some(row) = self.dashboard_rows().into_iter().nth(selected) else {
            return;
        };
        match row.target {
            Target::Action(path) => self.open_path(path, &Fill::new()),
            Target::Section(section) => self.go(section),
            Target::Video(reference) => self.open_video(&reference),
        }
    }

    fn activate_item(&mut self, collection: &'static Collection) {
        let Some(item) = self.selected_item() else {
            return;
        };
        match collection.opening {
            Opening::Video => self.open_video(&item.key),
            Opening::Menu => self.open_menu(),
            Opening::Inspect => {}
            _ => self.open_collection_item(collection, &item),
        }
    }

    fn open_collection_item(
        &mut self,
        collection: &Collection,
        item: &super::super::collection::Item,
    ) {
        if collection.opening == Opening::Diagnostic {
            self.open_path("explain", &vec![("code".into(), item.key.clone())]);
            return;
        }
        let command = match collection.opening {
            Opening::Series => Some(("series show", "series")),
            Opening::Reconcile => Some(("account reconcile", "attempt")),
            Opening::Attach => Some(("queue attach", "id")),
            _ => None,
        };
        if let Some((path, field)) = command {
            self.open_path(path, &vec![(field.into(), item.key.clone())]);
        } else if collection.opening == Opening::Document {
            let kind = if collection == &super::super::collection::GUIDES {
                "guides"
            } else {
                "channels"
            };
            self.open_path(
                &format!("{kind} document"),
                &vec![("id".into(), item.key.clone())],
            );
        } else if collection.opening == Opening::Limit {
            self.open_path("account limit", &super::super::money::limit_fill(&item.raw));
        } else {
            self.open_provider_item(collection, item);
        }
    }

    fn open_provider_item(
        &mut self,
        collection: &Collection,
        item: &super::super::collection::Item,
    ) {
        match collection.opening {
            Opening::Route => self.open_route(item),
            Opening::Discovery => {
                self.open_path("providers add", &super::super::discovery::fill(item));
            }
            _ => self.push(Page::Reading(Reading::provider(&item.key))),
        }
    }

    fn open_route(&mut self, item: &super::super::collection::Item) {
        let inventory = super::super::store::words("providers list");
        let Some(providers) = self.store.contents(&inventory) else {
            self.toast(
                "Provider inventory unavailable. Refresh Providers, then try again.".into(),
                super::Tone::Notice,
            );
            return;
        };
        self.modal = Some(Modal::Route(super::super::routes::Editor::new(
            &item.key, &item.raw, providers,
        )));
    }

    pub fn open_video(&mut self, reference: &str) {
        self.go(Section::Videos);
        self.push(Page::Video(VideoPage::new(reference)));
    }

    pub(super) fn create(&mut self) {
        if let Tab::Collection(collection) = self.tab()
            && let Some(path) = collection.create
        {
            let fill: Fill = collection
                .create_with
                .iter()
                .map(|(id, value)| ((*id).to_owned(), (*value).to_owned()))
                .collect();
            self.open_path(path, &fill);
        }
    }

    pub(super) fn push(&mut self, page: Page) {
        let here = self.here();
        self.pages[here].push(page);
    }

    pub(super) fn pop(&mut self) {
        let here = self.here();
        self.pages[here].pop();
    }

    /// Opens the action at `path`, such as `video continue`.
    pub fn open_path(&mut self, path: &str, fill: &Fill) {
        let index = self
            .catalog
            .iter()
            .position(|action| action.command() == path);
        if let Some(index) = index {
            self.open_action(index, fill);
        }
    }

    /// Reads with everything filled in run at once; anything else opens its form first.
    pub fn open_action(&mut self, index: usize, fill: &Fill) {
        let Some(action) = self.catalog.get(index).cloned() else {
            return;
        };
        let form = super::onboarding::defaults(Form::new(action));
        let form = fill
            .iter()
            .fold(form, |form, (id, value)| form.with(id, value));
        let form = super::super::defaults::fill(form, &self.defaults);
        let reads = form.action.risk == Risk::Read;
        let asks_nothing = form.action.fields.is_empty();
        if reads && form.complete() {
            self.read(form);
        } else if asks_nothing {
            self.modal = Some(Modal::Confirm(form));
        } else {
            self.push(Page::Form(form));
        }
    }

    fn read(&mut self, form: Form) {
        let stream =
            (form.action.command() == "queue attach").then(|| super::super::stream::Stream {
                follow: true,
                ..Default::default()
            });
        self.push(Page::Reading(Reading {
            title: form.action.label(),
            words: form.arguments(),
            form: Some(form),
            scroll: 0,
            stream,
        }));
    }

    pub(super) fn submit(&mut self, mut form: Form) {
        form.problem = form.review_problem();
        self.pop();
        let invalid = form.problem.is_some();
        let reads = form.action.risk == Risk::Read;
        if invalid {
            self.push(Page::Form(form));
        } else if reads {
            self.read(form);
        } else {
            self.confirm(form);
        }
    }

    pub(super) fn confirm(&mut self, form: Form) {
        if let Some(preview) = super::super::preview::Preview::new(&form) {
            self.store.remove(&preview.words);
            self.modal = Some(Modal::Preview(preview));
            return;
        }
        let permanent = form.arguments().iter().any(|word| word == "--force");
        if permanent {
            self.modal = Some(Modal::Destroy {
                form,
                typed: super::super::input::Input::default(),
            });
            return;
        }
        if let Some(review) = super::super::production::Review::new(form.clone()) {
            self.store.remove(&review.words);
            self.modal = Some(Modal::Plan(review));
        } else {
            self.modal = Some(Modal::Confirm(form));
        }
    }

    pub(super) fn run(&mut self, form: Form) {
        if let Some(reason) = self.workspace_wait(&form) {
            self.toast(reason.into(), super::Tone::Notice);
            self.push(Page::Form(form));
        } else {
            self.dispatch(confirmed(form));
        }
    }

    /// Sends a confirmed action and opens the page its result will arrive on.
    fn dispatch(&mut self, form: Form) {
        self.next_id = self.next_id.saturating_add(1);
        let id = self.next_id;
        let words = form.arguments();
        self.queue_action(id, words.clone());
        self.push(Page::Outcome(Outcome {
            id,
            form,
            words,
            payload: Option::default(),
            started: self.now,
            scroll: 0,
        }));
    }

    /// Marks the action running until its reply arrives, so quitting asks first.
    fn queue_action(&mut self, id: u64, words: Vec<String>) {
        self.running.insert(id);
        self.jobs.push(Job::Action { id, words });
    }

    pub(super) fn edit_again(&mut self) {
        let form = match self.page() {
            Some(Page::Reading(reading)) => reading.form.clone(),
            Some(Page::Outcome(outcome)) => Some(outcome.form.clone()),
            _ => Option::default(),
        };
        if let Some(form) = form {
            self.pop();
            self.push(Page::Form(form));
        }
    }

    pub(super) fn video_action(&mut self, path: &str) {
        let Some(Page::Video(video)) = self.page() else {
            return;
        };
        let fill = vec![("reference".to_owned(), video.reference.clone())];
        self.open_path(path, &fill);
    }

    /// Values for the selected item or open video, for the action's subject argument.
    #[must_use]
    pub(super) fn context(&self, action: &Action) -> Fill {
        if let Some(Page::Video(video)) = self.page() {
            return subject_fill(action, &["reference"], &video.reference);
        }
        let (Tab::Collection(collection), Some(item)) = (self.tab(), self.selected_item()) else {
            return Fill::new();
        };
        let command = action.command();
        let applies = collection
            .groups
            .iter()
            .any(|group| command.starts_with(group));
        if applies {
            subject_fill(action, collection.subjects, &item.key)
        } else {
            Fill::new()
        }
    }
}

fn subject_fill(action: &Action, subjects: &[&str], key: &str) -> Fill {
    action
        .fields
        .iter()
        .find(|field| field.long.is_none() && subjects.contains(&field.id.as_str()))
        .map(|field| vec![(field.id.clone(), key.to_owned())])
        .unwrap_or_default()
}

/// Removals were confirmed in the app, so the command's own `--yes` prompt is answered.
fn confirmed(form: Form) -> Form {
    if form.action.risk == Risk::Destructive {
        form.with("yes", "true")
    } else {
        form
    }
}
