//! Retained provider setup and immutable reviews; no I/O runs in native input callbacks.
use super::{Destination, Shell};
use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
use gpui_kit::{AppContext as _, Context, Entity, Subscription, Window};
use retold_app::desktop::providers::{self, Change, DefinitionDraft, Kind, Review, Snapshot};
use std::path::PathBuf;

pub(super) struct ProviderPanel {
    pub snapshot: Option<Snapshot>,
    pub form: Option<ProviderForm>,
    pub review: Option<Review>,
    pub inventory: Option<providers::Inventory>,
    pub observations: Option<providers::ObservationSet>,
    pub error: Option<String>,
    pub busy: bool,
    /// Show every task's route; by default only the one drafting needs and any already set.
    pub show_all_routes: bool,
    sequence: u64,
}
impl ProviderPanel {
    pub fn new() -> Self {
        Self {
            snapshot: None,
            form: None,
            review: None,
            inventory: None,
            show_all_routes: false,
            observations: None,
            error: None,
            busy: false,
            sequence: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) enum Mode {
    ClearRoute(String),
    Add,
    Replace(String),
    Remove(String),
    Route(String),
}
pub(super) struct ProviderForm {
    pub mode: Mode,
    pub kind: Kind,
    pub advanced: bool,
    pub id: Entity<InputState>,
    pub parallel: Entity<InputState>,
    pub binary: Entity<InputState>,
    pub profile: Entity<InputState>,
    pub key: Entity<InputState>,
    pub selector: Entity<InputState>,
    pub model: Entity<InputState>,
    pub ffmpeg: Entity<InputState>,
    pub order: Entity<InputState>,
    pub definition: Entity<TextareaState>,
    _subscriptions: Vec<Subscription>,
}
impl ProviderForm {
    fn new(mode: Mode, window: &mut Window, cx: &mut Context<Shell>) -> Self {
        let fields = [
            "",
            "1",
            "",
            "",
            "OPENROUTER_API_KEY",
            "default",
            "",
            "ffmpeg",
            "",
        ];
        let inputs: Vec<_> = fields
            .into_iter()
            .map(|text| {
                cx.new(|cx| {
                    let mut input = InputState::new(window, cx);
                    input.set_value(text, window, cx);
                    input
                })
            })
            .collect();
        let subscriptions = inputs
            .iter()
            .map(|input| {
                cx.subscribe(input, |this, _, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.providers.review = None;
                        cx.notify();
                    }
                })
            })
            .collect::<Vec<_>>();
        let definition = cx.new(|cx| TextareaState::new(window, cx));
        let mut form = Self {
            mode,
            kind: Kind::LocalVoice,
            advanced: false,
            id: inputs[0].clone(),
            parallel: inputs[1].clone(),
            binary: inputs[2].clone(),
            profile: inputs[3].clone(),
            key: inputs[4].clone(),
            selector: inputs[5].clone(),
            model: inputs[6].clone(),
            ffmpeg: inputs[7].clone(),
            order: inputs[8].clone(),
            definition,
            _subscriptions: subscriptions,
        };
        form._subscriptions.push(cx.subscribe(
            &form.definition,
            |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.providers.review = None;
                    cx.notify();
                }
            },
        ));
        form
    }
    fn change(&self, cx: &Context<Shell>) -> Result<Change, String> {
        match &self.mode {
            Mode::Add => Ok(Change::Add {
                id: value(&self.id, cx),
                definition: self.definition_text(cx)?,
            }),
            Mode::Replace(id) => Ok(Change::Replace {
                id: id.clone(),
                definition: self.definition_text(cx)?,
            }),
            Mode::Remove(id) => Ok(Change::Remove { id: id.clone() }),
            Mode::ClearRoute(tier) => Ok(Change::ClearRoute { tier: tier.clone() }),
            Mode::Route(tier) => Ok(Change::Route {
                tier: tier.clone(),
                order: value(&self.order, cx),
            }),
        }
    }
    fn definition_text(&self, cx: &Context<Shell>) -> Result<String, String> {
        if self.advanced {
            return Ok(self.definition.read(cx).value().to_string());
        }
        DefinitionDraft {
            kind: self.kind,
            parallel: &value(&self.parallel, cx),
            binary: &value(&self.binary, cx),
            profile: &value(&self.profile, cx),
            key_variable: &value(&self.key, cx),
            selector: &value(&self.selector, cx),
            model: &value(&self.model, cx),
            ffmpeg: &value(&self.ffmpeg, cx),
        }
        .definition()
        .map_err(|error| error.message)
    }
}
fn value(input: &Entity<InputState>, cx: &Context<Shell>) -> String {
    input.read(cx).value().trim().to_owned()
}

