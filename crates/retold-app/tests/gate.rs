//! Engine ledgers gated by a real account. Isolated accounts and fake requests; no provider I/O.
#![expect(clippy::unwrap_used, reason = "Isolated test fixtures must succeed")]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use retold_app::account::types::{
    Action, Balance, Capability, Job, Limit, Resources, Scope, Unit, Window, Workspace,
};
use retold_app::account::{Account, Clock};
use retold_app::execution::{AccountGate, operation_node};
use retold_engine::gate::{GateError, PaidCapability, PaidEvidence, Usd};
use retold_engine::ledger::{Ledger, LedgerError, Purchase};

const QUOTE: Usd = Usd::from_micros(10_000);
const SETTLED_COST: Usd = Usd::from_micros(4_000);
const RUN_BUDGET: Usd = Usd::from_micros(1_000_000);
const GENEROUS_USD: u64 = 1_000_000;
const GENEROUS_REQUESTS: u64 = 100;
const ROUTE: &str = "openrouter";
const IMAGE_STAGE: &str = "images";
const VOICE_STAGE: &str = "voices";
const VIDEO_A: &str = "video-a";
const VIDEO_B: &str = "video-b";

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
    sandbox: tempfile::TempDir,
    directory: PathBuf,
    clock: Arc<TestClock>,
    workspace: Workspace,
}

impl Fixture {
    fn new(daily_usd: u64) -> Self {
        let sandbox = tempfile::tempdir().unwrap();
        let directory = sandbox.path().join("account");
        Account::initialize(&directory, "UTC").unwrap();
        let clock = Arc::new(TestClock(Mutex::new(
            "2030-01-15T12:00:00Z".parse().unwrap(),
        )));
        let workspace = Workspace {
            id: "workspace".into(),
            root: sandbox.path().canonicalize().unwrap(),
        };
        let fixture = Self {
            sandbox,
            directory,
            clock,
            workspace,
        };
        let account = fixture.account();
        account
            .register_workspace(fixture.workspace.clone())
            .unwrap();
        account.register_route(ROUTE, "openrouter-account").unwrap();
        account.set_limits(limits(daily_usd)).unwrap();
        fixture
    }

    fn account(&self) -> Account {
        Account::with_clock(&self.directory, Arc::<TestClock>::clone(&self.clock)).unwrap()
    }

    fn admit(&self, job_id: &str, video_id: &str, image_requests: u64) -> Job {
        let job = self.job(job_id, video_id, image_requests);
        self.account().admit(job.clone()).unwrap();
        job
    }

    /// Declares only the images node; voices work is outside this job's operation tree.
    fn job(&self, id: &str, video_id: &str, image_requests: u64) -> Job {
        let image_node = operation_node(video_id, IMAGE_STAGE, Capability::Image).unwrap();
        Job {
            id: id.into(),
            workspace: self.workspace.clone(),
            video_id: video_id.into(),
            series_id: None,
            plan_hash: hash("plan"),
            input_versions: BTreeMap::from([("script".into(), hash("script"))]),
            action: Action::Continue,
            routes: BTreeSet::from([ROUTE.into()]),
            capabilities: vec![Capability::Image, Capability::Speech],
            resources: resources(GENEROUS_REQUESTS, GENEROUS_USD),
            operations: BTreeMap::from([(image_node, resources(image_requests, GENEROUS_USD))]),
            deadline: Some("2030-01-15T13:00:00Z".parse().unwrap()),
        }
    }

    fn ledger_path(&self, name: &str) -> PathBuf {
        self.sandbox.path().join(name).join("ledger.json")
    }

    fn ledger(&self, name: &str, job: &Job) -> Ledger {
        gated(
            &self.ledger_path(name),
            AccountGate::for_job(self.account(), job),
        )
    }

    fn lifetime(&self, unit: Unit) -> Balance {
        let summary = self.account().summary().unwrap();
        summary.lifetime.get(&unit).cloned().unwrap_or_default()
    }

    fn advance(&self, time: &str) {
        self.clock.set(time);
    }
}

fn gated(path: &Path, gate: AccountGate) -> Ledger {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    Ledger::open(path, RUN_BUDGET)
        .unwrap()
        .with_gate(Arc::new(gate))
}

fn resources(requests: u64, usd_micros: u64) -> Resources {
    BTreeMap::from([(Unit::Requests, requests), (Unit::UsdMicros, usd_micros)])
}

fn limits(daily_usd: u64) -> Vec<Limit> {
    let generous = resources(GENEROUS_REQUESTS, GENEROUS_USD);
    let lifetime = |scope| Limit {
        scope,
        window: Window::Lifetime,
        resources: generous.clone(),
    };
    vec![
        Limit {
            scope: Scope::Account,
            window: Window::Day,
            resources: resources(GENEROUS_REQUESTS, daily_usd),
        },
        Limit {
            scope: Scope::Account,
            window: Window::Month,
            resources: generous.clone(),
        },
        lifetime(Scope::Workspace("workspace".into())),
        lifetime(Scope::Video(VIDEO_A.into())),
        lifetime(Scope::Video(VIDEO_B.into())),
    ]
}

