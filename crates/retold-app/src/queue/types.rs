//! Queue domain types and transition rules.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Queued,
    Running,
    Pausing,
    Paused,
    Stopping,
    Stopped,
    Completed,
    Failed,
    Blocked,
}

const STATUS_TABLE: &[(JobStatus, &str)] = &[
    (JobStatus::Queued, "queued"),
    (JobStatus::Running, "running"),
    (JobStatus::Pausing, "pausing"),
    (JobStatus::Paused, "paused"),
    (JobStatus::Stopping, "stopping"),
    (JobStatus::Stopped, "stopped"),
    (JobStatus::Completed, "completed"),
    (JobStatus::Failed, "failed"),
    (JobStatus::Blocked, "blocked"),
];

impl JobStatus {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        for &(status, name) in STATUS_TABLE {
            if status == self {
                return name;
            }
        }
        "unknown"
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        for &(status, name) in STATUS_TABLE {
            if name == value {
                return Some(status);
            }
        }
        None
    }

    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Stopped | Self::Completed | Self::Failed | Self::Blocked
        )
    }
}

const fn valid_from_running(to: JobStatus) -> bool {
    matches!(
        to,
        JobStatus::Pausing
            | JobStatus::Completed
            | JobStatus::Blocked
            | JobStatus::Failed
            | JobStatus::Stopping
    )
}

const fn valid_from_pausing(to: JobStatus) -> bool {
    matches!(
        to,
        JobStatus::Paused
            | JobStatus::Completed
            | JobStatus::Blocked
            | JobStatus::Failed
            | JobStatus::Stopping
    )
}

const fn valid_from_paused(from: JobStatus, to: JobStatus) -> bool {
    matches!(from, JobStatus::Paused) && matches!(to, JobStatus::Queued | JobStatus::Stopped)
}

#[must_use]
pub const fn can_transition(from: JobStatus, to: JobStatus) -> bool {
    if matches!(from, JobStatus::Stopping) {
        return matches!(
            to,
            JobStatus::Stopped | JobStatus::Completed | JobStatus::Failed | JobStatus::Blocked
        );
    }
    match from {
        JobStatus::Queued => matches!(
            to,
            JobStatus::Running | JobStatus::Paused | JobStatus::Stopped
        ),
        JobStatus::Running => valid_from_running(to),
        JobStatus::Pausing => valid_from_pausing(to),
        _ => valid_from_paused(from, to),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobRecord {
    pub id: String,
    pub idempotency_key: String,
    pub video_ref: String,
    pub plan_hash: String,
    pub status: JobStatus,
    pub priority: i64,
    pub created_at: String,
    pub updated_at: String,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct JobSpec<'a> {
    pub idempotency_key: &'a str,
    pub video_ref: &'a str,
    pub plan_hash: &'a str,
}

#[derive(Debug, Clone, Copy)]
pub struct TransitionSpec<'a> {
    pub id: &'a str,
    pub target: JobStatus,
    pub error: Option<(&'a str, &'a str)>,
}

#[derive(Debug, thiserror::Error)]
pub enum QueueError {
    #[error(
        "This workspace is inactive or retired. Ownership and accounting reconciliation are required before changing queue storage."
    )]
    Inactive,
    #[error(
        "This queue cursor is invalid or the queue changed. Restart queue list without --cursor."
    )]
    CursorInvalid,
    #[error("Queue data is invalid: {0}")]
    Corrupt(&'static str),
    #[error("Queue storage IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Invalid(&'static str),
    #[error("Job prefix is ambiguous: {0}")]
    Ambiguous(String),
    #[error("Queue data is invalid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("Invalid job transition from {0:?} to {1:?}")]
    InvalidTransition(JobStatus, JobStatus),
    #[error("Job not found: {0}")]
    NotFound(String),
    #[error("Mutex lock poisoned")]
    LockPoisoned,
}

impl QueueError {
    #[must_use]
    pub fn diagnostic(&self) -> crate::protocol::Diagnostic {
        use crate::protocol::ErrorCode;
        if self.is_busy() {
            return crate::protocol::Diagnostic::new(ErrorCode::AccountBusy, &self.to_string());
        }
        let code = match self {
            Self::Inactive => ErrorCode::NotReady,
            Self::CursorInvalid => ErrorCode::CursorInvalid,
            Self::Ambiguous(_) => ErrorCode::RefAmbiguous,
            Self::NotFound(_) => ErrorCode::RefNotFound,
            Self::Invalid(_) | Self::InvalidTransition(..) => ErrorCode::InputRequired,
            _ => ErrorCode::DataInvalid,
        };
        crate::protocol::Diagnostic::new(code, &self.to_string())
    }

    fn is_busy(&self) -> bool {
        matches!(self, Self::Database(error) if matches!(error.sqlite_error_code(),
            Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked)))
    }
}
