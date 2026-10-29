//! Executable intents and event cursors live in the same durable database as job state.
use std::path::PathBuf;

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

use super::store::{apply_transition, enqueue_locked, read_job_by_id};
use super::{JobRecord, JobSpec, JobStatus, QueueError, QueueStore, TransitionSpec};
use crate::plan::PlanOptions;
use crate::protocol::Payload;
use crate::studio::ProgressEvent;

const MAX_INTENT_BYTES: usize = 64 * 1024;
pub const EVENT_PAGE_SIZE: usize = 128;
const MAX_EVENT_BYTES: usize = 2048;
const SELECT_INTENT: &str = include_str!("select_intent.sql");
const SELECT_EVENTS: &str = include_str!("select_events.sql");
const SELECT_LAST_SEQUENCE: &str = include_str!("select_last_sequence.sql");
const SELECT_RESULT: &str = include_str!("select_result.sql");
const UPSERT_OUTCOME_SQL: &str = include_str!("save_result.sql");
const INSERT_INTENT: &str = include_str!("insert_intent.sql");
const APPEND_EVENT_SQL: &str = include_str!("insert_event.sql");

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionIntent {
    pub library: PathBuf,
    pub plan: PlanOptions,
}

impl QueueStore {
    pub fn enqueue_executable(
        &self,
        spec: JobSpec<'_>,
        intent: &ExecutionIntent,
        now: &str,
    ) -> Result<JobRecord, QueueError> {
        let encoded = encode_intent(intent)?;
        let mut connection = self.write_lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let job = enqueue_locked(&transaction, spec, now)?;
        save_intent(&transaction, &job.id, &encoded)?;
        transaction.commit()?;
        Ok(job)
    }

    pub fn intent(&self, id: &str) -> Result<Option<ExecutionIntent>, QueueError> {
        let connection = self.lock()?;
        read_intent(&connection, id)
    }

    pub fn record_event(&self, event: ProgressEvent) -> Result<ProgressEvent, QueueError> {
        let connection = self.write_lock()?;
        insert_event(&connection, event)
    }

    pub fn events(&self, id: &str, after: u64) -> Result<Vec<ProgressEvent>, QueueError> {
        let connection = self.lock()?;
        read_events(&connection, id, after)
    }

    /// Outcome and terminal event commit atomically; no completion is acknowledged first.
    pub fn finish(
        &self,
        id: &str,
        result: &Payload,
        event: ProgressEvent,
    ) -> Result<JobRecord, QueueError> {
        Completion {
            id,
            result,
            event,
            mode: CompletionMode::Finished,
        }
        .commit(self)
    }

    pub fn recover(&self, id: &str, event: ProgressEvent) -> Result<JobRecord, QueueError> {
        let result = crate::protocol::Diagnostic::new(crate::protocol::ErrorCode::PlanChanged,
            "The previous worker exited before confirming completion. Inspect saved attempts and review a fresh plan; no automatic retry occurred.").into_payload();
        Completion {
            id,
            result: &result,
            event,
            mode: CompletionMode::Interrupted,
        }
        .commit(self)
    }

    pub fn result(&self, id: &str) -> Result<Option<Payload>, QueueError> {
        let connection = self.lock()?;
        read_result(&connection, id)
    }
}

pub(super) fn read_events(
    connection: &Connection,
    id: &str,
    after: u64,
) -> Result<Vec<ProgressEvent>, QueueError> {
    let after = i64::try_from(after).map_err(|_| QueueError::CursorInvalid)?;
    if !super::schema::has_table(connection, "job_events")? {
        return if after == 0 {
            Ok(Vec::new())
        } else {
            Err(QueueError::CursorInvalid)
        };
    }
    let last: i64 = connection.query_row(SELECT_LAST_SEQUENCE, [id], |row| row.get(0))?;
    if after > last {
        return Err(QueueError::CursorInvalid);
    }
    let limit = i64::try_from(EVENT_PAGE_SIZE).unwrap_or(i64::MAX);
    let mut statement = connection.prepare(SELECT_EVENTS)?;
    let rows = statement.query_map(params![id, after, limit], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })?;
    rows.map(|row| {
        let (sequence, body) = row?;
        decode_event(sequence, &body, id)
    })
    .collect()
}

const MAX_RESULT_BYTES: usize = 512 * 1024;

pub(super) fn read_intent(
    connection: &Connection,
    id: &str,
) -> Result<Option<ExecutionIntent>, QueueError> {
    if !super::schema::has_table(connection, "job_execution")? {
        return Ok(Option::default());
    }
    let encoded: Option<String> = connection
        .query_row(SELECT_INTENT, [id], |row| row.get(0))
        .optional()?;
    encoded
        .map(|text| {
            if text.len() > MAX_INTENT_BYTES {
                return Err(QueueError::Invalid("Saved job intent is oversized."));
            }
            serde_json::from_str(&text).map_err(QueueError::from)
        })
        .transpose()
}

pub(super) fn read_result(
    connection: &Connection,
    id: &str,
) -> Result<Option<Payload>, QueueError> {
    if !super::schema::has_table(connection, "job_outcomes")? {
        return Ok(Option::default());
    }
    let encoded: Option<String> = connection
        .query_row(SELECT_RESULT, [id], |row| row.get(0))
        .optional()?;
    encoded
        .map(|text| {
            if text.len() > MAX_RESULT_BYTES {
                return Err(QueueError::Corrupt(
                    "Saved job outcome exceeds the inspection limit.",
                ));
            }
            serde_json::from_str(&text).map_err(QueueError::from)
        })
        .transpose()
}

