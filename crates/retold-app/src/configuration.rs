//! Configuration validation does not load secrets or probe providers.
use crate::protocol::{Diagnostic, ErrorCode, Outcome, Payload};
use std::path::Path;

#[must_use]
pub fn validate(root: &Path) -> Payload {
    match retold_engine::configuration::validate(root) {
        Ok(()) => Payload::Result {
            outcome: Outcome::Completed,
            contents: serde_json::json!({"valid": true, "text": "Configuration is valid. Provider availability and production readiness were not tested."}),
        },
        Err(message) => Diagnostic::new(ErrorCode::InputRequired, &message).into_payload(),
    }
}

#[must_use]
pub fn migrate(root: &Path) -> Payload {
    respond(retold_engine::configuration::migrate(root))
}

#[must_use]
pub fn dry_run_migrate(root: &Path) -> Payload {
    respond(retold_engine::configuration::dry_run_migrate(root))
}

fn respond(result: Result<retold_engine::configuration::MigrationResult, String>) -> Payload {
    match result {
        Ok(outcome_data) => Payload::Result {
            outcome: Outcome::Completed,
            contents: serde_json::to_value(&outcome_data).unwrap_or_default(),
        },
        Err(message) => Diagnostic::new(ErrorCode::InputRequired, &message).into_payload(),
    }
}
