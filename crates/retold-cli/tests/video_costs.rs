//! Video costs inspection and reconciliation CLI tests; no dispatch authority is granted.
#![expect(clippy::unwrap_used, reason = "fixture helpers")]
use assert_cmd::Command;
use retold_app::account::{Account, types::*};
use retold_engine::gate::Usd;
use retold_engine::ledger::Charge;
use retold_engine::{NewRun, Quality, run};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const VIDEO_CAP: Usd = Usd::from_micros(5_000_000);
const NO_PROVIDERS: &str = "schema_version = 1\n[providers]\n[routing]\n";
const CEILING: u64 = 10;
const MICROS_CAP: u64 = 10_000_000;
const HELD_MICROS: i64 = 1_000_000;
const RECONCILED_MICROS: i64 = 500_000;

struct SeedAttempt {
    workspace: Workspace,
    attempt_id: String,
    plan_hash: String,
}

struct Installation {
    sandbox: tempfile::TempDir,
    video_id: String,
    meta_dir: PathBuf,
}

impl Installation {
    fn new() -> Self {
        let sandbox = tempfile::tempdir().unwrap();
        let channel = channel_settings(&sandbox.path().join("config"));
        let (video_id, meta_dir) = create_video(&sandbox.path().join("library"), &channel);
        let installation = Self {
            sandbox,
            video_id,
            meta_dir,
        };
        installation.initialize_account();
        installation
    }

    fn seed_attempt(&self, request_id: &str) -> SeedAttempt {
        let canonical_lib = self.path("library").canonicalize().unwrap();
        let hex = retold_engine::store::hash(&canonical_lib).unwrap();
        let workspace_id = format!("library-{}", &hex[..16]);
        let attempt_id =
            retold_engine::store::hash(&("attempt", &workspace_id, &self.video_id, request_id))
                .unwrap();
        SeedAttempt {
            workspace: Workspace {
                id: workspace_id,
                root: canonical_lib,
            },
            attempt_id,
            plan_hash: retold_engine::store::hash(&"plan").unwrap(),
        }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.sandbox.path().join(name)
    }

    fn command(&self) -> Command {
        let mut command = Command::cargo_bin("retold").unwrap();
        command.env("HOME", self.path("home"));
        command.env("RETOLD_CONFIG_DIR", self.path("config"));
        command.env("RETOLD_LIBRARY", self.path("library"));
        command.env_remove("XDG_DATA_HOME");
        command
    }

    fn initialize_account(&self) {
        let mut cmd = self.command();
        cmd.args(["account", "init", "--timezone", "UTC"]);
        let assertion = cmd.assert();
        assertion.success();
    }

    fn invoke(&self, args: &[&str]) -> (serde_json::Value, i32) {
        let mut cmd = self.command();
        cmd.arg("--json");
        cmd.args(args);
        let output = cmd.output().unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        let status = output.status.code().unwrap();
        (serde_json::from_str(&stdout).unwrap(), status)
    }

    fn invoke_human(&self, args: &[&str]) -> (String, i32) {
        let mut cmd = self.command();
        cmd.args(args);
        let output = cmd.output().unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        let status = output.status.code().unwrap();
        (stdout, status)
    }

    fn account_dir(&self) -> PathBuf {
        self.path("home").join(if cfg!(target_os = "macos") {
            "Library/Application Support/retold-account"
        } else {
            ".local/share/retold-account"
        })
    }

    fn ledger_path(&self) -> PathBuf {
        self.meta_dir.join("ledger.json")
    }
}

fn channel_settings(config: &Path) -> retold_guide::Channel {
    let target = config.join("channels/cost-studio");
    let studio = retold_guide::channel::init_from_template("test-studio", &target).unwrap();
    std::fs::write(config.join("providers.toml"), NO_PROVIDERS).unwrap();
    studio
}

