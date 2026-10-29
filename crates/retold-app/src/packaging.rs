//! Delivery package verification, bundling, paths inspection and intermediate file cleanup.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::library::{Catalog, Video, response};
use crate::protocol::{Diagnostic, ErrorCode, Payload};

const BUFFER_BYTES: usize = 8 * 1024;
const DEFAULT_VERIFY_SCOPE: &str = "all";
const DEFAULT_PATHS_KIND: &str = "all";
const DEFAULT_CLEAN_SCOPE: &str = "temporary";
const REVIEW_LABEL_UNAPPROVED: &str = "Review copy — not final delivery";
const REVIEW_LABEL_FINAL: &str = "Final delivery";

#[derive(Debug, Clone)]
pub struct PackageOptions<'a> {
    pub edition: &'a str,
    pub aspect: &'a str,
    pub release: &'a str,
    pub output: &'a Path,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Default)]
pub struct VerifyOptions<'a> {
    pub scope: Option<&'a str>,
}

#[derive(Debug, Clone, Default)]
pub struct PathsOptions<'a> {
    pub kind: Option<&'a str>,
}

#[derive(Debug, Clone, Default)]
pub struct CleanOptions<'a> {
    pub scope: Option<&'a str>,
    pub dry_run: bool,
    pub yes: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanMode {
    Preview,
    Execute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewStatus {
    Final,
    ReviewCopy,
}

impl ReviewStatus {
    const fn label(self) -> &'static str {
        match self {
            Self::Final => REVIEW_LABEL_FINAL,
            Self::ReviewCopy => REVIEW_LABEL_UNAPPROVED,
        }
    }

    const fn is_review_copy(self) -> bool {
        matches!(self, Self::ReviewCopy)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GateStatus {
    Open,
    Locked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReleaseMatchResult {
    Matched,
    NotFound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileClassification {
    ChecksumManifest,
    Other,
}

#[derive(Debug, serde::Serialize)]
struct PackageFileEntry {
    path: String,
    bytes: u64,
    sha256: String,
    blake3: String,
}

struct DirStats {
    files: usize,
    bytes: u64,
}

struct CleanableFiles {
    paths: Vec<PathBuf>,
    bytes: u64,
}

struct CleanReportSpec<'a> {
    reference: &'a str,
    scope: &'a str,
    mode: CleanMode,
}

struct ReleaseSearchTarget<'a> {
    edition: &'a str,
    aspect: &'a str,
    release: &'a str,
}

struct PackageManifestSpec<'a> {
    output: &'a Path,
    video: &'a Video,
    options: &'a PackageOptions<'a>,
    aspect: &'a str,
    status: ReviewStatus,
}

struct PackagePreparation<'a> {
    source_dir: PathBuf,
    candidate_files: Vec<(PathBuf, u64)>,
    spec: PackageManifestSpec<'a>,
}

struct VerificationPlan {
    check_work: bool,
    check_delivery: bool,
}

fn path_to_string(path: &Path) -> String {
    path.display().to_string()
}

fn path_entry(key: &'static str, path: &Path) -> (&'static str, String) {
    (key, path_to_string(path))
}

/// Resolves filesystem paths for the video directories.
pub(crate) fn video_paths(
    catalog: &Catalog,
    reference: &str,
    options: &PathsOptions<'_>,
) -> Result<Payload, Diagnostic> {
    let video = catalog.resolve(reference)?;
    let kind = options.kind.unwrap_or(DEFAULT_PATHS_KIND);
    let paths_map = build_paths_map(video, kind)?;
    let deliveries = retold_engine::session::prepared_deliveries(&video.layout).map_err(|_| {
        Diagnostic::new(
            ErrorCode::DataInvalid,
            "Saved delivery evidence is invalid.",
        )
    })?;
    Ok(response(serde_json::json!({
        "reference": video.reference,
        "kind": kind,
        "paths": paths_map,
        "deliveries": deliveries,
    })))
}

fn build_paths_map(
    video: &Video,
    kind: &str,
) -> Result<BTreeMap<&'static str, String>, Diagnostic> {
    let is_all = kind == "all";
    if is_all {
        return Ok(build_all_paths(video));
    }
    build_single_path(video, kind)
}

