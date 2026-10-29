//! Native workspace backups: preview and create, verify offline, and preview/restore inactively.
//! Restored workspaces stay inactive; activation remains an explicit, separately reviewed step.
use std::path::{Path, PathBuf};

use crate::protocol::{Diagnostic, ErrorCode, Payload};
use crate::storage::{self, activation, creation, restoration};

/// A reviewed backup or restore: its exact paths and the shared preview it was computed from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reviewed {
    pub source: PathBuf,
    pub output: PathBuf,
    pub preview: serde_json::Value,
}

fn contents(payload: Payload) -> Result<serde_json::Value, Diagnostic> {
    match payload {
        Payload::Result { contents, .. } => Ok(contents),
        Payload::Error { diagnostic, .. } => Err(diagnostic),
        Payload::Event(_) => Err(Diagnostic::new(
            ErrorCode::DataInvalid,
            "Unexpected backup response.",
        )),
    }
}

/// A new, not-yet-existing backup folder inside `parent`.
#[must_use]
pub fn destination(parent: &Path) -> PathBuf {
    let stamp = jiff::Timestamp::now().strftime("%Y%m%d-%H%M%S").to_string();
    parent.join(format!("retold-backup-{stamp}"))
}

fn backup(
    library: &Path,
    output: &Path,
    mode: creation::Mode,
) -> Result<serde_json::Value, Diagnostic> {
    contents(creation::respond(library, output, mode))
}

pub fn review_backup(library: &Path, output: &Path) -> Result<Reviewed, Diagnostic> {
    Ok(Reviewed {
        source: library.into(),
        output: output.into(),
        preview: backup(library, output, creation::Mode::Preview)?,
    })
}

/// Create the reviewed backup only while its preview is unchanged.
pub fn create_backup(reviewed: &Reviewed) -> Result<serde_json::Value, Diagnostic> {
    require_unchanged(
        reviewed,
        &backup(&reviewed.source, &reviewed.output, creation::Mode::Preview)?,
    )?;
    backup(&reviewed.source, &reviewed.output, creation::Mode::Create)
}

/// Verify a backup folder's index and checksums offline.
pub fn verify(archive: &Path) -> Result<storage::Verified, Diagnostic> {
    storage::verify(archive)
}

fn restore(
    archive: &Path,
    output: &Path,
    mode: restoration::Mode,
) -> Result<serde_json::Value, Diagnostic> {
    contents(restoration::respond(archive, output, mode))
}

pub fn review_restore(archive: &Path, output: &Path) -> Result<Reviewed, Diagnostic> {
    Ok(Reviewed {
        source: archive.into(),
        output: output.into(),
        preview: restore(archive, output, restoration::Mode::Preview)?,
    })
}

/// Restore the reviewed backup as an inactive workspace only while its preview is unchanged.
pub fn restore_backup(reviewed: &Reviewed) -> Result<serde_json::Value, Diagnostic> {
    require_unchanged(
        reviewed,
        &restore(
            &reviewed.source,
            &reviewed.output,
            restoration::Mode::Preview,
        )?,
    )?;
    restore(
        &reviewed.source,
        &reviewed.output,
        restoration::Mode::Restore,
    )
}

fn require_unchanged(reviewed: &Reviewed, fresh: &serde_json::Value) -> Result<(), Diagnostic> {
    if *fresh != reviewed.preview {
        return Err(Diagnostic::new(
            ErrorCode::PlanChanged,
            "The workspace or backup changed since review. Review it again.",
        ));
    }
    Ok(())
}

/// A reviewed activation: the restored root and the plan hash the shared activator accepts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Activation {
    pub root: PathBuf,
    pub plan_hash: String,
    pub text: String,
}

fn text_at(value: &serde_json::Value, pointer: &str) -> Option<String> {
    value
        .pointer(pointer)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

/// Read-only activation plan for a restored workspace; nothing transfers yet.
pub fn review_activation(root: &Path) -> Result<Activation, Diagnostic> {
    let value = contents(activation::respond(root, activation::Mode::Preview))?;
    let plan_hash = text_at(&value, "/activation_plan/plan_hash").ok_or_else(|| {
        Diagnostic::new(ErrorCode::DataInvalid, "The activation plan has no hash.")
    })?;
    Ok(Activation {
        root: root.into(),
        plan_hash,
        text: text_at(&value, "/text").unwrap_or_default(),
    })
}

/// Transfer ownership to the restored root under exactly the reviewed plan.
pub fn activate(reviewed: &Activation) -> Result<serde_json::Value, Diagnostic> {
    contents(activation::respond(
        &reviewed.root,
        activation::Mode::Activate(&reviewed.plan_hash),
    ))
}