fn create_video(library: &Path, channel: &retold_guide::Channel) -> (String, PathBuf) {
    let guide = retold_guide::catalog::bundled().unwrap().remove(0);
    let raw: indexmap::IndexMap<String, String> = [
        ("title".into(), "Nosferatu".into()),
        ("kind".into(), "film".into()),
        ("year".into(), "1922".into()),
    ]
    .into();
    let new = NewRun {
        guide: &guide,
        channel,
        inputs: retold_guide::inputs::resolve(guide.manifest(), &raw).unwrap(),
        variants: vec!["en".parse().unwrap()],
        quality: Quality::Standard,
        budget: VIDEO_CAP,
        skip_reviews: true,
        source_file: None,
    };
    let (layout, meta) = run::create(library, new).unwrap();
    (meta.id, layout.meta())
}

fn build_limits(workspace_id: &str, video_id: &str) -> Vec<Limit> {
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
            scope: Scope::Video(video_id.into()),
            window: Window::Lifetime,
            resources,
        },
    ]
}

fn init_seed_account(account: &Account, installation: &Installation, seed: &SeedAttempt) {
    let _ = account.register_workspace(seed.workspace.clone());
    let _ = account.register_route("route", "provider");
    let limits = build_limits(&seed.workspace.id, &installation.video_id);
    let _ = account.set_limits(limits);
}

fn admit_seed_job(account: &Account, installation: &Installation, seed: &SeedAttempt) {
    let inputs_hash = retold_engine::store::hash(&"inputs").unwrap();
    let resources = BTreeMap::from([(Unit::Requests, CEILING), (Unit::UsdMicros, MICROS_CAP)]);
    let op_resources = BTreeMap::from([(Unit::Requests, 1), (Unit::UsdMicros, 0)]);
    let op_key = retold_engine::store::hash(&seed.attempt_id).unwrap();
    let job = Job {
        id: format!("job-{}", seed.attempt_id),
        workspace: seed.workspace.clone(),
        video_id: installation.video_id.clone(),
        series_id: None,
        plan_hash: seed.plan_hash.clone(),
        input_versions: BTreeMap::from([("manifest".into(), inputs_hash)]),
        action: Action::Continue,
        routes: ["route".into()].into(),
        capabilities: vec![Capability::Text],
        resources,
        operations: BTreeMap::from([(op_key, op_resources)]),
        deadline: None,
    };
    account.admit(job).unwrap();
}

fn reserve_seed_attempt(account: &Account, seed: &SeedAttempt) {
    let op_key = retold_engine::store::hash(&seed.attempt_id).unwrap();
    let reservation = Reservation {
        attempt_id: seed.attempt_id.clone(),
        job_id: format!("job-{}", seed.attempt_id),
        operation_key: op_key,
        node: None,
        route: "route".into(),
        capability: Capability::Text,
        plan_hash: seed.plan_hash.clone(),
        resources: BTreeMap::from([(Unit::Requests, 1), (Unit::UsdMicros, 0)]),
    };
    account.reserve(reservation).unwrap();
    account
        .mark_dispatched(&seed.attempt_id, &seed.plan_hash)
        .unwrap();
}

fn seed_reconciliation_attempt(installation: &Installation, request_id: &str) {
    let seed = installation.seed_attempt(request_id);
    let account = Account::open(&installation.account_dir()).unwrap();
    init_seed_account(&account, installation, &seed);
    admit_seed_job(&account, installation, &seed);
    reserve_seed_attempt(&account, &seed);
}

fn seed_unresolved_charge(installation: &Installation, request_id: &str) {
    let charge = Charge::Conservative {
        ceiling: Usd::from_micros(HELD_MICROS),
        generation: None,
    };
    let entries = BTreeMap::from([(request_id.to_string(), charge)]);
    std::fs::write(
        installation.ledger_path(),
        serde_json::to_string(&entries).unwrap(),
    )
    .unwrap();
    seed_reconciliation_attempt(installation, request_id);
}

#[test]
fn video_costs_inspection_shows_spend_and_settlement() {
    let installation = Installation::new();
    let video_ref = installation.video_id.as_str();
    assert_costs_clean(&installation, video_ref);
    assert_costs_unresolved(&installation, video_ref);
}

