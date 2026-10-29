//! Integration tests for config profile CLI commands.
#![expect(clippy::unwrap_used, reason = "test helpers")]

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use serde_json::Value;

struct ProfileRunner<'a> {
    config: &'a Path,
}

impl<'a> ProfileRunner<'a> {
    const fn new(config: &'a Path) -> Self {
        Self { config }
    }

    fn run_json(&self, args: &[&str]) -> Value {
        let mut cmd = Command::cargo_bin("retold").unwrap();
        cmd.env("RETOLD_CONFIG_DIR", self.config);
        cmd.arg("--json");
        cmd.args(args);
        let assert_res = cmd.assert();
        let bytes = &assert_res.get_output().stdout;
        serde_json::from_slice(bytes).unwrap()
    }

    fn run_plain(&self, args: &[&str]) -> String {
        let mut cmd = Command::cargo_bin("retold").unwrap();
        cmd.env("RETOLD_CONFIG_DIR", self.config);
        cmd.arg("--plain");
        cmd.args(args);
        let assert_res = cmd.assert().success();
        let bytes = &assert_res.get_output().stdout;
        String::from_utf8_lossy(bytes).into_owned()
    }
}

fn write_file(parent: &Path, filename: &str, content: &str) -> PathBuf {
    let file_path = parent.join(filename);
    fs::write(&file_path, content.as_bytes()).unwrap();
    file_path
}

#[test]
fn profile_create_and_list() {
    let root = tempfile::tempdir().unwrap();
    let runner = ProfileRunner::new(root.path());

    let empty = runner.run_json(&["config", "profile", "list"]);
    assert_eq!(empty["type"], "result");
    let items = empty["payload"]["data"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 0);

    let created = runner.run_json(&["config", "profile", "new", "dev"]);
    assert_eq!(created["type"], "result");
    assert_eq!(created["payload"]["data"]["profile"]["name"], "dev");

    let listed = runner.run_json(&["config", "profile", "list"]);
    let listed_items = listed["payload"]["data"]["items"].as_array().unwrap();
    assert_eq!(listed_items.len(), 1);
    assert_eq!(listed_items[0]["name"], "dev");
}

#[test]
fn profile_show_and_use() {
    let root = tempfile::tempdir().unwrap();
    let runner = ProfileRunner::new(root.path());
    runner.run_json(&["config", "profile", "new", "prod"]);

    let switched = runner.run_json(&["config", "profile", "use", "prod"]);
    assert_eq!(switched["type"], "result");
    assert_eq!(switched["payload"]["data"]["active_profile"], "prod");

    let shown = runner.run_json(&["config", "profile", "show", "prod"]);
    assert_eq!(shown["type"], "result");
    assert_eq!(shown["payload"]["data"]["profile"]["active"], true);

    let plain_list = runner.run_plain(&["config", "profile", "list"]);
    assert!(plain_list.contains("prod"));
}

#[test]
fn profile_rejects_secrets() {
    let root = tempfile::tempdir().unwrap();
    let runner = ProfileRunner::new(root.path());
    let secret_content = "name = \"bad\"\napi_key = \"secret_12345\"\n";
    let file = write_file(root.path(), "bad.toml", secret_content);

    let rejected = runner.run_json(&[
        "config",
        "profile",
        "new",
        "bad",
        "--file",
        file.to_str().unwrap(),
    ]);
    assert_eq!(rejected["type"], "error");
    assert_eq!(rejected["payload"]["code"], "RG-INPUT-REQUIRED");
}

#[test]
fn profile_validation_and_missing() {
    let root = tempfile::tempdir().unwrap();
    let runner = ProfileRunner::new(root.path());

    let missing = runner.run_json(&["config", "profile", "show", "ghost"]);
    assert_eq!(missing["type"], "error");
    assert_eq!(missing["payload"]["code"], "RG-REF-NOT-FOUND");

    let use_missing = runner.run_json(&["config", "profile", "use", "ghost"]);
    assert_eq!(use_missing["type"], "error");
    assert_eq!(use_missing["payload"]["code"], "RG-REF-NOT-FOUND");

    let invalid_name = runner.run_json(&["config", "profile", "new", "has space"]);
    assert_eq!(invalid_name["type"], "error");
    assert_eq!(invalid_name["payload"]["code"], "RG-INPUT-REQUIRED");
}

#[test]
fn profile_duplicate_refused() {
    let root = tempfile::tempdir().unwrap();
    let runner = ProfileRunner::new(root.path());
    runner.run_json(&["config", "profile", "new", "dev"]);

    let duplicate = runner.run_json(&["config", "profile", "new", "dev"]);
    assert_eq!(duplicate["type"], "error");
    assert_eq!(duplicate["payload"]["code"], "RG-EXECUTION-FAILED");
}
