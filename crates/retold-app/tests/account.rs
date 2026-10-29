//! Ledger tests use isolated accounts and never dispatch providers.
#![expect(clippy::unwrap_used, reason = "Isolated test fixtures must succeed")]
const OVERAGE: u64 = 3;
use retold_app::account::{Account, Clock, Error, types::*};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

#[derive(Debug)]
struct TestClock(Mutex<jiff::Timestamp>);
impl Clock for TestClock {
    fn now(&self) -> jiff::Timestamp {
        *self.0.lock().unwrap()
    }
}
impl TestClock {
    fn set(&self, time: &str) {
        *self.0.lock().unwrap() = time.parse().unwrap();
    }
}

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    account: Account,
    clock: Arc<TestClock>,
    job: Job,
}
fn resources(requests: u64) -> Resources {
    BTreeMap::from([(Unit::Requests, requests), (Unit::UsdMicros, 0)])
}
fn hash(value: &str) -> String {
    retold_engine::store::hash(&value).unwrap()
}
fn fixture(ceiling: u64) -> Fixture {
    let sandbox = tempfile::tempdir().unwrap();
    let root = sandbox.path().join("account");
    Account::initialize(&root, "Asia/Karachi").unwrap();
    let clock = Arc::new(TestClock(Mutex::new(
        "2030-01-31T18:59:00Z".parse().unwrap(),
    )));
    let account = Account::with_clock(&root, Arc::<TestClock>::clone(&clock)).unwrap();
    let workspace = Workspace {
        id: "workspace".into(),
        root: sandbox.path().canonicalize().unwrap(),
    };
    account.register_workspace(workspace.clone()).unwrap();
    account.register_route("route", "provider").unwrap();
    account.register_route("alias", "provider").unwrap();
    account.set_limits(limits(ceiling)).unwrap();
    let job = job(workspace, ceiling);
    account.admit(job.clone()).unwrap();
    Fixture {
        _temp: sandbox,
        root,
        account,
        clock,
        job,
    }
}
fn request(key: &str) -> Reservation {
    Reservation {
        attempt_id: key.into(),
        job_id: "job".into(),
        operation_key: hash(key),
        node: None,
        route: "route".into(),
        capability: Capability::Text,
        plan_hash: hash("plan"),
        resources: resources(1),
    }
}
fn receipt(requests: Option<u64>) -> Receipt {
    Receipt {
        artifact_hash: hash("artifact"),
        receipt_hash: hash(&format!("receipt-{requests:?}")),
        observed: BTreeMap::from([(Unit::Requests, requests), (Unit::UsdMicros, Some(0))]),
    }
}

#[test]
fn replay_and_dispatch_boundary_are_idempotent() {
    let fixture = fixture(2);
    assert!(matches!(
        fixture.account.reserve(request("one")).unwrap(),
        Reserved::New(_)
    ));
    assert!(matches!(
        fixture.account.reserve(request("one")).unwrap(),
        Reserved::Existing(_)
    ));
    assert_eq!(held(&fixture.account), 1);
    let mut duplicate = request("one");
    duplicate.attempt_id = "duplicate".into();
    duplicate.route = "alias".into();
    assert!(matches!(
        fixture.account.reserve(duplicate),
        Err(Error::OperationExists)
    ));
}

#[test]
fn dispatched_attempt_survives_reopen_without_new_permit() {
    let fixture = fixture(2);
    fixture.account.reserve(request("one")).unwrap();
    assert!(
        fixture
            .account
            .mark_dispatched("one", &hash("stale"))
            .is_err()
    );
    let permit = fixture
        .account
        .mark_dispatched("one", &hash("plan"))
        .unwrap();
    assert_eq!(permit.attempt_id(), "one");
    assert!(
        fixture
            .account
            .mark_dispatched("one", &hash("plan"))
            .is_err()
    );
    assert!(fixture.account.cancel_reserved("one").is_err());
    let reopened =
        Account::with_clock(&fixture.root, Arc::<TestClock>::clone(&fixture.clock)).unwrap();
    assert_eq!(
        reopened.summary().unwrap().lifetime[&Unit::Requests].held,
        1
    );
}

