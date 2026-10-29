//! Read-only cache evidence. Envelope integrity does not prove current-input reuse or readiness.

use super::{Envelope, Record, SCHEMA, State, cache_path, hash};
use crate::{EngineError, RunLayout, fsx};
use serde::{Deserialize, Serialize};

pub const MAX_INSPECTION_BYTES: u64 = 8 * 1024 * 1024;
const MAX_INSPECTION_RECORDS: usize = 2_000;
const TOTAL_INSPECTION_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Integrity {
    EnvelopeVerified,
    Missing,
    Invalid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordedStep {
    pub step: String,
    pub integrity: Integrity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Inspection {
    pub state_present: bool,
    pub state_hash: String,
    pub active_tasks: Vec<String>,
    pub recorded_failure: bool,
    pub steps: Vec<RecordedStep>,
}

pub fn inspect(layout: &RunLayout) -> Result<Inspection, EngineError> {
    validate_directory(&layout.meta())?;
    let state_path = layout.meta().join("state.json");
    let bytes = fsx::read_bounded(&state_path, MAX_INSPECTION_BYTES)?;
    let inspection = inspect_state(layout, bytes.as_deref())?;
    if fsx::read_bounded(&state_path, MAX_INSPECTION_BYTES)? != bytes {
        return Err(EngineError::Setup(
            "Run state changed during inspection; retry the query.".into(),
        ));
    }
    Ok(inspection)
}

fn inspect_state(layout: &RunLayout, bytes: Option<&[u8]>) -> Result<Inspection, EngineError> {
    let state = parse_state(bytes)?;
    let steps = inspect_records(layout, &state)?;
    let mut active_tasks = state.active_tasks;
    active_tasks.extend(state.active);
    Ok(Inspection {
        state_present: bytes.is_some(),
        state_hash: hash(&bytes)?,
        active_tasks: active_tasks.into_iter().collect(),
        recorded_failure: state.error.is_some(),
        steps,
    })
}

fn parse_state(bytes: Option<&[u8]>) -> Result<State, EngineError> {
    let Some(bytes) = bytes else {
        return Ok(State::default());
    };
    let state: State = serde_json::from_slice(bytes).map_err(|_| {
        EngineError::Setup("Run state is invalid; restore it before execution.".into())
    })?;
    let supported = state.schema == SCHEMA;
    let bounded = state.completed.len() <= MAX_INSPECTION_RECORDS;
    if !supported || !bounded {
        return Err(EngineError::Setup(
            "Run state schema or record count is unsupported.".into(),
        ));
    }
    Ok(state)
}

fn inspect_records(layout: &RunLayout, state: &State) -> Result<Vec<RecordedStep>, EngineError> {
    let empty = state.completed.is_empty();
    if empty {
        return Ok(Vec::new());
    }
    let cache = layout.meta().join("cache");
    if cache.exists() {
        validate_directory(&cache)?;
    }
    let mut remaining = TOTAL_INSPECTION_BYTES;
    state
        .completed
        .iter()
        .map(|(step, record)| {
            let path = cache_path(layout, step, &record.key)?;
            let bytes = fsx::read_bounded(&path, remaining.min(MAX_INSPECTION_BYTES))?;
            remaining = remaining
                .saturating_sub(bytes.as_ref().map_or(0, |contents| contents.len() as u64));
            Ok(RecordedStep {
                step: step.clone(),
                integrity: integrity(bytes.as_deref(), step, record),
            })
        })
        .collect()
}

fn integrity(bytes: Option<&[u8]>, step: &str, record: &Record) -> Integrity {
    let Some(bytes) = bytes else {
        return Integrity::Missing;
    };
    let Ok(envelope) = serde_json::from_slice::<Envelope>(bytes) else {
        return Integrity::Invalid;
    };
    let expected = (&record.key, &record.output_hash);
    let actual = (&envelope.record.key, &envelope.record.output_hash);
    let valid = envelope.schema == SCHEMA
        && envelope.step.eq_ignore_ascii_case(step)
        && actual == expected
        && hash(&envelope.value).is_ok_and(|digest| digest == record.output_hash);
    if valid {
        Integrity::EnvelopeVerified
    } else {
        Integrity::Invalid
    }
}

pub fn validate_directory(path: &std::path::Path) -> Result<(), EngineError> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| EngineError::io(path, error))?;
    if !metadata.is_dir() {
        return Err(EngineError::Setup(
            "Inspection requires a real directory, not a symlink.".into(),
        ));
    }
    Ok(())
}
