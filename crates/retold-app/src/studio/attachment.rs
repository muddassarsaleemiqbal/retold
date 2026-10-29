//! An attachment observes an immutable job identity and never takes execution authority.
use std::fs::{File, TryLockError};
use std::path::Path;
use std::sync::Arc;

use super::{ProgressEvent, StudioError};
use crate::queue::{QueueError, QueueStore, ReplayPage};

#[derive(Debug, Clone)]
pub struct Attachment {
    state: Arc<Path>,
    queue: QueueStore,
    job_id: Arc<str>,
    sequence: u64,
}

impl Attachment {
    pub fn open(state: &Path, reference: &str, sequence: u64) -> Result<Self, StudioError> {
        let database = state.join(super::server::QUEUE_DB_NAME);
        if !database.exists() {
            return Err(QueueError::NotFound(reference.into()).into());
        }
        let queue = QueueStore::open_read_only(&database)?;
        let page = queue.replay(reference, sequence)?;
        Ok(Self {
            state: Arc::from(state),
            queue,
            job_id: Arc::from(page.job.id),
            sequence,
        })
    }

    pub fn poll(&self) -> Result<ReplayPage, StudioError> {
        self.queue
            .replay(&self.job_id, self.sequence)
            .map_err(StudioError::from)
    }

    pub fn acknowledged(&self, event: &ProgressEvent) -> Result<Self, StudioError> {
        let same_job = event.job_id == self.job_id.as_ref();
        let forward = event.sequence > self.sequence;
        if !same_job || !forward {
            return Err(QueueError::CursorInvalid.into());
        }
        Ok(Self {
            sequence: event.sequence,
            ..self.clone()
        })
    }

    #[must_use]
    pub fn job_id(&self) -> &str {
        &self.job_id
    }

    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    #[must_use]
    pub fn detached(&self) -> crate::protocol::Payload {
        crate::protocol::Payload::Result {
            outcome: crate::protocol::Outcome::Detached,
            contents: serde_json::json!({"text": format!("Detached from job {}. Inspect queue show for its current status.", self.job_id),
                "job_id": self.job_id.as_ref(), "after_sequence": self.sequence}),
        }
    }

    #[must_use]
    pub fn annotate(
        &self,
        mut diagnostic: crate::protocol::Diagnostic,
    ) -> crate::protocol::Payload {
        diagnostic.context = serde_json::json!({"job_id": self.job_id.as_ref(),
            "after_sequence": self.sequence, "cause": diagnostic.context});
        diagnostic.into_payload()
    }

    /// Probe the existing OS lock without creating or editing any state file.
    pub fn owner_present(&self) -> Result<bool, StudioError> {
        let path = self.state.join(super::server::LOCK_NAME);
        let owner = match File::open(&path) {
            Ok(owner) => owner,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.into()),
        };
        match owner.try_lock_shared() {
            Ok(()) => Ok(false),
            Err(TryLockError::WouldBlock) => Ok(true),
            Err(TryLockError::Error(error)) => Err(error.into()),
        }
    }
}