#[test]
fn unknown_observations_hold_across_timezone_rollover_until_reconciled() {
    let fixture = fixture(1);
    fixture.account.reserve(request("one")).unwrap();
    fixture
        .account
        .mark_dispatched("one", &hash("plan"))
        .unwrap();
    fixture
        .account
        .settle("one", receipt(Option::default()))
        .unwrap();
    fixture.clock.set("2030-01-31T19:01:00Z"); // February in the pinned timezone.
    assert!(matches!(
        fixture.account.reserve(request("two")),
        Err(Error::Limit)
    ));
    assert_eq!(held(&fixture.account), 1);
    fixture.account.reconcile("one", receipt(Some(1))).unwrap();
    assert_eq!(charged(&fixture.account), 1);
    assert_eq!(held(&fixture.account), 0);
    assert!(fixture.account.reconcile("one", receipt(Some(0))).is_err());
}

#[test]
fn actual_overage_is_recorded_and_blocks_more_work() {
    let fixture = fixture(2);
    fixture.account.reserve(request("one")).unwrap();
    fixture
        .account
        .mark_dispatched("one", &hash("plan"))
        .unwrap();
    fixture
        .account
        .settle("one", receipt(Some(OVERAGE)))
        .unwrap();
    fixture
        .account
        .settle("one", receipt(Some(OVERAGE)))
        .unwrap();
    assert_eq!(charged(&fixture.account), OVERAGE);
    assert!(matches!(
        fixture.account.reserve(request("two")),
        Err(Error::Limit)
    ));
}

#[test]
fn missing_limits_and_invalid_authority_fail_closed() {
    let fixture = fixture(2);
    let mut job = fixture.job.clone();
    job.id = "expired".into();
    job.deadline = Some("2029-01-01T00:00:00Z".parse().unwrap());
    assert!(fixture.account.admit(job).is_err());
    let mut job = fixture.job.clone();
    job.id = "export".into();
    job.capabilities.push(Capability::Export);
    assert!(fixture.account.admit(job).is_err());
    assert!(
        fixture
            .account
            .register_route("route", "other-provider")
            .is_err()
    );
    let mut workspace = fixture.job.workspace.clone();
    workspace.root = fixture.root.clone();
    assert!(fixture.account.register_workspace(workspace).is_err());
    fixture.account.set_limits(Vec::new()).unwrap();
    assert!(matches!(
        fixture.account.reserve(request("one")),
        Err(Error::Limit)
    ));
}

#[test]
fn only_definitely_undispatched_attempts_release_holds() {
    let fixture = fixture(1);
    fixture.account.reserve(request("one")).unwrap();
    fixture.account.cancel_reserved("one").unwrap();
    fixture.account.cancel_reserved("one").unwrap();
    assert_eq!(held(&fixture.account), 0);
    fixture.account.reserve(request("two")).unwrap();
    assert!(fixture.account.settle("two", receipt(Some(1))).is_err());
}

#[test]
fn lost_or_corrupt_account_is_never_automatically_replaced() {
    let fixture = fixture(1);
    std::fs::remove_file(fixture.root.join("accounting.sqlite")).unwrap();
    assert!(Account::open(&fixture.root).is_err());
    assert!(matches!(
        Account::initialize(&fixture.root, "UTC"),
        Err(Error::Exists)
    ));
    std::fs::write(fixture.root.join("accounting.sqlite"), b"broken").unwrap();
    assert!(Account::open(&fixture.root).is_err());
    assert!(matches!(
        Account::initialize(&fixture.root, "UTC"),
        Err(Error::Exists)
    ));
}

#[test]
fn reserve_in_child() {
    let Ok(root) = std::env::var("RETOLD_TEST_ACCOUNT") else {
        return;
    };
    let key = std::env::var("RETOLD_TEST_ATTEMPT").unwrap();
    let clock = Arc::new(TestClock(Mutex::new(
        "2030-02-01T00:00:00Z".parse().unwrap(),
    )));
    let account = Account::with_clock(std::path::Path::new(&root), clock).unwrap();
    std::fs::write(
        std::path::Path::new(&root).join(format!("{key}.ready")),
        b"ready",
    )
    .unwrap();
    wait_until(|| std::path::Path::new(&root).join("start").exists());
    match account.reserve(request(&key)) {
        Ok(Reserved::New(_)) => println!("RESERVED"),
        Err(Error::Limit) => println!("BLOCKED"),
        result => panic!("Unexpected worker result: {result:?}"),
    }
}

#[test]
fn separate_processes_cannot_oversubscribe_shared_cap() {
    let fixture = fixture(1);
    let children = spawn_competitors(&fixture.root);
    release_competitors(&fixture.root);
    let outputs: Vec<_> = children
        .into_iter()
        .map(|child| child.wait_with_output().unwrap())
        .collect();
    assert!(
        outputs.iter().all(|output| output.status.success()),
        "{outputs:?}"
    );
    let winners = outputs
        .iter()
        .filter(|output| String::from_utf8_lossy(&output.stdout).contains("RESERVED"))
        .count();
    assert_eq!(winners, 1);
    assert_eq!(held(&fixture.account), 1);
}

