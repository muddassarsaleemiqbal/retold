//! Bound identifiers and terminal text before storage and again when reading old records.
use super::{JobRecord, JobSpec, QueueError};

const MAX_KEY_BYTES: usize = 160;
const MAX_REFERENCE_BYTES: usize = 512;
const MAX_HASH_BYTES: usize = 256;
const MAX_TIMESTAMP_BYTES: usize = 64;
const MAX_ERROR_BYTES: usize = 1024;

impl JobSpec<'_> {
    pub(super) fn validate(&self) -> Result<(), QueueError> {
        let fields = [
            (self.idempotency_key, MAX_KEY_BYTES),
            (self.video_ref, MAX_REFERENCE_BYTES),
            (self.plan_hash, MAX_HASH_BYTES),
        ];
        let valid = fields
            .iter()
            .all(|(value, limit)| bounded_text(value, *limit));
        if valid {
            Ok(())
        } else {
            Err(QueueError::Invalid(
                "Job references, keys and plan hashes must be bounded, nonempty text without control characters.",
            ))
        }
    }
}

impl JobRecord {
    pub(super) fn validate(&self) -> Result<(), QueueError> {
        JobSpec {
            idempotency_key: &self.idempotency_key,
            video_ref: &self.video_ref,
            plan_hash: &self.plan_hash,
        }
        .validate()?;
        let fields = [
            (&*self.id, MAX_REFERENCE_BYTES),
            (&*self.created_at, MAX_TIMESTAMP_BYTES),
            (&*self.updated_at, MAX_TIMESTAMP_BYTES),
        ];
        let valid = fields
            .iter()
            .all(|(value, limit)| bounded_text(value, *limit));
        if !valid {
            return Err(QueueError::Corrupt("Invalid job identity or timestamp."));
        }
        validate_optional(self.error_code.as_deref())?;
        validate_optional(self.error_message.as_deref())
    }
}

fn validate_optional(text: Option<&str>) -> Result<(), QueueError> {
    let valid = text.is_none_or(|text| bounded_text(text, MAX_ERROR_BYTES));
    if valid {
        Ok(())
    } else {
        Err(QueueError::Corrupt("Invalid job error text."))
    }
}

fn bounded_text(text: &str, limit: usize) -> bool {
    !text.trim().is_empty() && safe_text(text, limit)
}

fn safe_text(text: &str, limit: usize) -> bool {
    text.len() <= limit && !text.chars().any(char::is_control)
}

impl crate::studio::ProgressEvent {
    pub(super) fn validate(&self) -> Result<(), QueueError> {
        let fields = [
            (&*self.job_id, MAX_REFERENCE_BYTES),
            (&*self.timestamp, MAX_TIMESTAMP_BYTES),
            (&*self.stage, MAX_REFERENCE_BYTES),
        ];
        let headers = fields
            .iter()
            .all(|(text, limit)| bounded_text(text, *limit));
        let message = safe_text(&self.message, MAX_ERROR_BYTES);
        let step = self
            .step
            .as_deref()
            .is_none_or(|step| safe_text(step, MAX_REFERENCE_BYTES));
        if headers && message && step {
            Ok(())
        } else {
            Err(QueueError::Corrupt(
                "Invalid progress event text or identity.",
            ))
        }
    }
}

pub(super) fn reference(text: &str) -> Result<(), QueueError> {
    if bounded_text(text, MAX_REFERENCE_BYTES) {
        Ok(())
    } else {
        Err(QueueError::Invalid(
            "Use a bounded, nonempty job reference without control characters.",
        ))
    }
}

pub(super) fn invalid_row(error: QueueError) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
}
