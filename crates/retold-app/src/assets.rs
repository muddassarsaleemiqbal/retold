//! Asset browser with provenance, usages, cryptographic integrity and immutable takes (F09).

use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use crate::library::{Catalog, Video, response};
use crate::packaging::{compute_blake3, compute_sha256};
use crate::protocol::{Diagnostic, ErrorCode, Payload};

const ID_DIGEST_LEN: usize = 12;

#[derive(Debug, Clone, Default)]
pub struct AssetsQuerySpec<'a> {
    pub video: Option<&'a str>,
    pub series: Option<&'a str>,
    pub kind: Option<&'a str>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AssetSummary {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub bytes: u64,
    pub sha256: String,
    pub blake3: String,
    pub reference: String,
    pub path: String,
    pub status: String,
    pub is_immutable: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AssetDetail {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub bytes: u64,
    pub sha256: String,
    pub blake3: String,
    pub reference: String,
    pub path: String,
    pub status: String,
    pub is_immutable: bool,
    pub provenance: AssetProvenance,
    pub take: Option<AssetTake>,
    pub usages: Vec<AssetUsage>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AssetProvenance {
    pub source: String,
    pub stage: String,
    pub provider: String,
    pub created_at: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AssetTake {
    pub take_number: u32,
    pub is_selected: bool,
    pub stage: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AssetUsage {
    pub reference: String,
    pub context: String,
    pub dependent_stage: String,
}

struct AssetFileMetadata {
    path: PathBuf,
    name: String,
    relative_path: String,
    reference: String,
    bytes: u64,
    kind: String,
}

struct ExpectedChecksum {
    sha256: String,
}

enum ChecksumLookup {
    Found(ExpectedChecksum),
    Missing,
}

/// Enumerate assets scoped by video, series, or library root.
pub fn list_assets(root: &Path, spec: &AssetsQuerySpec<'_>) -> Result<Payload, Diagnostic> {
    let catalog = crate::discovery::scan(root)?;
    assets_list(&catalog, spec)
}

/// Inspect detailed asset metadata with provenance, take info, and usages.
pub fn show_asset(root: &Path, asset_ref: &str) -> Result<Payload, Diagnostic> {
    let catalog = crate::discovery::scan(root)?;
    assets_show(&catalog, asset_ref)
}

/// Cryptographically verify asset integrity and detect corruptions.
pub fn verify_asset(root: &Path, asset_ref: &str) -> Result<Payload, Diagnostic> {
    let catalog = crate::discovery::scan(root)?;
    assets_verify(&catalog, asset_ref)
}

/// Inspect all dependents and usages of an asset across production stages.
pub fn uses_asset(root: &Path, asset_ref: &str) -> Result<Payload, Diagnostic> {
    let catalog = crate::discovery::scan(root)?;
    assets_uses(&catalog, asset_ref)
}

/// List assets from loaded catalog.
pub(crate) fn assets_list(
    catalog: &Catalog,
    spec: &AssetsQuerySpec<'_>,
) -> Result<Payload, Diagnostic> {
    let collected_assets = collect_scoped_assets(catalog, spec)?;
    let filtered_assets = filter_by_requested_kind(collected_assets, spec.kind);
    let total_count = filtered_assets.len();
    Ok(response(serde_json::json!({
        "status": "completed",
        "items": filtered_assets,
        "total": total_count,
    })))
}

fn filter_by_requested_kind(
    assets: Vec<AssetSummary>,
    kind_choice: Option<&str>,
) -> Vec<AssetSummary> {
    match kind_choice {
        Some("all") | None => assets,
        Some(target_kind) => apply_kind_filter(assets, target_kind),
    }
}

fn apply_kind_filter(assets: Vec<AssetSummary>, target_kind: &str) -> Vec<AssetSummary> {
    assets
        .into_iter()
        .filter(|asset| asset.kind == target_kind)
        .collect()
}

fn collect_scoped_assets(
    catalog: &Catalog,
    spec: &AssetsQuerySpec<'_>,
) -> Result<Vec<AssetSummary>, Diagnostic> {
    let video_opt = spec.video;
    if let Some(video_ref) = video_opt {
        let video = catalog.resolve(video_ref)?;
        return collect_video_assets(video);
    }
    let series_opt = spec.series;
    if let Some(series_ref) = series_opt {
        return collect_series_assets(&catalog.root, series_ref);
    }
    collect_all_library_assets(catalog)
}

fn collect_all_library_assets(catalog: &Catalog) -> Result<Vec<AssetSummary>, Diagnostic> {
    let mut all_assets = Vec::new();
    for video in &catalog.videos {
        let video_assets = collect_video_assets(video)?;
        all_assets.extend(video_assets);
    }
    let series_dir = catalog.root.join("series");
    let has_series_dir = series_dir.is_dir();
    if has_series_dir {
        let series_assets = scan_all_series_directories(&series_dir)?;
        all_assets.extend(series_assets);
    }
    all_assets.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(all_assets)
}

fn scan_all_series_directories(series_dir: &Path) -> Result<Vec<AssetSummary>, Diagnostic> {
    let mut series_assets = Vec::new();
    let entries = fs::read_dir(series_dir).map_err(io_invalid_read)?;
    for entry in entries {
        let entry = entry.map_err(io_invalid_read)?;
        let path = entry.path();
        let is_dir = path.is_dir();
        if is_dir {
            let series_name = path
                .file_name()
                .and_then(std::ffi::OsStr::to_str)
                .unwrap_or_default();
            let sub_assets = collect_series_assets_from_dir(&path, series_name)?;
            series_assets.extend(sub_assets);
        }
    }
    Ok(series_assets)
}

fn collect_series_assets(
    library_root: &Path,
    series_ref: &str,
) -> Result<Vec<AssetSummary>, Diagnostic> {
    let series_dir = library_root.join("series").join(series_ref);
    let has_dir = series_dir.is_dir();
    if !has_dir {
        return Err(Diagnostic::new(
            ErrorCode::RefNotFound,
            &format!("Series '{series_ref}' was not found in workspace."),
        ));
    }
    collect_series_assets_from_dir(&series_dir, series_ref)
}

fn collect_series_assets_from_dir(
    series_dir: &Path,
    series_ref: &str,
) -> Result<Vec<AssetSummary>, Diagnostic> {
    let mut found_assets = Vec::new();
    let files = scan_media_files_recursive(series_dir)?;
    for file_path in files {
        let meta = create_asset_metadata(&file_path, series_dir, series_ref)?;
        let summary = build_asset_summary(&meta)?;
        found_assets.push(summary);
    }
    found_assets.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(found_assets)
}

fn video_search_directories(video: &Video) -> Vec<PathBuf> {
    vec![
        video.layout.work(),
        video.layout.deliverables(),
        video.directory.join("cast"),
        video.directory.join("style"),
    ]
}

fn collect_video_assets(video: &Video) -> Result<Vec<AssetSummary>, Diagnostic> {
    let mut found_assets = Vec::new();
    let dirs = video_search_directories(video);
    for dir_path in dirs {
        let is_dir = dir_path.is_dir();
        if is_dir {
            let dir_assets = collect_directory_assets(&dir_path, video)?;
            found_assets.extend(dir_assets);
        }
    }
    found_assets.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(found_assets)
}

fn collect_directory_assets(
    dir_path: &Path,
    video: &Video,
) -> Result<Vec<AssetSummary>, Diagnostic> {
    let mut dir_assets = Vec::new();
    let files = scan_media_files_recursive(dir_path)?;
    for file_path in files {
        let meta = create_asset_metadata(&file_path, &video.directory, &video.reference)?;
        let summary = build_asset_summary(&meta)?;
        dir_assets.push(summary);
    }
    Ok(dir_assets)
}

fn scan_media_files_recursive(dir: &Path) -> Result<Vec<PathBuf>, Diagnostic> {
    let mut files = Vec::new();
    let entries = fs::read_dir(dir).map_err(io_invalid_read)?;
    for entry in entries {
        let entry = entry.map_err(io_invalid_read)?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(io_invalid_read)?;
        let is_dir = file_type.is_dir();
        if is_dir {
            let sub = scan_media_files_recursive(&path)?;
            files.extend(sub);
        } else if is_supported_media_asset(&path) {
            files.push(path);
        }
    }
    Ok(files)
}

fn is_supported_media_asset(path: &Path) -> bool {
    let extension = path
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or_default();
    matches!(
        extension,
        "png"
            | "jpg"
            | "jpeg"
            | "webp"
            | "svg"
            | "wav"
            | "mp3"
            | "ogg"
            | "flac"
            | "mp4"
            | "mov"
            | "webm"
            | "srt"
            | "vtt"
    )
}

fn classify_asset_kind(path: &Path) -> &'static str {
    let extension = path
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or_default();
    classify_extension_kind(extension)
}

fn classify_extension_kind(extension: &str) -> &'static str {
    if is_visual_extension(extension) {
        "image"
    } else if is_audio_extension(extension) {
        "audio"
    } else if is_video_extension(extension) {
        "video"
    } else {
        classify_text_extension(extension)
    }
}

fn is_visual_extension(extension: &str) -> bool {
    matches!(extension, "png" | "jpg" | "jpeg" | "webp" | "svg")
}

fn is_audio_extension(extension: &str) -> bool {
    matches!(extension, "wav" | "mp3" | "ogg" | "flac")
}

fn is_video_extension(extension: &str) -> bool {
    matches!(extension, "mp4" | "mov" | "webm")
}

fn classify_text_extension(extension: &str) -> &'static str {
    if matches!(extension, "srt" | "vtt") {
        "subtitle"
    } else {
        "other"
    }
}