fn job(workspace: Workspace, ceiling: u64) -> Job {
    let operations = ["one", "two", "three", "four"]
        .into_iter()
        .map(|key| (hash(key), resources(ceiling)))
        .collect();
    Job {
        id: "job".into(),
        workspace,
        video_id: "video".into(),
        series_id: None,
        plan_hash: hash("plan"),
        input_versions: BTreeMap::from([("manifest".into(), hash("inputs"))]),
        action: Action::Continue,
        routes: ["route".into(), "alias".into()].into(),
        capabilities: vec![Capability::Text],
        resources: resources(ceiling),
        operations,
        deadline: Some("2030-03-01T00:00:00Z".parse().unwrap()),
    }
}

fn limits(ceiling: u64) -> Vec<Limit> {
    [
        (Scope::Account, Window::Day),
        (Scope::Account, Window::Month),
        (Scope::Workspace("workspace".into()), Window::Lifetime),
        (Scope::Video("video".into()), Window::Lifetime),
    ]
    .into_iter()
    .map(|(scope, window)| Limit {
        scope,
        window,
        resources: resources(ceiling),
    })
    .collect()
}

fn spawn_competitors(root: &std::path::Path) -> Vec<std::process::Child> {
    let executable = std::env::current_exe().unwrap();
    ["one", "two", "three", "four"]
        .into_iter()
        .map(|key| {
            let mut command = std::process::Command::new(&executable);
            command.args(["--exact", "reserve_in_child", "--nocapture"]);
            command
                .env("RETOLD_TEST_ACCOUNT", root)
                .env("RETOLD_TEST_ATTEMPT", key);
            command
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped());
            command.spawn().unwrap()
        })
        .collect()
}

fn held(account: &Account) -> u64 {
    account.summary().unwrap().lifetime[&Unit::Requests].held
}
fn charged(account: &Account) -> u64 {
    account.summary().unwrap().lifetime[&Unit::Requests].charged
}

#[test]
fn known_usage_frees_only_new_periods_and_clock_cannot_move_backwards() {
    let fixture = fixture(2);
    fixture.account.set_limits(period_caps()).unwrap();
    fixture.account.reserve(request("one")).unwrap();
    fixture
        .account
        .mark_dispatched("one", &hash("plan"))
        .unwrap();
    fixture.account.settle("one", receipt(Some(1))).unwrap();
    assert!(matches!(
        fixture.account.reserve(request("two")),
        Err(Error::Limit)
    ));
    fixture.clock.set("2030-01-31T19:01:00Z");
    fixture.account.reserve(request("two")).unwrap();
    fixture.clock.set("2030-01-31T18:59:00Z");
    assert!(matches!(
        fixture.account.reserve(request("three")),
        Err(Error::Limit)
    ));
    fixture.account.cancel_reserved("two").unwrap();
    fixture.account.reserve(request("three")).unwrap();
    let stored = fixture.account.attempt("three").unwrap();
    assert_eq!(stored.day, "2030-02-01");
    assert_eq!(stored.month, "2030-02");
}

#[test]
fn provider_aliases_share_a_ceiling() {
    let fixture = fixture(2);
    let mut caps = limits(2);
    caps.push(Limit {
        scope: Scope::Provider("provider".into()),
        window: Window::Lifetime,
        resources: resources(1),
    });
    fixture.account.set_limits(caps).unwrap();
    fixture.account.reserve(request("one")).unwrap();
    let mut second = request("two");
    second.route = "alias".into();
    assert!(matches!(fixture.account.reserve(second), Err(Error::Limit)));
}

#[test]
fn job_and_operation_limits_are_separate_from_account_limits() {
    let fixture = fixture(2);
    let mut job = fixture.job.clone();
    job.id = "limited".into();
    job.operations.insert(hash("one"), Resources::new());
    fixture.account.admit(job).unwrap();
    let mut quote = request("one");
    quote.job_id = "limited".into();
    assert!(matches!(fixture.account.reserve(quote), Err(Error::Limit)));
    let mut job = fixture.job.clone();
    job.id = "no-job-budget".into();
    job.resources.clear();
    fixture.account.admit(job).unwrap();
    let mut quote = request("one");
    quote.job_id = "no-job-budget".into();
    assert!(matches!(fixture.account.reserve(quote), Err(Error::Limit)));
}

