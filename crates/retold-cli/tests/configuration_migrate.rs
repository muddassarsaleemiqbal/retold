//! Configuration migration CLI tests; verifies backups, idempotency and rollback.
#![expect(clippy::unwrap_used, reason = "fixture helpers")]
use assert_cmd::Command;
use std::path::{Path, PathBuf};

const EXIT_SUCCESS: i32 = 0;
const LEGACY_CONFIG: &str = "[providers.local]\nkind = 'codex_cli'\nmax_parallel = 1\n[routing]\n";

struct Fixture {
    root: tempfile::TempDir,
}

impl Fixture {
    fn new(initial_content: &str) -> Self {
        let root = tempfile::tempdir().unwrap();
        let config_path = root.path().join("providers.toml");
        std::fs::write(config_path, initial_content).unwrap();
        Self { root }
    }

    fn path(&self) -> &Path {
        self.root.path()
    }

    fn providers_path(&self) -> PathBuf {
        self.root.path().join("providers.toml")
    }

    fn invoke(&self, args: &[&str]) -> (String, i32) {
        let mut cmd = Command::cargo_bin("retold").unwrap();
        cmd.args(args);
        cmd.env("PATH", "");
        cmd.env("RETOLD_CONFIG_DIR", self.path());
        let output = cmd.output().unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        let status = output.status.code().unwrap();
        (stdout, status)
    }

    fn invoke_json(&self, args: &[&str]) -> (serde_json::Value, i32) {
        let mut all_args = vec!["--json"];
        all_args.extend_from_slice(args);
        let (stdout, code) = self.invoke(&all_args);
        (serde_json::from_str(&stdout).unwrap(), code)
    }
}

#[test]
fn config_migrate_upgrades_legacy_schema_with_backup() {
    let fix = Fixture::new(LEGACY_CONFIG);
    let root_str = fix.path().to_str().unwrap();

    let (json_output, code) = fix.invoke_json(&["config", "migrate", "--root", root_str]);
    assert_eq!(code, EXIT_SUCCESS);
    assert_eq!(json_output["payload"]["outcome"], "completed");
    let result = &json_output["payload"]["data"];
    assert_eq!(result["migrated"], true);
    assert_eq!(result["from_version"], 0);
    assert_eq!(result["to_version"], 1);
    assert!(result["backup"].is_string());

    let migrated_text = std::fs::read_to_string(fix.providers_path()).unwrap();
    assert!(migrated_text.contains("schema_version = 1"));

    let backup_path = PathBuf::from(result["backup"].as_str().unwrap());
    assert!(backup_path.exists());
    let backup_text = std::fs::read_to_string(backup_path).unwrap();
    assert_eq!(backup_text, LEGACY_CONFIG);
}

#[test]
fn config_migrate_repeated_is_idempotent() {
    let fix = Fixture::new(LEGACY_CONFIG);
    let root_str = fix.path().to_str().unwrap();

    let (_first, code1) = fix.invoke_json(&["config", "migrate", "--root", root_str]);
    assert_eq!(code1, EXIT_SUCCESS);

    let (json_output, code2) = fix.invoke_json(&["config", "migrate", "--root", root_str]);
    assert_eq!(code2, EXIT_SUCCESS);
    let result = &json_output["payload"]["data"];
    assert_eq!(result["migrated"], false);
    assert_eq!(result["to_version"], 1);

    let (human_output, code_human) = fix.invoke(&["config", "migrate", "--root", root_str]);
    assert_eq!(code_human, EXIT_SUCCESS);
    assert!(human_output.contains("Configuration is already at schema version 1."));
}

#[test]
fn config_migrate_dry_run_leaves_files_unmodified() {
    let fix = Fixture::new(LEGACY_CONFIG);
    let root_str = fix.path().to_str().unwrap();

    let (json_output, code) =
        fix.invoke_json(&["config", "migrate", "--root", root_str, "--dry-run"]);
    assert_eq!(code, EXIT_SUCCESS);
    let result = &json_output["payload"]["data"];
    assert_eq!(result["migrated"], true);
    assert_eq!(result["dry_run"], true);
    assert!(result["backup"].is_null());

    let unchanged_text = std::fs::read_to_string(fix.providers_path()).unwrap();
    assert_eq!(unchanged_text, LEGACY_CONFIG);
}

#[test]
fn config_migrate_cleans_up_stale_staging_files() {
    let fix = Fixture::new(LEGACY_CONFIG);
    let stale_tmp = fix.path().join(".providers.toml.tmp-stale");
    std::fs::write(&stale_tmp, "stale").unwrap();
    let root_str = fix.path().to_str().unwrap();

    let (_, code) = fix.invoke_json(&["config", "migrate", "--root", root_str]);
    assert_eq!(code, EXIT_SUCCESS);
    assert!(!stale_tmp.exists());
}
