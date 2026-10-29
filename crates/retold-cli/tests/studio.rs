//! Integration tests for `retold studio` commands.

#![expect(clippy::unwrap_used, reason = "test assertions unwrap")]

use std::process::Command;

fn run_studio(state_dir: &std::path::Path, args: &[&str]) -> (serde_json::Value, i32) {
    let exe = assert_cmd::cargo::cargo_bin("retold");
    let mut cmd = Command::new(exe);
    cmd.envs(isolated_environment(state_dir));
    cmd.env_remove("RETOLD_WORKSPACE")
        .env_remove("XDG_DATA_HOME");
    cmd.arg("--json");
    cmd.arg("studio");
    cmd.args(args);
    let output = cmd.output().unwrap();
    let json_value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    (json_value, output.status.code().unwrap_or(-1))
}

fn isolated_environment(
    state_dir: &std::path::Path,
) -> impl IntoIterator<Item = (&'static str, std::path::PathBuf)> {
    [
        ("HOME", state_dir.join("home")),
        ("RETOLD_CONFIG_DIR", state_dir.join("config")),
        ("RETOLD_LIBRARY", state_dir.join("library")),
        ("RETOLD_STATE", state_dir.to_owned()),
    ]
}

#[test]
fn studio_status_and_stop_offline() {
    let test_dir = tempfile::tempdir().unwrap();
    let (status_json, code) = run_studio(test_dir.path(), &["status"]);
    assert_eq!(code, 0);
    assert_eq!(status_json["payload"]["data"]["running"], false);
    assert_eq!(status_json["payload"]["data"]["active_jobs"], 0);

    let (stop_val, stop_code) = run_studio(test_dir.path(), &["stop"]);
    assert_eq!(stop_code, 0);
    assert_eq!(stop_val["payload"]["data"]["stopped"], false);
}

#[test]
fn studio_start_status_stop_lifecycle() {
    let test_dir = tempfile::tempdir().unwrap();
    let (start_val, start_code) = run_studio(test_dir.path(), &["start"]);
    assert_eq!(start_code, 0, "{start_val}");
    assert_eq!(start_val["payload"]["data"]["running"], true);

    let (status_val, status_code) = run_studio(test_dir.path(), &["status"]);
    assert_eq!(status_code, 0, "{status_val}");
    assert_eq!(status_val["payload"]["data"]["running"], true);

    let (stop_val, stop_code) = run_studio(test_dir.path(), &["stop"]);
    assert_eq!(stop_code, 0, "{stop_val}");
    assert_eq!(stop_val["payload"]["data"]["stopped"], true);

    let (after_val, after_code) = run_studio(test_dir.path(), &["status"]);
    assert_eq!(after_code, 0, "{after_val}");
    assert_eq!(after_val["payload"]["data"]["running"], false);
}

#[test]
fn failed_start_preserves_an_unrelated_file_at_the_socket_path() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("studio.sock");
    std::fs::write(&path, b"user-owned data").unwrap();
    let (failed, code) = run_studio(directory.path(), &["start"]);
    assert_eq!(code, 4, "{failed}");
    assert_eq!(std::fs::read(path).unwrap(), b"user-owned data");
}

#[test]
fn unreadable_queue_is_reported_instead_of_presented_as_empty() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("queue.sqlite");
    std::fs::write(&path, b"not a SQLite database").unwrap();
    let (failed, code) = run_studio(directory.path(), &["status"]);
    assert_eq!(code, 4, "{failed}");
    assert_eq!(failed["payload"]["code"], "RG-DATA-INVALID");
    assert_eq!(std::fs::read(path).unwrap(), b"not a SQLite database");
}