#[test]
fn reconciliation_retains_receipt_history_and_never_refunds_known_usage() {
    let fixture = fixture(2);
    fixture.account.reserve(request("one")).unwrap();
    fixture
        .account
        .mark_dispatched("one", &hash("plan"))
        .unwrap();
    let unknown = receipt(Option::default());
    fixture.account.settle("one", unknown.clone()).unwrap();
    fixture.account.reconcile("one", receipt(Some(1))).unwrap();
    fixture.account.reconcile("one", receipt(Some(1))).unwrap();
    assert_eq!(
        fixture.account.summary().unwrap().lifetime[&Unit::InputTokens].unknown_observations,
        0
    );
    let attempt = fixture.account.attempt("one").unwrap();
    assert_eq!(attempt.receipt_history, vec![unknown, receipt(Some(1))]);
    assert!(fixture.account.reconcile("one", receipt(Some(0))).is_err());
}

#[test]
fn zero_dollar_quotes_still_need_requests_and_expire_before_dispatch() {
    let fixture = fixture(2);
    let mut quote = request("one");
    quote.resources.remove(&Unit::Requests);
    assert!(fixture.account.reserve(quote).is_err());
    let mut quote = request("one");
    quote.resources.remove(&Unit::UsdMicros);
    assert!(fixture.account.reserve(quote).is_err());
    fixture.account.reserve(request("one")).unwrap();
    fixture.clock.set("2030-03-01T00:00:00Z");
    assert!(
        fixture
            .account
            .mark_dispatched("one", &hash("plan"))
            .is_err()
    );
    fixture.clock.set("2030-01-31T18:59:00Z");
    assert!(
        fixture
            .account
            .mark_dispatched("one", &hash("plan"))
            .is_err()
    );
    assert_eq!(held(&fixture.account), 1);
}

#[test]
fn new_job_cannot_bypass_existing_logical_operation() {
    let fixture = fixture(2);
    fixture.account.reserve(request("one")).unwrap();
    let mut job = fixture.job.clone();
    job.id = "new-job".into();
    fixture.account.admit(job).unwrap();
    let mut quote = request("one");
    quote.job_id = "new-job".into();
    quote.attempt_id = "new-attempt".into();
    assert!(matches!(
        fixture.account.reserve(quote),
        Err(Error::OperationExists)
    ));
}

fn period_caps() -> Vec<Limit> {
    limits(2)
        .into_iter()
        .map(|mut cap| {
            if cap.scope == Scope::Account {
                cap.resources = resources(1);
            }
            cap
        })
        .collect()
}

fn release_competitors(root: &std::path::Path) {
    wait_until(|| {
        ["one", "two", "three", "four"]
            .into_iter()
            .all(|key| root.join(format!("{key}.ready")).exists())
    });
    std::fs::write(root.join("start"), b"start").unwrap();
}
fn wait_until(ready: impl Fn() -> bool) {
    const WAIT_LIMIT: std::time::Duration = std::time::Duration::from_secs(5);
    const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(5);
    let started = std::time::Instant::now();
    while !ready() {
        assert!(
            started.elapsed() < WAIT_LIMIT,
            "competitor barrier timed out"
        );
        std::thread::sleep(POLL_INTERVAL);
    }
}

#[test]
fn attempt_inspection_pages_filters_and_preserves_balances() {
    use retold_app::account::inspection::Selection;
    const INSPECTION_CEILING: u64 = 10;
    let fixture = fixture(INSPECTION_CEILING);
    for id in ["three", "one", "two"] {
        fixture.account.reserve(request(id)).unwrap();
    }
    fixture
        .account
        .mark_dispatched("three", &hash("plan"))
        .unwrap();
    let before = serde_json::to_value(fixture.account.summary().unwrap()).unwrap();
    let mut selection = Selection {
        after: None,
        limit: 2,
        state: None,
    };
    let first = fixture.account.attempts(&selection).unwrap();
    assert_eq!(first.attempts[0].id, "one");
    assert_eq!(first.attempts[1].id, "three");
    assert_eq!(first.next_after.as_deref(), Some("three"));
    selection.after = first.next_after.as_deref();
    assert_last_attempt(&fixture.account, &selection);
    assert_filtered_inspection(&fixture.account);
    assert_eq!(
        serde_json::to_value(fixture.account.summary().unwrap()).unwrap(),
        before
    );
}

