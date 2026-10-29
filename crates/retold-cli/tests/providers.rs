//! Discovery is a noninteractive, secret-redacted terminal result.
use assert_cmd::Command;
use serde_json::Value;

#[test]
fn scan_is_offline_and_redacts_credentials() {
    let root = tempfile::tempdir().unwrap();
    let mut command = Command::cargo_bin("retold").unwrap();
    command
        .args(["--json", "providers", "scan"])
        .current_dir(root.path());
    command.env("HOME", root.path()).env("PATH", "");
    command.env("OPENROUTER_API_KEY", "SECRET-DO-NOT-ECHO");
    let assertion = command.assert().success();
    let output = assertion.get_output();
    assert_eq!(output.stderr, Vec::<u8>::new());
    let stdout = String::from_utf8(output.stdout.clone()).unwrap();
    assert_eq!(stdout.lines().count(), 1);
    assert!(!stdout.contains("SECRET-DO-NOT-ECHO"));
    let envelope: Value = serde_json::from_str(&stdout).unwrap();
    let inventory = &envelope["payload"]["data"]["inventory"];
    assert_eq!(inventory["authentication"], "not_checked");
    assert_eq!(inventory["credential_variables"][0]["present"], true);
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn remove_previews_applies_and_reports_missing_provider_offline() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("providers.toml");
    let source = "[providers.unused]\nkind = 'codex_cli'\nmax_parallel = 1\n[routing]\n";
    std::fs::write(&path, source).unwrap();
    for dry_run in [true, false] {
        let mut command = Command::cargo_bin("retold").unwrap();
        command
            .args(["--json", "providers", "remove", "unused", "--root"])
            .arg(root.path())
            .env("PATH", "");
        if dry_run {
            command.arg("--dry-run");
        }
        let assertion = command.assert().success();
        let output = assertion.get_output();
        assert_eq!(output.stderr, Vec::<u8>::new());
        let envelope: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(envelope["payload"]["data"]["removed"]["dry_run"], dry_run);
        if dry_run {
            assert_eq!(std::fs::read_to_string(&path).unwrap(), source);
            assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
        }
    }
    assert_missing(root.path());
}

#[expect(clippy::unwrap_used, reason = "fixture helper")]
fn assert_missing(root: &std::path::Path) {
    let mut command = Command::cargo_bin("retold").unwrap();
    command
        .args(["--json", "providers", "remove", "unused", "--root"])
        .arg(root)
        .env("PATH", "");
    command.assert().code(2);
}
