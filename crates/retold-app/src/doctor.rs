//! Diagnostic health checks and repairs for the Retold workspace.

pub mod bundle;

use std::fs;
use std::path::{Path, PathBuf};

use retold_engine::retold_guide::Catalog;
use retold_engine::retold_guide::channel::Channel;
use serde::{Deserialize, Serialize};

use crate::discovery;
use crate::protocol::{Diagnostic, ErrorCode, Outcome, Payload};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoctorMode {
    Inspect,
    Repair,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorCheck {
    pub name: String,
    pub status: String,
    pub message: String,
    pub fixable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoDiskUsage {
    pub slug: String,
    pub bytes: u64,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskReport {
    pub total_bytes: u64,
    pub video_count: usize,
    pub videos: Vec<VideoDiskUsage>,
    pub orphaned_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorReport {
    pub healthy: bool,
    pub checks: Vec<DoctorCheck>,
    pub disk: DiskReport,
    pub fixes_applied: Vec<String>,
}

fn informational_check(name: &str, status: &str, message: String) -> DoctorCheck {
    DoctorCheck {
        name: name.to_string(),
        status: status.to_string(),
        message,
        fixable: false,
    }
}

fn fixable_check(name: &str, status: &str, message: String) -> DoctorCheck {
    DoctorCheck {
        name: name.to_string(),
        status: status.to_string(),
        message,
        fixable: true,
    }
}

fn check_editor_installed() -> DoctorCheck {
    let binary = retold_engine::workflow::default_editor();
    let path = Path::new(binary);
    let installed = if path.is_absolute() {
        path.is_file()
    } else {
        find_in_path(binary).is_some()
    };
    let (status, description) = if installed {
        ("ok", "is available")
    } else {
        ("warn", "was not found")
    };
    informational_check(
        "filmcraft_editor",
        status,
        format!("FilmCraft editor {description}: {binary}"),
    )
}

fn find_in_path(binary: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let full = dir.join(binary);
        if full.is_file() {
            return Some(full);
        }
    }
    None
}

fn check_binary(binary: &'static str, description: &'static str) -> DoctorCheck {
    if let Some(path) = find_in_path(binary) {
        informational_check(
            binary,
            "ok",
            format!("{binary} is available at {}", path.display()),
        )
    } else {
        informational_check(
            binary,
            "warn",
            format!("{binary} not found on PATH. {description}"),
        )
    }
}

fn check_directory(dir: &Path, name: &'static str) -> DoctorCheck {
    let exists = dir.exists();
    let (status, message) = if exists {
        ("ok", format!("Directory '{name}' exists."))
    } else {
        ("warn", format!("Directory '{name}' is missing."))
    };
    fixable_check(name, status, message)
}

fn evaluate_catalog(catalog: &Catalog) -> DoctorCheck {
    let broken = catalog.broken().len();
    let total = catalog.entries().len();
    if broken > 0 {
        informational_check(
            "guides_catalog",
            "warn",
            format!("{broken} guide(s) failed to parse cleanly."),
        )
    } else {
        informational_check(
            "guides_catalog",
            "ok",
            format!("{total} guide(s) installed and parsed cleanly."),
        )
    }
}

fn check_guides_catalog(library_root: &Path) -> DoctorCheck {
    let guides_dir = library_root.join("guides");
    if !guides_dir.is_dir() {
        return fixable_check(
            "guides_catalog",
            "warn",
            "Guides directory does not exist.".to_string(),
        );
    }
    match Catalog::load(Some(&guides_dir)) {
        Ok(loaded) => evaluate_catalog(&loaded),
        Err(err) => informational_check(
            "guides_catalog",
            "warn",
            format!("Failed to read guides folder: {err}"),
        ),
    }
}

fn test_entry_channel(entry_path: &Path) -> Option<bool> {
    let is_dir = entry_path.is_dir();
    let manifest_exists = entry_path.join("channel.toml").exists();
    if !is_dir || !manifest_exists {
        return None;
    }
    Some(Channel::load(entry_path).is_ok())
}

fn count_channel_issues(channels_dir: &Path) -> (usize, usize) {
    let mut valid = 0;
    let mut broken = 0;
    if let Ok(entries) = fs::read_dir(channels_dir) {
        for entry in entries.flatten() {
            match test_entry_channel(&entry.path()) {
                Some(true) => valid += 1,
                Some(false) => broken += 1,
                None => {}
            }
        }
    }
    (valid, broken)
}

fn evaluate_channel_counts(valid: usize, broken: usize) -> DoctorCheck {
    if broken > 0 {
        informational_check(
            "channels_catalog",
            "warn",
            format!("{broken} channel(s) failed to parse cleanly."),
        )
    } else {
        informational_check(
            "channels_catalog",
            "ok",
            format!("{valid} channel(s) installed and parsed cleanly."),
        )
    }
}

fn check_channels_catalog(library_root: &Path) -> DoctorCheck {
    let channels_dir = library_root.join("channels");
    if !channels_dir.is_dir() {
        return fixable_check(
            "channels_catalog",
            "warn",
            "Channels directory does not exist.".to_string(),
        );
    }
    let (valid, broken) = count_channel_issues(&channels_dir);
    evaluate_channel_counts(valid, broken)
}

fn compute_file_size(path: &Path) -> u64 {
    fs::metadata(path).map_or(0, |meta| meta.len())
}

fn measure_directory_bytes(dir: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    let mut total: u64 = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        let bytes = if path.is_dir() {
            measure_directory_bytes(&path)
        } else {
            compute_file_size(&path)
        };
        total = total.saturating_add(bytes);
    }
    total
}

fn is_temporary_artifact_name(name: &str) -> bool {
    let has_tmp_extension = Path::new(name)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("tmp"));
    has_tmp_extension || name.starts_with(".tmp-")
}

fn collect_entry_orphans(entry_path: &Path) -> Vec<String> {
    let file_name = entry_path.file_name();
    let name_str = file_name.map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    if is_temporary_artifact_name(&name_str) {
        vec![entry_path.display().to_string()]
    } else if entry_path.is_dir() {
        discover_orphaned_temporary_entries(entry_path)
    } else {
        Vec::new()
    }
}

fn discover_orphaned_temporary_entries(dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut orphans = Vec::new();
    for entry in entries.flatten() {
        let found = collect_entry_orphans(&entry.path());
        orphans.extend(found);
    }
    orphans
}

fn fallback_catalog(root: &Path) -> crate::library::Catalog {
    crate::library::Catalog {
        root: root.to_path_buf(),
        videos: Vec::new(),
        issues: Vec::new(),
    }
}

fn measure_video_disk_usage(video: &crate::library::Video) -> VideoDiskUsage {
    let video_dir = video.layout.video_dir();
    let bytes = measure_directory_bytes(video_dir);
    VideoDiskUsage {
        slug: video.layout.slug().to_string(),
        bytes,
        path: video_dir.display().to_string(),
    }
}

fn collect_video_disk_usage(library_root: &Path) -> (Vec<VideoDiskUsage>, u64) {
    let catalog = discovery::scan(library_root).unwrap_or_else(|_| fallback_catalog(library_root));
    let mut usages = Vec::new();
    let mut total_bytes = 0u64;
    for video in catalog.videos {
        let usage = measure_video_disk_usage(&video);
        total_bytes = total_bytes.saturating_add(usage.bytes);
        usages.push(usage);
    }
    (usages, total_bytes)
}

fn build_disk_report(library_root: &Path) -> DiskReport {
    let (videos, total_bytes) = collect_video_disk_usage(library_root);
    let orphaned_files = discover_orphaned_temporary_entries(library_root);
    let video_count = videos.len();
    DiskReport {
        total_bytes,
        video_count,
        videos,
        orphaned_files,
    }
}

fn repair_missing_dir(dir: &Path, label: &str) -> Option<String> {
    if !dir.exists() && fs::create_dir_all(dir).is_ok() {
        Some(format!("Created missing directory: {label}"))
    } else {
        None
    }
}

fn remove_orphaned_file(path_str: &str) -> Option<String> {
    let path = Path::new(path_str);
    if !path.exists() {
        return None;
    }
    let removed = if path.is_dir() {
        fs::remove_dir_all(path).is_ok()
    } else {
        fs::remove_file(path).is_ok()
    };
    if removed {
        Some(format!("Removed orphaned file: {path_str}"))
    } else {
        None
    }
}

fn repair_standard_directories(context: &crate::workspace::context::Context) -> Vec<String> {
    let mut fixes = Vec::new();
    for (label, folder) in context.directories() {
        if let Some(fix) = repair_missing_dir(&folder, label) {
            fixes.push(fix);
        }
    }
    fixes
}

fn purge_orphaned_files(orphans: &[String]) -> Vec<String> {
    let mut fixes = Vec::new();
    for orphan in orphans {
        if let Some(fix) = remove_orphaned_file(orphan) {
            fixes.push(fix);
        }
    }
    fixes
}

fn apply_repairs(context: &crate::workspace::context::Context, orphans: &[String]) -> Vec<String> {
    let mut fixes = repair_standard_directories(context);
    fixes.extend(purge_orphaned_files(orphans));
    fixes
}

fn inspect_workspace_components(context: &crate::workspace::context::Context) -> Vec<DoctorCheck> {
    let mut checks = vec![
        check_editor_installed(),
        check_binary("ffmpeg", "Video rendering and audio export require ffmpeg."),
        check_binary("ffprobe", "Media stream analysis requires ffprobe."),
        check_guides_catalog(&context.config),
        check_channels_catalog(&context.config),
    ];
    checks.extend(
        context
            .directories()
            .into_iter()
            .map(|(label, path)| check_directory(&path, label)),
    );
    checks
}

fn format_check_line(check: &DoctorCheck) -> String {
    let mark = if check.status == "ok" { "[ok]" } else { "[!!]" };
    format!("  {mark} {}: {}", check.name, check.message)
}

fn format_all_checks(checks: &[DoctorCheck]) -> Vec<String> {
    checks.iter().map(format_check_line).collect()
}

fn format_all_fixes(fixes: &[String]) -> Vec<String> {
    if fixes.is_empty() {
        return Vec::new();
    }
    let mut entries = vec!["  Fixes applied:".to_string()];
    for fix in fixes {
        entries.push(format!("    - {fix}"));
    }
    entries
}

fn doctor_headline(checks: &[DoctorCheck]) -> String {
    let issue_count = checks.iter().filter(|check| check.status != "ok").count();
    if issue_count == 0 {
        "Doctor inspection completed: workspace is healthy.".to_string()
    } else {
        format!("Doctor inspection completed with {issue_count} issue(s).")
    }
}

fn build_doctor_summary_text(checks: &[DoctorCheck], fixes: &[String]) -> String {
    let mut lines = vec![doctor_headline(checks)];
    lines.extend(format_all_checks(checks));
    lines.extend(format_all_fixes(fixes));
    lines.join("\n")
}

fn resolve_applied_fixes(
    context: &crate::workspace::context::Context,
    disk: &DiskReport,
    mode: DoctorMode,
) -> Vec<String> {
    if mode == DoctorMode::Repair {
        apply_repairs(context, &disk.orphaned_files)
    } else {
        Vec::new()
    }
}

pub fn generate_doctor_report(library_root: &Path) -> Result<DoctorReport, Diagnostic> {
    let context = crate::workspace::context::Context::resolve(Some(library_root))?;
    doctor_report(&context)
}

fn doctor_report(context: &crate::workspace::context::Context) -> Result<DoctorReport, Diagnostic> {
    let library_exists = context.library.exists();
    if !library_exists {
        return Err(Diagnostic::new(
            ErrorCode::InputRequired,
            "Library path does not exist.",
        ));
    }
    let checks = inspect_workspace_components(context);
    let disk = build_disk_report(&context.root);
    let healthy = checks.iter().all(|check| check.status == "ok");
    Ok(DoctorReport {
        healthy,
        checks,
        disk,
        fixes_applied: Vec::new(),
    })
}

pub fn run_doctor(
    context: &crate::workspace::context::Context,
    mode: DoctorMode,
) -> Result<Payload, Diagnostic> {
    let mut report = doctor_report(context)?;
    let fixes_applied = resolve_applied_fixes(context, &report.disk, mode);
    report.fixes_applied = fixes_applied;
    let text = build_doctor_summary_text(&report.checks, &report.fixes_applied);
    Ok(Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({ "text": text, "report": report }),
    })
}
