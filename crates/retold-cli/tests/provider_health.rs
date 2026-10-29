//! Health inspection is offline and read-only.
#![expect(clippy::unwrap_used, reason = "fixture helper")]
use assert_cmd::Command;
use retold_providers::health::Health;
const EXECUTION_FAILURE: i32 = 4;

fn invoke(root: &std::path::Path, format: &str, exit: i32) -> String {
    let mut command = Command::cargo_bin("retold").unwrap();
    command
        .args([format, "providers", "health", "--root"])
        .arg(root)
        .env("PATH", "");
    let assertion = command.assert().code(exit);
    let output = assertion.get_output();
    assert_eq!(output.stderr, Vec::<u8>::new());
    String::from_utf8(output.stdout.clone()).unwrap()
}

#[test]
fn unknown_health_is_never_claimed_as_healthy_and_inspection_writes_nothing() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("providers.toml"),
        "[providers.a]\nkind = 'codex_cli'\nmax_parallel = 1\n[routing]",
    )
    .unwrap();
    let json = invoke(root.path(), "--json", 0);
    assert_eq!(json.lines().count(), 1);
    assert!(json.contains("not_checked"));
    assert!(invoke(root.path(), "--plain", 0).contains("Recorded cooldowns only"));
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
    std::fs::write(root.path().join("provider-health.json"), "PRIVATE").unwrap();
    let error = invoke(root.path(), "--json", EXECUTION_FAILURE);
    assert!(!error.contains("PRIVATE"));
}

#[test]
fn health_inspection_renders_logged_out_and_failing_states() {
    let root = tempfile::tempdir().unwrap();
    let config_text = "[providers.a]\nkind = 'codex_cli'\nmax_parallel = 1\n[routing]\n";
    std::fs::write(root.path().join("providers.toml"), config_text).unwrap();
    let config = retold_engine::provider_settings::load(root.path()).unwrap();
    let ledger = retold_engine::provider_health::HealthLedger::new(root.path(), &config).unwrap();
    ledger
        .failed(
            "a",
            &retold_providers::ProviderError::Unavailable("please log in with /login".into()),
        )
        .unwrap();

    let plain = invoke(root.path(), "--plain", 0);
    assert!(plain.contains("a · logged out · authentication required"));

    let json = invoke(root.path(), "--json", 0);
    assert!(json.contains("\"status\":\"logged_out\""));
}
