//! Validation stays offline and preserves both valid and invalid source bytes.
#![expect(clippy::unwrap_used, reason = "fixture assertions")]
use assert_cmd::Command;
use serde_json::Value;

fn validate(source: &str, exit: i32) -> Value {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("providers.toml");
    std::fs::write(&path, source).unwrap();
    let mut command = Command::cargo_bin("retold").unwrap();
    command
        .args(["--json", "config", "validate", "--root"])
        .arg(root.path())
        .env("PATH", "");
    let assertion = command.assert().code(exit);
    let output = assertion.get_output().clone();
    assert_eq!(output.stderr, Vec::<u8>::new());
    assert_eq!(std::fs::read_to_string(path).unwrap(), source);
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout.lines().count(), 1);
    serde_json::from_str(&stdout).unwrap()
}

#[test]
fn typed_configuration_is_checked_without_binaries_or_writes() {
    let value = validate("schema_version = 1\n[providers]\n[routing]\n", 0);
    assert_eq!(value["payload"]["data"]["valid"], true);
}

#[test]
fn rejects_future_unknown_and_invalid_route_configuration_without_echoing_secrets() {
    for source in [
        "schema_version = 99\n[providers]\n[routing]\n",
        "secret = 'DO-NOT-PRINT'\n[providers]\n[routing]\n",
        "[providers]\n[routing]\nsmart = []\n",
    ] {
        let value = validate(source, 2);
        assert_eq!(value["type"], "error");
        assert!(!value.to_string().contains("DO-NOT-PRINT"));
    }
}
