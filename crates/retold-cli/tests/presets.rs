//! Integration tests for export-presets CLI commands.
#![expect(clippy::unwrap_used, reason = "test helpers")]

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use serde_json::Value;

struct PresetsRunner<'a> {
    library: &'a Path,
    config: PathBuf,
}

impl<'a> PresetsRunner<'a> {
    fn new(library: &'a Path) -> Self {
        let config = library.join(".preset-config");
        Self { library, config }
    }

    fn run_json(&self, args: &[&str]) -> Value {
        let mut cmd = Command::cargo_bin("retold").unwrap();
        cmd.env("RETOLD_CONFIG_DIR", &self.config);
        cmd.arg("--json").arg("--library").arg(self.library);
        cmd.args(args);
        let assert_res = cmd.assert();
        let bytes = &assert_res.get_output().stdout;
        serde_json::from_slice(bytes).unwrap()
    }

    fn run_plain(&self, args: &[&str]) -> String {
        let mut cmd = Command::cargo_bin("retold").unwrap();
        cmd.env("RETOLD_CONFIG_DIR", &self.config);
        cmd.arg("--plain").arg("--library").arg(self.library);
        cmd.args(args);
        let assert_res = cmd.assert().success();
        let bytes = &assert_res.get_output().stdout;
        String::from_utf8_lossy(bytes).into_owned()
    }
}

const fn sample_preset_json() -> &'static str {
    r#"{
  "aspects": ["9:16", "1:1"],
  "container": "mp4",
  "video_codec": "h264",
  "audio_codec": "aac",
  "caption_mode": "burned"
}"#
}

fn write_spec_file(parent: &Path, filename: &str, content: &str) -> PathBuf {
    let file_path = parent.join(filename);
    fs::write(&file_path, content.as_bytes()).unwrap();
    file_path
}

#[test]
fn preset_creation_and_listing() {
    let root = tempfile::tempdir().unwrap();
    let runner = PresetsRunner::new(root.path());
    let spec = write_spec_file(root.path(), "portrait.json", sample_preset_json());

    let empty = runner.run_json(&["export-presets", "list"]);
    assert_eq!(empty["type"], "result");
    let items = empty["payload"]["data"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 0);

    let created = runner.run_json(&[
        "export-presets",
        "new",
        "portrait",
        "--file",
        spec.to_str().unwrap(),
    ]);
    assert_eq!(created["type"], "result");
    assert_eq!(created["payload"]["data"]["preset"]["name"], "portrait");

    let listed = runner.run_json(&["export-presets", "list"]);
    let listed_items = listed["payload"]["data"]["items"].as_array().unwrap();
    assert_eq!(listed_items.len(), 1);
    assert_eq!(listed_items[0]["name"], "portrait");
}

#[test]
fn preset_show_and_plain_output() {
    let root = tempfile::tempdir().unwrap();
    let runner = PresetsRunner::new(root.path());
    let spec = write_spec_file(root.path(), "portrait.json", sample_preset_json());
    runner.run_json(&[
        "export-presets",
        "new",
        "portrait",
        "--file",
        spec.to_str().unwrap(),
    ]);

    let shown = runner.run_json(&["export-presets", "show", "portrait"]);
    assert_eq!(shown["type"], "result");
    let preset_data = &shown["payload"]["data"]["preset"];
    assert_eq!(preset_data["container"], "mp4");
    assert_eq!(preset_data["caption_mode"], "burned");

    let plain_list = runner.run_plain(&["export-presets", "list"]);
    assert!(plain_list.contains("portrait"));
}

#[test]
fn preset_removal_requires_confirmation_and_removes() {
    let root = tempfile::tempdir().unwrap();
    let runner = PresetsRunner::new(root.path());
    let spec = write_spec_file(root.path(), "portrait.json", sample_preset_json());
    runner.run_json(&[
        "export-presets",
        "new",
        "portrait",
        "--file",
        spec.to_str().unwrap(),
    ]);

    let unconfirmed = runner.run_json(&["export-presets", "remove", "portrait"]);
    assert_eq!(unconfirmed["type"], "error");
    assert_eq!(unconfirmed["payload"]["code"], "RG-INPUT-REQUIRED");

    let removed = runner.run_json(&["export-presets", "remove", "portrait", "--yes"]);
    assert_eq!(removed["type"], "result");
    assert_eq!(removed["payload"]["data"]["removed"], "portrait");

    let missing = runner.run_json(&["export-presets", "show", "portrait"]);
    assert_eq!(missing["type"], "error");
    assert_eq!(missing["payload"]["code"], "RG-REF-NOT-FOUND");
}

#[test]
fn preset_validation_failures() {
    let root = tempfile::tempdir().unwrap();
    let runner = PresetsRunner::new(root.path());
    let bad_aspect = r#"{"aspects": ["21:9"], "container": "mp4", "video_codec": "h264", "audio_codec": "aac", "caption_mode": "burned"}"#;
    let bad_spec = write_spec_file(root.path(), "bad.json", bad_aspect);

    let rejected = runner.run_json(&[
        "export-presets",
        "new",
        "ultrawide",
        "--file",
        bad_spec.to_str().unwrap(),
    ]);
    assert_eq!(rejected["type"], "error");
    assert_eq!(rejected["payload"]["code"], "RG-INPUT-REQUIRED");
}
