//! Integration tests for doctor, setup, and top-level lifecycle commands.
#![expect(clippy::unwrap_used, reason = "test helpers")]

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use serde_json::Value;

fn query_cli_json(library: &Path, args: &[&str]) -> Value {
    let mut cmd = Command::cargo_bin("retold").unwrap();
    let sandbox = library.with_extension("sandbox");
    let config_dir = sandbox.join("config");
    cmd.env("HOME", &sandbox);
    cmd.env("RETOLD_CONFIG_DIR", config_dir);
    cmd.env_remove("XDG_DATA_HOME");
    cmd.arg("--json");
    cmd.arg("--library");
    cmd.arg(library);
    cmd.args(args);
    let assert_res = cmd.assert();
    let bytes = &assert_res.get_output().stdout;
    serde_json::from_slice(bytes).unwrap()
}

fn create_test_orphan(root: &Path) -> PathBuf {
    let channels = root.join("channels");
    fs::create_dir_all(&channels).unwrap();
    let orphan = channels.join("stale.tmp");
    fs::write(&orphan, b"temporary data").unwrap();
    orphan
}

#[test]
fn doctor_inspect_lifecycle() {
    let root = tempfile::tempdir().unwrap();
    let initial = query_cli_json(root.path(), &["doctor"]);
    assert_eq!(initial["type"], "result");
    let report = &initial["payload"]["data"]["report"];
    assert!(report["checks"].is_array());
    assert!(report["disk"]["total_bytes"].is_number());
    let checks = report["checks"].as_array().unwrap();
    let names: Vec<&str> = checks
        .iter()
        .filter_map(|check| check["name"].as_str())
        .collect();
    assert!(names.contains(&"filmcraft_editor"));
    assert!(names.contains(&"ffmpeg"));
    assert!(names.contains(&"ffprobe"));
    assert!(names.contains(&"guides_catalog"));
    assert!(names.contains(&"channels_catalog"));
    assert!(names.contains(&"presets"));
}

#[test]
fn doctor_repair_lifecycle() {
    let root = tempfile::tempdir().unwrap();
    let orphan = create_test_orphan(root.path());
    let repaired = query_cli_json(root.path(), &["doctor", "--fix"]);
    assert_eq!(repaired["type"], "result");
    let fix_report = &repaired["payload"]["data"]["report"];
    let fixes = fix_report["fixes_applied"].as_array().unwrap();
    assert_ne!(fixes.len(), 0);
    assert!(!orphan.exists());
}

#[test]
fn setup_initializes_clean_workspace() {
    let root = tempfile::tempdir().unwrap();
    let library = root.path().join("my-library");
    let setup_result = query_cli_json(&library, &["setup"]);
    assert_eq!(setup_result["type"], "result");
    let report = &setup_result["payload"]["data"]["report"];
    assert!(report["initialized_dirs"].is_array());
    assert!(library.join("channels").exists());
    assert!(library.join("guides").exists());
    assert!(library.join("series").exists());
    assert!(library.join("presets").exists());
}

#[test]
fn top_level_usage_is_available_offline() {
    let root = tempfile::tempdir().unwrap();
    let usage = query_cli_json(root.path(), &["usage"]);
    assert_eq!(usage["type"], "result");
    assert!(usage["payload"]["data"]["text"].is_string());
}

#[test]
fn completions_and_man_plain_execution() {
    let mut completions_cmd = Command::cargo_bin("retold").unwrap();
    completions_cmd.args(["--plain", "completions", "bash"]);
    let comp_assert = completions_cmd.assert().success();
    let comp_output = comp_assert.get_output();
    let comp_str = String::from_utf8_lossy(&comp_output.stdout);
    assert!(comp_str.contains("retold"));

    let mut man_cmd = Command::cargo_bin("retold").unwrap();
    man_cmd.args(["--plain", "man"]);
    let man_assert = man_cmd.assert().success();
    let man_output = man_assert.get_output();
    let man_str = String::from_utf8_lossy(&man_output.stdout);
    assert!(man_str.contains(".TH retold 1"));
}

#[cfg(target_os = "linux")]
#[test]
fn doctor_checks_the_native_editor_on_path_without_running_it() {
    let root = tempfile::tempdir().unwrap();
    let executable = root.path().join("filmcraft");
    let marker = root.path().join("invoked");
    fs::write(
        &executable,
        format!("#!/bin/sh\ntouch '{}'\n", marker.display()),
    )
    .unwrap();
    assert_native_editor_status(root.path(), "ok");
    fs::remove_file(&executable).unwrap();
    assert_native_editor_status(root.path(), "warn");
    assert!(!marker.exists());
}

#[cfg(target_os = "linux")]
fn assert_native_editor_status(root: &Path, expected: &str) {
    let mut command = Command::cargo_bin("retold").unwrap();
    command.env("HOME", root).env("PATH", root);
    command.env("RETOLD_CONFIG_DIR", root.join("config"));
    command.env_remove("XDG_DATA_HOME");
    command.args(["--json", "--library"]).arg(root);
    command.arg("doctor");
    let output = command.output().unwrap();
    let envelope: Value = serde_json::from_slice(&output.stdout).unwrap();
    let checks = envelope["payload"]["data"]["report"]["checks"]
        .as_array()
        .unwrap();
    let editor = checks
        .iter()
        .find(|check| check["name"] == "filmcraft_editor")
        .unwrap();
    assert_eq!(editor["status"], expected);
}
