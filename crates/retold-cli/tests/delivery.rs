//! Integration tests for video delivery packaging, verification, paths and cleanup.
#![expect(clippy::unwrap_used, reason = "test assertions and fixtures")]

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use serde_json::{Value, json};
use tempfile::TempDir;

struct DeliveryFixture {
    _temp: TempDir,
    library: PathBuf,
    video_dir: PathBuf,
}

fn construct_delivery_paths(base_path: &Path) -> (PathBuf, PathBuf) {
    let library = base_path.join("library");
    let video_dir = library.join("brand").join("episode-1");
    (library, video_dir)
}

fn init_delivery_paths(base_path: &Path) -> (PathBuf, PathBuf) {
    let (library, video_dir) = construct_delivery_paths(base_path);
    fs::create_dir_all(&video_dir).unwrap();
    (library, video_dir)
}

impl DeliveryFixture {
    fn setup() -> Self {
        let temp_workspace = TempDir::new().unwrap();
        let (library, video_dir) = init_delivery_paths(temp_workspace.path());

        write_fixture_run_manifest(&video_dir);
        seed_work_files(&video_dir);
        seed_release_deliverables(&video_dir);

        Self {
            _temp: temp_workspace,
            library,
            video_dir,
        }
    }

    fn run_cli(&self, args: &[&str]) -> Value {
        let mut command = Command::cargo_bin("retold").unwrap();
        command.arg("--json").arg("--library").arg(&self.library);
        command.args(args);
        let assert_res = command.assert();
        let output = assert_res.get_output();
        serde_json::from_slice(&output.stdout).unwrap()
    }
}

fn meta_manifest_path(video_dir: &Path) -> PathBuf {
    video_dir.join(".retold").join("run.json")
}

fn release_path(video_dir: &Path) -> PathBuf {
    video_dir
        .join("deliverables")
        .join("en")
        .join("9x16")
        .join("releases")
        .join("rel-export-01")
}

fn release_video_file(video_dir: &Path) -> PathBuf {
    release_path(video_dir).join("episode-1.en.9x16.mp4")
}

fn release_subtitles_file(video_dir: &Path) -> PathBuf {
    release_path(video_dir).join("subtitles.en.srt")
}

fn release_checksums_file(video_dir: &Path) -> PathBuf {
    release_path(video_dir).join("CHECKSUMS.sha256")
}

fn work_scratch_path(video_dir: &Path) -> PathBuf {
    video_dir.join("work").join("scratch.tmp")
}

fn work_audio_path(video_dir: &Path) -> PathBuf {
    video_dir.join("work").join("audio.wav")
}

fn write_fixture_run_manifest(video_dir: &Path) {
    let manifest_file = meta_manifest_path(video_dir);
    if let Some(parent) = manifest_file.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let manifest = json!({
        "schema": 1,
        "id": "vid-deliv-1",
        "created": "2026-10-20T10:00:00Z",
        "title": "Delivery Test Episode",
        "guide": {"id": "recap", "version": "1", "hash": "h1"},
        "channel": "brand",
        "inputs": {},
        "variants": ["en:en"],
        "quality": "standard",
        "budget": 50,
        "skip_reviews": false,
        "folder": ["episode-1"],
        "cast_levels": 0
    });
    fs::write(manifest_file, manifest.to_string()).unwrap();
}

fn seed_work_files(video_dir: &Path) {
    let scratch_file = work_scratch_path(video_dir);
    if let Some(parent) = scratch_file.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(scratch_file, b"temporary render chunk").unwrap();
    fs::write(work_audio_path(video_dir), b"rebuildable audio buffer").unwrap();
}

fn seed_release_deliverables(video_dir: &Path) {
    let release_dir = release_path(video_dir);
    fs::create_dir_all(release_dir).unwrap();

    fs::write(release_video_file(video_dir), b"mp4-video-content-stream").unwrap();
    fs::write(
        release_subtitles_file(video_dir),
        b"1\n00:00:01,000 --> 00:00:02,000\nHello",
    )
    .unwrap();

    let sha_video = "eef43ad652f6d6eba0b79c9cd48db5e67f36dff352e0f66a10d0328b4234818e";
    let sha_srt = "de5bcfe10428c0811da6def97f324c98c4d06adb43af5f4208e160dee112808d";
    let checksums = format!("{sha_video}  episode-1.en.9x16.mp4\n{sha_srt}  subtitles.en.srt\n");
    fs::write(release_checksums_file(video_dir), checksums).unwrap();
}

#[test]
fn paths_inspection_returns_resolved_directories() {
    let fixture = DeliveryFixture::setup();
    let result = fixture.run_cli(&["video", "paths", "brand/episode-1"]);
    assert_eq!(result["payload"]["outcome"], "completed");
    let paths_payload = &result["payload"]["data"];
    assert_eq!(paths_payload["reference"], "brand/episode-1");
    let paths = &paths_payload["paths"];
    assert!(paths["root"].as_str().unwrap().contains("episode-1"));
    assert!(paths["work"].as_str().unwrap().contains("work"));
    assert!(paths["delivery"].as_str().unwrap().contains("deliverables"));
    assert!(paths["state"].as_str().unwrap().contains(".retold"));
}

