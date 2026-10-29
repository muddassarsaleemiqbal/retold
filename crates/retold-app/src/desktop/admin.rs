//! Typed native administration: workspace health, reviewed repair, presets and profiles.
use std::path::Path;

use crate::doctor::{self, DoctorMode, DoctorReport};
use crate::presets::{self, PresetFile, PresetRecord, RemovalConsent};
use crate::profile::{self, ProfileRecord};
use crate::protocol::{Diagnostic, ErrorCode, Payload};
use crate::workspace::context::Context;

fn contents(payload: Payload) -> Result<serde_json::Value, Diagnostic> {
    match payload {
        Payload::Result { contents, .. } => Ok(contents),
        _ => Err(invalid("unexpected response")),
    }
}

fn field<T: serde::de::DeserializeOwned>(
    value: &serde_json::Value,
    name: &str,
) -> Result<T, Diagnostic> {
    serde_json::from_value(value.get(name).cloned().unwrap_or_default()).map_err(invalid)
}

/// Read-only workspace health and disk usage.
pub fn health(library: &Path) -> Result<DoctorReport, Diagnostic> {
    doctor::generate_doctor_report(library)
}

/// Apply only the fixable repairs that the reviewed report listed.
pub fn repair(library: &Path, reviewed: &DoctorReport) -> Result<DoctorReport, Diagnostic> {
    let current = health(library)?;
    let same = fixable(&current) == fixable(reviewed);
    if !same {
        return Err(Diagnostic::new(
            ErrorCode::PlanChanged,
            "Workspace health changed. Review the repairs again.",
        ));
    }
    let context = Context::resolve(Some(library))?;
    field(
        &contents(doctor::run_doctor(&context, DoctorMode::Repair)?)?,
        "report",
    )
}

fn fixable(report: &DoctorReport) -> Vec<(&str, &str)> {
    report
        .checks
        .iter()
        .filter(|check| check.fixable)
        .map(|check| (check.name.as_str(), check.message.as_str()))
        .collect()
}

fn report_lines(report: &DoctorReport) -> String {
    let state = if report.healthy {
        "healthy"
    } else {
        "needs attention"
    };
    let mut lines = vec![format!("Retold workspace health: {state}")];
    lines.extend(
        report
            .checks
            .iter()
            .map(|check| format!("- {} [{}] {}", check.name, check.status, check.message)),
    );
    let disk = &report.disk;
    lines.push(format!(
        "Videos: {} · {} bytes · {} orphaned files",
        disk.video_count,
        disk.total_bytes,
        disk.orphaned_files.len()
    ));
    lines.join("\n")
}

fn home() -> Option<String> {
    let home = std::env::var_os("HOME")?.to_string_lossy().into_owned();
    let meaningful = home.len() > 1;
    meaningful.then_some(home)
}

/// A shareable report with workspace and home paths replaced by placeholders.
#[must_use]
pub fn redacted(report: &DoctorReport, library: &Path) -> String {
    let library = library.to_string_lossy();
    let mut text = report_lines(report);
    if !library.is_empty() {
        text = text.replace(library.as_ref(), "<workspace>");
    }
    match home() {
        Some(home) => text.replace(&home, "~"),
        None => text,
    }
}

pub fn presets(library: &Path) -> Result<Vec<PresetRecord>, Diagnostic> {
    field(&contents(presets::list_presets(library)?)?, "items")
}

/// Remove one export preset after explicit confirmation in the client.
pub fn remove_preset(library: &Path, name: &str) -> Result<Vec<PresetRecord>, Diagnostic> {
    presets::remove_preset(library, name, RemovalConsent::Confirmed)?;
    presets(library)
}

/// Configuration profiles and the active one.
/// Validate and save a new export preset, then list presets.
pub fn create_preset(
    library: &Path,
    name: &str,
    file: &PresetFile,
) -> Result<Vec<PresetRecord>, Diagnostic> {
    presets::create_preset_value(library, name, file)?;
    presets(library)
}

/// Create an empty named configuration profile; it is not selected.
pub fn create_profile(
    config: &Path,
    name: &str,
) -> Result<(Vec<ProfileRecord>, Option<String>), Diagnostic> {
    profile::create_profile(Some(config), name, Option::default())?;
    profiles(config)
}

pub fn profiles(config: &Path) -> Result<(Vec<ProfileRecord>, Option<String>), Diagnostic> {
    let value = contents(profile::list_profiles(Some(config))?)?;
    Ok((field(&value, "items")?, field(&value, "active_profile")?))
}

pub fn use_profile(
    config: &Path,
    name: &str,
) -> Result<(Vec<ProfileRecord>, Option<String>), Diagnostic> {
    profile::use_profile(Some(config), name)?;
    profiles(config)
}

fn invalid(error: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::new(ErrorCode::DataInvalid, &format!("Administration: {error}"))
}
