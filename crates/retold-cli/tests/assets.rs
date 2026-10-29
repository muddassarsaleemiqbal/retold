//! Integration tests for asset browser, provenance, integrity verification and usages (F09).
#![expect(clippy::unwrap_used, reason = "test assertions and fixtures")]

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use serde_json::{Value, json};
use tempfile::TempDir;

const SHA256_EPISODE1: &str = "43ef127e7a4e7ba52824742d5d57d2aee8d531fc12ac1facf88e8d614d8b786d";

struct AssetFixture {
    _temp: TempDir,
    library: PathBuf,
    video_dir: PathBuf,
}

fn init_library_dirs(workspace: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let library = workspace.join("library");
    let video_dir = library.join("brand").join("episode-1");
    let series_dir = library.join("series").join("season-1");
    (library, video_dir, series_dir)
}

impl AssetFixture {
    fn setup() -> Self {
        let temp_workspace = TempDir::new().unwrap();
        let (library, video_dir, series_dir) = init_library_dirs(temp_workspace.path());

        fs::create_dir_all(&video_dir).unwrap();
        fs::create_dir_all(&series_dir).unwrap();

        write_run_manifest(&video_dir);
        seed_media_assets(&video_dir, &series_dir);

        Self {
            _temp: temp_workspace,
            library,
            video_dir,
        }
    }

    fn query_json(&self, subcommands: &[&str]) -> Value {
        let mut cli = Command::cargo_bin("retold").unwrap();
        cli.args(["--json", "--library"])
            .arg(&self.library)
            .args(subcommands);
        let output = cli.output().unwrap();
        serde_json::from_slice(&output.stdout).unwrap()
    }

    fn query_human(&self, subcommands: &[&str]) -> String {
        let mut cli = Command::cargo_bin("retold").unwrap();
        cli.args(["--plain", "--library"])
            .arg(&self.library)
            .args(subcommands);
        let output = cli.output().unwrap();
        String::from_utf8_lossy(&output.stdout).into_owned()
    }
}