fn assert_costs_clean(installation: &Installation, video_ref: &str) {
    let (json_output, exit_code) = installation.invoke(&["video", "costs", video_ref]);
    assert_eq!(exit_code, 0);
    assert_eq!(json_output["payload"]["outcome"], "completed");
    let result = &json_output["payload"]["data"];
    assert_eq!(result["all_settled"], true);
    assert_eq!(result["dispatch_authorized"], false);
    assert_eq!(result["total_spent_formatted"], "$0.00");

    let (human_output, exit_code) = installation.invoke_human(&["video", "costs", video_ref]);
    assert_eq!(exit_code, 0);
    assert!(human_output.contains("Nosferatu"));
    assert!(human_output.contains("Spend: $0.00 of $5.00 budget"));
    assert!(human_output.contains("All paid requests are settled."));
    assert!(human_output.contains("Inspection only. No dispatch authority granted."));
}

fn assert_costs_unresolved(installation: &Installation, video_ref: &str) {
    let charge = Charge::Conservative {
        ceiling: Usd::from_micros(HELD_MICROS),
        generation: None,
    };
    let entries = BTreeMap::from([("req-42".to_string(), charge)]);
    std::fs::write(
        installation.ledger_path(),
        serde_json::to_string(&entries).unwrap(),
    )
    .unwrap();

    let (json_output, exit_code) = installation.invoke(&["video", "costs", video_ref]);
    assert_eq!(exit_code, 0);
    let result = &json_output["payload"]["data"];
    assert_eq!(result["all_settled"], false);
    assert_eq!(result["unresolved"][0]["request"], "req-42");
    assert_eq!(result["unresolved"][0]["held_formatted"], "$1.00");

    let (human_output, exit_code) = installation.invoke_human(&["video", "costs", video_ref]);
    assert_eq!(exit_code, 0);
    assert!(human_output.contains("Unresolved charges:"));
    assert!(human_output.contains("req-42: held at $1.00"));
    assert!(human_output.contains("Settle with: retold video costs"));
}

#[test]
fn video_costs_reconcile_charge_without_dispatch_authority() {
    let installation = Installation::new();
    let video_ref = installation.video_id.as_str();
    assert_reconcile_json(&installation, video_ref);
    assert_reconcile_human(&installation, video_ref);
}

fn assert_reconcile_json(installation: &Installation, video_ref: &str) {
    seed_unresolved_charge(installation, "req-settle");
    let (json_output, exit_code) = installation.invoke(&[
        "video",
        "costs",
        video_ref,
        "--request",
        "req-settle",
        "--cost",
        "0.50",
    ]);
    assert_reconcile_json_fields(&json_output, exit_code);
}

fn assert_reconcile_json_fields(json_output: &serde_json::Value, exit_code: i32) {
    assert_eq!(exit_code, 0);
    assert_eq!(json_output["payload"]["outcome"], "completed");
    let result = &json_output["payload"]["data"];
    assert_eq!(result["reconciled"], true);
    assert_eq!(result["request"], "req-settle");
    assert_eq!(result["cost_formatted"], "$0.50");
    assert_eq!(result["cost_usd_micros"], RECONCILED_MICROS);
    assert_eq!(result["dispatch_authorized"], false);
}

fn assert_reconcile_human(installation: &Installation, video_ref: &str) {
    seed_unresolved_charge(installation, "req-human");
    let (human_output, exit_code) = installation.invoke_human(&[
        "video",
        "costs",
        video_ref,
        "--request",
        "req-human",
        "--cost",
        "0.75",
    ]);
    assert_eq!(exit_code, 0);
    assert!(human_output.contains("Reconciled"));
    assert!(human_output.contains("request `req-human` at $0.75."));
    assert!(human_output.contains("No dispatch authority granted."));
}

#[test]
fn video_costs_rejects_missing_or_invalid_cost() {
    let installation = Installation::new();
    let video_ref = installation.video_id.as_str();
    let (_output, exit_code) =
        installation.invoke(&["video", "costs", video_ref, "--cost", "not-usd"]);
    assert_ne!(exit_code, 0);
}