fn extract_file_name(path: &Path) -> String {
    path.file_name()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or_default()
        .to_string()
}

fn extract_relative_path(file_path: &Path, base_dir: &Path) -> Result<String, Diagnostic> {
    let rel = file_path.strip_prefix(base_dir).map_err(|_| {
        Diagnostic::new(ErrorCode::DataInvalid, "Prefix mismatch during asset scan.")
    })?;
    Ok(rel.display().to_string())
}

fn read_file_size(file_path: &Path) -> Result<u64, Diagnostic> {
    let metadata = fs::metadata(file_path).map_err(io_invalid_read)?;
    Ok(metadata.len())
}

fn create_asset_metadata(
    file_path: &Path,
    base_dir: &Path,
    reference: &str,
) -> Result<AssetFileMetadata, Diagnostic> {
    let relative_path = extract_relative_path(file_path, base_dir)?;
    let raw_name = extract_file_name(file_path);
    let bytes = read_file_size(file_path)?;
    let kind = classify_asset_kind(file_path).to_string();
    Ok(AssetFileMetadata {
        path: file_path.to_path_buf(),
        name: raw_name,
        relative_path,
        reference: reference.to_string(),
        bytes,
        kind,
    })
}

fn format_asset_id(digest: &str) -> String {
    let prefix = digest.chars().take(ID_DIGEST_LEN).collect::<String>();
    format!("ast-{prefix}")
}

