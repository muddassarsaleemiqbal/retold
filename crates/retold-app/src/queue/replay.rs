//! Read a bounded progress page and final outcome from the same committed job snapshot.
use serde::{Deserialize, Serialize};

use super::{EVENT_PAGE_SIZE, JobRecord, QueueError, QueueStore};
use crate::protocol::Payload;
use crate::studio::ProgressEvent;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayPage {
    pub job: JobRecord,
    pub events: Vec<ProgressEvent>,
    pub result: Option<Payload>,
    pub is_terminal: bool,
    pub next_sequence: u64,
}

impl ReplayPage {
    /// The saved execution outcome, with an observer cursor; never new execution authority.
    #[must_use]
    pub fn terminal_result(mut self) -> Payload {
        if !self.is_terminal {
            return crate::protocol::Diagnostic::new(
                crate::protocol::ErrorCode::NotReady,
                "The attachment has not reached the end of a terminal job.",
            )
            .into_payload();
        }
        let metadata = self.attachment_metadata();
        if self.job.status == super::JobStatus::Stopped {
            return self.stopped_result(&metadata);
        }
        match self.result.take() {
            Some(result) => recorded_result(result, metadata),
            None => self.observed_terminal(&metadata),
        }
    }

    fn attachment_metadata(&self) -> serde_json::Value {
        serde_json::json!({"job_id": self.job.id, "after_sequence": self.next_sequence,
            "status": self.job.status.as_str()})
    }

    fn stopped_result(&self, metadata: &serde_json::Value) -> Payload {
        Payload::Result {
            outcome: crate::protocol::Outcome::Completed,
            contents: serde_json::json!({"text": format!("Job {} stopped.", self.job.id),
                "attachment": metadata, "execution_result": self.result}),
        }
    }

    fn observed_terminal(&self, metadata: &serde_json::Value) -> Payload {
        use super::JobStatus;
        use crate::protocol::{Diagnostic, ErrorCode, Outcome};
        let text = format!(
            "Job {} is {}. No detailed execution outcome was recorded.",
            self.job.id,
            self.job.status.as_str()
        );
        match self.job.status {
            JobStatus::Completed | JobStatus::Stopped => Payload::Result {
                outcome: Outcome::Completed,
                contents: serde_json::json!({"text": text, "attachment": metadata}),
            },
            status => {
                let code = if status == JobStatus::Failed {
                    ErrorCode::ExecutionFailed
                } else {
                    ErrorCode::NotReady
                };
                let mut diagnostic = Diagnostic::new(code, &text);
                diagnostic.context = serde_json::json!({"attachment": metadata});
                diagnostic.into_payload()
            }
        }
    }
}

fn recorded_result(result: Payload, metadata: serde_json::Value) -> Payload {
    match result {
        Payload::Result {
            outcome: crate::protocol::Outcome::Completed,
            contents,
        } => Payload::Result {
            outcome: crate::protocol::Outcome::Completed,
            contents: attach_context(contents, metadata),
        },
        Payload::Error {
            outcome,
            mut diagnostic,
        } => {
            diagnostic.context = attach_context(diagnostic.context, metadata);
            Payload::Error {
                outcome,
                diagnostic,
            }
        }
        _ => crate::protocol::Diagnostic::new(
            crate::protocol::ErrorCode::DataInvalid,
            "Saved job outcome is not a terminal execution result.",
        )
        .into_payload(),
    }
}

fn attach_context(
    mut contents: serde_json::Value,
    metadata: serde_json::Value,
) -> serde_json::Value {
    if let Some(object) = contents.as_object_mut() {
        object.insert("attachment".into(), metadata);
        contents
    } else {
        serde_json::json!({"execution": contents, "attachment": metadata})
    }
}

impl QueueStore {
    pub fn replay(&self, reference: &str, after: u64) -> Result<ReplayPage, QueueError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction()?;
        let job = super::store::read_by_reference(&transaction, reference)?
            .ok_or_else(|| QueueError::NotFound(reference.into()))?;
        let events = super::durable::read_events(&transaction, &job.id, after)?;
        let result = super::durable::read_result(&transaction, &job.id)?;
        let is_terminal = job.status.is_terminal() && events.len() < EVENT_PAGE_SIZE;
        let next_sequence = events.last().map_or(after, |event| event.sequence);
        let page = ReplayPage {
            job,
            events,
            result,
            is_terminal,
            next_sequence,
        };
        transaction.commit()?;
        Ok(page)
    }
}
