//! Workspace-wide admission inhibition and idempotent stop receipts share one transaction.
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

use super::{JobStatus, QueueError, QueueStore, TransitionSpec};

const SELECT_STOPPABLE: &str = include_str!("select_stoppable.sql");
const SELECT_DISPATCH: &str = include_str!("select_dispatch.sql");
const SELECT_CONTROL: &str = include_str!("select_control.sql");
const SAVE_CONTROL: &str = include_str!("save_control.sql");
const HOLD_DISPATCH: &str = include_str!("hold_dispatch.sql");
const RELEASE_DISPATCH: &str = include_str!("release_dispatch.sql");
const MAX_CONTROL_BYTES: usize = 160;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DispatchState {
    /// The accepting control request. Clearing this hold never restarts stopped jobs.
    pub inhibited_by: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StopReceipt {
    pub control_id: String,
    pub accepted_at: String,
    /// Stopped before dispatch, including paused jobs.
    pub stopped: Vec<String>,
    /// Cancellation requested; completion must be established from each job's outcome.
    pub stopping: Vec<String>,
}

impl QueueStore {
    pub fn dispatch_inhibited(&self) -> Result<bool, QueueError> {
        let state = self.dispatch_state()?;
        Ok(state.inhibited_by.is_some())
    }

    pub fn dispatch_state(&self) -> Result<DispatchState, QueueError> {
        let connection = self.lock()?;
        read_dispatch(&connection)
    }

    /// Persists the hold, affected jobs, transition events and receipt atomically.
    /// Safe without a reachable daemon: active jobs remain unconfirmed until observed.
    pub fn stop_all(&self, control_id: &str, now: &str) -> Result<StopReceipt, QueueError> {
        validate_control(control_id, now)?;
        let mut connection = self.write_lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(receipt) = read_receipt(&transaction, control_id)? {
            return Ok(receipt);
        }
        let receipt = stop_snapshot(&transaction, control_id, now)?;
        let encoded = serde_json::to_string(&receipt)?;
        transaction.execute(SAVE_CONTROL, params![control_id, encoded])?;
        transaction.execute(HOLD_DISPATCH, [control_id])?;
        transaction.commit()?;
        Ok(receipt)
    }

    /// Compare-and-clear prevents an older window from clearing a newer stop request.
    pub fn resume_dispatch(&self, control_id: &str) -> Result<DispatchState, QueueError> {
        super::validation::reference(control_id)?;
        let mut connection = self.write_lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = read_dispatch(&transaction)?;
        let newer_hold = current
            .inhibited_by
            .as_deref()
            .is_some_and(|id| id != control_id);
        if newer_hold {
            return Err(QueueError::Invalid(
                "A newer stop request holds this queue. Refresh before enabling dispatch.",
            ));
        }
        transaction.execute(RELEASE_DISPATCH, [control_id])?;
        transaction.commit()?;
        Ok(DispatchState::default())
    }
}

pub(super) fn read_dispatch(connection: &Connection) -> Result<DispatchState, QueueError> {
    if !super::schema::has_table(connection, "queue_dispatch")? {
        return Ok(DispatchState::default());
    }
    let inhibited_by = connection
        .query_row(SELECT_DISPATCH, [], |row| row.get(0))
        .optional()?;
    Ok(DispatchState { inhibited_by })
}

pub(super) fn require_open(connection: &Connection) -> Result<(), QueueError> {
    let state = read_dispatch(connection)?;
    let inhibited = state.inhibited_by.is_some();
    if inhibited {
        return Err(QueueError::Invalid(
            "Workspace dispatch is inhibited by Stop all. Explicitly enable dispatch before admitting new work.",
        ));
    }
    Ok(())
}

fn read_receipt(connection: &Connection, id: &str) -> Result<Option<StopReceipt>, QueueError> {
    let encoded: Option<String> = connection
        .query_row(SELECT_CONTROL, [id], |row| row.get(0))
        .optional()?;
    encoded
        .map(|text| serde_json::from_str(&text).map_err(QueueError::from))
        .transpose()
}

fn stop_snapshot(connection: &Connection, id: &str, now: &str) -> Result<StopReceipt, QueueError> {
    let mut statement = connection.prepare(SELECT_STOPPABLE)?;
    let jobs = statement
        .query_map([], super::store::parse_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut receipt = StopReceipt {
        control_id: id.into(),
        accepted_at: now.into(),
        stopped: Vec::new(),
        stopping: Vec::new(),
    };
    for job in jobs {
        let target = stop_target(job.status);
        if target != job.status {
            let spec = TransitionSpec {
                id: &job.id,
                target,
                error: None,
            };
            super::store::apply_transition(connection, spec, now)?;
            super::durable::record_transition(connection, spec, now)?;
        }
        match target {
            JobStatus::Stopped => receipt.stopped.push(job.id),
            _ => receipt.stopping.push(job.id),
        }
    }
    Ok(receipt)
}

const fn stop_target(status: JobStatus) -> JobStatus {
    match status {
        JobStatus::Queued | JobStatus::Paused => JobStatus::Stopped,
        _ => JobStatus::Stopping,
    }
}

fn validate_control(id: &str, now: &str) -> Result<(), QueueError> {
    let valid = !id.is_empty()
        && id.len() <= MAX_CONTROL_BYTES
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
    if !valid {
        return Err(QueueError::Invalid(
            "Use a control ID of at most 160 ASCII letters, digits, hyphens or underscores.",
        ));
    }
    super::validation::reference(now)
}