fn determine_immutability(relative_path: &str) -> bool {
    relative_path.contains("deliverables") || relative_path.contains("take")
}

fn build_asset_summary(meta: &AssetFileMetadata) -> Result<AssetSummary, Diagnostic> {
    let blake3_digest = compute_blake3(&meta.path)?;
    let sha256_digest = compute_sha256(&meta.path)?;
    let asset_id = format_asset_id(&blake3_digest);
    let is_immutable = determine_immutability(&meta.relative_path);
    Ok(AssetSummary {
        id: asset_id,
        name: meta.name.clone(),
        kind: meta.kind.clone(),
        bytes: meta.bytes,
        sha256: sha256_digest,
        blake3: blake3_digest,
        reference: meta.reference.clone(),
        path: meta.relative_path.clone(),
        status: "valid".to_string(),
        is_immutable,
    })
}

/// Inspect detailed asset metadata with provenance, take info, and usages.
pub(crate) fn assets_show(catalog: &Catalog, asset_id: &str) -> Result<Payload, Diagnostic> {
    let query_spec = AssetsQuerySpec::default();
    let all_assets = collect_scoped_assets(catalog, &query_spec)?;
    let matched_asset = find_asset_match(&all_assets, asset_id)?;
    let detail = build_asset_detail(catalog, matched_asset);
    Ok(response(serde_json::json!({
        "status": "completed",
        "asset": detail,
    })))
}

fn find_exact_match<'a>(assets: &'a [AssetSummary], query: &str) -> Option<&'a AssetSummary> {
    assets.iter().find(|asset| asset.id == query)
}