fn assert_filtered_inspection(account: &Account) {
    use retold_app::account::inspection::{MAX_PAGE, Selection};
    let mut selection = Selection {
        after: None,
        limit: 1,
        state: Some(AttemptState::Dispatched),
    };
    let result = account.attempts(&selection).unwrap();
    assert_eq!(result.attempts.len(), 1);
    assert_eq!(result.attempts[0].id, "three");
    for limit in [0, MAX_PAGE + 1] {
        selection.limit = limit;
        assert!(account.attempts(&selection).is_err());
    }
}

fn assert_last_attempt(
    account: &Account,
    selection: &retold_app::account::inspection::Selection<'_>,
) {
    let last = account.attempts(selection).unwrap();
    assert_eq!(last.attempts[0].id, "two");
    assert!(last.next_after.is_none());
}

#[test]
fn reconcile_attempt_across_lifecycle_states() {
    const CEILING_RECONCILE: u64 = 5;
    let reserved_fixture = fixture(CEILING_RECONCILE);
    assert_reconcile_reserved(&reserved_fixture.account);
    let dispatched_fixture = fixture(CEILING_RECONCILE);
    assert_reconcile_dispatched(&dispatched_fixture.account);
    let settled_fixture = fixture(CEILING_RECONCILE);
    assert_reconcile_settled(&settled_fixture.account);
}

const POSITIVE_COST_MICROS: i64 = 1_000_000;
const DISPATCHED_COST_MICROS: i64 = 1_500_000;
const SETTLED_COST_MICROS: i64 = 750_000;
const DIFFERENT_COST_MICROS: i64 = 900_000;

fn assert_reconcile_reserved(account: &Account) {
    account.reserve(request("one")).unwrap();
    let positive_cost = retold_app::account::Usd::from_micros(POSITIVE_COST_MICROS);
    assert!(matches!(
        account.reconcile_attempt("one", positive_cost),
        Err(Error::Transition)
    ));
    account
        .reconcile_attempt("one", retold_app::account::Usd::ZERO)
        .unwrap();
    let attempt = account.attempt("one").unwrap();
    assert_eq!(attempt.state, AttemptState::NotDispatched);
}

fn assert_reconcile_dispatched(account: &Account) {
    account.reserve(request("two")).unwrap();
    account.mark_dispatched("two", &hash("plan")).unwrap();
    let cost = retold_app::account::Usd::from_micros(DISPATCHED_COST_MICROS);
    account.reconcile_attempt("two", cost).unwrap();
    let attempt = account.attempt("two").unwrap();
    assert_eq!(attempt.state, AttemptState::Settled);
    let receipt = attempt.receipt.unwrap();
    assert_eq!(
        receipt.observed[&Unit::UsdMicros],
        Some(DISPATCHED_COST_MICROS as u64)
    );
}

fn assert_reconcile_settled(account: &Account) {
    account.reserve(request("three")).unwrap();
    account.mark_dispatched("three", &hash("plan")).unwrap();
    let unknown_receipt = Receipt {
        artifact_hash: hash("artifact"),
        receipt_hash: hash("receipt-unknown"),
        observed: BTreeMap::from([(Unit::Requests, Some(1)), (Unit::UsdMicros, None)]),
    };
    account.settle("three", unknown_receipt).unwrap();
    let cost = retold_app::account::Usd::from_micros(SETTLED_COST_MICROS);
    account.reconcile_attempt("three", cost).unwrap();
    let attempt = account.attempt("three").unwrap();
    let receipt = attempt.receipt.unwrap();
    assert_eq!(
        receipt.observed[&Unit::UsdMicros],
        Some(SETTLED_COST_MICROS as u64)
    );
    let different_cost = retold_app::account::Usd::from_micros(DIFFERENT_COST_MICROS);
    assert!(matches!(
        account.reconcile_attempt("three", different_cost),
        Err(Error::Transition)
    ));
}

#[test]
fn inactive_marker_revokes_previously_admitted_job_dispatch() {
    let fixture = fixture(2);
    fixture.account.reserve(request("one")).unwrap();
    std::fs::write(
        fixture
            .job
            .workspace
            .root
            .join(retold_engine::recovery::INACTIVE_MARKER),
        b"{}",
    )
    .unwrap();
    assert!(fixture.account.admit(fixture.job.clone()).is_err());
    assert!(fixture.account.reserve(request("two")).is_err());
    assert!(
        fixture
            .account
            .mark_dispatched("one", &fixture.job.plan_hash)
            .is_err()
    );
    assert_eq!(fixture.account.summary().unwrap().attempts, 1);
    assert_eq!(held(&fixture.account), 1);
}
