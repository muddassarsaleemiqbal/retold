//! Explicit typed replacement never starts a provider or edits its credentials.
#![expect(clippy::unwrap_used, reason = "fixture assertions")]
use assert_cmd::Command;
use serde_json::Value;
use std::path::Path;

const ORIGINAL: &str = "[providers.personal]\nkind = 'codex_cli'\nmax_parallel = 1\n[routing]\n\"text.smart\" = ['personal:smart']\n";

fn invoke(root: &Path, flags: &[&str], exit: i32) -> Value {
    let mut command = Command::cargo_bin("retold").unwrap();
    command.args(["--json", "providers", "edit", "personal", "--root"]);
    command
        .arg(root)
        .arg("--file")
        .arg(root.join("definition.toml"));
    command.env("PATH", "").args(flags);
    let assertion = command.assert().code(exit);
    let output = assertion.get_output();
    assert_eq!(output.stderr, Vec::<u8>::new());
    let stdout = String::from_utf8(output.stdout.clone()).unwrap();
    assert_eq!(stdout.lines().count(), 1);
    serde_json::from_str(&stdout).unwrap()
}

#[test]
fn editing_is_offline_and_returns_preview_and_backup() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("providers.toml");
    std::fs::write(&path, ORIGINAL).unwrap();
    std::fs::write(
        root.path().join("definition.toml"),
        "kind = 'codex_cli'\nmax_parallel = 2",
    )
    .unwrap();
    let preview = invoke(root.path(), &["--dry-run"], 0);
    assert_eq!(preview["payload"]["data"]["edited"]["dry_run"], true);
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 2);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), ORIGINAL);
    let result = invoke(root.path(), &[], 0);
    let backup = result["payload"]["data"]["edited"]["backup"]
        .as_str()
        .unwrap();
    assert_eq!(std::fs::read_to_string(backup).unwrap(), ORIGINAL);
}

#[test]
fn editing_refuses_invalid_definition_without_echoing_values() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("providers.toml"), ORIGINAL).unwrap();
    std::fs::write(root.path().join("definition.toml"), "secret = 'NEVER-ECHO'").unwrap();
    let error = invoke(root.path(), &[], 2);
    assert!(!error.to_string().contains("NEVER-ECHO"));
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 2);
}