fn hash(value: &str) -> String {
    retold_engine::store::hash(&value).unwrap()
}

const fn purchase<'a>(
    logical: &'a str,
    stage: &'a str,
    capability: PaidCapability,
) -> Purchase<'a> {
    Purchase {
        logical,
        stage,
        provider: ROUTE,
        capability,
        ceiling: QUOTE,
    }
}

const fn image(logical: &str) -> Purchase<'_> {
    purchase(logical, IMAGE_STAGE, PaidCapability::Image)
}

fn artifact_hash() -> &'static str {
    static ARTIFACT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    ARTIFACT.get_or_init(|| hash("artifact"))
}

fn known_cost(cost: Usd) -> PaidEvidence<'static> {
    PaidEvidence {
        artifact_hash: artifact_hash(),
        cost: Some(cost),
    }
}

fn unknown_cost() -> PaidEvidence<'static> {
    PaidEvidence {
        artifact_hash: artifact_hash(),
        cost: None,
    }
}

const fn refused(result: &Result<String, LedgerError>) -> bool {
    matches!(result, Err(LedgerError::Gate(GateError::Refused(_))))
}

#[test]
fn account_ceiling_is_shared_by_videos_with_separate_run_ledgers() {
    let fixture = Fixture::new(QUOTE.micros().unsigned_abs());
    let first = fixture.ledger(VIDEO_A, &fixture.admit("job-a", VIDEO_A, 1));
    let second = fixture.ledger(VIDEO_B, &fixture.admit("job-b", VIDEO_B, 1));
    let logical = hash("frame");
    first.begin_paid(&image(&logical)).unwrap();
    assert!(refused(&second.begin_paid(&image(&logical))));
    assert!(second.entries().unwrap().is_empty());
    assert_eq!(
        fixture.lifetime(Unit::UsdMicros).held,
        QUOTE.micros().unsigned_abs()
    );
}

#[test]
fn work_outside_or_beyond_a_declared_operation_node_is_refused() {
    let fixture = Fixture::new(GENEROUS_USD);
    let ledger = fixture.ledger(VIDEO_A, &fixture.admit("job", VIDEO_A, 1));
    let (first, second, voice) = (hash("frame-1"), hash("frame-2"), hash("line-1"));
    ledger.begin_paid(&image(&first)).unwrap();
    assert!(refused(&ledger.begin_paid(&image(&second))));
    let speech = purchase(&voice, VOICE_STAGE, PaidCapability::Speech);
    assert!(refused(&ledger.begin_paid(&speech)));
    assert_eq!(fixture.account().summary().unwrap().attempts, 1);
}

#[test]
fn a_lost_run_ledger_never_redispatches_an_attempt() {
    let fixture = Fixture::new(GENEROUS_USD);
    let job = fixture.admit("job", VIDEO_A, 2);
    let logical = hash("frame");
    fixture
        .ledger("original", &job)
        .begin_paid(&image(&logical))
        .unwrap();
    let same_job = fixture.ledger("lost", &job);
    assert!(matches!(
        same_job.begin_paid(&image(&logical)),
        Err(LedgerError::Gate(GateError::Uncertain(_)))
    ));
    let next_job = fixture.admit("next-job", VIDEO_A, 2);
    let restarted = fixture.ledger("restarted", &next_job);
    assert!(restarted.begin_paid(&image(&logical)).is_err());
    assert!(restarted.entries().unwrap().is_empty());
    assert_eq!(fixture.lifetime(Unit::Requests).held, 1);
}

#[test]
fn unknown_cost_stays_held_until_explicit_reconciliation() {
    let fixture = Fixture::new(GENEROUS_USD);
    let job = fixture.admit("job", VIDEO_A, 2);
    let ledger = fixture.ledger(VIDEO_A, &job);
    let request = ledger.begin_paid(&image(&hash("frame"))).unwrap();
    ledger
        .settle_with_evidence(&request, None, &unknown_cost())
        .unwrap();
    let held = fixture.lifetime(Unit::UsdMicros);
    assert_eq!(held.held, QUOTE.micros().unsigned_abs());
    assert_eq!(held.unknown_observations, 1);
    drop(ledger);
    let gate = AccountGate::for_reconciliation(fixture.account(), "workspace", VIDEO_A);
    let reconciling = gated(&fixture.ledger_path(VIDEO_A), gate);
    assert!(reconciling.begin_paid(&image(&hash("other"))).is_err());
    reconciling.reconcile(&request, SETTLED_COST).unwrap();
    let settled = fixture.lifetime(Unit::UsdMicros);
    assert_eq!(settled.charged, SETTLED_COST.micros().unsigned_abs());
    assert_eq!(settled.held, 0);
}

