//! A background submission pins the confirmed plan; dequeue never turns confirmation into a new plan.
use std::path::Path;

use crate::continuation::{self, ContinueOptions};
use crate::protocol::{Diagnostic, ErrorCode, Outcome, Payload};
use crate::queue::{ExecutionIntent, JobRecord, JobSpec, QueueStore};

use super::{query_status, resolve_workspace_state};

const MAX_KEY_BYTES: usize = 160;
mod repeat;

#[derive(Debug, Clone, Default)]
pub struct SubmitOptions {
    pub execution: ContinueOptions,
    pub idempotency_key: Option<String>,
}

#[must_use]
pub fn submit(root: &Path, reference: &str, options: &SubmitOptions) -> Payload {
    submit_job(root, reference, options)
        .map_or_else(Diagnostic::into_payload, |job| submitted(&job))
}

pub fn submit_job(
    root: &Path,
    reference: &str,
    options: &SubmitOptions,
) -> Result<JobRecord, Diagnostic> {
    let catalog = crate::discovery::scan(root)?;
    if let Some(receipt) = repeat::previous(&catalog, reference, options)? {
        return Ok(receipt);
    }
    let (plan, _) = crate::plan::compile(&catalog, reference, &options.execution.plan)?;
    continuation::confirm(&plan, &options.execution)?;
    save_submission(&plan, options)
}

fn save_submission(
    plan: &crate::plan::ExecutablePlan,
    options: &SubmitOptions,
) -> Result<JobRecord, Diagnostic> {
    let library = &plan.runtime.paths.library;
    let state = resolve_workspace_state(Some(library));
    if !query_status(&state).running {
        return Err(Diagnostic::new(
            ErrorCode::StudioRequired,
            "Start studio before submitting a background job.",
        ));
    }
    let _owner = continuation::hold_account("submit background job")?;
    let queue = QueueStore::open(&state.join(super::server::QUEUE_DB_NAME)).map_err(queue_error)?;
    let key = idempotency_key(options, &plan.plan_hash)?;
    let spec = JobSpec {
        idempotency_key: &key,
        video_ref: &plan.authority.video_id,
        plan_hash: &plan.plan_hash,
    };
    let intent = ExecutionIntent {
        library: library.clone(),
        plan: options.execution.plan.clone(),
    };
    let job = queue
        .enqueue_executable(spec, &intent, &jiff::Timestamp::now().to_string())
        .map_err(queue_error)?;
    Ok(job)
}

fn idempotency_key(options: &SubmitOptions, hash: &str) -> Result<String, Diagnostic> {
    let key = options
        .idempotency_key
        .clone()
        .unwrap_or_else(|| hash.into());
    let valid = valid_key(&key);
    if !valid {
        return Err(Diagnostic::new(
            ErrorCode::InputRequired,
            "Use a nonempty idempotency key of at most 160 bytes, without control characters.",
        ));
    }
    Ok(key)
}

fn valid_key(key: &str) -> bool {
    !key.trim().is_empty() && key.len() <= MAX_KEY_BYTES && !contains_control(key)
}

fn contains_control(key: &str) -> bool {
    key.chars().any(char::is_control)
}

fn submitted(job: &crate::queue::JobRecord) -> Payload {
    let terminal = job.status.is_terminal();
    let outcome = if terminal {
        Outcome::Completed
    } else {
        Outcome::Queued
    };
    Payload::Result {
        outcome,
        contents: serde_json::json!({
            "text": format!("Job {} is {}. Inspect it with: retold queue show {}", job.id, job.status.as_str(), job.id),
            "job": job,
        }),
    }
}

fn queue_error(error: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::new(ErrorCode::ExecutionFailed, &error.to_string())
}
