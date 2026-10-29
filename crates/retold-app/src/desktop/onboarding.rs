//! First-launch preparation: the installation account, attempt reconciliation and provider login.
//! Every change is previewed first; nothing here contacts a provider.
use std::path::Path;

use crate::account::Usd;
use crate::account::commands::{self, Request};
use crate::protocol::{Diagnostic, ErrorCode, Payload};
use crate::providers::{self, LoginRequest, LoginStatus};

fn contents(payload: Payload) -> Result<serde_json::Value, Diagnostic> {
    match payload {
        Payload::Result { contents, .. } => Ok(contents),
        Payload::Error { diagnostic, .. } => Err(diagnostic),
        Payload::Event(_) => Err(Diagnostic::new(
            ErrorCode::DataInvalid,
            "Unexpected setup response.",
        )),
    }
}

/// The system time zone, used for daily and monthly account windows.
#[must_use]
pub fn local_timezone() -> String {
    jiff::tz::TimeZone::system()
        .iana_name()
        .unwrap_or("UTC")
        .to_owned()
}

/// Whether the installation account exists, read without creating it.
pub fn account(timezone: &str) -> Result<serde_json::Value, Diagnostic> {
    contents(commands::respond(Request::Initialize {
        timezone,
        dry_run: true,
    }))
}

/// Create the installation account after its preview.
pub fn initialize(timezone: &str) -> Result<serde_json::Value, Diagnostic> {
    contents(commands::respond(Request::Initialize {
        timezone,
        dry_run: false,
    }))
}

/// Preview recording the provider's real cost for one held attempt.
pub fn preview_reconcile(attempt_id: &str, cost: Usd) -> Result<serde_json::Value, Diagnostic> {
    contents(commands::respond(Request::PreviewReconcile {
        attempt_id,
        cost,
    }))
}

/// Record the reviewed cost only while the preview is unchanged.
pub fn reconcile(
    attempt_id: &str,
    cost: Usd,
    reviewed: &serde_json::Value,
) -> Result<serde_json::Value, Diagnostic> {
    let fresh = preview_reconcile(attempt_id, cost)?;
    if fresh != *reviewed {
        return Err(Diagnostic::new(
            ErrorCode::PlanChanged,
            "The attempt changed. Review the reconciliation again.",
        ));
    }
    contents(commands::respond(Request::Reconcile { attempt_id, cost }))
}

/// Offline login state of one configured provider; credentials are never read.
pub fn login_status(settings: &Path, provider: &str) -> Result<LoginStatus, Diagnostic> {
    retold_engine::provider_login::status(settings, provider)
        .map_err(|error| Diagnostic::new(ErrorCode::InputRequired, &error.to_string()))
}

/// Point a provider at the environment variable holding its key; previewed, then applied.
pub fn configure_login(
    settings: &Path,
    provider: &str,
    (key_env, apply): (&str, Apply),
) -> Result<serde_json::Value, Diagnostic> {
    let mut request = LoginRequest::new(settings, provider);
    request.key_env = Some(key_env);
    request.dry_run = apply == Apply::Preview;
    contents(providers::login(&request))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Apply {
    Preview,
    Save,
}
