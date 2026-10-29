//! Runs an existing video from installation settings, under its run lock and through a
//! caller-supplied accounting gate. A session stops at `ready`: it never opens the editor,
//! creates a project or exports.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use retold_guide::{Channel, Guide};
use retold_providers::{ProviderConfig, ProvidersConfig, Router};
use secrecy::SecretString;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

mod control;
mod export;
pub use control::Control;
pub use export::{PreparedDelivery, Selection, export, prepared_deliveries};

use crate::context::Context;
use crate::gate::PaidGate;
use crate::ledger::{Ledger, LedgerError};
use crate::media_runtime::{MediaError, MediaRegistry, MediaRuntime};
use crate::pipeline::{PipelineError, Until};
use crate::store::{Store, hash, inspection};
use crate::workflow::{self, WorkflowError};
use crate::{EngineError, Paths, RunLayout, RunMeta};

const MANIFEST_LIMIT: u64 = 2 * 1024 * 1024;

/// The installation's provider settings, and a digest that pins them in a plan.
#[derive(Debug, Clone)]
pub struct Providers {
    config: ProvidersConfig,
    digest: String,
}

impl Providers {
    /// Reads `providers.toml`; an absent file means no providers are configured yet.
    pub fn load(paths: &Paths) -> Result<Self, EngineError> {
        let path = paths.providers_file();
        let config = match std::fs::read_to_string(&path) {
            Ok(text) => parse_providers(&path, &text)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => ProvidersConfig::empty(),
            Err(error) => return Err(EngineError::io(&path, error)),
        };
        let digest = hash(&config)?;
        Ok(Self { config, digest })
    }

    #[must_use]
    pub(crate) const fn config(&self) -> &ProvidersConfig {
        &self.config
    }

    #[must_use]
    pub fn digest(&self) -> &str {
        &self.digest
    }

    /// Per-use routes and the provider account whose allowance each one draws on. Routes that
    /// share an API key share one account, so aliases cannot multiply a provider cap.
    #[must_use]
    pub fn per_use_routes(&self) -> BTreeMap<String, String> {
        self.config
            .providers
            .iter()
            .filter_map(|(id, spec)| match spec {
                ProviderConfig::OpenRouter { key_env, .. } => {
                    Some((id.clone(), format!("openrouter:{key_env}")))
                }
                _ => None,
            })
            .collect()
    }
}

fn parse_providers(path: &Path, text: &str) -> Result<ProvidersConfig, EngineError> {
    let config: ProvidersConfig = toml::from_str(text).map_err(|error| EngineError::Config {
        path: path.into(),
        message: error.to_string(),
    })?;
    let newer = config.schema_version > retold_providers::config::SCHEMA_VERSION;
    if newer {
        return Err(EngineError::Config {
            path: path.into(),
            message: "this provider settings format is newer than this Retold".into(),
        });
    }
    Ok(config)
}

/// The last stages each stop reaches; later stops reach the whole graph.
const STOP_TARGETS: &[(Until, &[&str])] = &[
    (Until::Created, &[]),
    (Until::Research, &["research"]),
    (Until::Script, &["script"]),
    (Until::Storyboard, &["storyboard"]),
    (Until::Characters, &["characters"]),
    (Until::Images, &["images"]),
    (Until::Voice, &["images", "voices"]),
    (Until::Captions, &["images", "voices", "captions"]),
];
const WHOLE_GRAPH: &[&str] = &["metrics", "edit"];

/// Stages a workflow stopping at `until` may run, including their prerequisites.
#[must_use]
pub fn stages_through(until: Until) -> BTreeSet<String> {
    let targets = STOP_TARGETS
        .iter()
        .find(|(stop, _)| *stop == until)
        .map_or(WHOLE_GRAPH, |(_, targets)| *targets);
    crate::graph::Graph::stages().prerequisites(targets)
}

/// What the reviewed plan saw; execution refuses to start if either changed.
#[derive(Debug, Clone)]
pub struct Expected {
    pub manifest_hash: String,
    pub state_hash: String,
    pub source_hash: String,
    pub editing_hash: String,
}

