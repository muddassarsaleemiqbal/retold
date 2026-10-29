//! Native workspace shell with asynchronous application requests and explicit scope.
mod account_admin;
mod activity;
mod admin;
mod authoring;
mod authoring_forms;
pub mod backend;
mod backups;
mod budget;
mod catalog;
mod characters;
pub mod commands;
mod controls;
mod delivery;
mod delivery_presets;
mod destinations;
mod draft;
mod editing;
mod follow;
mod form_choices;
pub(super) mod forms;
mod lifecycle;
mod lines;
mod materials;
mod narration;
pub mod navigation;
mod notes;
mod onboarding;
mod player;
mod preferences;
mod providers;
mod readiness;
mod recovery;
mod retake;
mod run;
mod saved_media;
mod scenes;
mod screen;
mod series;
mod setup;
mod source;
mod video_budget;
mod video_lifecycle;
mod views;
mod writing;

use std::path::PathBuf;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{AppContext, Context, Entity, PathPromptOptions, Subscription, Window};
use retold_app::desktop::WorkspaceSnapshot;

use self::backend::Request;
use self::draft::DraftForm;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Destination {
    Home,
    Videos,
    Series,
    Activity,
    Providers,
    Budget,
    Settings,
    Help,
}

pub struct Shell {
    focus: gpui_kit::FocusHandle,
    pub snapshot: Option<WorkspaceSnapshot>,
    providers: providers::ProviderPanel,
    budget: budget::BudgetPanel,
    delivery: Option<delivery::DeliveryPanel>,
    characters: Option<characters::CharacterPanel>,
    saved_media: Option<saved_media::MediaPanel>,
    notes: Option<notes::NotePanel>,
    recovery: recovery::Recovery,
    show_notes: bool,
    pub destination: Destination,
    pub selected_video: Option<String>,
    pub busy: bool,
    pub control_busy: bool,
    pub status: String,
    pub light: bool,
    zoom: u16,
    appearance_mode: retold_app::desktop::preferences::Appearance,
    high_contrast: bool,
    preferences: retold_app::desktop::preferences::Preferences,
    preferences_enabled: bool,
    preference_saving: bool,
    pending_preferences: Vec<retold_app::desktop::preferences::Change>,
    preference_error: Option<String>,
    quit_after_preferences: bool,
    presentation_generation: u64,
    catalog_page: Option<retold_app::library::VideoPage>,
    catalog_options: retold_app::library::ListOptions,
    catalog_back: Vec<Option<String>>,
    review: Option<retold_app::desktop::ReviewedRun>,
    allowance: Entity<InputState>,
    request_limit: Entity<InputState>,
    new_root: Option<PathBuf>,
    workspace_name: Entity<InputState>,
    import: Option<setup::ImportForm>,
    prompting: bool,
    activity_options: retold_app::queue::ListOptions,
    observed_job: Option<backend::ObservedJob>,
    follow: follow::Follow,
    last_stop: Option<retold_app::studio::ScopedStop>,
    control_generation: u64,
    refresh_error: Option<String>,
    search: Entity<InputState>,
    video_budget: Entity<InputState>,
    /// Bumped on every keystroke so only the last pause in typing runs a search.
    search_generation: u64,
    draft: Option<DraftForm>,
    editor: Option<editing::ScriptEditor>,
    show_script: bool,
    scenes: Option<scenes::SceneEditor>,
    lines: Option<lines::LinesPanel>,
    materials: Option<materials::MaterialsPanel>,
    narration: Option<narration::NarrationPanel>,
    retake: Option<retold_app::retake::ReviewedRetake>,
    /// The video a pending retake review belongs to; a review never follows you to another video.
    retake_video: Option<String>,
    series: series::SeriesPanel,
    admin: admin::AdminPanel,
    forms: authoring_forms::CreationForms,
    authoring: authoring::AuthoringPanel,
    views: views::ViewsPanel,
    audio_form: materials::AudioForm,
    backups: backups::BackupPanel,
    show_administration: bool,
    copies: destinations::DestinationPanel,
    account_admin: account_admin::AccountAdminPanel,
    onboarding: onboarding::OnboardingPanel,
    confirm_trash: bool,
    selection: std::collections::BTreeSet<String>,
    readiness: readiness::ReadinessPanel,
    form: Option<forms::OperationForm>,
    show_actions: bool,
    show_developer: bool,
    confirm_stop: bool,
    status_seen: (String, std::time::Instant),
    shown_page: String,
    show_scenes: bool,
    compact_layout: bool,
    lab: Entity<crate::view::Studio>,
    show_lab: bool,
    request_sequence: u64,
    _subscriptions: Vec<Subscription>,
}

