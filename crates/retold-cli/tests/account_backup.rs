//! `account backup` snapshots the installation account only when nothing holds it, and
//! `account verify-backup` checks a backup offline. No provider is contacted.
#![expect(clippy::unwrap_used, reason = "fixture helpers")]
use assert_cmd::Command;
use retold_app::account::Account;
use serde_json::Value;
use std::path::{Path, PathBuf};

const INPUT_ERROR: i32 = 2;
const BLOCKED: i32 = 3;
const DATA_INVALID: i32 = 4;

fn invoke(home: &Path, args: &[&str]) -> (String, i32) {
    let executable = assert_cmd::cargo::cargo_bin("retold");
    let mut command = Command::new(executable);
    command
        .env("HOME", home)
        .env_remove("XDG_DATA_HOME")
        .env("PATH", "");
    let output = command.args(args).output().unwrap();
    assert_eq!(output.stderr, Vec::<u8>::new());
    let stdout = String::from_utf8(output.stdout).unwrap();
    (stdout, output.status.code().unwrap())
}

fn json(home: &Path, args: &[&str]) -> (Value, i32) {
    let arguments: Vec<_> = std::iter::once("--json")
        .chain(args.iter().copied())
        .collect();
    let (stdout, exit) = invoke(home, &arguments);
    (serde_json::from_str(&stdout).unwrap(), exit)
}

/// Where `account init` puts the installation account for this `home`.
fn account_directory(home: &Path) -> PathBuf {
    let application_data = if cfg!(target_os = "macos") {
        "Library/Application Support"
    } else {
        ".local/share"
    };
    home.join(application_data).join("retold-account")
}

fn initialized() -> (tempfile::TempDir, String) {
    let home = tempfile::tempdir().unwrap();
    invoke(home.path(), &["account", "init"]);
    let output = home.path().join("backup");
    (home, output.to_str().unwrap().into())
}

#[test]
fn backups_publish_once_and_verify_offline() {
    let (home, output) = initialized();
    let (preview, exit) = json(
        home.path(),
        &["account", "backup", "--output", &output, "--dry-run"],
    );
    assert_eq!(exit, 0, "{preview}");
    assert_eq!(
        preview["payload"]["data"]["backup_preview"]["busy"],
        serde_json::json!([])
    );
    assert!(!Path::new(&output).exists(), "dry runs write nothing");
    let (created, exit) = invoke(home.path(), &["account", "backup", "--output", &output]);
    assert_eq!(exit, 0, "{created}");
    assert!(
        created.contains("Not included: provider secrets"),
        "{created}"
    );
    let (_, exit) = json(home.path(), &["account", "backup", "--output", &output]);
    assert_eq!(exit, INPUT_ERROR, "backups never overwrite");
    let (verified, exit) = json(home.path(), &["account", "verify-backup", &output]);
    assert_eq!(exit, 0, "{verified}");
    assert_eq!(
        verified["payload"]["data"]["backup_verified"]["same_account"],
        true
    );
    check_tampering(home.path(), &output);
}

fn check_tampering(home: &Path, output: &str) {
    std::fs::write(Path::new(output).join("accounting.sqlite"), b"tampered").unwrap();
    let (tampered, exit) = json(home, &["account", "verify-backup", output]);
    assert_eq!(
        (tampered["payload"]["code"].as_str(), exit),
        (Some("RG-DATA-INVALID"), DATA_INVALID)
    );
}

#[test]
fn busy_accounts_refuse_backups_and_name_their_owners() {
    let (home, output) = initialized();
    let account = Account::open(&account_directory(home.path())).unwrap();
    let held = account.hold("video continue job-test").unwrap();
    let (busy, exit) = json(home.path(), &["account", "backup", "--output", &output]);
    assert_eq!(
        (busy["payload"]["code"].as_str(), exit),
        (Some("RG-ACCOUNT-BUSY"), BLOCKED)
    );
    assert_eq!(
        busy["payload"]["context"]["owners"][0]["purpose"],
        "video continue job-test"
    );
    let (plain, _) = invoke(home.path(), &["account", "backup", "--output", &output]);
    assert!(
        plain.contains("  video continue job-test · process"),
        "{plain}"
    );
    assert!(!Path::new(&output).exists());
    drop(held);
    let (_, exit) = json(home.path(), &["account", "backup", "--output", &output]);
    assert_eq!(exit, 0);
}