#[test]
fn unbilled_failure_settles_at_zero_and_frees_only_a_new_slot() {
    let fixture = Fixture::new(GENEROUS_USD);
    let ledger = fixture.ledger(VIDEO_A, &fixture.admit("job", VIDEO_A, 2));
    let logical = hash("frame");
    let failed = ledger.begin_paid(&image(&logical)).unwrap();
    ledger.reconcile(&failed, Usd::ZERO).unwrap();
    assert_eq!(fixture.lifetime(Unit::UsdMicros).charged, 0);
    assert_eq!(fixture.lifetime(Unit::Requests).charged, 1);
    let retry = ledger.begin_paid(&image(&logical)).unwrap();
    assert_eq!(retry, format!("{failed}-retry-1"));
    ledger
        .settle_with_evidence(&retry, None, &known_cost(SETTLED_COST))
        .unwrap();
    assert_eq!(
        fixture.lifetime(Unit::UsdMicros).charged,
        SETTLED_COST.micros().unsigned_abs()
    );
}

#[test]
fn expired_authority_dispatches_nothing_and_keeps_no_hold() {
    let fixture = Fixture::new(GENEROUS_USD);
    let ledger = fixture.ledger(VIDEO_A, &fixture.admit("job", VIDEO_A, 2));
    fixture.advance("2030-01-15T13:00:01Z");
    assert!(matches!(
        ledger.begin_paid(&image(&hash("frame"))),
        Err(LedgerError::Gate(GateError::Blocked(_)))
    ));
    assert!(ledger.entries().unwrap().is_empty());
    assert_eq!(fixture.account().summary().unwrap().attempts, 0);
}

#[test]
fn settled_operation_prevents_duplicate_dispatch_across_different_jobs() {
    let fixture = Fixture::new(GENEROUS_USD);
    let first_job = fixture.admit("first-job", VIDEO_A, 2);
    let first_ledger = fixture.ledger(VIDEO_A, &first_job);
    let logical = hash("hero-shot-1");
    let request = first_ledger.begin_paid(&image(&logical)).unwrap();
    first_ledger
        .settle_with_evidence(&request, None, &known_cost(SETTLED_COST))
        .unwrap();

    let second_job = fixture.admit("second-job", VIDEO_A, 2);
    let second_ledger = fixture.ledger(VIDEO_A, &second_job);
    let redispatch = second_ledger.begin_paid(&image(&logical));
    assert!(matches!(
        redispatch,
        Err(LedgerError::Gate(GateError::Uncertain(_)))
    ));
}

const TESTS: &str = "providers:test";
const TEST_STAGE: &str = "provider-test";

fn provider_test_job(fixture: &Fixture, capabilities: Vec<Capability>) -> Job {
    let node = operation_node(TESTS, TEST_STAGE, Capability::Text).unwrap();
    let ceiling = resources(1, GENEROUS_USD);
    Job {
        id: "provider-test".into(),
        video_id: TESTS.into(),
        action: Action::ProviderTest {
            providers: BTreeSet::from([ROUTE.into()]),
        },
        capabilities,
        resources: ceiling.clone(),
        operations: BTreeMap::from([(node, ceiling)]),
        ..fixture.job("unused", TESTS, 0)
    }
}

const fn text_call(request: &str) -> retold_engine::gate::PaidCall<'_> {
    retold_engine::gate::PaidCall {
        request,
        logical: "",
        stage: TEST_STAGE,
        provider: ROUTE,
        capability: PaidCapability::Text,
        ceiling: QUOTE,
    }
}

/// The fixture's limits plus a lifetime ceiling for the route's provider account.
fn with_provider_limit() -> Vec<Limit> {
    let mut limits = limits(GENEROUS_USD);
    limits.push(Limit {
        scope: Scope::Provider("openrouter-account".into()),
        window: Window::Lifetime,
        resources: resources(GENEROUS_REQUESTS, GENEROUS_USD),
    });
    limits
}

#[test]
fn provider_tests_draw_on_provider_ceilings_instead_of_workspace_and_video() {
    use retold_engine::gate::PaidGate;
    let fixture = Fixture::new(GENEROUS_USD);
    let account = fixture.account();
    let exporting = provider_test_job(&fixture, vec![Capability::Text, Capability::Export]);
    assert!(account.admit(exporting).is_err());
    let job = provider_test_job(&fixture, vec![Capability::Text]);
    account.admit(job.clone()).unwrap();
    let gate = AccountGate::for_job(fixture.account(), &job);
    let unlimited = gate.reserve(&text_call("first"));
    assert!(
        matches!(unlimited, Err(GateError::Refused(_))),
        "{unlimited:?}"
    );
    account.set_limits(with_provider_limit()).unwrap();
    gate.reserve(&text_call("second")).unwrap();
    gate.dispatch("second").unwrap();
    gate.settle("second", &known_cost(SETTLED_COST)).unwrap();
}
