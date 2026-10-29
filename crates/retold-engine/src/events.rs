//! One typed event stream for terminal progress, JSON logs and benchmarks.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::EngineError;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Running,
    Cached,
    Complete,
    Stopped,
}

/// Structured facts for new consumers; old log records remain readable without this field.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Detail {
    JobProgress {
        timestamp: String,
        stage: String,
        step: Option<String>,
        message: String,
    },
    StageStarted {
        stage: String,
    },
    ItemProgress {
        stage: String,
        done: usize,
        total: usize,
    },
    StepFailed {
        stage: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        edition: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        item: Option<std::num::NonZeroUsize>,
        kind: FailureKind,
        code: String,
        reason: String,
        suggested_actions: Vec<Vec<String>>,
        redacted: bool,
    },
}

/// An unclassified execution failure never implies permission to retry.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    Execution,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub timestamp: String,
    pub task: String,
    pub status: Status,
    pub elapsed_seconds: Option<f64>,
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<Detail>,
}

pub type Observer = Arc<dyn Fn(&Event) + Send + Sync>;
pub trait EventObserver: std::fmt::Debug + Send + Sync {
    fn observe(&self, event: &Event) -> Result<(), EngineError>;
}
pub type CheckedObserver = Arc<dyn EventObserver>;

pub struct Events {
    path: PathBuf,
    extra: Option<PathBuf>,
    lock: Mutex<()>,
    observer: Option<Observer>,
    checked_observer: Option<CheckedObserver>,
}

impl std::fmt::Debug for Events {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Events")
            .field("path", &self.path)
            .field("extra", &self.extra)
            .finish_non_exhaustive()
    }
}

impl Events {
    #[must_use]
    pub fn new(path: &Path, extra: Option<PathBuf>, observer: Option<Observer>) -> Self {
        Self {
            path: path.into(),
            extra,
            observer,
            checked_observer: None,
            lock: Mutex::new(()),
        }
    }
    #[must_use]
    pub fn with_checked_observer(mut self, observer: Option<CheckedObserver>) -> Self {
        self.checked_observer = observer;
        self
    }
    pub fn emit(&self, event: &Event) -> Result<(), EngineError> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| EngineError::Setup("event log lock failed".into()))?;
        let mut bytes = serde_json::to_vec(event).map_err(|e| EngineError::Setup(e.to_string()))?;
        bytes.push(b'\n');
        for path in std::iter::once(&self.path).chain(self.extra.as_ref()) {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| EngineError::io(parent, e))?;
            }
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .map_err(|e| EngineError::io(path, e))?;
            file.write_all(&bytes)
                .and_then(|()| file.sync_data())
                .map_err(|e| EngineError::io(path, e))?;
        }
        if let Some(observer) = &self.observer {
            observer(event);
        }
        if let Some(observer) = &self.checked_observer {
            observer.observe(event)?;
        }
        Ok(())
    }
}

impl Event {
    #[must_use]
    pub fn new(
        task: &str,
        status: Status,
        elapsed_seconds: Option<f64>,
        message: Option<String>,
    ) -> Self {
        Self {
            timestamp: jiff::Timestamp::now().to_string(),
            task: task.into(),
            status,
            elapsed_seconds,
            message,
            detail: None,
        }
    }

    #[must_use]
    pub fn branch_failed(stage: &str, edition: Option<&str>) -> Self {
        let reason = "A branch stopped; inspect the saved run logs before retrying.";
        let mut event = Self {
            timestamp: jiff::Timestamp::now().to_string(),
            task: stage.into(),
            status: Status::Stopped,
            elapsed_seconds: None,
            message: Some(reason.into()),
            detail: None,
        };
        event.detail = Some(Detail::StepFailed {
            stage: stage.into(),
            edition: edition.map(str::to_owned),
            item: None,
            kind: FailureKind::Execution,
            code: "RG-EXECUTION-FAILED".into(),
            reason: reason.into(),
            suggested_actions: vec![vec![
                "retold".into(),
                "explain".into(),
                "RG-EXECUTION-FAILED".into(),
            ]],
            redacted: true,
        });
        event
    }
}

/// Observe a branch before a join waits for its peers. Never cancels or retries peer work.
pub async fn observe_branch<T, E, F, R>(future: F, report: R) -> Result<T, E>
where
    F: std::future::Future<Output = Result<T, E>>,
    R: AsyncFnOnce() -> Result<(), EngineError>,
    E: From<EngineError>,
{
    let result = future.await;
    if result.is_err() {
        report().await?;
    }
    result
}
