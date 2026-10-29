//! Configured provider inspection works offline with predictable terminal output.
#![expect(clippy::unwrap_used, reason = "fixture assertions")]
use assert_cmd::Command;
use serde_json::Value;
use std::path::Path;

fn invoke(root: &Path, mode: &str, exit: i32) -> String {
    let mut command = Command::cargo_bin("retold").unwrap();
    command
        .args([mode, "providers", "list", "--root"])
        .arg(root)
        .env("PATH", "");
    let assertion = command.assert().code(exit);
    let output = assertion.get_output();
    assert_eq!(output.stderr, Vec::<u8>::new());
    String::from_utf8(output.stdout.clone()).unwrap()
}

#[test]
fn provider_list_has_plain_and_json_empty_states_and_redacted_errors() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("providers.toml");
    std::fs::write(&target, "[providers]\n[routing]").unwrap();
    let plain = invoke(root.path(), "--plain", 0);
    assert!(plain.contains("No providers configured"));
    let stdout = invoke(root.path(), "--json", 0);
    assert_eq!(stdout.lines().count(), 1);
    let envelope: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(
        envelope["payload"]["data"]["providers"],
        serde_json::json!([])
    );
    std::fs::write(&target, "PRIVATE-INVALID").unwrap();
    let error = invoke(root.path(), "--json", 2);
    assert!(!error.contains("PRIVATE"));
}

#[test]
fn plain_provider_labels_cannot_inject_terminal_controls() {
    let root = tempfile::tempdir().unwrap();
    let source = "[providers.\"bad\\u001b[31m\"]\nkind = 'codex_cli'\nmax_parallel = 2\nconfig_dir = '/PRIVATE'\n[routing]";
    std::fs::write(root.path().join("providers.toml"), source).unwrap();
    let plain = invoke(root.path(), "--plain", 0);
    assert!(!plain.contains('\u{1b}'));
    assert!(!plain.contains("PRIVATE"));
    assert!(plain.contains("codex_cli"));
    assert!(plain.contains("health not checked"));
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
}
