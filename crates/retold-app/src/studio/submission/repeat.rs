//! Explicit idempotency keys return the original receipt after progress changes the current plan.
use crate::library::Catalog;
use crate::protocol::{Diagnostic, ErrorCode};
use crate::queue::{ExecutionIntent, JobRecord, QueueStore};
use crate::studio::resolve_workspace_state;

use super::{SubmitOptions, valid_key};

pub(super) fn previous(
    catalog: &Catalog,
    reference: &str,
    options: &SubmitOptions,
) -> Result<Option<JobRecord>, Diagnostic> {
    let Some(key) = options.idempotency_key.as_deref() else {
        return Ok(Option::default());
    };
    if !valid_key(key) {
        return Err(conflict("The idempotency key is invalid."));
    }
    let state = resolve_workspace_state(Some(&catalog.root));
    let database = state.join(crate::studio::server::QUEUE_DB_NAME);
    if !database.exists() {
        return Ok(Option::default());
    }
    let queue = QueueStore::open_read_only(&database).map_err(|error| error.diagnostic())?;
    let Some(job) = queue.find_by_key(key).map_err(|error| error.diagnostic())? else {
        return Ok(Option::default());
    };
    let intent = queue.intent(&job.id).map_err(|error| error.diagnostic())?
        .ok_or_else(|| conflict("This key belongs to a job without an executable intent; inspect that job and choose a new key."))?;
    let video = catalog.resolve(reference)?;
    let same_binding = job.video_ref == video.id && intent.library == catalog.root;
    if !same_binding {
        return Err(conflict(
            "This idempotency key belongs to another video or library.",
        ));
    }
    validate_repeat(options, &job, &intent)?;
    Ok(Some(job))
}

fn validate_repeat(
    options: &SubmitOptions,
    job: &JobRecord,
    intent: &ExecutionIntent,
) -> Result<(), Diagnostic> {
    if options.execution.plan != intent.plan {
        return Err(conflict(
            "This idempotency key already names a different execution request.",
        ));
    }
    let reviewed = options.execution.plan_hash.as_deref();
    let same_hash = reviewed.is_none_or(|hash| hash == job.plan_hash);
    if !same_hash {
        return Err(Diagnostic::new(
            ErrorCode::PlanChanged,
            "This key already names a different reviewed plan. Inspect the original job.",
        ));
    }
    Ok(())
}

fn conflict(message: &str) -> Diagnostic {
    Diagnostic::new(ErrorCode::InputRequired, message)
}
