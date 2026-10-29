//! IPC request/response types and status models for the background studio.

use serde::{Deserialize, Serialize};

use crate::queue::JobRecord;

pub(crate) const MAX_RESPONSE_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StudioStatus {
    pub running: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uptime_seconds: Option<u64>,
    pub active_jobs: usize,
    pub queue_depth: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<crate::protocol::Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgressEvent {
    pub sequence: u64,
    pub job_id: String,
    pub timestamp: String,
    pub stage: String,
    pub step: Option<String>,
    pub message: String,
    pub is_terminal: bool,
}

impl ProgressEvent {
    #[must_use]
    pub fn detail(&self) -> retold_engine::events::Detail {
        retold_engine::events::Detail::JobProgress {
            timestamp: self.timestamp.clone(),
            stage: self.stage.clone(),
            step: self.step.clone(),
            message: self.message.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum StudioRequest {
    Capabilities,
    GetDispatch,
    StopAll {
        control_id: String,
    },
    ResumeDispatch {
        control_id: String,
    },
    Ping,
    Shutdown,
    ListQueue {
        #[serde(default)]
        options: crate::queue::ListOptions,
    },
    GetJob {
        id: String,
    },
    PauseJob {
        id: String,
    },
    UnpauseJob {
        id: String,
    },
    StopJob {
        id: String,
    },
    MoveJob {
        id: String,
        before: String,
    },
    AttachEvents {
        job_id: String,
        after_sequence: Option<u64>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "response", rename_all = "snake_case")]
pub enum StudioResponse {
    Capabilities {
        protocol: u32,
        features: Vec<String>,
    },
    Dispatch {
        state: crate::queue::DispatchState,
    },
    StopAccepted {
        receipt: crate::queue::StopReceipt,
    },
    Pong {
        pid: u32,
        uptime_seconds: u64,
        active_jobs: usize,
        queue_depth: usize,
    },
    Stopped,
    ShutdownAccepted,
    Queue {
        #[serde(flatten)]
        page: crate::queue::JobPage,
    },
    Job {
        job: Option<JobRecord>,
        #[serde(default)]
        result: Option<crate::protocol::Payload>,
        #[serde(default)]
        execution: Option<Box<crate::queue::ExecutionIntent>>,
    },
    JobTransitioned {
        job: JobRecord,
    },
    Events {
        events: Vec<ProgressEvent>,
        is_terminal: bool,
    },
    Error {
        code: crate::protocol::ErrorCode,
        message: String,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum StudioError {
    #[error("Remote studio request failed: {message}")]
    Remote {
        code: crate::protocol::ErrorCode,
        message: String,
    },
    #[error("Studio is not running")]
    NotRunning,
    #[error("Studio is already running")]
    AlreadyRunning,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Queue error: {0}")]
    Queue(#[from] crate::queue::QueueError),
    #[error("Protocol error: {0}")]
    Protocol(String),
}

impl StudioError {
    #[must_use]
    pub fn diagnostic(&self) -> crate::protocol::Diagnostic {
        use crate::protocol::{Diagnostic, ErrorCode};
        match self {
            Self::Queue(error) => error.diagnostic(),
            Self::Remote { code, message } => Diagnostic::new(*code, message),
            Self::NotRunning => {
                Diagnostic::new(ErrorCode::StudioRequired, "Studio is not running.")
            }
            _ => Diagnostic::new(ErrorCode::ExecutionFailed, &self.to_string()),
        }
    }
}

impl From<crate::protocol::Diagnostic> for StudioError {
    fn from(diagnostic: crate::protocol::Diagnostic) -> Self {
        Self::Remote {
            code: diagnostic.code,
            message: diagnostic.message,
        }
    }
}
