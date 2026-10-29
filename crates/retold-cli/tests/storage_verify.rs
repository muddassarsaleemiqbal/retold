//! Offline portable integrity checks use isolated fixtures and no provider tools.
#![expect(clippy::unwrap_used, reason = "test fixtures must succeed")]
use serde_json::{Value, json};
use std::path::Path;

fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("manifest.json"), b"{}").unwrap();
    write_index(root.path(), "manifest.json", 2);
    root
}

fn write_index(root: &Path, path: &str, bytes: u64) {
    let checksum = blake3::hash(b"{}").to_hex().to_string();
    let index = json!({
        "schema_version": 1,
        "format": "retold-workspace-backup",
        "workspace_id": "workspace-1",
        "account_id": "account-1",
        "files": [{"path": path, "bytes": bytes, "blake3": checksum}],
        "omissions": ["secrets", "login profiles"]
    });
    std::fs::write(root.join("index.json"), serde_json::to_vec(&index).unwrap()).unwrap();
}

fn invoke(root: &Path) -> (Value, i32) {
    let executable = assert_cmd::cargo::cargo_bin("retold");
    let mut command = std::process::Command::new(executable);
    command
        .args(["--json", "storage", "verify"])
        .arg(root)
        .env("PATH", "");
    let output = command.output().unwrap();
    assert_eq!(output.stderr, Vec::<u8>::new());
    (
        serde_json::from_slice(&output.stdout).unwrap(),
        output.status.code().unwrap(),
    )
}

#[test]
fn verifies_offline_without_granting_authority_or_changing_files() {
    let root = fixture();
    let before = std::fs::read(root.path().join("index.json")).unwrap();
    let (result, exit) = invoke(root.path());
    assert_eq!(exit, 0, "{result}");
    assert_eq!(
        result["payload"]["data"]["backup_verified"]["execution_authorized"],
        false
    );
    assert_eq!(
        result["payload"]["data"]["backup_verified"]["managed_workspace_verified"],
        false
    );
    assert_eq!(
        std::fs::read(root.path().join("index.json")).unwrap(),
        before
    );
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 2);
}

#[test]
fn refuses_traversal_platform_paths_and_oversized_entries() {
    let root = fixture();
    let forbidden = [
        "../manifest.json",
        "/manifest.json",
        "a/../manifest.json",
        "a\\b",
        "C:secret",
        "./manifest.json",
        "index.json",
    ];
    for path in forbidden {
        write_index(root.path(), path, 2);
        let (result, exit) = invoke(root.path());
        assert_eq!(exit, 4, "{path}: {result}");
    }
    write_index(root.path(), "manifest.json", u64::MAX);
    assert_eq!(invoke(root.path()).1, 4);
}

#[test]
fn refuses_changed_files_duplicate_entries_and_unknown_schema() {
    let root = fixture();
    std::fs::write(root.path().join("manifest.json"), b"[]").unwrap();
    assert_eq!(invoke(root.path()).1, 4);
    std::fs::write(root.path().join("manifest.json"), b"{}").unwrap();
    check_index(root.path());
}

fn check_index(root: &Path) {
    let path = root.join("index.json");
    let mut index: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    index["files"] = json!([index["files"][0], index["files"][0]]);
    std::fs::write(&path, serde_json::to_vec(&index).unwrap()).unwrap();
    assert_eq!(invoke(root).1, 4);
    write_index(root, "manifest.json", 2);
    index["files"] = json!([index["files"][0]]);
    index["schema_version"] = json!(2);
    std::fs::write(&path, serde_json::to_vec(&index).unwrap()).unwrap();
    assert_eq!(invoke(root).1, 4);
}

#[cfg(unix)]
#[test]
fn refuses_symlink_files_and_parent_directories() {
    let root = fixture();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("data"), b"{}").unwrap();
    check_file_symlink(root.path(), outside.path());
    check_parent_symlink(root.path(), outside.path());
}

#[cfg(unix)]
fn check_parent_symlink(root: &Path, outside: &Path) {
    std::os::unix::fs::symlink(outside, root.join("parent")).unwrap();
    write_index(root, "parent/data", 2);
    assert_eq!(invoke(root).1, 4);
}

#[cfg(unix)]
fn check_file_symlink(root: &Path, outside: &Path) {
    let target = outside.join("data");
    let link = root.join("link");
    std::os::unix::fs::symlink(target, link).unwrap();
    write_index(root, "link", 2);
    assert_eq!(invoke(root).1, 4);
}

#[test]
fn rejects_unindexed_files_and_missing_files() {
    let root = fixture();
    let extra = root.path().join("unindexed");
    std::fs::write(&extra, b"unexpected").unwrap();
    assert_eq!(invoke(root.path()).1, 4);
    std::fs::remove_file(extra).unwrap();
    write_index(root.path(), "missing", 2);
    assert_eq!(invoke(root.path()).1, 4);
}
