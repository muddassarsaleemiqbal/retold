//! Integration tests for workspace storage usage analysis and breakdown (F19).
#![expect(clippy::unwrap_used, reason = "test assertions and fixtures")]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use assert_cmd::Command;
use serde_json::{Value, json};
use tempfile::TempDir;

const CAST_BYTES: usize = 1_000;
const WORK_BYTES: usize = 5_000;
const DELIV_BYTES: usize = 20_000;
const SERIES_BYTES: usize = 2_000;
const MIN_TOTAL_BYTES: u64 = 28_000;
const MIN_VIDEO_BYTES: u64 = 26_000;
const RECLAIMABLE_WORK_BYTES: u64 = 5_000;

struct StorageFixture {
    _temp: TempDir,
    library: PathBuf,
}

fn storage_library_dir(workspace: &Path) -> PathBuf {
    workspace.join("library")
}

fn storage_video_dir(library: &Path) -> PathBuf {
    library.join("brand").join("episode-1")
}

fn storage_series_dir(library: &Path) -> PathBuf {
    library.join("series").join("season-1")
}

fn init_storage_workspace(workspace: &Path) -> PathBuf {
    let library = storage_library_dir(workspace);
    let video_dir = storage_video_dir(&library);
    let series_dir = storage_series_dir(&library);
    fs::create_dir_all(&video_dir).unwrap();
    fs::create_dir_all(&series_dir).unwrap();
    write_storage_video_manifest(&video_dir);
    seed_storage_files(&video_dir, &series_dir);
    library
}

fn run_cli_output(library: &Path, output_flag: &str, subcommands: &[&str]) -> Output {
    let mut cli = Command::cargo_bin("retold").unwrap();
    cli.args([output_flag, "--library"]).arg(library);
    cli.args(subcommands);
    cli.output().unwrap()
}

impl StorageFixture {
    fn setup() -> Self {
        let temp_workspace = TempDir::new().unwrap();
        let library = init_storage_workspace(temp_workspace.path());
        Self {
            _temp: temp_workspace,
            library,
        }
    }

    fn run_json(&self, subcommands: &[&str]) -> Value {
        let output = run_cli_output(&self.library, "--json", subcommands);
        serde_json::from_slice(&output.stdout).unwrap()
    }

    fn run_plain(&self, subcommands: &[&str]) -> String {
        let output = run_cli_output(&self.library, "--plain", subcommands);
        String::from_utf8_lossy(&output.stdout).into_owned()
    }
}

fn storage_video_manifest() -> serde_json::Value {
    json!({
        "schema": 1,
        "id": "vid-storage-1",
        "created": "2026-10-20T10:00:00Z",
        "title": "Storage Test Video",
        "guide": {"id": "recap", "version": "1", "hash": "h1"},
        "channel": "brand",
        "inputs": {},
        "variants": ["en:en"],
        "quality": "standard",
        "budget": 50,
        "skip_reviews": false,
        "folder": ["episode-1"],
        "cast_levels": 0
    })
}

fn manifest_file_path(video_dir: &Path) -> PathBuf {
    video_dir.join(".retold").join("run.json")
}

fn write_storage_video_manifest(video_dir: &Path) {
    let run_file = manifest_file_path(video_dir);
    if let Some(parent) = run_file.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let manifest_text = storage_video_manifest().to_string();
    fs::write(run_file, manifest_text).unwrap();
}

fn write_seeded_file(file_path: PathBuf, byte_count: usize) {
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(file_path, vec![0_u8; byte_count]).unwrap();
}

fn cast_portrait_path(video_dir: &Path) -> PathBuf {
    video_dir.join("cast").join("portrait.png")
}

fn work_scratch_path(video_dir: &Path) -> PathBuf {
    video_dir.join("work").join("audio_scratch.tmp")
}

fn deliverable_final_path(video_dir: &Path) -> PathBuf {
    video_dir.join("deliverables").join("final.mp4")
}

fn series_logo_path(series_dir: &Path) -> PathBuf {
    series_dir.join("shared_logo.png")
}

fn seed_storage_files(video_dir: &Path, series_dir: &Path) {
    write_seeded_file(cast_portrait_path(video_dir), CAST_BYTES);
    write_seeded_file(work_scratch_path(video_dir), WORK_BYTES);
    write_seeded_file(deliverable_final_path(video_dir), DELIV_BYTES);
    write_seeded_file(series_logo_path(series_dir), SERIES_BYTES);
}

#[test]
fn test_storage_usage_summary() {
    let fixture = StorageFixture::setup();

    let res = fixture.run_json(&["storage", "usage"]);
    assert_eq!(res["payload"]["outcome"], "completed");

    let report = &res["payload"]["data"];
    assert_eq!(report["grouping"], "summary");
    assert!(report["total_bytes"].as_u64().unwrap() >= MIN_TOTAL_BYTES);
    assert_eq!(report["reclaimable_bytes"], RECLAIMABLE_WORK_BYTES);
}

#[test]
fn test_storage_usage_by_video() {
    let fixture = StorageFixture::setup();

    let res = fixture.run_json(&["storage", "usage", "--by", "video"]);
    assert_eq!(res["payload"]["outcome"], "completed");

    let report = &res["payload"]["data"];
    assert_eq!(report["grouping"], "video");
    let groups = report["groups"].as_array().unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0]["name"], "brand/episode-1");
    assert!(groups[0]["bytes"].as_u64().unwrap() >= MIN_VIDEO_BYTES);
}

#[test]
fn test_storage_usage_by_kind() {
    let fixture = StorageFixture::setup();

    let res = fixture.run_json(&["storage", "usage", "--by", "kind"]);
    assert_eq!(res["payload"]["outcome"], "completed");

    let report = &res["payload"]["data"];
    assert_eq!(report["grouping"], "kind");
    let groups = report["groups"].as_array().unwrap();
    assert!(groups.len() >= 3);

    let has_work = groups
        .iter()
        .any(|group| group["name"] == "work" && group["bytes"] == WORK_BYTES as u64);
    assert!(has_work);

    let has_deliv = groups
        .iter()
        .any(|group| group["name"] == "deliverables" && group["bytes"] == DELIV_BYTES as u64);
    assert!(has_deliv);
}

#[test]
fn test_storage_usage_human_projection() {
    let fixture = StorageFixture::setup();

    let plain_summary = fixture.run_plain(&["storage", "usage"]);
    assert!(plain_summary.contains("Storage usage (summary):"));
    assert!(plain_summary.contains("reclaimable"));

    let plain_kind = fixture.run_plain(&["storage", "usage", "--by", "kind"]);
    assert!(plain_kind.contains("Storage usage (kind):"));
    assert!(plain_kind.contains("deliverables"));
    assert!(plain_kind.contains("work"));
}