fn fixture_manifest() -> serde_json::Value {
    json!({
        "schema": 1,
        "id": "vid-assets-1",
        "created": "2026-10-20T10:00:00Z",
        "title": "Assets Test Video",
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

fn manifest_path(video_dir: &Path) -> PathBuf {
    video_dir.join(".retold").join("run.json")
}

fn write_run_manifest(video_dir: &Path) {
    let run_file = manifest_path(video_dir);
    if let Some(parent) = run_file.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let manifest_text = fixture_manifest().to_string();
    fs::write(run_file, manifest_text).unwrap();
}

fn cast_hero_path(video_dir: &Path) -> PathBuf {
    video_dir.join("cast").join("hero.png")
}

fn seed_cast_assets(video_dir: &Path) {
    let hero_png = cast_hero_path(video_dir);
    if let Some(parent) = hero_png.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(hero_png, b"\x89PNG\r\n\x1a\nfake hero png data").unwrap();
}

fn work_audio_path(video_dir: &Path) -> PathBuf {
    video_dir.join("work").join("audio").join("take-1.wav")
}

fn seed_work_assets(video_dir: &Path) {
    let voice_wav = work_audio_path(video_dir);
    if let Some(parent) = voice_wav.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(voice_wav, b"RIFFfake wave header and audio take 1").unwrap();
}

fn delivery_video_path(video_dir: &Path) -> PathBuf {
    video_dir
        .join("deliverables")
        .join("en")
        .join("9x16")
        .join("episode-1.mp4")
}

fn write_delivery_checksums(deliv_dir: &Path) {
    let checksums = format!("{SHA256_EPISODE1}  episode-1.mp4\n");
    fs::write(deliv_dir.join("CHECKSUMS.sha256"), checksums).unwrap();
}

fn seed_delivery_assets(video_dir: &Path) {
    let final_mp4 = delivery_video_path(video_dir);
    if let Some(parent) = final_mp4.parent() {
        fs::create_dir_all(parent).unwrap();
        write_delivery_checksums(parent);
    }
    fs::write(final_mp4, b"fake mp4 video bytes 001").unwrap();
}

fn seed_series_assets(series_dir: &Path) {
    fs::create_dir_all(series_dir).unwrap();
    let series_logo = series_dir.join("intro.png");
    fs::write(series_logo, b"\x89PNG\r\n\x1a\nseries intro logo").unwrap();
}

fn seed_media_assets(video_dir: &Path, series_dir: &Path) {
    seed_cast_assets(video_dir);
    seed_work_assets(video_dir);
    seed_delivery_assets(video_dir);
    seed_series_assets(series_dir);
}

fn corrupt_deliverable_file(video_dir: &Path) {
    let mp4_file = delivery_video_path(video_dir);
    fs::write(mp4_file, b"corrupted bytes inside release video").unwrap();
}

#[test]
fn test_assets_list_all_and_filtered_by_kind() {
    let fixture = AssetFixture::setup();

    let all_assets = fixture.query_json(&["assets", "list"]);
    assert_eq!(all_assets["payload"]["outcome"], "completed");
    let items = all_assets["payload"]["data"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 4);

    let image_assets = fixture.query_json(&["assets", "list", "--kind", "image"]);
    let image_items = image_assets["payload"]["data"]["items"].as_array().unwrap();
    assert_eq!(image_items.len(), 2);
    for item in image_items {
        assert_eq!(item["kind"], "image");
    }

    let audio_assets = fixture.query_json(&["assets", "list", "--kind", "audio"]);
    let audio_items = audio_assets["payload"]["data"]["items"].as_array().unwrap();
    assert_eq!(audio_items.len(), 1);
    assert_eq!(audio_items[0]["name"], "take-1.wav");

    let video_assets = fixture.query_json(&["assets", "list", "--kind", "video"]);
    let video_items = video_assets["payload"]["data"]["items"].as_array().unwrap();
    assert_eq!(video_items.len(), 1);
    assert_eq!(video_items[0]["name"], "episode-1.mp4");
}

#[test]
fn test_assets_list_scoped_by_video_and_series() {
    let fixture = AssetFixture::setup();

    let video_assets = fixture.query_json(&["assets", "list", "--video", "brand/episode-1"]);
    assert_eq!(video_assets["payload"]["outcome"], "completed");
    let v_items = video_assets["payload"]["data"]["items"].as_array().unwrap();
    assert_eq!(v_items.len(), 3);

    let series_assets = fixture.query_json(&["assets", "list", "--series", "season-1"]);
    assert_eq!(series_assets["payload"]["outcome"], "completed");
    let s_items = series_assets["payload"]["data"]["items"]
        .as_array()
        .unwrap();
    assert_eq!(s_items.len(), 1);
    assert_eq!(s_items[0]["name"], "intro.png");

    let missing_series = fixture.query_json(&["assets", "list", "--series", "non-existent-series"]);
    assert_eq!(missing_series["payload"]["outcome"], "failed");
    assert_eq!(missing_series["payload"]["code"], "RG-REF-NOT-FOUND");
}

#[test]
fn test_assets_show_details_and_provenance() {
    let fixture = AssetFixture::setup();

    let show_by_name = fixture.query_json(&["assets", "show", "hero.png"]);
    assert_eq!(show_by_name["payload"]["outcome"], "completed");
    let asset = &show_by_name["payload"]["data"]["asset"];
    assert_eq!(asset["name"], "hero.png");
    assert_eq!(asset["kind"], "image");
    assert_eq!(asset["status"], "valid");
    assert_eq!(asset["provenance"]["source"], "workspace");

    let show_take = fixture.query_json(&["assets", "show", "take-1.wav"]);
    assert_eq!(show_take["payload"]["outcome"], "completed");
    let take_asset = &show_take["payload"]["data"]["asset"];
    assert!(take_asset["is_immutable"].as_bool().unwrap());
    assert_eq!(take_asset["take"]["take_number"], 1);

    let missing = fixture.query_json(&["assets", "show", "does-not-exist.png"]);
    assert_eq!(missing["payload"]["outcome"], "failed");
    assert_eq!(missing["payload"]["code"], "RG-REF-NOT-FOUND");
}

#[test]
fn test_assets_verify_integrity_and_detect_corruption() {
    let fixture = AssetFixture::setup();

    let verify_res = fixture.query_json(&["assets", "verify", "episode-1.mp4"]);
    assert_eq!(verify_res["payload"]["outcome"], "completed");
    assert!(verify_res["payload"]["data"]["verified"].as_bool().unwrap());

    corrupt_deliverable_file(&fixture.video_dir);

    let corrupted_verify = fixture.query_json(&["assets", "verify", "episode-1.mp4"]);
    assert_eq!(corrupted_verify["payload"]["outcome"], "failed");
    assert_eq!(corrupted_verify["payload"]["code"], "RG-DATA-INVALID");
}

#[test]
fn test_assets_uses_dependencies() {
    let fixture = AssetFixture::setup();

    let uses_res = fixture.query_json(&["assets", "uses", "hero.png"]);
    assert_eq!(uses_res["payload"]["outcome"], "completed");
    let usages = uses_res["payload"]["data"]["usages"].as_array().unwrap();
    assert_eq!(usages.len(), 1);
    assert_eq!(usages[0]["reference"], "brand/episode-1");
    assert_eq!(usages[0]["dependent_stage"], "delivery");

    let series_uses = fixture.query_json(&["assets", "uses", "intro.png"]);
    assert_eq!(series_uses["payload"]["outcome"], "completed");
    let s_usages = series_uses["payload"]["data"]["usages"].as_array().unwrap();
    assert_eq!(s_usages.len(), 1);
    assert_eq!(s_usages[0]["reference"], "season-1");
    assert_eq!(s_usages[0]["dependent_stage"], "production");
}

#[test]
fn test_assets_human_projection() {
    let fixture = AssetFixture::setup();

    let human_list = fixture.query_human(&["assets", "list"]);
    assert!(human_list.contains("hero.png"));
    assert!(human_list.contains("take-1.wav"));
    assert!(human_list.contains("episode-1.mp4"));

    let show_human = fixture.query_human(&["assets", "show", "hero.png"]);
    assert!(show_human.contains("Asset ast-"));
    assert!(show_human.contains("Kind: image"));

    let verify_human = fixture.query_human(&["assets", "verify", "episode-1.mp4"]);
    assert!(verify_human.contains("Verified integrity for asset"));
    assert!(verify_human.contains("Status: verified OK"));

    let uses_human = fixture.query_human(&["assets", "uses", "hero.png"]);
    assert!(uses_human.contains("Usages for asset"));
    assert!(uses_human.contains("brand/episode-1 [delivery]"));
}
