//! Provider login status inspection and credential profile updates.
#![expect(clippy::unwrap_used, reason = "fixture assertions")]
use assert_cmd::Command;
use serde_json::Value;
use std::path::Path;

fn invoke(root: &Path, id: &str, extra: &[&str]) -> Value {
    let mut command = Command::cargo_bin("retold").unwrap();
    command.args(["--json", "providers", "login", id, "--root"]);
    command.arg(root);
    command.args(extra);
    command.env("PATH", "");
    let assertion = command.assert().code(0);
    let output = assertion.get_output();
    assert_eq!(output.stderr, Vec::<u8>::new());
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn offline_cli_profile_presence_does_not_verify_authentication() {
    let root = tempfile::tempdir().unwrap();
    let config = "[providers.codex]\nkind = 'codex_cli'\nmax_parallel = 1\nconfig_dir = '/nonexistent/codex/path'\n[routing]\n";
    std::fs::write(root.path().join("providers.toml"), config).unwrap();

    let status = invoke(root.path(), "codex", &["--status"]);
    let payload = &status["payload"]["data"]["login_status"];
    assert_eq!(payload["id"], "codex");
    assert_eq!(payload["kind"], "codex_cli");
    assert_eq!(payload["status"], "missing_profile");
    assert!(!payload["authenticated"].as_bool().unwrap());

    let profile = tempfile::tempdir().unwrap();
    let updated = invoke(
        root.path(),
        "codex",
        &["--profile-dir", profile.path().to_str().unwrap()],
    );
    let result = &updated["payload"]["data"]["login_configured"];
    assert_eq!(result["id"], "codex");
    assert_eq!(result["status"]["status"], "profile_found");
    assert_eq!(result["status"]["authenticated"], false);
    assert_eq!(result["status"]["authentication_checked"], false);
}

#[test]
fn login_inspects_openrouter_credentials_without_leaking_secrets() {
    let root = tempfile::tempdir().unwrap();
    let config = "[providers.openrouter]\nkind = 'openrouter'\nkey_env = 'TEST_SECRET_KEY_VAR'\nmax_parallel = 2\n[routing]\n";
    std::fs::write(root.path().join("providers.toml"), config).unwrap();

    let status = invoke(root.path(), "openrouter", &[]);
    let payload = &status["payload"]["data"]["login_status"];
    assert_eq!(payload["id"], "openrouter");
    assert_eq!(payload["status"], "missing_credential");
    assert!(!payload["authenticated"].as_bool().unwrap());

    let mut command = Command::cargo_bin("retold").unwrap();
    command.args(["--json", "providers", "login", "openrouter", "--root"]);
    command.arg(root.path());
    command.env(
        "TEST_SECRET_KEY_VAR",
        "sk-top-secret-canary-never-expose-12345",
    );
    let output = command.output().unwrap();
    let output_str = String::from_utf8(output.stdout).unwrap();
    assert!(!output_str.contains("sk-top-secret-canary"));
    let envelope: Value = serde_json::from_str(&output_str).unwrap();
    let status = &envelope["payload"]["data"]["login_status"];
    assert_eq!(status["status"], "configured");
    assert_eq!(status["authenticated"], false);
    assert_eq!(status["authentication_checked"], false);
}

#[test]
fn login_dry_run_previews_changes_without_writing() {
    let root = tempfile::tempdir().unwrap();
    let config = "[providers.openrouter]\nkind = 'openrouter'\nkey_env = 'OLD_KEY'\nmax_parallel = 2\n[routing]\n";
    let target = root.path().join("providers.toml");
    std::fs::write(&target, config).unwrap();

    let dry_run = invoke(
        root.path(),
        "openrouter",
        &["--key-env", "NEW_KEY", "--dry-run"],
    );
    assert!(
        dry_run["payload"]["data"]["login_configured"]["dry_run"]
            .as_bool()
            .unwrap()
    );
    let current_content = std::fs::read_to_string(&target).unwrap();
    assert!(current_content.contains("OLD_KEY"));
    assert!(!current_content.contains("NEW_KEY"));
}

fn environment_profile(kind: &str, variable: &str) -> Value {
    let root = tempfile::tempdir().unwrap();
    let profile = root.path().join("environment-profile");
    std::fs::create_dir(&profile).unwrap();
    let source = format!(
        "[providers.cli]\nkind = '{kind}'\nmax_parallel = 1\ntiers = {{ smart = 'sonnet' }}\n[routing]\n"
    );
    std::fs::write(root.path().join("providers.toml"), source).unwrap();
    let mut command = profile_status_command(root.path(), variable, profile);
    let output = command.assert().success().get_output().stdout.clone();
    serde_json::from_slice(&output).unwrap()
}

fn profile_status_command(root: &Path, variable: &str, profile: std::path::PathBuf) -> Command {
    let mut command = Command::cargo_bin("retold").unwrap();
    command.args(["--json", "providers", "login", "cli", "--status", "--root"]);
    command.arg(root).env("PATH", "");
    command.env("HOME", root.join("empty-home"));
    command
        .env_remove("CODEX_HOME")
        .env_remove("CLAUDE_CONFIG_DIR");
    command.env(variable, profile);
    command
}

#[test]
fn offline_profile_inspection_matches_the_cli_environment_override() {
    for (kind, variable) in [
        ("codex_cli", "CODEX_HOME"),
        ("claude_cli", "CLAUDE_CONFIG_DIR"),
    ] {
        let response = environment_profile(kind, variable);
        let status = &response["payload"]["data"]["login_status"];
        let path = Path::new(status["profile_dir"].as_str().unwrap());
        assert_eq!(path.file_name().unwrap(), "environment-profile");
        assert_eq!(status["status"], "profile_found");
        assert_eq!(status["authenticated"], false);
        assert_eq!(status["authentication_checked"], false);
    }
}