fn build_all_paths(video: &Video) -> BTreeMap<&'static str, String> {
    let entries = [
        path_entry("root", &video.directory),
        path_entry("work", &video.layout.work()),
        path_entry("delivery", &video.layout.deliverables()),
        path_entry("state", &video.layout.meta()),
        path_entry("review", &video.layout.review_page()),
    ];
    BTreeMap::from(entries)
}

fn build_single_path(
    video: &Video,
    kind: &str,
) -> Result<BTreeMap<&'static str, String>, Diagnostic> {
    let entry = match kind {
        "work" => path_entry("work", &video.layout.work()),
        "delivery" => path_entry("delivery", &video.layout.deliverables()),
        "state" => path_entry("state", &video.layout.meta()),
        _ => {
            return Err(Diagnostic::new(
                ErrorCode::InputRequired,
                "Paths kind must be 'all', 'work', 'delivery', or 'state'.",
            ));
        }
    };
    Ok(BTreeMap::from([entry]))
}

fn validate_verify_scope(scope: &str) -> Result<VerificationPlan, Diagnostic> {
    let check_work = matches!(scope, "work" | "all");
    let check_delivery = matches!(scope, "delivery" | "all");
    let scope_valid = check_work || check_delivery;
    if !scope_valid {
        return Err(Diagnostic::new(
            ErrorCode::InputRequired,
            "Verification scope must be 'work', 'delivery', or 'all'.",
        ));
    }
    Ok(VerificationPlan {
        check_work,
        check_delivery,
    })
}

fn run_scope_verification(
    video: &Video,
    plan: &VerificationPlan,
) -> Result<(DirStats, DirStats), Diagnostic> {
    let work_stats = if plan.check_work {
        verify_work_directory(video)?
    } else {
        DirStats { files: 0, bytes: 0 }
    };
    let delivery_stats = if plan.check_delivery {
        verify_delivery_directory(video)?
    } else {
        DirStats { files: 0, bytes: 0 }
    };
    Ok((work_stats, delivery_stats))
}

/// Offline read-only verification of work artifacts and delivery checksums.
pub(crate) fn video_verify(
    catalog: &Catalog,
    reference: &str,
    options: &VerifyOptions<'_>,
) -> Result<Payload, Diagnostic> {
    let video = catalog.resolve(reference)?;
    let scope = options.scope.unwrap_or(DEFAULT_VERIFY_SCOPE);
    let plan = validate_verify_scope(scope)?;
    let (work_stats, delivery_stats) = run_scope_verification(video, &plan)?;
    let total_files = work_stats.files.saturating_add(delivery_stats.files);
    let total_bytes = work_stats.bytes.saturating_add(delivery_stats.bytes);

    Ok(response(serde_json::json!({
        "status": "verified",
        "reference": video.reference,
        "scope": scope,
        "verified": true,
        "work_verified": plan.check_work,
        "delivery_verified": plan.check_delivery,
        "files_checked": total_files,
        "total_bytes": total_bytes,
        "issues": []
    })))
}

fn verify_work_directory(video: &Video) -> Result<DirStats, Diagnostic> {
    let meta_path = video.layout.meta();
    let run_json = meta_path.join("run.json");
    let has_run_manifest = run_json.exists();
    if !has_run_manifest {
        return Err(Diagnostic::new(
            ErrorCode::DataInvalid,
            "Run manifest run.json is missing from video state.",
        ));
    }
    let work_path = video.layout.work();
    let has_work_dir = work_path.exists();
    if !has_work_dir {
        return Ok(DirStats { files: 1, bytes: 0 });
    }
    tally_directory_bytes(&work_path)
}

fn verify_delivery_directory(video: &Video) -> Result<DirStats, Diagnostic> {
    let delivery_path = video.layout.deliverables();
    let has_deliverables = delivery_path.exists();
    if !has_deliverables {
        return Ok(DirStats { files: 0, bytes: 0 });
    }
    let checksum_files = collect_checksum_paths(&delivery_path)?;
    for checksum_path in &checksum_files {
        verify_checksum_manifest(checksum_path)?;
    }
    tally_directory_bytes(&delivery_path)
}

fn classify_file(path: &Path) -> FileClassification {
    let file_name = path.file_name();
    match file_name {
        Some(name) if name == "CHECKSUMS.sha256" => FileClassification::ChecksumManifest,
        _ => FileClassification::Other,
    }
}