#[test]
fn verify_validates_delivery_checksums() {
    let fixture = DeliveryFixture::setup();
    let result = fixture.run_cli(&["video", "verify", "brand/episode-1"]);
    assert_eq!(result["payload"]["outcome"], "completed");
    let verify_info = &result["payload"]["data"];
    assert_eq!(verify_info["verified"], true);
    assert_eq!(verify_info["delivery_verified"], true);
    assert_eq!(verify_info["work_verified"], true);
}

fn corrupt_release_video(video_dir: &Path) {
    let target = release_video_file(video_dir);
    fs::write(target, b"tampered-content").unwrap();
}

#[test]
fn verify_detects_corrupted_deliverables() {
    let fixture = DeliveryFixture::setup();
    corrupt_release_video(&fixture.video_dir);
    let result = fixture.run_cli(&["video", "verify", "brand/episode-1"]);
    assert_eq!(result["payload"]["outcome"], "failed");
    assert_eq!(result["payload"]["code"], "RG-DATA-INVALID");
}

fn assert_clean_lifecycle(fixture: &DeliveryFixture, scratch: &Path) {
    let unconfirmed = fixture.run_cli(&["video", "clean", "brand/episode-1"]);
    assert_eq!(unconfirmed["payload"]["outcome"], "failed");
    assert_eq!(unconfirmed["payload"]["code"], "RG-INPUT-REQUIRED");

    let preview = fixture.run_cli(&["video", "clean", "brand/episode-1", "--dry-run"]);
    assert_eq!(preview["payload"]["outcome"], "completed");
    assert_eq!(preview["payload"]["data"]["status"], "preview");
    assert!(scratch.exists());

    let cleaned = fixture.run_cli(&["video", "clean", "brand/episode-1", "--yes"]);
    assert_eq!(cleaned["payload"]["outcome"], "completed");
    assert_eq!(cleaned["payload"]["data"]["status"], "cleaned");
    assert!(!scratch.exists());
}

#[test]
fn clean_removes_scratch_files_with_confirmation() {
    let fixture = DeliveryFixture::setup();
    let work_scratch = work_scratch_path(&fixture.video_dir);
    assert!(work_scratch.exists());
    assert_clean_lifecycle(&fixture, &work_scratch);
    let deliverables = fixture.video_dir.join("deliverables");
    assert!(deliverables.exists());
}

fn assert_package_deliverables(out_dir: &Path) {
    assert!(out_dir.join("episode-1.en.9x16.mp4").exists());
    assert!(out_dir.join("subtitles.en.srt").exists());
    assert!(out_dir.join("CHECKSUMS.sha256").exists());
    assert!(out_dir.join("package.json").exists());

    let package_json_bytes = fs::read(out_dir.join("package.json")).unwrap();
    let manifest: Value = serde_json::from_slice(&package_json_bytes).unwrap();
    assert_eq!(manifest["format"], "retold-delivery-package");
    assert_eq!(manifest["is_review_copy"], true);
    assert_eq!(manifest["edition"], "en");
    assert_eq!(manifest["aspect"], "9x16");
}

#[test]
fn package_bundles_release_with_checksums_and_manifest() {
    let fixture = DeliveryFixture::setup();
    let out_dir = fixture.library.join("handoff-pack");

    let result = fixture.run_cli(&[
        "video",
        "package",
        "brand/episode-1",
        "--edition",
        "en",
        "--aspect",
        "9x16",
        "--release",
        "rel-export-01",
        "--output",
        out_dir.to_str().unwrap(),
    ]);

    assert_eq!(result["payload"]["outcome"], "completed");
    let summary = &result["payload"]["data"];
    assert_eq!(summary["status"], "packaged");
    assert_eq!(summary["is_review_copy"], true);
    assert_eq!(summary["label"], "Review copy — not final delivery");
    assert_package_deliverables(&out_dir);
}

#[test]
fn package_nonexistent_release_fails_with_actionable_error() {
    let fixture = DeliveryFixture::setup();
    let out_dir = fixture.library.join("handoff-missing");

    let result = fixture.run_cli(&[
        "video",
        "package",
        "brand/episode-1",
        "--edition",
        "en",
        "--aspect",
        "9x16",
        "--release",
        "non-existent-release",
        "--output",
        out_dir.to_str().unwrap(),
    ]);

    assert_eq!(result["payload"]["outcome"], "failed");
    assert_eq!(result["payload"]["code"], "RG-REF-NOT-FOUND");
    assert!(
        result["payload"]["message"]
            .as_str()
            .unwrap()
            .contains("video export")
    );
}
