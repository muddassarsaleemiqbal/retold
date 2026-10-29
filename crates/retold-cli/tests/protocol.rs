//! Closed-stdin acceptance tests in an empty workspace, with no provider configuration.

#![expect(
    clippy::unwrap_used,
    reason = "fixture helper assertions fail on malformed protocol output"
)]

use assert_cmd::Command;
use serde_json::Value;

fn invoke(args: &[&str], exit: i32) -> Value {
    let root = tempfile::tempdir().unwrap();
    let mut command = isolated_command(root.path());
    let assertion = command.args(args).assert().code(exit);
    let output = assertion.get_output().clone();
    assert_eq!(output.stderr, Vec::<u8>::new());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout.lines().count(), 1);
    assert!(!stdout.contains('\u{1b}'));
    let value: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert!(value["request_id"].as_str().unwrap().starts_with("req-"));
    assert!(value.get("job_id").is_none());
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    value
}

fn isolated_command(root: &std::path::Path) -> Command {
    let mut command = Command::cargo_bin("retold").unwrap();
    command.current_dir(root).env("NO_COLOR", "1");
    command.env("RETOLD_CONFIG_DIR", root.join("config"));
    command.env("RETOLD_LIBRARY", root.join("library"));
    command.env_remove("OPENROUTER_API_KEY");
    command
}

#[test]
fn bare_json_is_one_completed_usage_result() {
    let value = invoke(&["--json"], 0);
    assert_eq!(value["type"], "result");
}

#[test]
fn parser_errors_are_terminal_envelopes_in_either_flag_order() {
    for args in [
        vec!["--json", "--plain"],
        vec!["--plain", "--json"],
        vec!["--json=false"],
    ] {
        let value = invoke(&args, 2);
        assert_eq!(value["type"], "error");
        assert_eq!(value["payload"]["code"], "RG-INPUT-REQUIRED");
        assert_eq!(value["payload"]["suggested_actions"][0][0], "retold");
    }
}

#[test]
fn unavailable_actions_never_dispatch_or_create_files() {
    let value = invoke(
        &["--json", "publish", "--yes"],
        i32::from(retold_app::protocol::EXIT_BLOCKED),
    );
    assert_eq!(value["payload"]["code"], "RG-NOT-AVAILABLE");
    assert_eq!(value["payload"]["outcome"], "blocked");
}

#[test]
fn help_version_and_quiet_keep_exactly_one_terminal_result() {
    for args in [
        vec!["--json", "--help"],
        vec!["--version", "--json"],
        vec!["--json", "--quiet"],
    ] {
        assert_eq!(invoke(&args, 0)["type"], "result");
    }
}

#[test]
fn schema_describes_envelopes_and_typed_engine_failures() {
    let value = invoke(&["--json", "schema"], 0);
    let schema = value["payload"]["data"].to_string();
    assert!(schema.contains("schema_version"));
    assert!(schema.contains("step_failed"));
    assert!(schema.contains("suggested_actions"));
}

#[test]
fn explain_uses_shared_diagnostics_and_rejects_unknown_codes() {
    let value = invoke(&["explain", "RG-EXECUTION-FAILED", "--json"], 0);
    assert_eq!(value["payload"]["data"]["code"], "RG-EXECUTION-FAILED");
    assert_eq!(
        invoke(&["--json", "explain", "unknown"], 2)["type"],
        "error"
    );
}

#[test]
fn plain_bare_invocation_and_help_finish_without_prompting() {
    for args in [vec![], vec!["--plain"], vec!["--plain", "--help"]] {
        let mut command = Command::cargo_bin("retold").unwrap();
        command.args(args).assert().success();
    }
}

#[test]
fn parser_error_does_not_echo_secret_arguments() {
    let value = invoke(&["--json", "--unknown-secret=canary-secret"], 2);
    assert!(!value.to_string().contains("canary-secret"));
}

#[test]
fn account_preview_is_nonmutating_and_library_independent() {
    let value = invoke(
        &[
            "--json",
            "account",
            "init",
            "--dry-run",
            "--timezone",
            "Asia/Karachi",
        ],
        0,
    );
    assert_eq!(value["payload"]["data"]["dry_run"], true);
    assert_eq!(value["payload"]["data"]["default_limits"], "zero");
    assert_eq!(value["payload"]["data"]["timezone"], "Asia/Karachi");
    let directory = value["payload"]["data"]["directory"].as_str().unwrap();
    assert!(!directory.contains("library"));
}

#[test]
fn account_rejects_invalid_timezone_and_ledger_path_override() {
    let value = invoke(
        &[
            "--json",
            "account",
            "init",
            "--dry-run",
            "--timezone",
            "invalid-zone",
        ],
        2,
    );
    assert_eq!(value["payload"]["code"], "RG-INPUT-REQUIRED");
    let value = invoke(&["--json", "account", "show", "--ledger", "alternate"], 2);
    assert_eq!(value["payload"]["code"], "RG-INPUT-REQUIRED");
}

#[test]
fn completions_generation_supports_canonical_shells() {
    for shell in ["bash", "elvish", "fish", "powershell", "zsh"] {
        let value = invoke(&["--json", "completions", shell], 0);
        assert_eq!(value["type"], "result");
        assert_eq!(value["payload"]["data"]["shell"], shell);
        assert!(value["payload"]["data"]["text"].is_string());
    }
}

#[test]
fn man_generation_emits_valid_roff() {
    let value = invoke(&["--json", "man"], 0);
    assert_eq!(value["type"], "result");
    assert_eq!(value["payload"]["data"]["format"], "roff");
    assert!(
        value["payload"]["data"]["text"]
            .as_str()
            .unwrap()
            .contains(".TH retold 1")
    );
}

#[test]
fn transitional_alias_uses_the_current_protocol_and_command_surface() {
    let root = tempfile::tempdir().unwrap();
    for args in [
        ["--json", "schema"],
        ["--json", "--help"],
        ["--json", "make"],
    ] {
        let expected = if args[1] == "make" { 2 } else { 0 };
        let current = named_protocol("retold", root.path(), &args, expected);
        let alias = named_protocol("retold-next", root.path(), &args, expected);
        assert_eq!(alias["payload"], current["payload"]);
        assert_eq!(alias["type"], current["type"]);
    }
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

fn named_protocol(name: &str, root: &std::path::Path, args: &[&str], exit: i32) -> Value {
    let mut command = Command::cargo_bin(name).unwrap();
    command.current_dir(root).env("NO_COLOR", "1");
    command.env("RETOLD_CONFIG_DIR", root.join("config"));
    command.env("RETOLD_LIBRARY", root.join("library"));
    let assertion = command.args(args).assert().code(exit);
    let output = assertion.get_output();
    assert_eq!(output.stderr, Vec::<u8>::new());
    serde_json::from_slice(&output.stdout).unwrap()
}