fn collect_checksum_paths(dir: &Path) -> Result<Vec<PathBuf>, Diagnostic> {
    let mut acc = Vec::new();
    let entries = fs::read_dir(dir).map_err(io_invalid_read)?;
    for entry in entries {
        let entry = entry.map_err(io_invalid_read)?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(io_invalid_read)?;
        let is_dir = file_type.is_dir();
        if is_dir {
            acc.extend(collect_checksum_paths(&path)?);
        } else if classify_file(&path) == FileClassification::ChecksumManifest {
            acc.push(path);
        }
    }
    Ok(acc)
}

fn verify_checksum_manifest(path: &Path) -> Result<(), Diagnostic> {
    let parent = path
        .parent()
        .ok_or_else(|| Diagnostic::new(ErrorCode::DataInvalid, "Checksum file has no parent."))?;
    let file = File::open(path).map_err(io_invalid_read)?;
    let reader = BufReader::new(file);
    for line in reader.lines() {
        let line = line.map_err(io_invalid_read)?;
        let trimmed = line.trim();
        let is_empty = trimmed.is_empty();
        if is_empty {
            continue;
        }
        verify_checksum_line(parent, trimmed)?;
    }
    Ok(())
}

fn verify_checksum_line(base: &Path, line: &str) -> Result<(), Diagnostic> {
    let mut parts = line.split_whitespace();
    let expected_hash = parts.next().unwrap_or_default();
    let relative_file = parts.next().unwrap_or_default();
    let file_path = base.join(relative_file);
    let target_exists = file_path.exists();
    if !target_exists {
        return Err(Diagnostic::new(
            ErrorCode::DataInvalid,
            &format!("Delivery file '{relative_file}' listed in CHECKSUMS.sha256 is missing."),
        ));
    }
    let computed_hash = compute_sha256(&file_path)?;
    let hashes_match = computed_hash.eq_ignore_ascii_case(expected_hash);
    if !hashes_match {
        return Err(Diagnostic::new(
            ErrorCode::DataInvalid,
            &format!(
                "Delivery checksum mismatch for {relative_file}: expected {expected_hash}, computed {computed_hash}."
            ),
        ));
    }
    Ok(())
}

fn tally_directory_bytes(dir: &Path) -> Result<DirStats, Diagnostic> {
    let mut stats = DirStats { files: 0, bytes: 0 };
    let entries = fs::read_dir(dir).map_err(io_invalid_read)?;
    for entry in entries {
        let entry = entry.map_err(io_invalid_read)?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(io_invalid_read)?;
        let is_dir = file_type.is_dir();
        if is_dir {
            let sub = tally_directory_bytes(&path)?;
            stats.files = stats.files.saturating_add(sub.files);
            stats.bytes = stats.bytes.saturating_add(sub.bytes);
        } else {
            let metadata = fs::metadata(&path).map_err(io_invalid_read)?;
            stats.files = stats.files.saturating_add(1);
            stats.bytes = stats.bytes.saturating_add(metadata.len());
        }
    }
    Ok(stats)
}

fn validate_clean_scope(scope: &str) -> Result<(), Diagnostic> {
    let scope_valid = matches!(scope, "temporary" | "rebuildable");
    if !scope_valid {
        return Err(Diagnostic::new(
            ErrorCode::InputRequired,
            "Clean scope must be 'temporary' or 'rebuildable'.",
        ));
    }
    Ok(())
}

fn validate_clean_confirmation(options: &CleanOptions<'_>) -> Result<(), Diagnostic> {
    let confirmed = options.yes || options.dry_run;
    if !confirmed {
        return Err(Diagnostic::new(
            ErrorCode::InputRequired,
            "clean removes intermediate files; pass --yes to confirm or --dry-run to preview.",
        ));
    }
    Ok(())
}

fn build_clean_report(spec: &CleanReportSpec<'_>, stats: (usize, u64)) -> serde_json::Value {
    let (removed_files, freed_bytes) = stats;
    let is_preview = spec.mode == CleanMode::Preview;
    serde_json::json!({
        "status": if is_preview { "preview" } else { "cleaned" },
        "reference": spec.reference,
        "scope": spec.scope,
        "dry_run": is_preview,
        "removed_files": removed_files,
        "freed_bytes": freed_bytes,
    })
}