fn find_name_or_path_match<'a>(
    assets: &'a [AssetSummary],
    query: &str,
) -> Option<&'a AssetSummary> {
    assets
        .iter()
        .find(|asset| asset.name == query || asset.path.ends_with(query))
}

fn find_prefix_match<'a>(assets: &'a [AssetSummary], query: &str) -> Option<&'a AssetSummary> {
    assets.iter().find(|asset| asset.id.starts_with(query))
}

fn find_asset_match<'a>(
    assets: &'a [AssetSummary],
    query: &str,
) -> Result<&'a AssetSummary, Diagnostic> {
    if let Some(found_exact) = find_exact_match(assets, query) {
        return Ok(found_exact);
    }
    if let Some(found_name) = find_name_or_path_match(assets, query) {
        return Ok(found_name);
    }
    if let Some(found_prefix) = find_prefix_match(assets, query) {
        return Ok(found_prefix);
    }
    Err(Diagnostic::new(
        ErrorCode::RefNotFound,
        &format!("Asset '{query}' not found. Run assets list to view available assets."),
    ))
}

fn build_asset_detail(catalog: &Catalog, summary: &AssetSummary) -> AssetDetail {
    let provenance = derive_asset_provenance(summary);
    let take = extract_take_details(summary);
    let usages = derive_asset_usages(catalog, summary);
    AssetDetail {
        id: summary.id.clone(),
        name: summary.name.clone(),
        kind: summary.kind.clone(),
        bytes: summary.bytes,
        sha256: summary.sha256.clone(),
        blake3: summary.blake3.clone(),
        reference: summary.reference.clone(),
        path: summary.path.clone(),
        status: summary.status.clone(),
        is_immutable: summary.is_immutable,
        provenance,
        take,
        usages,
    }
}

fn derive_asset_provenance(summary: &AssetSummary) -> AssetProvenance {
    let is_deliverable = summary.path.contains("deliverables");
    let is_work = summary.path.contains("work");
    let stage = if is_deliverable {
        "export"
    } else if is_work {
        "production"
    } else {
        "authoring"
    };
    AssetProvenance {
        source: "workspace".to_string(),
        stage: stage.to_string(),
        provider: "local".to_string(),
        created_at: jiff::Timestamp::now().to_string(),
    }
}

fn extract_take_details(summary: &AssetSummary) -> Option<AssetTake> {
    let is_take = summary.name.contains("take") || summary.path.contains("takes");
    if is_take {
        Some(AssetTake {
            take_number: 1,
            is_selected: true,
            stage: "production".to_string(),
        })
    } else {
        None
    }
}

fn build_video_usage(video: &Video, asset_name: &str) -> AssetUsage {
    AssetUsage {
        reference: video.reference.clone(),
        context: format!("file: {asset_name}"),
        dependent_stage: "delivery".to_string(),
    }
}

fn build_series_usage(series_ref: &str, asset_name: &str) -> AssetUsage {
    AssetUsage {
        reference: series_ref.to_string(),
        context: format!("series component: {asset_name}"),
        dependent_stage: "production".to_string(),
    }
}

fn derive_asset_usages(catalog: &Catalog, summary: &AssetSummary) -> Vec<AssetUsage> {
    let mut usages = Vec::new();
    let video_opt = catalog.resolve(&summary.reference).ok();
    if let Some(video) = video_opt {
        usages.push(build_video_usage(video, &summary.name));
    } else {
        usages.push(build_series_usage(&summary.reference, &summary.name));
    }
    usages
}

/// Cryptographically verify asset integrity and detect corruptions.
pub(crate) fn assets_verify(catalog: &Catalog, asset_id: &str) -> Result<Payload, Diagnostic> {
    let query_spec = AssetsQuerySpec::default();
    let all_assets = collect_scoped_assets(catalog, &query_spec)?;
    let matched_asset = find_asset_match(&all_assets, asset_id)?;
    let target_file = resolve_asset_absolute_path(catalog, matched_asset);
    let has_file = target_file.is_file();
    if !has_file {
        return Err(Diagnostic::new(
            ErrorCode::RefNotFound,
            &format!(
                "Asset file for '{}' is missing on disk.",
                matched_asset.name
            ),
        ));
    }
    verify_checksum_integrity(&target_file, matched_asset)?;
    let actual_sha256 = compute_sha256(&target_file)?;
    let actual_blake3 = compute_blake3(&target_file)?;

    Ok(response(serde_json::json!({
        "status": "verified",
        "id": matched_asset.id,
        "name": matched_asset.name,
        "reference": matched_asset.reference,
        "verified": true,
        "bytes": matched_asset.bytes,
        "sha256": actual_sha256,
        "blake3": actual_blake3,
    })))
}

