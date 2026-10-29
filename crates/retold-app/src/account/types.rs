//! Finite, integer accounting values; no saved run flags participate in authority.
pub use retold_engine::gate::Usd;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

pub type Resources = BTreeMap<Unit, u64>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    UsdMicros,
    Requests,
    InputTokens,
    OutputTokens,
    AudioMillis,
    LocalMillis,
    VisionPixels,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum Scope {
    Account,
    Workspace(String),
    Video(String),
    Series(String),
    Provider(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Window {
    Lifetime,
    Day,
    Month,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limit {
    pub scope: Scope,
    pub window: Window,
    pub resources: Resources,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Workspace {
    pub id: String,
    pub root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    Continue,
    Export {
        editions: BTreeSet<String>,
        formats: BTreeSet<String>,
    },
    /// Explicit provider tests. Paid attempts draw on account and provider ceilings, not on a
    /// workspace or video.
    ProviderTest {
        providers: BTreeSet<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Text,
    Image,
    Speech,
    Transcription,
    Check,
    Local,
    Export,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Job {
    pub id: String,
    pub workspace: Workspace,
    pub video_id: String,
    pub series_id: Option<String>,
    pub plan_hash: String,
    pub input_versions: BTreeMap<String, String>,
    pub action: Action,
    pub routes: BTreeSet<String>,
    pub capabilities: Vec<Capability>,
    #[serde(default)]
    pub resources: Resources,
    /// Ceilings per declared operation-tree node; attempts outside them are refused.
    #[serde(default)]
    pub operations: BTreeMap<String, Resources>,
    pub deadline: Option<jiff::Timestamp>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reservation {
    pub attempt_id: String,
    pub job_id: String,
    /// Single-flight identity within the workspace.
    pub operation_key: String,
    /// Declared operation-tree node whose ceiling this attempt draws on; defaults to the key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    pub route: String,
    pub capability: Capability,
    pub plan_hash: String,
    pub resources: Resources,
}

impl Reservation {
    #[must_use]
    pub fn budget_node(&self) -> &str {
        self.node.as_deref().unwrap_or(&self.operation_key)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub artifact_hash: String,
    pub receipt_hash: String,
    pub observed: BTreeMap<Unit, Option<u64>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptState {
    Reserved,
    Dispatched,
    Settled,
    NotDispatched,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attempt {
    pub reservation: Reservation,
    pub provider_account: String,
    pub workspace_id: String,
    pub video_id: String,
    pub series_id: Option<String>,
    pub day: String,
    pub month: String,
    pub timezone: String,
    pub state: AttemptState,
    pub receipt: Option<Receipt>,
    pub receipt_history: Vec<Receipt>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "outcome", content = "attempt", rename_all = "snake_case")]
pub enum Reserved {
    New(Attempt),
    Existing(Attempt),
}

/// Constructed only by the database's first reserved→dispatched transition.
#[derive(Debug)]
pub struct DispatchPermit {
    pub(super) attempt_id: String,
}

impl DispatchPermit {
    #[must_use]
    pub fn attempt_id(&self) -> &str {
        &self.attempt_id
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Balance {
    pub charged: u64,
    pub held: u64,
    pub unknown_observations: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Summary {
    pub account_id: String,
    pub timezone: String,
    pub workspaces: usize,
    pub jobs: usize,
    pub attempts: usize,
    pub limits: Vec<Limit>,
    pub lifetime: BTreeMap<Unit, Balance>,
}