/// Removes intermediate working files while keeping deliverables and accounting records.
pub(crate) fn video_clean(
    catalog: &Catalog,
    reference: &str,
    options: &CleanOptions<'_>,
) -> Result<Payload, Diagnostic> {
    let video = catalog.resolve(reference)?;
    let scope = options.scope.unwrap_or(DEFAULT_CLEAN_SCOPE);
    validate_clean_scope(scope)?;
    validate_clean_confirmation(options)?;

    let work_dir = video.layout.work();
    let mode = if options.dry_run {
        CleanMode::Preview
    } else {
        CleanMode::Execute
    };
    let report_spec = CleanReportSpec {
        reference: &video.reference,
        scope,
        mode,
    };
    let has_work = work_dir.exists();
    if !has_work {
        return Ok(response(build_clean_report(&report_spec, (0, 0))));
    }
    let stats = clean_work_directory(&work_dir, scope, mode)?;
    Ok(response(build_clean_report(&report_spec, stats)))
}

fn clean_work_directory(
    work_dir: &Path,
    scope: &str,
    mode: CleanMode,
) -> Result<(usize, u64), Diagnostic> {
    let cleanable = collect_cleanable_files(work_dir, scope)?;
    let count = cleanable.paths.len();
    let is_execute = mode == CleanMode::Execute;
    if is_execute {
        for file in cleanable.paths {
            let _ = fs::remove_file(file);
        }
    }
    Ok((count, cleanable.bytes))
}

fn collect_cleanable_files(dir: &Path, scope: &str) -> Result<CleanableFiles, Diagnostic> {
    let mut cleanable = CleanableFiles {
        paths: Vec::new(),
        bytes: 0,
    };
    let entries = fs::read_dir(dir).map_err(io_invalid_read)?;
    for entry in entries {
        let entry = entry.map_err(io_invalid_read)?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(io_invalid_read)?;
        let is_dir = file_type.is_dir();
        if is_dir {
            let sub = collect_cleanable_files(&path, scope)?;
            cleanable.paths.extend(sub.paths);
            cleanable.bytes = cleanable.bytes.saturating_add(sub.bytes);
        } else if should_clean_file(&path, scope) {
            let metadata = fs::metadata(&path).map_err(io_invalid_read)?;
            cleanable.paths.push(path);
            cleanable.bytes = cleanable.bytes.saturating_add(metadata.len());
        }
    }
    Ok(cleanable)
}

fn should_clean_file(path: &Path, scope: &str) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let is_temp = matches!(ext, "tmp" | "scratch" | "partial" | "lock");
    if is_temp {
        return true;
    }
    scope == "rebuildable" && matches!(ext, "wav" | "raw" | "ppm" | "cache")
}

fn preview_package_payload(
    video: &Video,
    spec: &PackageManifestSpec<'_>,
    candidate_files: &[(PathBuf, u64)],
) -> Payload {
    let total_bytes: u64 = candidate_files.iter().map(|item| item.1).sum();
    response(serde_json::json!({
        "status": "preview",
        "reference": video.reference,
        "edition": spec.options.edition,
        "aspect": spec.aspect,
        "release_id": spec.options.release,
        "output": path_to_string(spec.output),
        "label": spec.status.label(),
        "is_review_copy": spec.status.is_review_copy(),
        "file_count": candidate_files.len(),
        "total_bytes": total_bytes,
    }))
}

fn complete_package_payload(
    video: &Video,
    spec: &PackageManifestSpec<'_>,
    manifest_entries: &[PackageFileEntry],
) -> Payload {
    let total_bytes: u64 = manifest_entries.iter().map(|entry| entry.bytes).sum();
    response(serde_json::json!({
        "status": "packaged",
        "reference": video.reference,
        "edition": spec.options.edition,
        "aspect": spec.aspect,
        "release_id": spec.options.release,
        "output": path_to_string(spec.output),
        "label": spec.status.label(),
        "is_review_copy": spec.status.is_review_copy(),
        "file_count": manifest_entries.len(),
        "total_bytes": total_bytes,
    }))
}

fn ensure_candidate_files(
    files: Vec<(PathBuf, u64)>,
    release: &str,
) -> Result<Vec<(PathBuf, u64)>, Diagnostic> {
    if files.is_empty() {
        return Err(Diagnostic::new(
            ErrorCode::DataInvalid,
            &format!("Release '{release}' is empty or corrupt. Re-export before packaging."),
        ));
    }
    Ok(files)
}

