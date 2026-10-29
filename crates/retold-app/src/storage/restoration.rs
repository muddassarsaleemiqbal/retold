//! Offline restore publishes an inactive workspace; financial evidence never becomes a ledger.
mod tasks;

use super::{Verified, creation, files, invalid, publication};
use crate::protocol::{Diagnostic, ErrorCode, Outcome, Payload};
use retold_engine::recovery::INACTIVE_MARKER;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const RECOVERY: &str = ".retold-recovery";

#[derive(Debug, Clone, Copy)]
pub enum Mode {
    Preview,
    Restore,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Inactive {
    pub(super) schema_version: u32,
    pub(super) workspace_id: String,
    pub(super) account_id: String,
    pub(super) recovery_index_blake3: String,
    pub(super) execution_authorized: bool,
    pub(super) queues_stopped: bool,
    pub(super) accounting_installed: bool,
}

#[must_use]
pub fn respond(archive: &Path, output: &Path, mode: Mode) -> Payload {
    match apply(archive, output, mode) {
        Ok(contents) => Payload::Result {
            outcome: Outcome::Completed,
            contents,
        },
        Err(error) => error.into_payload(),
    }
}

pub fn apply(archive: &Path, output: &Path, mode: Mode) -> Result<serde_json::Value, Diagnostic> {
    super::regular_directory(archive)?;
    let archive = archive.canonicalize().map_err(|_| invalid())?;
    let verified = managed(&archive)?;
    let output = publication::destination(output, &archive)?;
    match mode {
        Mode::Preview => Ok(preview(&output, &verified)),
        Mode::Restore => restore(&archive, &output, &verified),
    }
}

fn managed(archive: &Path) -> Result<Verified, Diagnostic> {
    let verified = super::verify(archive)?;
    if !verified.managed_workspace_verified {
        return Err(Diagnostic::new(
            ErrorCode::DataInvalid,
            "Restore requires a version 2 managed workspace backup; byte-only indexes cannot be restored.",
        ));
    }
    Ok(verified)
}

fn preview(output: &Path, verified: &Verified) -> serde_json::Value {
    serde_json::json!({
        "text": "Read-only restore preview. Includes private scripts/media and account cost metadata. The restored workspace will be inactive; no files written.",
        "restore_preview": { "output": output, "workspace_id": verified.index.workspace_id,
            "account_id": verified.index.account_id, "total_bytes": verified.total_bytes,
            "execution_authorized": false, "queues_stopped": true, "accounting_installed": false }
    })
}

fn restore(
    archive: &Path,
    output: &Path,
    verified: &Verified,
) -> Result<serde_json::Value, Diagnostic> {
    let staging = publication::staging(output)?;
    let evidence = staging.path().join(RECOVERY);
    copy_snapshot(archive, &evidence, verified)?;
    let copied = managed(&evidence)?;
    let inactive = inactive(&evidence, &copied)?;
    creation::write_json(&staging.path().join(INACTIVE_MARKER), &inactive)?;
    unpack_workspace(&evidence, staging.path())?;
    let catalog = crate::discovery::scan(staging.path())?;
    tasks::stop(&catalog, &evidence)?;
    mark_runs(&catalog, &inactive)?;
    creation::validate_runs(&catalog)?;
    publication::publish(staging.path(), output)?;
    Ok(serde_json::json!({
        "text": "Workspace restored inactive. Inspect it offline; execution requires future ownership and accounting reconciliation. Canonical accounting was not changed.",
        "workspace_restored": { "output": output, "inactive": inactive, "videos": catalog.videos.len() }
    }))
}

fn copy_snapshot(archive: &Path, evidence: &Path, verified: &Verified) -> Result<(), Diagnostic> {
    std::fs::create_dir(evidence).map_err(|_| invalid())?;
    std::fs::create_dir(evidence.join("workspace")).map_err(|_| invalid())?;
    for entry in &verified.index.files {
        files::copy_entry(archive, evidence, entry)?;
    }
    creation::write_json(&evidence.join("index.json"), &verified.index)
}

fn inactive(evidence: &Path, verified: &Verified) -> Result<Inactive, Diagnostic> {
    let path = evidence.join("index.json");
    let bytes = super::regular_file(&path)?;
    Ok(Inactive {
        schema_version: 1,
        workspace_id: verified.index.workspace_id.clone(),
        account_id: verified.index.account_id.clone(),
        recovery_index_blake3: super::digest(&path, bytes)?,
        execution_authorized: false,
        queues_stopped: true,
        accounting_installed: false,
    })
}

fn unpack_workspace(evidence: &Path, target: &Path) -> Result<(), Diagnostic> {
    let workspace = evidence.join("workspace");
    for entry in std::fs::read_dir(&workspace).map_err(|_| invalid())? {
        let entry = entry.map_err(|_| invalid())?;
        let output: PathBuf = target.join(entry.file_name());
        if std::fs::symlink_metadata(&output).is_ok() {
            return Err(invalid());
        }
        std::fs::rename(entry.path(), output).map_err(|_| invalid())?;
    }
    std::fs::remove_dir(&workspace).map_err(|_| invalid())
}

fn mark_runs(catalog: &crate::library::Catalog, inactive: &Inactive) -> Result<(), Diagnostic> {
    for video in &catalog.videos {
        creation::write_json(&video.directory.join(INACTIVE_MARKER), inactive)?;
    }
    Ok(())
}