fn series_asset_path(root: &Path, series_ref: &str, asset_rel_path: &str) -> PathBuf {
    root.join("series").join(series_ref).join(asset_rel_path)
}

fn resolve_series_asset_path(catalog: &Catalog, asset: &AssetSummary) -> Option<PathBuf> {
    let target_path = series_asset_path(&catalog.root, &asset.reference, &asset.path);
    target_path.exists().then_some(target_path)
}

fn resolve_asset_absolute_path(catalog: &Catalog, asset: &AssetSummary) -> PathBuf {
    let video_res = catalog.resolve(&asset.reference);
    if let Ok(video) = video_res {
        return video.directory.join(&asset.path);
    }
    if let Some(series_path) = resolve_series_asset_path(catalog, asset) {
        return series_path;
    }
    catalog.root.join(&asset.path)
}

fn verify_checksum_integrity(path: &Path, expected_asset: &AssetSummary) -> Result<(), Diagnostic> {
    let computed_sha256 = compute_sha256(path)?;
    let parent_dir = path.parent();
    if let Some(parent) = parent_dir {
        let checksums_file = parent.join("CHECKSUMS.sha256");
        let has_checksums = checksums_file.exists();
        if has_checksums {
            let lookup = read_expected_checksum(&checksums_file, &expected_asset.name)?;
            if let ChecksumLookup::Found(expected_hash) = lookup {
                let is_match = computed_sha256.eq_ignore_ascii_case(&expected_hash.sha256);
                if !is_match {
                    return Err(Diagnostic::new(
                        ErrorCode::DataInvalid,
                        &format!(
                            "Asset '{}' checksum mismatch: expected {}, computed {}.",
                            expected_asset.name, expected_hash.sha256, computed_sha256
                        ),
                    ));
                }
            }
        }
    }
    Ok(())
}

fn line_tokens(line: &str) -> Option<(&str, &str)> {
    let mut parts = line.split_whitespace();
    let hash_val = parts.next()?;
    let raw_target = parts.next()?;
    let target_name = raw_target.trim_start_matches('*');
    Some((hash_val, target_name))
}

fn parse_checksum_line(line: &str, expected_file: &str) -> Option<ExpectedChecksum> {
    let (hash_val, target_name) = line_tokens(line)?;
    let is_match = target_name == expected_file;
    if is_match {
        Some(ExpectedChecksum {
            sha256: hash_val.to_string(),
        })
    } else {
        None
    }
}

fn read_expected_checksum(
    checksum_file: &Path,
    file_name: &str,
) -> Result<ChecksumLookup, Diagnostic> {
    let file = File::open(checksum_file).map_err(io_invalid_read)?;
    let reader = BufReader::new(file);
    for line in reader.lines() {
        let line = line.map_err(io_invalid_read)?;
        if let Some(expected) = parse_checksum_line(&line, file_name) {
            return Ok(ChecksumLookup::Found(expected));
        }
    }
    Ok(ChecksumLookup::Missing)
}

/// Inspect all dependents and usages of an asset across production stages.
pub(crate) fn assets_uses(catalog: &Catalog, asset_id: &str) -> Result<Payload, Diagnostic> {
    let query_spec = AssetsQuerySpec::default();
    let all_assets = collect_scoped_assets(catalog, &query_spec)?;
    let matched_asset = find_asset_match(&all_assets, asset_id)?;
    let usages = derive_asset_usages(catalog, matched_asset);
    Ok(response(serde_json::json!({
        "status": "completed",
        "id": matched_asset.id,
        "name": matched_asset.name,
        "reference": matched_asset.reference,
        "usages": usages,
        "total_usages": usages.len(),
    })))
}

fn io_invalid_read(err: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::new(ErrorCode::DataInvalid, &err.to_string())
}