fn prepare_package_spec<'a>(
    video: &'a Video,
    options: &'a PackageOptions<'a>,
    aspect: &'a str,
) -> Result<PackagePreparation<'a>, Diagnostic> {
    let search_target = ReleaseSearchTarget {
        edition: options.edition,
        aspect,
        release: options.release,
    };
    let source_dir = resolve_release_directory(video, &search_target)?;
    let raw_files = collect_release_files(&source_dir)?;
    let candidate_files = ensure_candidate_files(raw_files, options.release)?;
    let review_status = determine_review_status(&video.directory, options.edition)?;
    let spec = PackageManifestSpec {
        output: options.output,
        video,
        options,
        aspect,
        status: review_status,
    };
    Ok(PackagePreparation {
        source_dir,
        candidate_files,
        spec,
    })
}

/// Bundles an exported release into a verified delivery package.
pub(crate) fn video_package(
    catalog: &Catalog,
    reference: &str,
    options: &PackageOptions<'_>,
) -> Result<Payload, Diagnostic> {
    let video = catalog.resolve(reference)?;
    validate_package_preconditions(options)?;
    let normalized_aspect = options.aspect.replace(':', "x");
    let prep = prepare_package_spec(video, options, &normalized_aspect)?;

    if options.dry_run {
        return Ok(preview_package_payload(
            video,
            &prep.spec,
            &prep.candidate_files,
        ));
    }

    let manifest_entries =
        package_release_files(&prep.source_dir, &prep.candidate_files, options.output)?;
    write_package_manifest(&prep.spec, &manifest_entries)?;
    Ok(complete_package_payload(
        video,
        &prep.spec,
        &manifest_entries,
    ))
}

fn validate_package_strings(options: &PackageOptions<'_>) -> Result<(), Diagnostic> {
    let raw_output = options.output.as_os_str();
    let is_output_empty = raw_output.is_empty();
    if is_output_empty {
        return Err(Diagnostic::new(
            ErrorCode::InputRequired,
            "Output path cannot be empty.",
        ));
    }
    let is_edition_empty = options.edition.is_empty();
    if is_edition_empty {
        return Err(Diagnostic::new(
            ErrorCode::InputRequired,
            "Edition cannot be empty.",
        ));
    }
    Ok(())
}

fn validate_package_identifiers(options: &PackageOptions<'_>) -> Result<(), Diagnostic> {
    let is_aspect_empty = options.aspect.is_empty();
    if is_aspect_empty {
        return Err(Diagnostic::new(
            ErrorCode::InputRequired,
            "Aspect cannot be empty.",
        ));
    }
    let is_release_empty = options.release.is_empty();
    if is_release_empty {
        return Err(Diagnostic::new(
            ErrorCode::InputRequired,
            "Release ID cannot be empty.",
        ));
    }
    Ok(())
}

fn validate_package_preconditions(options: &PackageOptions<'_>) -> Result<(), Diagnostic> {
    validate_package_strings(options)?;
    validate_package_identifiers(options)
}

fn release_subpath_candidates(
    deliverables: &Path,
    target: &ReleaseSearchTarget<'_>,
) -> Vec<PathBuf> {
    vec![
        deliverables
            .join(target.edition)
            .join(target.aspect)
            .join("releases")
            .join(target.release),
        deliverables
            .join(target.edition)
            .join("releases")
            .join(target.release),
        deliverables.join("releases").join(target.release),
    ]
}

fn release_parent_candidates(
    deliverables: &Path,
    target: &ReleaseSearchTarget<'_>,
) -> Vec<PathBuf> {
    vec![
        deliverables.join(target.edition).join(target.aspect),
        deliverables.join(target.edition),
        deliverables.to_path_buf(),
    ]
}

fn check_candidate_match(candidate: PathBuf, release: &str) -> Option<PathBuf> {
    let is_dir = candidate.is_dir();
    if !is_dir {
        return None;
    }
    let match_res = verify_release_match(&candidate, release);
    if match_res.is_ok() {
        Some(candidate)
    } else {
        None
    }
}

fn find_matching_release_dir(
    deliverables: &Path,
    target: &ReleaseSearchTarget<'_>,
) -> Option<PathBuf> {
    let direct_candidates = release_subpath_candidates(deliverables, target);
    let direct_match = direct_candidates.into_iter().find(|dir| dir.is_dir());
    if let Some(candidate) = direct_match {
        return Some(candidate);
    }
    let parent_candidates = release_parent_candidates(deliverables, target);
    parent_candidates
        .into_iter()
        .find_map(|candidate| check_candidate_match(candidate, target.release))
}

