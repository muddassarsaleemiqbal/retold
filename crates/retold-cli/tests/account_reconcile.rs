//! Account reconciliation CLI integration tests; no dispatch authority is ever granted.
#![expect(clippy::unwrap_used, reason = "fixture helpers")]
use assert_cmd::Command;
use retold_app::account::{Account, types::*};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const CEILING: u64 = 10;
const MICROS_CAP: u64 = 10_000_000;
const HELD_MICROS: u64 = 500_000;
const PREVIEW_COST: i64 = 2_500_000;

struct Fixture {
    home: tempfile::TempDir,
    account_dir: PathBuf,
}

fn fixture() -> Fixture {
    let home = tempfile::tempdir().unwrap();
    let account_dir = home.path().join(if cfg!(target_os = "macos") {
        "Library/Application Support/retold-account"
    } else {
        ".local/share/retold-account"
    });
    let mut init = Command::cargo_bin("retold").unwrap();
    init.env("HOME", home.path());
    init.env_remove("XDG_DATA_HOME");
    let assertion = init.args(["account", "init", "--timezone", "UTC"]).assert();
    assertion.success();
    Fixture { home, account_dir }
}

fn command(home: &Path) -> Command {
    let mut command = Command::cargo_bin("retold").unwrap();
    command.env("HOME", home);
    command.env_remove("XDG_DATA_HOME");
    command.env("PATH", "");
    command
}

fn invoke(home: &Path, args: &[&str]) -> (serde_json::Value, i32) {
    let mut cmd = command(home);
    cmd.arg("--json");
    cmd.args(args);
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let status = output.status.code().unwrap();
    (serde_json::from_str(&stdout).unwrap(), status)
}

fn invoke_human(home: &Path, args: &[&str]) -> (String, i32) {
    let mut cmd = command(home);
    cmd.args(args);
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let status = output.status.code().unwrap();
    (stdout, status)
}

fn build_limits(workspace_id: &str) -> Vec<Limit> {
    let resources = BTreeMap::from([(Unit::Requests, CEILING), (Unit::UsdMicros, MICROS_CAP)]);
    vec![
        Limit {
            scope: Scope::Account,
            window: Window::Day,
            resources: resources.clone(),
        },
        Limit {
            scope: Scope::Account,
            window: Window::Month,
            resources: resources.clone(),
        },
        Limit {
            scope: Scope::Workspace(workspace_id.into()),
            window: Window::Lifetime,
            resources: resources.clone(),
        },
        Limit {
            scope: Scope::Video("video".into()),
            window: Window::Lifetime,
            resources,
        },
    ]
}

fn create_job(workspace: Workspace, attempt_id: &str, plan_hash: String) -> Job {
    let inputs_hash = retold_engine::store::hash(&"inputs").unwrap();
    let job_id = format!("job-{attempt_id}");
    let resources = BTreeMap::from([(Unit::Requests, CEILING), (Unit::UsdMicros, MICROS_CAP)]);
    let op_resources = BTreeMap::from([(Unit::Requests, 1), (Unit::UsdMicros, HELD_MICROS)]);
    Job {
        id: job_id,
        workspace,
        video_id: "video".into(),
        series_id: None,
        plan_hash,
        input_versions: BTreeMap::from([("manifest".into(), inputs_hash)]),
        action: Action::Continue,
        routes: ["route".into()].into(),
        capabilities: vec![Capability::Text],
        resources,
        operations: BTreeMap::from([(
            retold_engine::store::hash(&attempt_id).unwrap(),
            op_resources,
        )]),
        deadline: None,
    }
}

fn seed_attempt(account_dir: &Path, attempt_id: &str) {
    let account = Account::open(account_dir).unwrap();
    let canonical = account_dir.canonicalize().unwrap();
    let workspace = Workspace {
        id: "test-workspace".into(),
        root: canonical,
    };
    let _ = account.register_workspace(workspace.clone());
    let _ = account.register_route("route", "provider");
    let _ = account.set_limits(build_limits("test-workspace"));
    let plan_hash = retold_engine::store::hash(&"plan").unwrap();
    let job = create_job(workspace, attempt_id, plan_hash.clone());
    let job_id = job.id.clone();
    account.admit(job).unwrap();
    let reservation = Reservation {
        attempt_id: attempt_id.into(),
        job_id,
        operation_key: retold_engine::store::hash(&attempt_id).unwrap(),
        node: None,
        route: "route".into(),
        capability: Capability::Text,
        plan_hash: plan_hash.clone(),
        resources: BTreeMap::from([(Unit::Requests, 1), (Unit::UsdMicros, HELD_MICROS)]),
    };
    account.reserve(reservation).unwrap();
    account.mark_dispatched(attempt_id, &plan_hash).unwrap();
}

