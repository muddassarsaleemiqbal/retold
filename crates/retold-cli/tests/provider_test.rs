//! `providers test` shows its plan first, needs confirmed account authority and records health.
//! The only provider that answers is a local fake completion script; nothing leaves the machine.
#![cfg(unix)]
#![expect(
    clippy::unwrap_used,
    reason = "fixture helpers must fail on invalid test setup or protocol"
)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use serde_json::{Value, json};

const BLOCKED: i32 = 3;
const EXECUTION_FAILURE: i32 = 4;
const STALE_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";
const ANSWERS: &str = "#!/bin/sh\necho '{\"ok\": true}'\n";
const FAILS: &str = "#!/bin/sh\necho 'model missing' >&2\nexit 1\n";

struct Installation {
    sandbox: tempfile::TempDir,
}

impl Installation {
    /// A local text provider backed by `script`, and an unconfigured per-use provider.
    fn new(script: &str) -> Self {
        let sandbox = tempfile::tempdir().unwrap();
        write_settings(&sandbox.path().join("settings"), script);
        Self { sandbox }
    }

    fn root(&self) -> PathBuf {
        self.sandbox.path().join("settings")
    }

    fn command(&self, format: &str, args: &[&str]) -> Command {
        let mut command = Command::cargo_bin("retold").unwrap();
        command.env("HOME", self.sandbox.path().join("home"));
        command.env("RETOLD_CONFIG_DIR", self.root());
        command
            .env_remove("XDG_DATA_HOME")
            .env_remove("FIXTURE_KEY");
        command.arg(format).args(args);
        command
    }

    fn retold(&self, args: &[&str]) -> (Value, i32) {
        let output = self.command("--json", args).output().unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert_eq!(stdout.lines().count(), 1, "{stdout}");
        (
            serde_json::from_str(&stdout).unwrap(),
            output.status.code().unwrap(),
        )
    }

    fn test(&self, extra: &[&str]) -> (Value, i32) {
        let root = self.root();
        let mut args = vec!["providers", "test", "--root", root.to_str().unwrap()];
        args.extend_from_slice(extra);
        self.retold(&args)
    }

    fn plain(&self, extra: &[&str]) -> String {
        let root = self.root();
        let mut args = vec!["providers", "test", "--root", root.to_str().unwrap()];
        args.extend_from_slice(extra);
        let output = self.command("--plain", &args).output().unwrap();
        String::from_utf8(output.stdout).unwrap()
    }

    fn initialize_account(&self) {
        let (_, exit) = self.retold(&["account", "init", "--timezone", "UTC"]);
        assert_eq!(exit, 0);
    }

    fn jobs(&self) -> u64 {
        let (show, exit) = self.retold(&["account", "show"]);
        assert_eq!(exit, 0, "{show}");
        show["payload"]["data"]["jobs"].as_u64().unwrap()
    }
}

const OWNER_ONLY_MODE: u32 = 0o700;

/// Provider settings, a fake completion binary and a placeholder model in `root`.
fn write_settings(root: &Path, script: &str) {
    let binary = file(root, "completion", script.as_bytes());
    make_executable(&binary);
    let model = file(root, "fixture.gguf", b"fixture, not a real model");
    file(root, "providers.toml", settings(&binary, &model).as_bytes());
}

fn file(root: &Path, name: &str, contents: &[u8]) -> PathBuf {
    std::fs::create_dir_all(root).unwrap();
    let path = root.join(name);
    std::fs::write(&path, contents).unwrap();
    path
}

fn make_executable(path: &Path) {
    let permissions = std::fs::Permissions::from_mode(OWNER_ONLY_MODE);
    std::fs::set_permissions(path, permissions).unwrap();
}

fn settings(binary: &Path, model: &Path) -> String {
    format!(
        "schema_version = 1\n\
         [providers.local]\nkind = \"local_text\"\nbinary = '{}'\nmax_parallel = 1\n\
         [providers.local.models]\nm = '{}'\n\
         [providers.paid]\nkind = \"openrouter\"\nkey_env = \"FIXTURE_KEY\"\nmax_parallel = 1\n\
         [routing]\n\"text.local\" = [\"local:m\"]\n\"text.smart\" = [\"paid:vendor/model\"]\n",
        binary.display(),
        model.display()
    )
}

fn files(root: &Path) -> usize {
    std::fs::read_dir(root).unwrap().count()
}

fn plan(installation: &Installation, extra: &[&str]) -> Value {
    let (plan, exit) = installation.test(extra);
    assert_eq!(exit, 0, "{plan}");
    plan["payload"]["data"]["test_plan"].clone()
}

fn code(envelope: &Value) -> &str {
    envelope["payload"]["code"].as_str().unwrap()
}

