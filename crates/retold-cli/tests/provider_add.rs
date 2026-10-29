//! Adding a provider stays noninteractive and offline.
#![expect(clippy::unwrap_used, reason = "fixture helper assertions")]
use assert_cmd::Command;
use serde_json::Value;

const PROVIDER_TOML: &str = "kind = 'codex_cli'\nmax_parallel = 2\n";

#[test]
fn preview_then_add_creates_only_the_validated_configuration() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("config");
    std::fs::create_dir(&root).unwrap();
    let definition = temporary.path().join("provider.toml");
    std::fs::write(&definition, PROVIDER_TOML).unwrap();
    let preview = invoke((&root, &definition), &["--dry-run"], 0);
    assert_eq!(preview["payload"]["data"]["added"]["dry_run"], true);
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
    let result = invoke((&root, &definition), &[], 0);
    assert_eq!(result["payload"]["data"]["added"]["id"], "personal");
    let duplicate = invoke((&root, &definition), &[], 2);
    assert_eq!(duplicate["payload"]["code"], "RG-INPUT-REQUIRED");
}

fn invoke(paths: (&std::path::Path, &std::path::Path), flags: &[&str], exit: i32) -> Value {
    let (root, definition) = paths;
    let mut command = Command::cargo_bin("retold").unwrap();
    command.args(["--json", "providers", "add", "personal", "--root"]);
    command.arg(root).arg("--file").arg(definition);
    command.env("PATH", "");
    command.args(flags);
    let assertion = command.assert().code(exit);
    let output = assertion.get_output();
    assert_eq!(output.stderr, Vec::<u8>::new());
    let stdout = String::from_utf8(output.stdout.clone()).unwrap();
    assert_eq!(stdout.lines().count(), 1);
    serde_json::from_str(&stdout).unwrap()
}

#[test]
fn invalid_provider_definition_is_redacted_and_creates_no_files() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("config");
    std::fs::create_dir(&root).unwrap();
    let definition = temporary.path().join("provider.toml");
    std::fs::write(&definition, "key = 'SECRET-DO-NOT-ECHO'").unwrap();
    let error = invoke((&root, &definition), &[], 2);
    assert!(!error.to_string().contains("SECRET-DO-NOT-ECHO"));
    assert_eq!(std::fs::read_dir(root).unwrap().count(), 0);
}
