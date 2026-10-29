//! Canonical account accounting. No provider dispatch, filesystem repair or automatic replacement.
pub mod activation;
pub mod backup;
pub mod barrier;
mod budget;
pub mod commands;
mod database;
pub mod inspection;
mod lifecycle;
pub mod limits;
pub(crate) mod physical;
mod policy;
pub mod processes;
pub mod report;
mod state;
pub mod types;
mod usage;

pub use retold_engine::gate::Usd;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use types::{DispatchPermit, Job, Limit, Receipt, Reservation, Reserved, Summary, Workspace};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Your spending account is not set up yet.")]
    Missing,
    #[error("Account storage already exists; it must never be initialized over existing history.")]
    Exists,
    #[error(
        "Accounting is unavailable or invalid; restore authoritative accounting before execution."
    )]
    Corrupt,
    #[error("Accounting is busy; retry without dispatching work.")]
    Busy,
    #[error("{0}")]
    Invalid(&'static str),
    #[error("A resource ceiling would be exceeded or a required finite limit is missing.")]
    Limit,
    #[error(
        "Operation already has a durable attempt; recover or inspect it instead of dispatching again."
    )]
    OperationExists,
    #[error("This attempt is already dispatched or cannot make that transition; retain its holds.")]
    Transition,
    #[error("Running jobs or account changes hold the account; nothing was changed.")]
    Occupied(Vec<barrier::Owner>),
    #[error("{0}")]
    Backup(&'static str),
    #[error("{0}")]
    BackupInvalid(&'static str),
}

/// Explicit clocks support deterministic tests; production always uses `SystemClock`!.
pub trait Clock: std::fmt::Debug + Send + Sync {
    fn now(&self) -> jiff::Timestamp;
}

#[derive(Debug)]
pub struct SystemClock;
impl Clock for SystemClock {
    fn now(&self) -> jiff::Timestamp {
        jiff::Timestamp::now()
    }
}

#[derive(Debug)]
pub struct Account {
    directory: PathBuf,
    clock: Arc<dyn Clock>,
    /// Shared barrier ownership held for a whole job; changes then reuse it.
    held: Option<barrier::Token>,
}

pub fn canonical_directory() -> Result<PathBuf> {
    directories::ProjectDirs::from("", "", "retold-account")
        .map(|paths| paths.data_local_dir().to_owned())
        .ok_or(Error::Missing)
}

/// Offline mutations join an existing account's barrier without initializing financial history.
pub fn activity(purpose: &str) -> Result<Option<Account>> {
    let directory = canonical_directory()?;
    match Account::open(&directory) {
        Ok(account) => account.hold(purpose).map(Some),
        Err(Error::Missing) => Ok(Option::default()),
        Err(error) => Err(error),
    }
}

impl Account {
    pub fn initialize(directory: &Path, timezone: &str) -> Result<Self> {
        database::initialize(directory, timezone)?;
        Self::open(directory)
    }

    pub fn open(directory: &Path) -> Result<Self> {
        Self::with_clock(directory, Arc::new(SystemClock))
    }

    /// Low-level explicit paths/clocks are for embedders and isolated tests, never CLI overrides.
    pub fn with_clock(directory: &Path, clock: Arc<dyn Clock>) -> Result<Self> {
        database::read(directory)?.validate()?;
        Ok(Self {
            directory: directory.into(),
            clock,
            held: None,
        })
    }

    /// The same account, holding the maintenance barrier until it is dropped. Jobs hold it from
    /// admission to their last settlement, so no backup can capture them half done.
    pub fn hold(&self, purpose: &str) -> Result<Self> {
        Ok(Self {
            directory: self.directory.clone(),
            clock: Arc::clone(&self.clock),
            held: Some(barrier::join(&self.directory, purpose)?),
        })
    }

    #[cfg_attr(
        not(unix),
        expect(dead_code, reason = "only the Unix supervisor reaches this")
    )]
    pub(crate) fn physical_claim(&self) -> Result<physical::Claim> {
        physical::Claim::begin(&self.directory)
    }

    pub fn attempt(&self, attempt_id: &str) -> Result<types::Attempt> {
        database::read(&self.directory)?
            .attempts
            .get(attempt_id)
            .cloned()
            .ok_or(Error::Transition)
    }

    /// Stable registered identity survives an explicit same-account root transfer.
    pub fn workspace_at(&self, root: &Path) -> Result<Option<Workspace>> {
        let root = root
            .canonicalize()
            .map_err(|_| Error::Invalid("Workspace root is unavailable."))?;
        Ok(database::read(&self.directory)?
            .workspaces
            .into_values()
            .find(|workspace| workspace.root == root))
    }

    pub fn registered_workspaces(&self) -> Result<Vec<Workspace>> {
        let state = database::read(&self.directory)?;
        Ok(state.workspaces.into_values().collect())
    }

    pub fn canonical_account_id(&self) -> Result<String> {
        let state = database::read(&self.directory)?;
        Ok(state.account_id)
    }

    pub fn summary(&self) -> Result<Summary> {
        usage::summary(&database::read(&self.directory)?)
    }

    /// Charged, held and unknown lifetime totals of one job's attempts.
    pub fn job_usage(&self, job_id: &str) -> Result<BTreeMap<types::Unit, types::Balance>> {
        usage::job(&database::read(&self.directory)?, job_id)
    }

    pub fn set_limits(&self, limits: Vec<Limit>) -> Result<()> {
        self.change(|state| policy::set_limits(state, limits))
    }

    pub fn register_workspace(&self, workspace: Workspace) -> Result<()> {
        self.change(|state| policy::register_workspace(state, workspace))
    }

    pub fn register_route(&self, route: &str, provider_account: &str) -> Result<()> {
        self.change(|state| policy::register_route(state, route, provider_account))
    }

    pub fn admit(&self, job: Job) -> Result<()> {
        self.change(|state| policy::admit(state, job))
    }

    pub fn reserve(&self, reservation: Reservation) -> Result<Reserved> {
        self.change(|state| lifecycle::reserve(state, reservation))
    }

    pub fn mark_dispatched(&self, attempt_id: &str, plan_hash: &str) -> Result<DispatchPermit> {
        self.change(|state| lifecycle::mark_dispatched(state, attempt_id, plan_hash))
    }

    pub fn cancel_reserved(&self, attempt_id: &str) -> Result<()> {
        self.change(|state| lifecycle::cancel_reserved(state, attempt_id))
    }

    pub fn settle(&self, attempt_id: &str, receipt: Receipt) -> Result<()> {
        self.change(|state| lifecycle::settle(state, attempt_id, receipt))
    }

    pub fn reconcile(&self, attempt_id: &str, receipt: Receipt) -> Result<()> {
        self.change(|state| lifecycle::reconcile(state, attempt_id, receipt))
    }

    /// Account totals before and after reconciling `attempt_id` at `cost`, computed on a read
    /// copy of the state: nothing is written, so this is safe while work runs.
    pub fn preview_reconcile(&self, attempt_id: &str, cost: Usd) -> Result<(Summary, Summary)> {
        let state = database::read(&self.directory)?;
        let before = usage::summary(&state)?;
        let (reconciled, ()) = lifecycle::reconcile_attempt(state, attempt_id, cost)?;
        Ok((before, usage::summary(&reconciled)?))
    }

    /// Reconciles an attempt at an explicit cost without dispatch authority.
    pub fn reconcile_attempt(&self, attempt_id: &str, cost: Usd) -> Result<()> {
        self.change(|state| lifecycle::reconcile_attempt(state, attempt_id, cost))
    }

    /// Every change runs inside the barrier: the job's, or its own for this change alone.
    fn change<T>(
        &self,
        mutation: impl FnOnce(state::State) -> Result<(state::State, T)>,
    ) -> Result<T> {
        let _own = match self.held {
            Some(_) => None,
            None => Some(barrier::join(&self.directory, "account change")?),
        };
        database::transaction(&self.directory, self.clock.now(), mutation)
    }
}