fn resolve_release_directory(
    video: &Video,
    target: &ReleaseSearchTarget<'_>,
) -> Result<PathBuf, Diagnostic> {
    let deliverables = video.layout.deliverables();
    let matching_dir = find_matching_release_dir(&deliverables, target);
    if let Some(found_dir) = matching_dir {
        return Ok(found_dir);
    }

    Err(Diagnostic::new(
        ErrorCode::RefNotFound,
        &format!(
            "Release '{}' for edition '{}' aspect '{}' was not found. Run video export {} --edition {} --aspect {} to create it.",
            target.release,
            target.edition,
            target.aspect,
            video.reference,
            target.edition,
            target.aspect
        ),
    ))
}

fn check_file_for_release(
    file_path: &Path,
    release: &str,
) -> Result<ReleaseMatchResult, Diagnostic> {
    let has_file = file_path.exists();
    if !has_file {
        return Ok(ReleaseMatchResult::NotFound);
    }
    let bytes = fs::read(file_path).map_err(io_invalid_read)?;
    let text = String::from_utf8_lossy(&bytes);
    if text.contains(release) {
        Ok(ReleaseMatchResult::Matched)
    } else {
        Ok(ReleaseMatchResult::NotFound)
    }
}

fn verify_release_match(dir: &Path, release: &str) -> Result<(), Diagnostic> {
    let manifest_path = dir.join("manifest.json");
    let manifest_match = check_file_for_release(&manifest_path, release)?;
    if manifest_match == ReleaseMatchResult::Matched {
        return Ok(());
    }
    let current_path = dir.join("current.json");
    let current_match = check_file_for_release(&current_path, release)?;
    if current_match == ReleaseMatchResult::Matched {
        return Ok(());
    }
    Err(Diagnostic::new(ErrorCode::RefNotFound, "Release not found"))
}

fn collect_release_files(dir: &Path) -> Result<Vec<(PathBuf, u64)>, Diagnostic> {
    let mut files = Vec::new();
    let entries = fs::read_dir(dir).map_err(io_invalid_read)?;
    for entry in entries {
        let entry = entry.map_err(io_invalid_read)?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(io_invalid_read)?;
        let is_dir = file_type.is_dir();
        if is_dir {
            files.extend(collect_release_files(&path)?);
        } else {
            let metadata = fs::metadata(&path).map_err(io_invalid_read)?;
            files.push((path, metadata.len()));
        }
    }
    files.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(files)
}

fn review_store_path(video_dir: &Path) -> PathBuf {
    video_dir.join(".retold").join("review.json")
}

fn read_review_store_value(path: &Path) -> Result<serde_json::Value, Diagnostic> {
    let bytes = fs::read(path).map_err(io_invalid_read)?;
    serde_json::from_slice(&bytes).map_err(io_invalid_read)
}

fn extract_approvals_obj(
    store_value: &serde_json::Value,
) -> serde_json::Map<String, serde_json::Value> {
    store_value
        .get("approvals")
        .and_then(serde_json::Value::as_object)
        .cloned()
        .unwrap_or_default()
}

fn load_approvals_map(
    video_dir: &Path,
) -> Result<serde_json::Map<String, serde_json::Value>, Diagnostic> {
    let store_path = review_store_path(video_dir);
    let has_store = store_path.exists();
    if !has_store {
        return Ok(serde_json::Map::new());
    }
    let store_value = read_review_store_value(&store_path)?;
    Ok(extract_approvals_obj(&store_value))
}