fn decode_event(sequence: i64, body: &str, id: &str) -> Result<ProgressEvent, QueueError> {
    if body.len() > MAX_EVENT_BYTES {
        return Err(QueueError::Corrupt(
            "Saved progress event exceeds the inspection limit.",
        ));
    }
    let mut event: ProgressEvent = serde_json::from_str(body)?;
    event.validate()?;
    if event.job_id != id {
        return Err(QueueError::Corrupt("Saved event belongs to another job."));
    }
    event.sequence =
        u64::try_from(sequence).map_err(|_| QueueError::Corrupt("Invalid event sequence."))?;
    Ok(event)
}

enum CompletionMode {
    Finished,
    Interrupted,
}

struct Completion<'a> {
    id: &'a str,
    result: &'a Payload,
    event: ProgressEvent,
    mode: CompletionMode,
}

impl Completion<'_> {
    fn commit(self, store: &QueueStore) -> Result<JobRecord, QueueError> {
        let mut connection = store.write_lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = read_job_by_id(&transaction, self.id)?;
        let target = match self.mode {
            CompletionMode::Finished => outcome_status(&current, self.result),
            CompletionMode::Interrupted => JobStatus::Blocked,
        };
        if !super::types::can_transition(current.status, target) {
            return Err(QueueError::InvalidTransition(current.status, target));
        }
        let event = outcome_event(self.event, self.id, target)?;
        let spec = TransitionSpec {
            id: self.id,
            target,
            error: outcome_error(self.result),
        };
        apply_transition(&transaction, spec, &event.timestamp)?;
        save_result(&transaction, self.id, self.result)?;
        insert_event(&transaction, event)?;
        let job = read_job_by_id(&transaction, self.id)?;
        transaction.commit()?;
        Ok(job)
    }
}

fn outcome_event(
    mut event: ProgressEvent,
    id: &str,
    target: JobStatus,
) -> Result<ProgressEvent, QueueError> {
    if event.job_id != id {
        return Err(QueueError::Invalid("Outcome event belongs to another job."));
    }
    event.stage = target.as_str().into();
    event.is_terminal = target.is_terminal();
    Ok(event)
}

fn save_result(connection: &Connection, id: &str, result: &Payload) -> Result<(), QueueError> {
    let encoded = serde_json::to_string(result)?;
    if encoded.len() > MAX_RESULT_BYTES {
        return Err(QueueError::Invalid(
            "Job outcome exceeds the bounded record size.",
        ));
    }
    connection.execute(UPSERT_OUTCOME_SQL, params![id, encoded])?;
    Ok(())
}

fn encode_intent(intent: &ExecutionIntent) -> Result<String, QueueError> {
    let encoded = serde_json::to_string(intent)?;
    let bounded = encoded.len() <= MAX_INTENT_BYTES;
    let absolute = intent.library.is_absolute();
    if !absolute || !bounded {
        return Err(QueueError::Invalid(
            "Job intent needs an absolute library path and bounded inputs.",
        ));
    }
    Ok(encoded)
}

fn save_intent(connection: &Connection, id: &str, encoded: &str) -> Result<(), QueueError> {
    let prior: Option<String> = connection
        .query_row(SELECT_INTENT, [id], |row| row.get(0))
        .optional()?;
    if let Some(prior) = prior {
        return if prior == encoded {
            Ok(())
        } else {
            Err(QueueError::Invalid(
                "A saved execution intent is immutable.",
            ))
        };
    }
    connection.execute(INSERT_INTENT, params![id, encoded])?;
    Ok(())
}

fn insert_event(
    connection: &Connection,
    mut event: ProgressEvent,
) -> Result<ProgressEvent, QueueError> {
    event.validate()?;
    event.sequence = 0;
    let body = serde_json::to_string(&event)?;
    if body.len() > MAX_EVENT_BYTES {
        return Err(QueueError::Invalid(
            "Job event exceeds the bounded record size.",
        ));
    }
    connection.execute(APPEND_EVENT_SQL, params![event.job_id, body])?;
    event.sequence = u64::try_from(connection.last_insert_rowid())
        .map_err(|_| QueueError::Invalid("Invalid event sequence."))?;
    Ok(event)
}

pub(super) fn record_transition(
    connection: &Connection,
    spec: TransitionSpec<'_>,
    timestamp: &str,
) -> Result<(), QueueError> {
    let event = ProgressEvent {
        sequence: 0,
        job_id: spec.id.into(),
        timestamp: timestamp.into(),
        stage: spec.target.as_str().into(),
        step: None,
        message: format!("Job is {}", spec.target.as_str()),
        is_terminal: spec.target.is_terminal(),
    };
    insert_event(connection, event)?;
    Ok(())
}

fn outcome_status(job: &JobRecord, result: &Payload) -> JobStatus {
    let cancelled = is_cancelled_result(result);
    if cancelled {
        return match job.status {
            JobStatus::Pausing => JobStatus::Paused,
            JobStatus::Stopping => JobStatus::Stopped,
            _ => JobStatus::Failed,
        };
    }
    match result {
        Payload::Result {
            outcome: crate::protocol::Outcome::Completed,
            ..
        } => JobStatus::Completed,
        Payload::Error {
            outcome: crate::protocol::Outcome::Blocked,
            ..
        } => JobStatus::Blocked,
        _ => JobStatus::Failed,
    }
}

fn is_cancelled_result(result: &Payload) -> bool {
    matches!(result, Payload::Error { diagnostic, .. }
        if diagnostic.context.get("cancelled") == Some(&serde_json::Value::Bool(true)))
}

fn outcome_error(result: &Payload) -> Option<(&str, &str)> {
    match result {
        Payload::Error { diagnostic, .. } => Some((diagnostic.code.as_str(), &diagnostic.message)),
        _ => None,
    }
}
