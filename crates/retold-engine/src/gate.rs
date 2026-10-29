//! External accounting authority for per-use dispatch. When a ledger carries a gate, each paid
//! call crosses it twice before network I/O: a durable reservation, then a one-time dispatch
//! transition. Settlement needs artifact evidence; ambiguous outcomes keep their holds.

/// The gate's money type, re-exported so gate implementers need no extra dependency.
pub use retold_core::Usd;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaidCapability {
    Text,
    Image,
    Speech,
    Transcription,
}

/// One physical attempt. `request` is the per-run attempt slot; `logical` is its stable
/// operation identity without the retry ordinal.
#[derive(Debug, Clone, Copy)]
pub struct PaidCall<'a> {
    pub request: &'a str,
    pub logical: &'a str,
    pub stage: &'a str,
    pub provider: &'a str,
    pub capability: PaidCapability,
    pub ceiling: Usd,
}

/// What a committed response proved. `cost: None` keeps the quoted hold until reconciliation.
#[derive(Debug, Clone, Copy)]
pub struct PaidEvidence<'a> {
    pub artifact_hash: &'a str,
    pub cost: Option<Usd>,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum GateError {
    /// A ceiling would be exceeded; another allowed route may still fit.
    #[error("account limit reached: {0}")]
    Refused(String),
    /// A prior attempt may have dispatched; recover or reconcile it before retrying.
    #[error("account attempt has an unresolved outcome: {0}")]
    Uncertain(String),
    /// Authority, binding or accounting storage refuses dispatch outright.
    #[error("account authority blocked dispatch: {0}")]
    Blocked(String),
}

pub trait PaidGate: std::fmt::Debug + Send + Sync {
    fn reserve(&self, call: &PaidCall<'_>) -> Result<(), GateError>;
    /// The last step before network I/O. It succeeds once per reserved attempt.
    fn dispatch(&self, request: &str) -> Result<(), GateError>;
    /// Releases a reservation that definitely never dispatched.
    fn withdraw(&self, request: &str) -> Result<(), GateError>;
    fn settle(&self, request: &str, evidence: &PaidEvidence<'_>) -> Result<(), GateError>;
    /// Explicit billing resolution, including a provider-confirmed unbilled failure.
    fn reconcile(&self, request: &str, cost: Usd) -> Result<(), GateError>;
}