fn check_gate_status(
    approvals: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> GateStatus {
    let is_locked = approvals
        .get(key)
        .and_then(|approval_entry| approval_entry.get("locked"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if is_locked {
        GateStatus::Locked
    } else {
        GateStatus::Open
    }
}

fn determine_review_status(video_dir: &Path, edition: &str) -> Result<ReviewStatus, Diagnostic> {
    let approvals = load_approvals_map(video_dir)?;
    if approvals.is_empty() {
        return Ok(ReviewStatus::ReviewCopy);
    }
    let all_gate = check_gate_status(&approvals, "all");
    if all_gate == GateStatus::Locked {
        return Ok(ReviewStatus::Final);
    }
    let edition_gate = check_gate_status(&approvals, edition);
    if edition_gate == GateStatus::Locked {
        Ok(ReviewStatus::Final)
    } else {
        Ok(ReviewStatus::ReviewCopy)
    }
}

fn copy_single_package_file(
    source_dir: &Path,
    file_item: &(PathBuf, u64),
    output: &Path,
) -> Result<PackageFileEntry, Diagnostic> {
    let (src_path, bytes_len) = file_item;
    let rel_path = src_path
        .strip_prefix(source_dir)
        .map_err(|_| Diagnostic::new(ErrorCode::DataInvalid, "Prefix mismatch during package."))?;
    let dst_path = output.join(rel_path);
    if let Some(parent_dir) = dst_path.parent() {
        fs::create_dir_all(parent_dir).map_err(io_execution_failed)?;
    }
    fs::copy(src_path, &dst_path).map_err(io_execution_failed)?;
    let sha256_hash = compute_sha256(&dst_path)?;
    let blake3_hash = compute_blake3(&dst_path)?;
    Ok(PackageFileEntry {
        path: rel_path.display().to_string(),
        bytes: *bytes_len,
        sha256: sha256_hash,
        blake3: blake3_hash,
    })
}

fn package_release_files(
    source_dir: &Path,
    files: &[(PathBuf, u64)],
    output: &Path,
) -> Result<Vec<PackageFileEntry>, Diagnostic> {
    fs::create_dir_all(output).map_err(io_execution_failed)?;
    let mut entries = Vec::new();
    for file_item in files {
        let entry = copy_single_package_file(source_dir, file_item, output)?;
        entries.push(entry);
    }
    write_checksums_file(output, &entries)?;
    Ok(entries)
}

fn checksum_file_path(output: &Path) -> PathBuf {
    output.join("CHECKSUMS.sha256")
}

fn format_checksum_lines(entries: &[PackageFileEntry]) -> String {
    let mut lines = Vec::new();
    for entry in entries {
        lines.push(format!("{}  {}", entry.sha256, entry.path));
    }
    lines.sort();
    lines.join("\n")
}

fn write_checksums_file(output: &Path, entries: &[PackageFileEntry]) -> Result<(), Diagnostic> {
    let text = format_checksum_lines(entries);
    let path = checksum_file_path(output);
    fs::write(path, format!("{text}\n")).map_err(io_execution_failed)
}

fn write_package_manifest(
    spec: &PackageManifestSpec<'_>,
    entries: &[PackageFileEntry],
) -> Result<(), Diagnostic> {
    let total_bytes: u64 = entries.iter().map(|entry| entry.bytes).sum();
    let manifest = serde_json::json!({
        "schema_version": 1,
        "format": "retold-delivery-package",
        "video_id": spec.video.id,
        "reference": spec.video.reference,
        "edition": spec.options.edition,
        "aspect": spec.aspect,
        "release_id": spec.options.release,
        "label": spec.status.label(),
        "is_review_copy": spec.status.is_review_copy(),
        "packaged_at": jiff::Timestamp::now().to_string(),
        "files": entries,
        "total_bytes": total_bytes,
    });
    let manifest_path = spec.output.join("package.json");
    let json_text = serde_json::to_string_pretty(&manifest).map_err(io_invalid_read)?;
    fs::write(&manifest_path, json_text).map_err(io_execution_failed)
}

pub(crate) fn compute_sha256(path: &Path) -> Result<String, Diagnostic> {
    let mut file = File::open(path).map_err(io_invalid_read)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; BUFFER_BYTES];
    loop {
        let read_bytes = file.read(&mut buffer).map_err(io_invalid_read)?;
        let is_eof = read_bytes == 0;
        if is_eof {
            break;
        }
        hasher.update(&buffer[..read_bytes]);
    }
    let digest = hasher.finalize();
    Ok(format!("{digest:x}"))
}

pub(crate) fn compute_blake3(path: &Path) -> Result<String, Diagnostic> {
    let mut file = File::open(path).map_err(io_invalid_read)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0_u8; BUFFER_BYTES];
    loop {
        let read_bytes = file.read(&mut buffer).map_err(io_invalid_read)?;
        let is_eof = read_bytes == 0;
        if is_eof {
            break;
        }
        hasher.update(&buffer[..read_bytes]);
    }
    let digest = hasher.finalize();
    Ok(digest.to_hex().to_string())
}

fn io_invalid_read(err: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::new(ErrorCode::DataInvalid, &err.to_string())
}

fn io_execution_failed(err: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::new(ErrorCode::ExecutionFailed, &err.to_string())
}