#[derive(Debug)]
pub struct Request<'a> {
    pub paths: &'a Paths,
    pub layout: &'a RunLayout,
    pub providers: &'a Providers,
    pub gate: Arc<dyn PaidGate>,
    /// Lets the engine consider per-use routes at all; the gate still bounds every call.
    pub allow_paid: bool,
    pub until: Until,
    pub expected: &'a Expected,
    /// The editor binary for exports; the installed FilmCraft when absent.
    pub editor: Option<std::path::PathBuf>,
    /// External owner control; absent for callers that only use terminal interrupts.
    pub control: Option<&'a Control>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopKind {
    /// Saved content waits for a person's review.
    Review,
    /// A ceiling, missing authority or an unresolved paid attempt stopped dispatch.
    Authority,
    Cancelled,
    /// The video changed after it was planned.
    Changed,
    /// Export needs saved ready deliveries for the whole selection.
    NotReady,
    Failed,
}

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error(transparent)]
    Editing(#[from] crate::editing::EditError),
    #[error("the session was cancelled; saved work and unresolved provider holds were retained")]
    Cancelled,
    #[error("the video changed after it was planned; plan it again")]
    Changed,
    #[error("continue stops at ready; projects and exports need a separate export request")]
    BeyondReady,
    #[error(
        "the video has no saved ready delivery for every requested edition and format; continue it to ready first"
    )]
    NotReady,
    #[error(transparent)]
    Engine(#[from] EngineError),
    #[error(transparent)]
    Ledger(#[from] LedgerError),
    #[error(transparent)]
    Workflow(#[from] WorkflowError),
}

impl SessionError {
    #[must_use]
    pub const fn kind(&self) -> StopKind {
        match self {
            Self::Cancelled | Self::Engine(EngineError::Cancelled) => StopKind::Cancelled,
            Self::Changed => StopKind::Changed,
            Self::NotReady => StopKind::NotReady,
            Self::BeyondReady | Self::Ledger(_) => StopKind::Authority,
            Self::Engine(_) => StopKind::Failed,
            Self::Workflow(error) => workflow_kind(error),
            Self::Editing(error) => editing_kind(error),
        }
    }
}

const fn editing_kind(error: &crate::editing::EditError) -> StopKind {
    use crate::editing::EditError;
    match error {
        EditError::Changed
        | EditError::SourceChanged
        | EditError::SceneChanged
        | EditError::SceneSourceChanged => StopKind::Changed,
        EditError::Unapproved | EditError::SceneUnapproved | EditError::Invalid(_) => {
            StopKind::Review
        }
        EditError::Integrity(_) | EditError::Engine(_) => StopKind::Failed,
    }
}

const fn workflow_kind(error: &WorkflowError) -> StopKind {
    match error {
        WorkflowError::Media(media) | WorkflowError::Text(PipelineError::Media(media)) => {
            media_kind(media)
        }
        WorkflowError::Text(
            PipelineError::Cancelled
            | PipelineError::Provider(retold_providers::RouteError::Cancelled)
            | PipelineError::Engine(EngineError::Cancelled),
        )
        | WorkflowError::Engine(EngineError::Cancelled) => StopKind::Cancelled,
        _ => StopKind::Failed,
    }
}

const fn media_kind(error: &MediaError) -> StopKind {
    match error {
        MediaError::Review(_) => StopKind::Review,
        MediaError::Ledger(_) => StopKind::Authority,
        MediaError::Provider(retold_providers::ProviderError::Cancelled)
        | MediaError::Engine(EngineError::Cancelled) => StopKind::Cancelled,
        _ => StopKind::Failed,
    }
}

/// Blocks until the workflow reaches `request.until` or stops.
pub fn run_until(request: &Request<'_>) -> Result<(), SessionError> {
    ensure_running(request)?;
    if request.until > Until::Ready {
        return Err(SessionError::BeyondReady);
    }
    let store = Store::open(request.layout)?;
    let meta = revalidate(request.layout, request.expected)?;
    let context = context(request, meta, store)?;
    let until = request.until;
    drive(context, |context| async move {
        workflow::run(context, until, &workflow::Options::default()).await
    })
}

/// Runs `work` to completion on a fresh runtime, cancelling it gracefully on Ctrl-C.
fn drive<F, Work>(context: Context, work: Work) -> Result<(), SessionError>
where
    Work: FnOnce(Arc<Context>) -> F,
    F: std::future::Future<Output = Result<(), WorkflowError>>,
{
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|error| EngineError::Setup(format!("starting the runtime: {error}")))?;
    let cancel = context.media.cancel.clone();
    runtime.block_on(async move {
        let watcher = tokio::spawn(interrupt(cancel));
        let result = work(Arc::new(context)).await;
        watcher.abort();
        result.map_err(SessionError::from)
    })
}

/// The first Ctrl-C stops gracefully and keeps saved work; a second also ends provider
/// processes so a stuck call cannot hold the run open.
pub(crate) async fn interrupt(cancel: CancellationToken) {
    let _ = tokio::signal::ctrl_c().await;
    cancel.cancel();
    let _ = tokio::signal::ctrl_c().await;
    retold_providers::process::kill_all();
}

/// Under the run lock, the manifest and saved state must still be what the plan inspected.
fn revalidate(layout: &RunLayout, expected: &Expected) -> Result<RunMeta, SessionError> {
    let manifest = crate::fsx::read_bounded(&layout.run_file(), MANIFEST_LIMIT)?
        .ok_or(SessionError::Changed)?;
    let state_hash = inspection::inspect(layout)?.state_hash;
    let source_hash = crate::research::saved_source_hash(layout)?;
    let editing_hash =
        crate::editing::digest(layout).map_err(|error| EngineError::Setup(error.to_string()))?;
    let sources_match =
        source_hash == expected.source_hash && editing_hash == expected.editing_hash;
    let unchanged = hash(&manifest)? == expected.manifest_hash
        && state_hash == expected.state_hash
        && sources_match;
    if !unchanged {
        return Err(SessionError::Changed);
    }
    crate::editing::ready(layout)?;
    serde_json::from_slice(&manifest)
        .map_err(|error| EngineError::Setup(format!("reading the run manifest: {error}")).into())
}

fn context(
    request: &Request<'_>,
    meta: RunMeta,
    mut store: Store,
) -> Result<Context, SessionError> {
    let layout = request.layout;
    let guide = saved_guide(layout, &meta)?;
    let channel = saved_channel(request.paths, layout, &meta)?;
    let ledger = Ledger::open(&layout.meta().join("ledger.json"), meta.budget)?
        .with_gate(Arc::clone(&request.gate));
    let (registry, router) = routes(request)?;
    let skip_reviews = meta.skip_reviews;
    store = attach_events(request, store);
    Ok(Context {
        layout: layout.clone(),
        meta,
        guide,
        channel,
        text: Arc::new(router),
        media: MediaRuntime {
            registry,
            store: Arc::new(Mutex::new(store)),
            ledger: Arc::new(ledger),
            cancel: request
                .control
                .map_or_else(CancellationToken::new, |control| control.cancel.clone()),
            allow_paid: request.allow_paid,
        },
        skip_reviews,
        depth: Arc::new(retold_parallax::depth::ModelDepth::default()),
        phonemes: Arc::new(crate::phonemes::LocalAligner),
    })
}

fn ensure_running(request: &Request<'_>) -> Result<(), SessionError> {
    let cancelled = request.control.is_some_and(Control::is_cancelled);
    if cancelled {
        Err(SessionError::Cancelled)
    } else {
        Ok(())
    }
}

fn attach_events(request: &Request<'_>, mut store: Store) -> Store {
    let observer = request.control.and_then(|control| control.observer.clone());
    let events = crate::events::Events::new(
        &request.layout.events_log(),
        Option::default(),
        Option::default(),
    )
    .with_checked_observer(observer);
    store.set_events(Arc::new(events));
    store
}

/// Text and media routes share each provider's concurrency permits.
fn routes(request: &Request<'_>) -> Result<(Arc<MediaRegistry>, Router), SessionError> {
    let secrets = Secrets::load(request.paths);
    let lookup = |name: &str| secrets.get(name);
    let config = &request.providers.config;
    let health = Arc::new(
        crate::provider_health::HealthLedger::new(&request.paths.config, config)
            .map_err(|_| EngineError::Setup("Could not initialize provider health.".into()))?,
    );
    let mut registry = MediaRegistry::from_config(config, &lookup);
    registry.health = Some(Arc::clone(&health) as Arc<dyn retold_providers::health::Health>);
    let registry = Arc::new(registry);
    let mut router = Router::from_config(config, &lookup).with_health(health);
    for (id, slot) in &registry.slots {
        router.share_permits(id, Arc::clone(&slot.permits));
    }
    Ok((registry, router))
}

/// The guide snapshot the run was created with; a modified snapshot is refused.
fn saved_guide(layout: &RunLayout, meta: &RunMeta) -> Result<Guide, EngineError> {
    let versioned = layout.meta().join("guides").join(&meta.guide.hash);
    let directory = if versioned.is_dir() {
        versioned
    } else {
        layout.guide_snapshot()
    };
    let guide = Guide::from_dir(&directory)
        .map_err(|error| EngineError::Setup(format!("loading the saved guide: {error}")))?;
    let modified = guide.content_hash() != meta.guide.hash;
    if modified {
        return Err(EngineError::Setup(
            "the saved guide was modified; restore it before continuing".into(),
        ));
    }
    Ok(guide)
}

/// The channel saved with the run, resolving shared assets in the channel's own folder.
fn saved_channel(
    paths: &Paths,
    layout: &RunLayout,
    meta: &RunMeta,
) -> Result<Channel, EngineError> {
    let assets = paths.channels().join(&meta.channel);
    let saved = layout.meta().join("channel.toml").is_file();
    let channel = if saved {
        Channel::load(&layout.meta()).map(|channel| channel.with_asset_dir(assets))
    } else {
        Channel::load(&assets)
    };
    channel.map_err(|error| EngineError::Setup(format!("loading the channel: {error}")))
}

/// Provider keys: the environment first, then the settings folder's `.env`. Nothing is
/// written into the process environment.
pub(crate) struct Secrets(BTreeMap<String, String>);

impl Secrets {
    pub(crate) fn load(paths: &Paths) -> Self {
        let file = dotenvy::from_path_iter(paths.env_file())
            .into_iter()
            .flatten()
            .flatten()
            .collect();
        Self(file)
    }

    pub(crate) fn get(&self, name: &str) -> Option<SecretString> {
        std::env::var(name)
            .ok()
            .or_else(|| self.0.get(name).cloned())
            .filter(|value| !value.trim().is_empty())
            .map(SecretString::from)
    }
}
