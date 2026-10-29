//! Storage usage analysis, breakdown by video and category, and reclaimable space reporting (F19).

use std::fs;
use std::path::Path;

use crate::discovery;
use crate::library::{Catalog, Video, response};
use crate::protocol::{Diagnostic, ErrorCode, Payload};

const PERCENT_MULTIPLIER: f64 = 100.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UsageGrouping {
    #[default]
    Summary,
    ByVideo,
    ByKind,
}

impl UsageGrouping {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Summary => "summary",
            Self::ByVideo => "video",
            Self::ByKind => "kind",
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StorageUsageReport {
    pub total_bytes: u64,
    pub total_files: usize,
    pub reclaimable_bytes: u64,
    pub grouping: String,
    pub groups: Vec<UsageGroupEntry>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UsageGroupEntry {
    pub name: String,
    pub category: String,
    pub files: usize,
    pub bytes: u64,
    pub percentage: f64,
}

#[derive(Debug, Default, Clone, Copy)]
struct DirectoryStats {
    files: usize,
    bytes: u64,
}

impl DirectoryStats {
    const fn add(&mut self, other: Self) {
        self.files = self.files.saturating_add(other.files);
        self.bytes = self.bytes.saturating_add(other.bytes);
    }
}

/// Compute workspace storage usage with optional grouping.
pub fn inspect_storage_usage(root: &Path, grouping: UsageGrouping) -> Result<Payload, Diagnostic> {
    let catalog = discovery::scan(root)?;
    let report = analyze_catalog_storage(&catalog, grouping)?;
    Ok(response(
        serde_json::to_value(report).map_err(io_invalid_read)?,
    ))
}

fn analyze_catalog_storage(
    catalog: &Catalog,
    grouping: UsageGrouping,
) -> Result<StorageUsageReport, Diagnostic> {
    match grouping {
        UsageGrouping::ByVideo => build_video_grouped_report(catalog),
        UsageGrouping::ByKind => build_kind_grouped_report(catalog),
        UsageGrouping::Summary => build_summary_grouped_report(catalog),
    }
}

fn build_video_grouped_report(catalog: &Catalog) -> Result<StorageUsageReport, Diagnostic> {
    let mut entries = Vec::new();
    let mut grand_total = DirectoryStats::default();
    let mut reclaimable_total: u64 = 0;

    for video in &catalog.videos {
        let stats = tally_video_storage(video)?;
        let work_stats = tally_path_storage(&video.layout.work())?;
        reclaimable_total = reclaimable_total.saturating_add(work_stats.bytes);
        grand_total.add(stats);
        entries.push((video.reference.clone(), stats));
    }

    let groups = format_grouped_entries(entries, grand_total.bytes, "video");
    Ok(StorageUsageReport {
        total_bytes: grand_total.bytes,
        total_files: grand_total.files,
        reclaimable_bytes: reclaimable_total,
        grouping: UsageGrouping::ByVideo.as_str().to_string(),
        groups,
    })
}

fn build_kind_grouped_report(catalog: &Catalog) -> Result<StorageUsageReport, Diagnostic> {
    let (deliverables, work, cast, state) = tally_all_video_categories(catalog)?;
    let series_stats = tally_series_storage(&catalog.root)?;

    let mut grand_total = DirectoryStats::default();
    grand_total.add(deliverables);
    grand_total.add(work);
    grand_total.add(cast);
    grand_total.add(state);
    grand_total.add(series_stats);

    let raw_entries = vec![
        ("deliverables".to_string(), deliverables),
        ("work".to_string(), work),
        ("cast".to_string(), cast),
        ("series".to_string(), series_stats),
        ("state".to_string(), state),
    ];

    let groups = format_grouped_entries(raw_entries, grand_total.bytes, "kind");
    Ok(StorageUsageReport {
        total_bytes: grand_total.bytes,
        total_files: grand_total.files,
        reclaimable_bytes: work.bytes,
        grouping: UsageGrouping::ByKind.as_str().to_string(),
        groups,
    })
}

fn build_summary_grouped_report(catalog: &Catalog) -> Result<StorageUsageReport, Diagnostic> {
    let (deliverables, work, cast, state) = tally_all_video_categories(catalog)?;
    let series_stats = tally_series_storage(&catalog.root)?;

    let mut grand_total = DirectoryStats::default();
    grand_total.add(deliverables);
    grand_total.add(work);
    grand_total.add(cast);
    grand_total.add(state);
    grand_total.add(series_stats);

    let raw_entries = vec![
        ("deliverables".to_string(), deliverables),
        ("work".to_string(), work),
        ("assets".to_string(), cast),
    ];

    let groups = format_grouped_entries(raw_entries, grand_total.bytes, "summary");
    Ok(StorageUsageReport {
        total_bytes: grand_total.bytes,
        total_files: grand_total.files,
        reclaimable_bytes: work.bytes,
        grouping: UsageGrouping::Summary.as_str().to_string(),
        groups,
    })
}

fn tally_all_video_categories(
    catalog: &Catalog,
) -> Result<
    (
        DirectoryStats,
        DirectoryStats,
        DirectoryStats,
        DirectoryStats,
    ),
    Diagnostic,
> {
    let mut deliv_total = DirectoryStats::default();
    let mut work_total = DirectoryStats::default();
    let mut cast_total = DirectoryStats::default();
    let mut state_total = DirectoryStats::default();

    for video in &catalog.videos {
        deliv_total.add(tally_path_storage(&video.layout.deliverables())?);
        work_total.add(tally_path_storage(&video.layout.work())?);
        cast_total.add(tally_path_storage(&video.directory.join("cast"))?);
        state_total.add(tally_path_storage(&video.directory.join(".retold"))?);
    }

    Ok((deliv_total, work_total, cast_total, state_total))
}

fn tally_series_storage(library_root: &Path) -> Result<DirectoryStats, Diagnostic> {
    let series_dir = library_root.join("series");
    tally_path_storage(&series_dir)
}

fn tally_video_storage(video: &Video) -> Result<DirectoryStats, Diagnostic> {
    tally_path_storage(&video.directory)
}

fn tally_path_storage(dir: &Path) -> Result<DirectoryStats, Diagnostic> {
    let has_dir = dir.is_dir();
    if !has_dir {
        return Ok(DirectoryStats::default());
    }
    let mut collected = DirectoryStats::default();
    let entries = fs::read_dir(dir).map_err(io_invalid_read)?;
    for entry in entries {
        let entry = entry.map_err(io_invalid_read)?;
        let child_path = entry.path();
        let file_type = entry.file_type().map_err(io_invalid_read)?;
        let is_dir = file_type.is_dir();
        if is_dir {
            let sub = tally_path_storage(&child_path)?;
            collected.add(sub);
        } else {
            let meta = fs::metadata(&child_path).map_err(io_invalid_read)?;
            collected.files = collected.files.saturating_add(1);
            collected.bytes = collected.bytes.saturating_add(meta.len());
        }
    }
    Ok(collected)
}

#[expect(clippy::cast_precision_loss, reason = "storage percentage display")]
fn calculate_percentage(bytes: u64, total: u64) -> f64 {
    let zero_total = total == 0;
    if zero_total {
        0.0
    } else {
        ((bytes as f64) / (total as f64)) * PERCENT_MULTIPLIER
    }
}

fn format_grouped_entries(
    raw_entries: Vec<(String, DirectoryStats)>,
    total_bytes: u64,
    category_label: &str,
) -> Vec<UsageGroupEntry> {
    let mut groups = Vec::new();
    for (name, stats) in raw_entries {
        let pct = calculate_percentage(stats.bytes, total_bytes);
        groups.push(UsageGroupEntry {
            name,
            category: category_label.to_string(),
            files: stats.files,
            bytes: stats.bytes,
            percentage: pct,
        });
    }
    groups.sort_by_key(|left| std::cmp::Reverse(left.bytes));
    groups
}

fn io_invalid_read(err: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::new(ErrorCode::DataInvalid, &err.to_string())
}