#[test]
fn account_reconcile_json_and_human_formats() {
    let fix = fixture();
    assert_account_reconcile_json(&fix);
    assert_account_reconcile_human(&fix);
}

fn assert_account_reconcile_json(fix: &Fixture) {
    seed_attempt(&fix.account_dir, "attempt-101");
    let (json_output, exit_code) = invoke(
        fix.home.path(),
        &[
            "account",
            "reconcile",
            "--attempt",
            "attempt-101",
            "--cost",
            "2.50",
        ],
    );
    assert_eq!(exit_code, 0);
    assert_eq!(json_output["payload"]["outcome"], "completed");
    let result = &json_output["payload"]["data"];
    assert_eq!(result["reconciled"], true);
    assert_eq!(result["attempt_id"], "attempt-101");
    assert_eq!(result["cost_formatted"], "$2.50");
    assert_eq!(result["cost_usd_micros"], 2_500_000);
    assert_eq!(result["dispatch_authorized"], false);
}

fn assert_account_reconcile_human(fix: &Fixture) {
    seed_attempt(&fix.account_dir, "attempt-102");
    let (human_output, exit_code) = invoke_human(
        fix.home.path(),
        &[
            "account",
            "reconcile",
            "--attempt",
            "attempt-102",
            "--cost",
            "1.75",
        ],
    );
    assert_eq!(exit_code, 0);
    assert!(human_output.contains("Reconciled account attempt `attempt-102` at $1.75."));
    assert!(human_output.contains("No dispatch authority granted."));
}

#[test]
fn account_reconcile_rejects_invalid_inputs() {
    let fix = fixture();
    let (_output, exit_code) = invoke(
        fix.home.path(),
        &[
            "account",
            "reconcile",
            "--attempt",
            "nonexistent",
            "--cost",
            "not-a-number",
        ],
    );
    assert_ne!(exit_code, 0);

    let (_output, exit_code) = invoke(
        fix.home.path(),
        &[
            "account",
            "reconcile",
            "--attempt",
            "nonexistent",
            "--cost",
            "1.00",
        ],
    );
    assert_ne!(exit_code, 0);
}

#[test]
fn reconciliation_preview_preserves_database_and_matches_actual_totals() {
    let fix = fixture();
    seed_attempt(&fix.account_dir, "preview");
    let database = fix.account_dir.join("accounting.sqlite");
    let before = std::fs::read(&database).unwrap();
    let args = [
        "account",
        "reconcile",
        "--attempt",
        "preview",
        "--cost",
        "2.50",
        "--dry-run",
    ];
    let (output, status) = invoke(fix.home.path(), &args);
    assert_eq!(status, 0);
    let preview = &output["payload"]["data"]["reconcile_preview"];
    assert_eq!(preview["dry_run"], true);
    assert_eq!(preview["cost_usd_micros"], PREVIEW_COST);
    let (human, status) = invoke_human(fix.home.path(), &args);
    assert_eq!(status, 0);
    assert!(human.contains("Charged: $0.00 → $2.50"));
    assert!(human.contains("Held: $0.50 → $0.00"));
    assert!(human.contains("Dry run; nothing was written."));
    assert_eq!(std::fs::read(&database).unwrap(), before);
    assert_preview_matches_reconciliation(&fix, preview);
}

fn assert_preview_matches_reconciliation(fix: &Fixture, preview: &serde_json::Value) {
    let account = Account::open(&fix.account_dir).unwrap();
    account
        .reconcile_attempt("preview", Usd::from_micros(PREVIEW_COST))
        .unwrap();
    let actual = serde_json::to_value(account.summary().unwrap()).unwrap();
    assert_eq!(preview["after"], actual["lifetime"]);
}

#[test]
fn invalid_preview_preserves_database() {
    let fix = fixture();
    let database = fix.account_dir.join("accounting.sqlite");
    let before = std::fs::read(&database).unwrap();
    let (_, status) = invoke(
        fix.home.path(),
        &[
            "account",
            "reconcile",
            "--attempt",
            "missing",
            "--cost",
            "1",
            "--dry-run",
        ],
    );
    assert_ne!(status, 0);
    assert_eq!(std::fs::read(database).unwrap(), before);
}