impl std::fmt::Debug for Shell {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Shell")
            .field("destination", &self.destination)
            .field("busy", &self.busy)
            .finish_non_exhaustive()
    }
}

impl Shell {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let weak = cx.entity().downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            weak.update(cx, |this, cx| {
                if this.has_unsaved_setup(cx) || this.preferences_enabled || this.recovery_pending()
                {
                    this.request_quit(window, cx);
                    false
                } else {
                    true
                }
            })
            .unwrap_or(true)
        });
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Find a video…"));
        let subscription = cx.subscribe(&search, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                this.search_videos(cx);
            }
            if matches!(event, InputEvent::Change) {
                this.search_as_you_type(cx);
                cx.notify();
            }
        });
        let allowance = zero_input(window, cx);
        let request_limit = zero_input(window, cx);
        let appearance_subscription = cx.observe_window_appearance(window, |this, window, cx| {
            if this.appearance_mode == retold_app::desktop::preferences::Appearance::System {
                this.apply_appearance(window, cx);
            }
        });
        let mut subscriptions = vec![subscription, appearance_subscription];
        for state in [&allowance, &request_limit] {
            subscriptions.push(cx.subscribe(state, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.review = None;
                    cx.notify();
                }
            }));
        }
        Self {
            focus,
            snapshot: None,
            providers: providers::ProviderPanel::new(),
            budget: budget::BudgetPanel::new(),
            delivery: None,
            characters: None,
            saved_media: None,
            notes: None,
            recovery: recovery::Recovery::default(),
            show_notes: false,
            destination: Destination::Home,
            selected_video: None,
            busy: false,
            control_busy: false,
            status: String::new(),
            light: false,
            zoom: 100,
            appearance_mode: retold_app::desktop::preferences::Appearance::Dark,
            high_contrast: false,
            preferences: Default::default(),
            preferences_enabled: false,
            preference_saving: false,
            pending_preferences: Vec::new(),
            preference_error: None,
            quit_after_preferences: false,
            presentation_generation: 0,
            catalog_page: None,
            catalog_options: Default::default(),
            catalog_back: Vec::new(),
            review: None,
            allowance,
            request_limit,
            new_root: None,
            workspace_name: cx.new(|cx| InputState::new(window, cx).placeholder("Workspace name")),
            search_generation: 0,
            video_budget: cx.new(|cx| InputState::new(window, cx).placeholder("USD, such as 2.50")),
            import: None,
            prompting: false,
            activity_options: Default::default(),
            observed_job: None,
            follow: follow::Follow::default(),
            last_stop: None,
            control_generation: 0,
            refresh_error: None,
            search,
            draft: None,
            editor: None,
            show_script: false,
            scenes: None,
            lines: None,
            materials: None,
            narration: None,
            retake: None,
            retake_video: None,
            series: series::SeriesPanel::default(),
            admin: admin::AdminPanel::default(),
            forms: authoring_forms::CreationForms::new(window, cx),
            authoring: authoring::AuthoringPanel::new(window, cx),
            views: views::ViewsPanel::new(window, cx),
            audio_form: materials::AudioForm::new(window, cx),
            backups: backups::BackupPanel::default(),
            show_administration: false,
            copies: destinations::DestinationPanel::default(),
            account_admin: account_admin::AccountAdminPanel::new(window, cx),
            onboarding: onboarding::OnboardingPanel::new(window, cx),
            confirm_trash: false,
            selection: std::collections::BTreeSet::new(),
            readiness: readiness::ReadinessPanel::default(),
            form: None,
            show_actions: false,
            show_developer: false,
            confirm_stop: false,
            status_seen: (String::new(), std::time::Instant::now()),
            shown_page: String::new(),
            show_scenes: false,
            compact_layout: false,
            lab: cx.new(|cx| crate::view::Studio::new(window, cx)),
            show_lab: false,
            request_sequence: 0,
            _subscriptions: subscriptions,
        }
    }

    pub fn open_workspace(&mut self, root: PathBuf, cx: &mut Context<Self>) {
        if self.control_busy {
            return;
        }
        if self.has_unsaved_setup(cx) {
            return;
        }
        self.request(Request::Open(root), cx);
    }

    fn choose_workspace(&mut self, cx: &mut Context<Self>) {
        if self.control_busy {
            return;
        }
        if self.busy || self.has_unsaved_setup(cx) {
            return;
        }
        let chooser = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open Retold workspace".into()),
        });
        cx.spawn(async move |this, cx| {
            let result = chooser.await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(Ok(Some(paths))) => {
                    if let Some(root) = paths.into_iter().next() {
                        this.open_workspace(root, cx);
                    }
                }
                Ok(Ok(None)) => (),
                _ => {
                    this.status = "The workspace chooser could not be opened. Try again.".into();
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn request(&mut self, request: Request, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.request_sequence = self.request_sequence.wrapping_add(1);
        let sequence = self.request_sequence;
        let control_generation = self.control_generation;
        self.status = "Working in the background…".into();
        let work = cx
            .background_executor()
            .spawn(async move { request.execute() });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                if this.request_sequence != sequence {
                    return;
                }
                this.busy = false;
                let controls_changed = this.control_generation != control_generation;
                match result {
                    Ok(backend::Reply::Source(preview)) => {
                        if let Some(form) = &mut this.draft
                            && form.source_path(cx).as_ref() == Some(&preview.path) {
                            form.preview = Some(preview);
                            this.status = "Source reviewed. Save the offline draft when its setup is ready.".into();
                        }
                    }
                    Ok(backend::Reply::Workspace {
                        snapshot,
                        message,
                        created,
                    }) => {
                        this.status = message;
                        this.review = None;
                        if let Some(delivery) = &mut this.delivery { delivery.review = None; }
                        match *snapshot {
                            Ok(mut snapshot) => {
                                let switching = this.snapshot.as_ref().is_some_and(|previous| previous.context.library != snapshot.context.library || previous.context.config != snapshot.context.config);
                                if switching && this.has_unsaved_setup(cx) {
                                    this.status = "The workspace's library mapping changed. Your unsaved changes are kept. Save or discard them before opening the updated workspace.".into();
                                    this.refresh_error = Some("Workspace mapping changed while a draft was open.".into());
                                    cx.notify();
                                    return;
                                }
                                if controls_changed && switching {
                                    this.status = "The current workspace's stop request took priority. Open the other workspace again after it finishes.".into();
                                    cx.notify();
                                    return;
                                }
                                this.refresh_error = None;
                                if this.providers.snapshot.as_ref().is_some_and(|providers| providers.root != snapshot.context.config) {
                                    this.providers = providers::ProviderPanel::new();
                                }
                                this.catalog_page = None;
                                this.catalog_back.clear();
                                this.catalog_options = Default::default();
                                if controls_changed && let Some(previous) = &this.snapshot { snapshot.activity = previous.activity.clone(); }
                                if this.snapshot.as_ref().is_some_and(|previous| {
                                    previous.context.library != snapshot.context.library
                                }) {
                                    this.editor = None;
                                    this.scenes = None;
                                    this.lines = None;
                                    this.materials = None;
                                    this.narration = None;
                                    this.show_scenes = false;
                                    this.delivery = None;
                                    this.characters = None;
                                    this.notes = None;
                                    this.show_notes = false;
                                    this.clear_media(cx);
                                    this.show_script = false;
                                    this.selected_video = None;
                                    this.observed_job = None;
                                    this.last_stop = None;
                                }
                                this.activity_options = Default::default();
                                this.snapshot = Some(snapshot);
                                if let Some(snapshot) = &this.snapshot {
                                    this.queue_preference(retold_app::desktop::preferences::Change::Workspace(snapshot.context.root.clone()), cx);
                                }
                                this.new_root = None;
                                this.import = None;
                                this.preload_providers(cx);
                            }
                            Err(error) => {
                                this.refresh_error = Some(error.message.clone());
                                this.status = "Couldn't read the workspace.".into();
                            }
                        }
                        if created {
                            this.draft = None;
                            this.destination = Destination::Videos;
                        }
                    }
                    Ok(backend::Reply::Review(review)) => {
                        if controls_changed { this.status = "Queue controls changed while planning. Review again.".into(); }
                        else { this.receive_review(*review, cx); }
                    }
                    Ok(backend::Reply::Videos { page, options, history }) => {
                        if this.snapshot.as_ref().is_some_and(|snapshot| snapshot.context.library == page.library) {
                            this.catalog_options = options;
                            this.catalog_back = history;
                            this.status = format!("Showing {} of {} videos.", page.items.len(), page.total);
                            this.catalog_page = Some(*page);
                        }
                    }
                    Ok(backend::Reply::Activity {
                        library,
                        options,
                        activity,
                    }) => {
                        if !controls_changed && let Some(snapshot) = &mut this.snapshot
                            && snapshot.context.library == library
                        {
                            snapshot.activity = Ok(*activity);
                            this.activity_options = options;
                            this.status =
                                "Activity updated."
                                    .into();
                        }
                    }
                    Ok(backend::Reply::Job(job)) => { if !controls_changed { this.receive_job(*job); } },
                    Ok(backend::Reply::Stopped { .. }) => { this.status = "Unexpected control reply on the normal request lane.".into(); },
                    Ok(backend::Reply::Submitted(job)) => {
                        this.status = format!(
                            "Submission acknowledged for job {} ({} at acceptance). Activity shows its current saved outcome.",
                            job.id,
                            job.status.as_str()
                        );
                        this.destination = Destination::Activity;
                        this.review = None;
                        this.refresh(cx);
                    }
                    Err(error) => {
                        this.status = format!("Could not complete the request: {error}");
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn refresh(&mut self, cx: &mut Context<Self>) {
        if let Some(snapshot) = &self.snapshot {
            self.request(Request::Open(snapshot.context.root.clone()), cx);
        }
    }

    fn start_worker(&mut self, cx: &mut Context<Self>) {
        let Some(snapshot) = &self.snapshot else {
            return;
        };
        self.request(
            Request::StartWorker {
                root: snapshot.context.root.clone(),
                library: snapshot.context.library.clone(),
            },
            cx,
        );
    }

    fn shutdown_worker(&mut self, cx: &mut Context<Self>) {
        let Some(snapshot) = &self.snapshot else {
            return;
        };
        let Ok(activity) = &snapshot.activity else {
            return;
        };
        self.request(
            Request::ShutdownWorker {
                root: snapshot.context.root.clone(),
                state: activity.state_directory.clone(),
            },
            cx,
        );
    }

    fn enable_dispatch(&mut self, cx: &mut Context<Self>) {
        let Some(snapshot) = &self.snapshot else {
            return;
        };
        let Ok(activity) = &snapshot.activity else {
            return;
        };
        let Some(control_id) = &activity.dispatch.inhibited_by else {
            return;
        };
        self.request(
            Request::Enable {
                root: snapshot.context.root.clone(),
                state: activity.state_directory.clone(),
                control_id: control_id.clone(),
            },
            cx,
        );
    }

    pub(super) fn begin_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.control_busy || self.import.is_some() || self.new_root.is_some() {
            return;
        }
        if self.notes.as_ref().is_some_and(|panel| panel.dirty(cx)) {
            self.return_to_notes(cx);
            self.status = "Save or clear your review note before creating another video.".into();
            return;
        }
        if self.editor.as_ref().is_some_and(|editor| editor.dirty(cx)) {
            self.status =
                "Save or discard your script changes before creating another video.".into();
            cx.notify();
            return;
        }
        if self.draft.is_some() {
            self.destination = Destination::Videos;
            cx.notify();
            return;
        }
        let Some(snapshot) = &self.snapshot else {
            return;
        };
        let Ok(choices) = &snapshot.choices else {
            return;
        };
        if choices.guides.is_empty() || choices.channels.is_empty() {
            return;
        }
        self.draft = Some(DraftForm::new(choices.clone(), window, cx));
        self.clear_media(cx);
        self.characters = None;
        self.selected_video = None;
        self.destination = Destination::Videos;
        cx.notify();
    }

    fn save_draft(&mut self, cx: &mut Context<Self>) {
        let (Some(snapshot), Some(form)) = (&self.snapshot, &self.draft) else {
            return;
        };
        let options = form.options(cx);
        if !form.source_ready(cx) {
            self.status = "Preview the selected source before saving.".into();
            cx.notify();
            return;
        }
        let source = form.preview.clone();
        self.request(
            Request::Create {
                root: snapshot.context.library.clone(),
                options,
                source,
            },
            cx,
        );
    }
}

impl gpui_kit::Focusable for Shell {
    fn focus_handle(&self, _: &gpui_kit::App) -> gpui_kit::FocusHandle {
        self.focus.clone()
    }
}

fn zero_input(window: &mut Window, cx: &mut Context<Shell>) -> Entity<InputState> {
    cx.new(|cx| {
        let mut state = InputState::new(window, cx);
        state.set_value("0", window, cx);
        state
    })
}
