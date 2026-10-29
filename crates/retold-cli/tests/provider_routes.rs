//! Route management never starts provider binaries or requests.
#![expect(clippy::unwrap_used, reason = "fixture helpers")]
use assert_cmd::Command;
use serde_json::Value;

const SOURCE: &str = "[providers.codex]\nkind = 'codex_cli'\nmax_parallel = 1\n[routing]\n\"text.smart\" = ['codex:smart']\n";

#[test]
fn inspect_preview_and_apply_work_without_provider_binaries() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("providers.toml"), SOURCE).unwrap();
    let shown = invoke(root.path(), &[], 0);
    assert_eq!(
        shown["payload"]["data"]["routing"]["text.smart"][0],
        "codex:smart"
    );
    let preview = invoke(
        root.path(),
        &["text.fast", "--order", "codex:fast", "--dry-run"],
        0,
    );
    assert_eq!(preview["payload"]["data"]["route"]["dry_run"], true);
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
    invoke(root.path(), &["text.fast", "--order", "codex:fast"], 0);
    let shown = invoke(root.path(), &[], 0);
    assert_eq!(
        shown["payload"]["data"]["routing"]["text.fast"][0],
        "codex:fast"
    );
}

#[test]
fn incomplete_orders_and_incompatible_tiers_are_terminal_errors() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("providers.toml"), SOURCE).unwrap();
    for arguments in [
        vec!["text.fast"],
        vec!["--order", "codex:fast"],
        vec!["--dry-run"],
        vec!["tts", "--order", "codex:default", "--dry-run"],
    ] {
        let error = invoke(root.path(), &arguments, 2);
        assert_eq!(error["payload"]["code"], "RG-INPUT-REQUIRED");
    }
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
}

fn invoke(root: &std::path::Path, arguments: &[&str], exit: i32) -> Value {
    let mut command = Command::cargo_bin("retold").unwrap();
    command
        .args(["--json", "providers", "route", "--root"])
        .arg(root);
    command.args(arguments).env("PATH", "");
    let assertion = command.assert().code(exit);
    let output = assertion.get_output();
    assert_eq!(output.stderr, Vec::<u8>::new());
    let stdout = String::from_utf8(output.stdout.clone()).unwrap();
    assert_eq!(stdout.lines().count(), 1);
    serde_json::from_str(&stdout).unwrap()
}

#[test]
fn plain_inspection_has_readable_routes_and_does_not_modify_files() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("providers.toml"), SOURCE).unwrap();
    let mut command = Command::cargo_bin("retold").unwrap();
    command
        .args(["--plain", "providers", "route", "--root"])
        .arg(root.path());
    command.env("PATH", "");
    let assertion = command.assert().success();
    let stdout = String::from_utf8(assertion.get_output().stdout.clone()).unwrap();
    assert!(stdout.contains("text.smart: codex:smart"));
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
}