#[test]
fn tests_need_an_account_and_a_current_plan() {
    let installation = Installation::new(ANSWERS);
    let before = files(&installation.root());
    let unaccounted = plan(&installation, &[]);
    assert_eq!(unaccounted["blockers"][0]["code"], "RG-ACCOUNT-REQUIRED");
    let (refused, exit) = installation.test(&["--yes"]);
    assert_eq!((code(&refused), exit), ("RG-ACCOUNT-REQUIRED", BLOCKED));
    installation.initialize_account();
    let (stale, exit) = installation.test(&["--plan-hash", STALE_HASH]);
    assert_eq!((code(&stale), exit), ("RG-PLAN-CHANGED", BLOCKED));
    assert_eq!(installation.jobs(), 0);
    assert_eq!(files(&installation.root()), before, "plans write nothing");
}

#[test]
fn a_confirmed_free_test_admits_one_job_and_records_health() {
    let installation = Installation::new(ANSWERS);
    installation.initialize_account();
    let reviewed = plan(&installation, &[]);
    assert_eq!(reviewed["blockers"], json!([]));
    let probes = &reviewed["authority"]["probes"];
    assert_eq!(probes[0]["decision"], "test");
    assert_eq!(probes[1]["reason"], "needs_spend");
    let hash = reviewed["plan_hash"].as_str().unwrap();
    let (tested, exit) = installation.test(&["--plan-hash", hash]);
    assert_eq!(exit, 0, "{tested}");
    let results = &tested["payload"]["data"]["tested"]["results"];
    assert_eq!(results[0]["status"], "ok");
    assert_eq!(results[1]["status"], "skipped");
    assert_eq!(installation.jobs(), 1);
    let root = installation.root();
    let health = ["providers", "health", "--root", root.to_str().unwrap()];
    let (health, _) = installation.retold(&health);
    assert_eq!(health["payload"]["data"]["health"][0]["status"], "ok");
}

fn expire_fixture_cooldown(root: &Path) {
    let path = root.join("provider-health.json");
    let mut state: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    for record in state["records"].as_object_mut().unwrap().values_mut() {
        record["kind"] = "limited".into();
        record["observed_at"] = 0.into();
        record["retry_at"] = 1.into();
    }
    std::fs::write(path, serde_json::to_vec(&state).unwrap()).unwrap();
}

#[test]
fn a_free_local_test_records_recovery_after_an_expired_cooldown() {
    let installation = Installation::new(ANSWERS);
    installation.initialize_account();
    let (_, exit) = installation.test(&["local", "--yes"]);
    assert_eq!(exit, 0);
    expire_fixture_cooldown(&installation.root());
    let (tested, exit) = installation.test(&["local", "--yes"]);
    assert_eq!(exit, 0, "{tested}");
    assert_eq!(
        tested["payload"]["data"]["tested"]["results"][0]["status"],
        "ok"
    );
    let root = installation.root();
    let (health, exit) =
        installation.retold(&["providers", "health", "--root", root.to_str().unwrap()]);
    assert_eq!(exit, 0, "{health}");
    assert_eq!(health["payload"]["data"]["health"][0]["status"], "ok");
    assert_eq!(
        health["payload"]["data"]["health"][0]["observation"]["retry_at"],
        0
    );
    assert_eq!(installation.jobs(), 2);
}

#[test]
fn paid_tests_need_spend_and_provider_scoped_limits() {
    let installation = Installation::new(ANSWERS);
    installation.initialize_account();
    let unpaid = plan(&installation, &["paid"]);
    assert_eq!(unpaid["authority"]["requests"], 0);
    let paid = plan(&installation, &["paid", "--spend", "1"]);
    assert_eq!(paid["authority"]["requests"], 1);
    let missing = &paid["blockers"][0]["context"]["missing_limits"];
    let provider = json!({
        "scope": {"kind": "provider", "id": "openrouter:FIXTURE_KEY"},
        "window": "lifetime",
    });
    assert!(missing.as_array().unwrap().contains(&provider), "{missing}");
    assert_ne!(paid["plan_hash"], unpaid["plan_hash"]);
    let (refused, exit) = installation.test(&["paid", "--spend", "1", "--yes"]);
    assert_eq!((code(&refused), exit), ("RG-ACCOUNT-REQUIRED", BLOCKED));
    assert_eq!(installation.jobs(), 0);
    let (unknown, exit) = installation.test(&["nobody"]);
    assert_eq!((code(&unknown), exit), ("RG-INPUT-REQUIRED", 2));
}

#[test]
fn failing_providers_fail_the_command_with_their_results() {
    let installation = Installation::new(FAILS);
    installation.initialize_account();
    let (failed, exit) = installation.test(&["local", "--yes"]);
    assert_eq!(
        (code(&failed), exit),
        ("RG-EXECUTION-FAILED", EXECUTION_FAILURE)
    );
    assert_eq!(
        failed["payload"]["context"]["results"][0]["status"],
        "failing"
    );
    let plain = installation.plain(&["local", "--yes"]);
    assert!(plain.contains("local · failing"), "{plain}");
    let shown = installation.plain(&["local"]);
    assert!(
        shown.contains("local · local · test text.local (m)"),
        "{shown}"
    );
    assert!(
        shown.contains("Nothing is sent until you confirm"),
        "{shown}"
    );
}