enum Task {
    Observe {
        root: PathBuf,
        version: String,
    },
    Load(Option<PathBuf>),
    Scan,
    Preview {
        root: PathBuf,
        version: String,
        change: Change,
    },
    Apply(Review),
}
enum Reply {
    Observed(providers::ObservationSet),
    Loaded(Snapshot),
    Scanned(providers::Inventory),
    Reviewed(Review),
    Applied(Snapshot),
}
impl Task {
    fn execute(self) -> Result<Reply, String> {
        let result = match self {
            Self::Observe { root, version } => {
                providers::observations(&root, &version).map(Reply::Observed)
            }
            Self::Load(root) => {
                let root = root
                    .map_or_else(providers::default_directory, Ok)
                    .map_err(|error| error.message)?;
                providers::inspect(&root).map(Reply::Loaded)
            }
            Self::Scan => providers::scan().map(Reply::Scanned),
            Self::Preview {
                root,
                version,
                change,
            } => providers::preview(&root, &version, &change).map(Reply::Reviewed),
            Self::Apply(review) => providers::apply(&review).map(Reply::Applied),
        };
        result.map_err(|error| error.message)
    }
}
/// How often and how long deferred work waits for the app to become idle.
const IDLE_POLL: std::time::Duration = std::time::Duration::from_millis(100);
const IDLE_ATTEMPTS: usize = 50;

impl Shell {
    pub(super) fn go_to(&mut self, destination: Destination, cx: &mut Context<Self>) {
        if destination != Destination::Videos {
            self.clear_media(cx);
        }
        if self.destination != destination {
            self.status.clear();
        }
        self.destination = destination;
        self.show_lab = false;
        self.confirm_stop = false;
        self.form = None;
        self.show_actions = false;
        if destination == Destination::Providers && self.providers.snapshot.is_none() {
            self.load_providers(cx);
        }
        if destination == Destination::Series && self.series.list.is_none() {
            self.series_task(super::series::SeriesTask::List, cx);
        }
        if destination == Destination::Budget && self.budget.report.is_none() {
            self.load_budget(cx);
        }
        cx.notify();
    }
    /// Read provider settings once a workspace opens, so Home can warn about missing ones.
    /// After a workspace opens: read provider settings, then run the setup checklist,
    /// each once the app is idle, so Home can guide a first-time setup.
    pub(super) fn preload_providers(&mut self, cx: &mut Context<Self>) {
        if self.providers.snapshot.is_none() {
            self.when_idle(cx, |this, cx| {
                if this.providers.snapshot.is_none() {
                    this.load_providers(cx);
                }
            });
        }
        self.when_idle(cx, |this, cx| {
            this.readiness_task(super::readiness::ReadinessTask::Check, cx);
        });
    }

