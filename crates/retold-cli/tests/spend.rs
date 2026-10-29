//! `spend` reads the installation account only. Attempts are seeded through the account API;
//! no provider is contacted and the report changes nothing.
#![expect(clippy::unwrap_used, reason = "fixture helpers")]
use assert_cmd::Command;
use retold_app::account::{Account, types::*};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const BLOCKED: i32 = 3;
const INVALID: i32 = 2;
const LIMIT_MICROS: u64 = 10_000_000;
const LIMIT_REQUESTS: u64 = 10;
const QUOTE_MICROS: u64 = 1_000_000;
const CHARGED_MICROS: u64 = 420_000;
const PROVIDER: &str = "openrouter:KEY";

fn command(home: &Path) -> Command {
    let mut command = Command::cargo_bin("retold").unwrap();
    command
        .env("HOME", home)
        .env_remove("XDG_DATA_HOME")
        .env("PATH", "");
    command
}

fn invoke(home: &Path, args: &[&str]) -> (String, i32) {
    let output = command(home).args(args).output().unwrap();
    assert_eq!(output.stderr, Vec::<u8>::new());
    let stdout = String::from_utf8(output.stdout).unwrap();
    (stdout, output.status.code().unwrap())
}

fn json(home: &Path, args: &[&str]) -> (Value, i32) {
    let mut arguments = vec!["--json"];
    arguments.extend_from_slice(args);
    let (stdout, exit) = invoke(home, &arguments);
    (serde_json::from_str(&stdout).unwrap(), exit)
}

/// Where `account init` puts the installation account for this `home`.
fn account_directory(home: &Path) -> PathBuf {
    let application_data = if cfg!(target_os = "macos") {
        "Library/Application Support"
    } else {
        ".local/share"
    };
    home.join(application_data).join("retold-account")
}

fn money(usd_micros: u64, requests: u64) -> Resources {
    BTreeMap::from([(Unit::UsdMicros, usd_micros), (Unit::Requests, requests)])
}

fn hash(value: &str) -> String {
    retold_engine::store::hash(&value).unwrap()
}

/// One settled charge and one dispatched attempt that still holds its quote.
fn seed(home: &Path) {
    let directory = account_directory(home);
    let account = Account::open(&directory).unwrap();
    let workspace = Workspace {
        id: "workspace".into(),
        root: home.canonicalize().unwrap(),
    };
    account.register_workspace(workspace.clone()).unwrap();
    account.register_route("route", PROVIDER).unwrap();
    account.set_limits(limits()).unwrap();
    account.admit(job(workspace)).unwrap();
    for attempt in ["settled", "held"] {
        account.reserve(reservation(attempt)).unwrap();
        account.mark_dispatched(attempt, &hash("plan")).unwrap();
    }
    settle(&account, "settled");
}

fn limits() -> Vec<Limit> {
    let scopes = [
        (Scope::Account, Window::Lifetime),
        (Scope::Account, Window::Day),
        (Scope::Account, Window::Month),
        (Scope::Workspace("workspace".into()), Window::Lifetime),
        (Scope::Video("video".into()), Window::Lifetime),
    ]
    .map(|(scope, window)| Limit {
        scope,
        window,
        resources: money(LIMIT_MICROS, LIMIT_REQUESTS),
    });
    scopes.into()
}

fn settle(account: &Account, attempt: &str) {
    let receipt = Receipt {
        artifact_hash: hash("artifact"),
        receipt_hash: hash("receipt"),
        observed: BTreeMap::from([
            (Unit::UsdMicros, Some(CHARGED_MICROS)),
            (Unit::Requests, Some(1)),
        ]),
    };
    account.settle(attempt, receipt).unwrap();
}

fn job(workspace: Workspace) -> Job {
    let operations = ["settled", "held"].map(|key| (hash(key), money(QUOTE_MICROS, 1)));
    Job {
        id: "job".into(),
        workspace,
        video_id: "video".into(),
        series_id: None,
        plan_hash: hash("plan"),
        input_versions: BTreeMap::from([("manifest".into(), hash("inputs"))]),
        action: Action::Continue,
        routes: ["route".into()].into(),
        capabilities: vec![Capability::Text],
        resources: money(LIMIT_MICROS, LIMIT_REQUESTS),
        operations: operations.into(),
        deadline: None,
    }
}

fn reservation(attempt: &str) -> Reservation {
    Reservation {
        attempt_id: attempt.into(),
        job_id: "job".into(),
        operation_key: hash(attempt),
        node: None,
        route: "route".into(),
        capability: Capability::Text,
        plan_hash: hash("plan"),
        resources: money(QUOTE_MICROS, 1),
    }
}

#[test]
fn spend_needs_an_account_and_valid_flags() {
    let home = tempfile::tempdir().unwrap();
    let (missing, exit) = json(home.path(), &["spend"]);
    assert_eq!(missing["payload"]["code"], "RG-ACCOUNT-REQUIRED");
    assert_eq!(exit, BLOCKED);
    invoke(home.path(), &["account", "init", "--timezone", "UTC"]);
    let (_, exit) = json(home.path(), &["spend", "--today", "--month"]);
    assert_eq!(exit, INVALID);
    let (_, exit) = json(home.path(), &["spend", "--by", "workspace"]);
    assert_eq!(exit, INVALID);
    let (empty, exit) = invoke(home.path(), &["--plain", "spend", "--today"]);
    assert_eq!(exit, 0);
    assert!(
        empty.contains("No recorded attempts in this period."),
        "{empty}"
    );
    assert!(
        empty.contains("No account day money limit is set"),
        "{empty}"
    );
}

#[test]
fn spend_reports_charges_and_holds_without_changing_the_account() {
    let home = tempfile::tempdir().unwrap();
    invoke(home.path(), &["account", "init", "--timezone", "UTC"]);
    seed(home.path());
    let database = account_directory(home.path()).join("accounting.sqlite");
    let before = std::fs::read(&database).unwrap();
    let (report, exit) = json(home.path(), &["spend", "--by", "provider"]);
    assert_eq!(exit, 0, "{report}");
    let spend = &report["payload"]["data"]["spend"];
    assert_eq!(spend["rows"][0]["key"], PROVIDER);
    let usd = &spend["total"]["resources"]["usd_micros"];
    assert_eq!(
        (usd["charged"].as_u64(), usd["held"].as_u64()),
        (Some(CHARGED_MICROS), Some(QUOTE_MICROS))
    );
    assert_eq!(spend["total"]["unresolved"], 1);
    let left = LIMIT_MICROS - CHARGED_MICROS - QUOTE_MICROS;
    assert_eq!(spend["ceiling"]["usd_micros"]["available"], left);
    assert_plain_report(home.path());
    assert_eq!(std::fs::read(&database).unwrap(), before);
}

fn assert_plain_report(home: &Path) {
    let (plain, _) = invoke(home, &["--plain", "spend", "--by", "video"]);
    let expected = [
        "Spend · all time (UTC) · by video",
        "video · $0.42 charged · $1.00 held · 1 request · 2 attempts · limit $10.00 · $8.58 left",
        "Account lifetime limit $10.00 · $8.58 left",
        "Unresolved: 1 attempt still holding quotes.",
        "Report only; nothing was changed.",
    ];
    for line in expected {
        assert!(plain.contains(line), "{line}\n{plain}");
    }
}
