//! Product commands always select the installation account, independently of the video library.
use super::{Account, Error, Result, canonical_directory, types::Limit};
use crate::protocol::{Diagnostic, ErrorCode, Outcome, Payload};
use retold_engine::gate::Usd;

#[derive(Debug, Clone, Copy)]
pub enum Request<'a> {
    Initialize {
        timezone: &'a str,
        dry_run: bool,
    },
    Show,
    SetLimits(&'a std::path::Path),
    Reconcile {
        attempt_id: &'a str,
        cost: Usd,
    },
    /// What `Reconcile` would change, without changing it.
    PreviewReconcile {
        attempt_id: &'a str,
        cost: Usd,
    },
    Backup {
        output: &'a std::path::Path,
        dry_run: bool,
    },
    VerifyBackup(&'a std::path::Path),
}

#[must_use]
pub fn respond(request: Request<'_>) -> Payload {
    match apply_request(request) {
        Ok(contents) => Payload::Result {
            outcome: Outcome::Completed,
            contents,
        },
        Err(error) => diagnostic(&error).into_payload(),
    }
}

fn dry_run_initialize(directory: &std::path::Path, timezone: &str) -> Result<serde_json::Value> {
    jiff::tz::TimeZone::get(timezone).map_err(|_| Error::Invalid("Unknown IANA timezone."))?;
    Ok(serde_json::json!({
        "dry_run": true,
        "directory": directory,
        "timezone": timezone,
        "default_limits": "zero",
    }))
}

fn create_account(directory: &std::path::Path, timezone: &str) -> Result<serde_json::Value> {
    jiff::tz::TimeZone::get(timezone).map_err(|_| Error::Invalid("Unknown IANA timezone."))?;
    let account = Account::initialize(directory, timezone)?;
    serde_json::to_value(account.summary()?).map_err(|_| Error::Corrupt)
}

fn reconcile_account(
    directory: &std::path::Path,
    attempt_id: &str,
    cost: Usd,
) -> Result<serde_json::Value> {
    let account = Account::open(directory)?;
    account.reconcile_attempt(attempt_id, cost)?;
    Ok(serde_json::json!({
        "reconciled": true,
        "attempt_id": attempt_id,
        "cost_usd_micros": cost.micros(),
        "cost_formatted": cost.to_string(),
        "dispatch_authorized": false,
    }))
}

fn preview_reconcile(
    directory: &std::path::Path,
    attempt_id: &str,
    cost: Usd,
) -> Result<serde_json::Value> {
    let (before, after) = Account::open(directory)?.preview_reconcile(attempt_id, cost)?;
    Ok(serde_json::json!({
        "reconcile_preview": {
            "attempt_id": attempt_id,
            "cost_usd_micros": cost.micros(),
            "cost_formatted": cost.to_string(),
            "before": before.lifetime,
            "after": after.lifetime,
            "dry_run": true,
        }
    }))
}

fn back_up(directory: &std::path::Path, output: &std::path::Path) -> Result<serde_json::Value> {
    let account = Account::open(directory)?;
    let published = account.backup(output)?;
    Ok(json_backup("backup_created", &published))
}

fn json_backup(key: &str, value: &impl serde::Serialize) -> serde_json::Value {
    serde_json::json!({ key: value })
}

fn show_account(directory: &std::path::Path) -> Result<serde_json::Value> {
    let summary = Account::open(directory)?.summary()?;
    serde_json::to_value(summary).map_err(|_| Error::Corrupt)
}

fn apply_request(request: Request<'_>) -> Result<serde_json::Value> {
    let directory = canonical_directory()?;
    match request {
        Request::Initialize {
            timezone,
            dry_run: true,
        } => dry_run_initialize(&directory, timezone),
        Request::Initialize {
            timezone,
            dry_run: false,
        } => create_account(&directory, timezone),
        Request::Show => show_account(&directory),
        Request::SetLimits(path) => set_limits(&directory, path),
        Request::Reconcile { attempt_id, cost } => reconcile_account(&directory, attempt_id, cost),
        Request::PreviewReconcile { attempt_id, cost } => {
            preview_reconcile(&directory, attempt_id, cost)
        }
        Request::Backup {
            output,
            dry_run: true,
        } => preview_backup(&directory, output),
        Request::Backup {
            output,
            dry_run: false,
        } => back_up(&directory, output),
        Request::VerifyBackup(path) => verify_backup(&directory, path),
    }
}

pub(crate) fn diagnostic(error: &Error) -> Diagnostic {
    let code = match error {
        Error::Missing | Error::Limit | Error::OperationExists | Error::Transition => {
            ErrorCode::AccountRequired
        }
        Error::Busy | Error::Occupied(_) => ErrorCode::AccountBusy,
        Error::Invalid(_) | Error::Exists => ErrorCode::InputRequired,
        Error::Corrupt => ErrorCode::AccountingUnavailable,
        Error::Backup(_) => ErrorCode::ExecutionFailed,
        Error::BackupInvalid(_) => ErrorCode::DataInvalid,
    };
    let mut diagnostic = Diagnostic::new(code, &error.to_string());
    if let Error::Occupied(owners) = error {
        diagnostic.context = serde_json::json!({ "owners": owners });
        diagnostic.suggested_actions =
            vec![vec!["retold".into(), "account".into(), "processes".into()]];
    }
    diagnostic
}

fn read_limits(path: &std::path::Path) -> Result<Vec<Limit>> {
    const MAX_LIMIT_BYTES: u64 = 1024 * 1024;
    let contents = retold_engine::fsx::read_bounded(path, MAX_LIMIT_BYTES)
        .map_err(|_| Error::Invalid("Limit file is unavailable or exceeds one MiB."))?
        .ok_or(Error::Invalid("Limit file is missing."))?;
    serde_json::from_slice(&contents)
        .map_err(|_| Error::Invalid("Expected a JSON array of finite integer resource limits."))
}

fn preview_backup(
    directory: &std::path::Path,
    output: &std::path::Path,
) -> Result<serde_json::Value> {
    let account = Account::open(directory)?;
    Ok(json_backup(
        "backup_preview",
        &account.preview_backup(output)?,
    ))
}

fn verify_backup(directory: &std::path::Path, path: &std::path::Path) -> Result<serde_json::Value> {
    let current = Account::open(directory).ok();
    let verified = super::backup::verify(path, current.as_ref())?;
    Ok(json_backup("backup_verified", &verified))
}

fn set_limits(directory: &std::path::Path, path: &std::path::Path) -> Result<serde_json::Value> {
    let account = Account::open(directory)?;
    account.set_limits(read_limits(path)?)?;
    serde_json::to_value(account.summary()?).map_err(|_| Error::Corrupt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_or_oversized_limits_never_reach_account_mutation() {
        const TOO_LARGE: usize = 1024 * 1024 + 1;
        let sandbox = tempfile::tempdir().unwrap();
        let path = sandbox.path().join("limits.json");
        std::fs::write(
            &path,
            r#"[{"scope":{"kind":"account"},"window":"day","resources":{"requests":1.5}}]"#,
        )
        .unwrap();
        assert!(read_limits(&path).is_err());
        std::fs::write(&path, vec![b' '; TOO_LARGE]).unwrap();
        assert!(read_limits(&path).is_err());
        std::fs::write(&path, "[]").unwrap();
        assert_eq!(read_limits(&path).unwrap(), Vec::<Limit>::new());
    }
}