    /// Run `then` as soon as no task is running, checking every 100ms for up to five seconds.
    pub(super) fn when_idle(
        &mut self,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Self, &mut Context<Self>) + 'static,
    ) {
        cx.spawn(async move |this, cx| {
            let mut then = Some(then);
            for _ in 0..IDLE_ATTEMPTS {
                let ran = this
                    .update(cx, |this, cx| {
                        if this.busy {
                            return false;
                        }
                        if let Some(then) = then.take() {
                            then(this, cx);
                        }
                        true
                    })
                    .unwrap_or(true);
                if ran {
                    return;
                }
                cx.background_executor().timer(IDLE_POLL).await;
            }
        })
        .detach();
    }

    pub(super) fn load_providers(&mut self, cx: &mut Context<Self>) {
        let root = self
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.context.config.clone());
        self.provider_task(Task::Load(root), cx);
    }
    pub(super) fn observe_providers(&mut self, cx: &mut Context<Self>) {
        if let Some(snapshot) = &self.providers.snapshot {
            self.provider_task(
                Task::Observe {
                    root: snapshot.root.clone(),
                    version: snapshot.version.clone(),
                },
                cx,
            );
        }
    }
    pub(super) fn scan_providers(&mut self, cx: &mut Context<Self>) {
        self.provider_task(Task::Scan, cx);
    }
    pub(super) fn begin_provider_form(
        &mut self,
        mode: Mode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.providers.busy || self.providers.form.is_some() {
            return;
        }
        let mut form = ProviderForm::new(mode.clone(), window, cx);
        if let Some(snapshot) = &self.providers.snapshot {
            match mode {
                Mode::Replace(id) => {
                    let Some(text) = snapshot.definitions.get(&id) else {
                        return;
                    };
                    form.advanced = true;
                    form.definition
                        .update(cx, |input, cx| input.set_value(text.clone(), window, cx));
                }
                Mode::Route(tier) => {
                    let order = snapshot
                        .routing
                        .get(&tier)
                        .map(|chain| chain.join(", "))
                        .unwrap_or_default();
                    form.order
                        .update(cx, |input, cx| input.set_value(order, window, cx));
                }
                _ => {}
            }
        } else {
            return;
        }
        self.providers.form = Some(form);
        self.providers.review = None;
        self.providers.error = None;
        cx.notify();
    }
    pub(super) fn provider_advanced(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = &mut self.providers.form else {
            return;
        };
        if self.providers.busy || form.advanced {
            return;
        }
        match form.definition_text(cx) {
            Ok(text) => {
                form.definition
                    .update(cx, |input, cx| input.set_value(text, window, cx));
                form.advanced = true;
                self.providers.review = None;
            }
            Err(error) => self.providers.error = Some(error),
        }
        cx.notify();
    }
    pub(super) fn review_provider(&mut self, cx: &mut Context<Self>) {
        let (Some(snapshot), Some(form)) = (&self.providers.snapshot, &self.providers.form) else {
            return;
        };
        match form.change(cx) {
            Ok(change) => self.provider_task(
                Task::Preview {
                    root: snapshot.root.clone(),
                    version: snapshot.version.clone(),
                    change,
                },
                cx,
            ),
            Err(error) => {
                self.providers.error = Some(error);
                cx.notify();
            }
        }
    }
    pub(super) fn apply_provider(&mut self, cx: &mut Context<Self>) {
        if let Some(review) = self.providers.review.clone() {
            self.provider_task(Task::Apply(review), cx);
        }
    }
    fn provider_task(&mut self, task: Task, cx: &mut Context<Self>) {
        if self.busy || self.providers.busy {
            return;
        }
        self.busy = true;
        self.providers.busy = true;
        self.providers.error = None;
        self.providers.sequence = self.providers.sequence.wrapping_add(1);
        let sequence = self.providers.sequence;
        let work = cx
            .background_executor()
            .spawn(async move { task.execute() });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                if this.providers.sequence != sequence { return; }
                this.providers.busy = false;
                this.busy = false;
                let settings_changed = matches!(result, Ok(Reply::Loaded(_) | Reply::Applied(_)));
                match result {
                    Ok(Reply::Loaded(snapshot)) => { this.providers.snapshot = Some(snapshot); this.providers.review = None; this.providers.observations = None; }
                    Ok(Reply::Observed(observations)) => this.providers.observations = Some(observations),
                    Ok(Reply::Scanned(inventory)) => this.providers.inventory = Some(inventory),
                    Ok(Reply::Reviewed(review)) => this.providers.review = Some(review),
                    Ok(Reply::Applied(snapshot)) => {
                        this.providers.snapshot = Some(snapshot);
                        this.providers.form = None;
                        this.providers.observations = None;
                        this.providers.review = None;
                        this.review = None;
                        if let Some(delivery) = &mut this.delivery { delivery.review = None; }
                        this.status = "Provider configuration saved. No requests sent. Review a fresh run to use it.".into();
                    }
                    Err(error) => { this.providers.error = Some(error); this.providers.review = None; this.providers.observations = None; }
                }
                if settings_changed {
                    this.readiness_task(super::readiness::ReadinessTask::Check, cx);
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }
}
