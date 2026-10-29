//! Spend reports attribute charges to the period their reservation started in, keep unresolved
//! holds visible across day and month boundaries, and never change the account. Targeted limit
//! changes keep every recorded charge and hold.
#![expect(clippy::unwrap_used, reason = "Isolated test fixtures must succeed")]
use retold_app::account::report::{Grouping, Query, Report, Row};
use retold_app::account::{Account, Clock, Error, Usd, limits, types::*};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

const CEILING_MICROS: u64 = 5_000_000;
const PROVIDER_CEILING_MICROS: u64 = 2_000_000;
const CEILING_REQUESTS: u64 = 100;
const QUOTE_MICROS: u64 = 1_000_000;
const JANUARY_MICROS: u64 = 400_000;
const FEBRUARY_MICROS: u64 = 250_000;
const RECONCILED_USD: &str = "0.20";
const RECONCILED_MICROS: u64 = 200_000;
/// Asia/Karachi is UTC+5: 18:00Z is 23:00 on 31 January there, 19:30Z is 00:30 on 1 February.
const JANUARY: &str = "2030-01-31T18:00:00Z";
const FEBRUARY: &str = "2030-01-31T19:30:00Z";
const OPENROUTER: &str = "openrouter:KEY";
const FAL: &str = "fal:KEY";

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
    account: Account,
    clock: Arc<TestClock>,
}

fn hash(value: &str) -> String {
    retold_engine::store::hash(&value).unwrap()
}

fn money(usd_micros: u64, requests: u64) -> Resources {
    BTreeMap::from([(Unit::UsdMicros, usd_micros), (Unit::Requests, requests)])
}

fn fixture() -> Fixture {
    let sandbox = tempfile::tempdir().unwrap();
    let directory = sandbox.path().join("account");
    Account::initialize(&directory, "Asia/Karachi").unwrap();
    let clock = Arc::new(TestClock(Mutex::new(JANUARY.parse().unwrap())));
    let account = Account::with_clock(&directory, Arc::<TestClock>::clone(&clock)).unwrap();
    let workspace = Workspace {
        id: "workspace".into(),
        root: sandbox.path().canonicalize().unwrap(),
    };
    account.register_workspace(workspace.clone()).unwrap();
    account.register_route("openrouter", OPENROUTER).unwrap();
    account.register_route("fal", FAL).unwrap();
    account.set_limits(limits()).unwrap();
    let mut serial = job("a", &workspace);
    serial.series_id = Some("saga".into());
    account.admit(serial).unwrap();
    account.admit(job("b", &workspace)).unwrap();
    Fixture {
        sandbox,
        account,
        clock,
    }
}

fn limits() -> Vec<Limit> {
    let ceiling = |scope, window| Limit {
        scope,
        window,
        resources: money(CEILING_MICROS, CEILING_REQUESTS),
    };
    let provider = Limit {
        scope: Scope::Provider(OPENROUTER.into()),
        window: Window::Lifetime,
        resources: money(PROVIDER_CEILING_MICROS, CEILING_REQUESTS),
    };
    vec![
        ceiling(Scope::Account, Window::Day),
        ceiling(Scope::Account, Window::Month),
        ceiling(Scope::Workspace("workspace".into()), Window::Lifetime),
        ceiling(Scope::Video("video-a".into()), Window::Lifetime),
        ceiling(Scope::Video("video-b".into()), Window::Lifetime),
        ceiling(Scope::Series("saga".into()), Window::Lifetime),
        provider,
    ]
}

fn job(name: &str, workspace: &Workspace) -> Job {
    let operations = ["1", "2"]
        .map(|key| (hash(&format!("{name}{key}")), money(QUOTE_MICROS, 1)))
        .into();
    Job {
        id: name.into(),
        workspace: workspace.clone(),
        video_id: format!("video-{name}"),
        series_id: Option::default(),
        plan_hash: hash("plan"),
        input_versions: BTreeMap::from([("manifest".into(), hash("inputs"))]),
        action: Action::Continue,
        routes: ["openrouter".into(), "fal".into()].into(),
        capabilities: vec![Capability::Text],
        resources: money(CEILING_MICROS, CEILING_REQUESTS),
        operations,
        deadline: None,
    }
}

/// Reserves and dispatches `attempt` (job letter, then operation digit) on `route`.
fn dispatch(account: &Account, attempt: &str, route: &str) {
    account.reserve(reservation(attempt, route)).unwrap();
    account.mark_dispatched(attempt, &hash("plan")).unwrap();
}

fn reservation(attempt: &str, route: &str) -> Reservation {
    Reservation {
        attempt_id: attempt.into(),
        job_id: attempt[..1].into(),
        operation_key: hash(attempt),
        node: None,
        route: route.into(),
        capability: Capability::Text,
        plan_hash: hash("plan"),
        resources: money(QUOTE_MICROS, 1),
    }
}

fn settle(account: &Account, attempt: &str, usd_micros: u64) {
    let receipt = Receipt {
        artifact_hash: hash(attempt),
        receipt_hash: hash(&format!("receipt-{attempt}")),
        observed: BTreeMap::from([
            (Unit::UsdMicros, Some(usd_micros)),
            (Unit::Requests, Some(1)),
        ]),
    };
    account.settle(attempt, receipt).unwrap();
}

/// January: one settled charge and one unresolved hold. February: one settled charge.
fn history() -> Fixture {
    let fixture = fixture();
    dispatch(&fixture.account, "a1", "openrouter");
    settle(&fixture.account, "a1", JANUARY_MICROS);
    dispatch(&fixture.account, "a2", "fal");
    fixture.clock.set(FEBRUARY);
    dispatch(&fixture.account, "b1", "openrouter");
    settle(&fixture.account, "b1", FEBRUARY_MICROS);
    fixture
}

fn report(account: &Account, window: Window, by: Grouping) -> Report {
    account.spend(Query { window, by }).unwrap()
}

fn usd(row: &Row) -> &Balance {
    &row.totals.resources[&Unit::UsdMicros]
}

fn keys(report: &Report) -> Vec<Option<&str>> {
    report.rows.iter().map(|row| row.key.as_deref()).collect()
}

#[test]
fn charges_stay_in_their_period_while_earlier_holds_carry_over() {
    let fixture = history();
    for window in [Window::Day, Window::Month] {
        let today = report(&fixture.account, window, Grouping::Provider);
        assert_eq!(keys(&today), [Some(OPENROUTER)]);
        assert_eq!(usd(&today.rows[0]).charged, FEBRUARY_MICROS);
        let carried = today.carried.unwrap();
        assert_eq!((carried.attempts, carried.unresolved), (1, 1));
        assert_eq!(carried.resources[&Unit::UsdMicros].held, QUOTE_MICROS);
        let ceiling = &today.ceiling.unwrap()[&Unit::UsdMicros];
        assert_eq!(ceiling.used, FEBRUARY_MICROS + QUOTE_MICROS);
        assert_eq!(ceiling.available, CEILING_MICROS - ceiling.used);
    }
    let today = report(&fixture.account, Window::Day, Grouping::Provider);
    assert_eq!(today.period.as_deref(), Some("2030-02-01"));
}

#[test]
fn lifetime_reports_group_by_provider_video_series_and_day() {
    let fixture = history();
    let providers = report(&fixture.account, Window::Lifetime, Grouping::Provider);
    assert_eq!(keys(&providers), [Some(FAL), Some(OPENROUTER)]);
    assert_eq!(usd(&providers.rows[0]).held, QUOTE_MICROS);
    assert_eq!(providers.rows[0].totals.unresolved, 1);
    assert_eq!(
        usd(&providers.rows[1]).charged,
        JANUARY_MICROS + FEBRUARY_MICROS
    );
    let provider_ceiling = providers.rows[1].ceiling.as_ref().unwrap();
    let left = PROVIDER_CEILING_MICROS - JANUARY_MICROS - FEBRUARY_MICROS;
    assert_eq!(provider_ceiling[&Unit::UsdMicros].available, left);
    assert!(providers.carried.is_none() && providers.ceiling.is_none());
    let videos = report(&fixture.account, Window::Lifetime, Grouping::Video);
    assert_eq!(keys(&videos), [Some("video-a"), Some("video-b")]);
    assert_eq!(videos.rows[0].totals.attempts, 2);
    let series = report(&fixture.account, Window::Lifetime, Grouping::Series);
    assert_eq!(keys(&series), [None, Some("saga")]);
    let days = report(&fixture.account, Window::Lifetime, Grouping::Day);
    assert_eq!(keys(&days), [Some("2030-01-31"), Some("2030-02-01")]);
    assert_eq!(days.total.resources[&Unit::Requests].charged, 2);
}

#[test]
fn reconciling_a_carried_hold_charges_its_own_period_exactly_once() {
    let fixture = history();
    let cost: Usd = RECONCILED_USD.parse().unwrap();
    fixture.account.reconcile_attempt("a2", cost).unwrap();
    let today = report(&fixture.account, Window::Day, Grouping::Provider);
    assert_eq!(today.carried.unwrap().attempts, 0);
    let ceiling = &today.ceiling.unwrap()[&Unit::UsdMicros];
    assert_eq!(ceiling.used, FEBRUARY_MICROS);
    let days = report(&fixture.account, Window::Lifetime, Grouping::Day);
    assert_eq!(
        usd(&days.rows[0]).charged,
        JANUARY_MICROS + RECONCILED_MICROS
    );
    assert_eq!(days.total.unresolved, 0);
}

#[test]
fn reports_read_the_current_period_and_change_nothing() {
    let fixture = history();
    let database = fixture.sandbox.path().join("account/accounting.sqlite");
    let before = std::fs::read(&database).unwrap();
    fixture.clock.set(JANUARY); // A clock moving backwards cannot reopen January.
    let today = report(&fixture.account, Window::Day, Grouping::Day);
    assert_eq!(today.period.as_deref(), Some("2030-02-01"));
    fixture.clock.set("2030-03-01T00:00:00Z");
    let march = report(&fixture.account, Window::Month, Grouping::Provider);
    assert!(march.rows.is_empty());
    assert_eq!(march.carried.unwrap().attempts, 1, "the hold still counts");
    assert_eq!(std::fs::read(&database).unwrap(), before);
    let held = fixture.account.summary().unwrap().lifetime[&Unit::UsdMicros].held;
    assert_eq!(held, QUOTE_MICROS);
}

fn set(scope: Scope, window: Window, usd_micros: u64) -> limits::Change {
    limits::Change {
        scope,
        window,
        resources: Some(BTreeMap::from([(Unit::UsdMicros, usd_micros)])),
    }
}

#[test]
fn lowering_a_limit_below_use_blocks_new_work_and_keeps_history() {
    let fixture = history();
    let recorded = fixture.account.summary().unwrap().lifetime[&Unit::UsdMicros].clone();
    let lowered = set(Scope::Account, Window::Day, QUOTE_MICROS);
    let changed = fixture.account.change_limit(&lowered).unwrap();
    assert_eq!(changed.before.unwrap()[&Unit::UsdMicros], CEILING_MICROS);
    let after = changed.after.unwrap();
    assert_eq!(
        after[&Unit::Requests],
        CEILING_REQUESTS,
        "other units are kept"
    );
    assert_eq!(changed.exhausted, [Unit::UsdMicros]);
    let blocked = fixture.account.reserve(reservation("b2", "openrouter"));
    assert!(matches!(blocked, Err(Error::Limit)));
    let unchanged = fixture.account.summary().unwrap().lifetime[&Unit::UsdMicros].clone();
    assert_eq!(
        (unchanged.charged, unchanged.held),
        (recorded.charged, recorded.held)
    );
    let raised = set(Scope::Account, Window::Day, CEILING_MICROS);
    fixture.account.change_limit(&raised).unwrap();
    fixture
        .account
        .reserve(reservation("b2", "openrouter"))
        .unwrap();
}

#[test]
fn previews_write_nothing_and_new_limits_bound_money_and_requests() {
    let fixture = history();
    let database = fixture.sandbox.path().join("account/accounting.sqlite");
    let before = std::fs::read(&database).unwrap();
    let preview = fixture
        .account
        .preview_limit(&set(Scope::Account, Window::Month, QUOTE_MICROS))
        .unwrap();
    assert!(preview.dry_run && preview.exhausted == [Unit::UsdMicros]);
    assert_eq!(std::fs::read(&database).unwrap(), before);
    let money_only = set(
        Scope::Series("other".into()),
        Window::Lifetime,
        QUOTE_MICROS,
    );
    assert!(fixture.account.change_limit(&money_only).is_err());
    let absent = limits::Change {
        resources: None,
        ..money_only
    };
    assert!(fixture.account.change_limit(&absent).is_err());
}

#[test]
fn removing_a_required_limit_fails_closed() {
    let fixture = history();
    let removal = limits::Change {
        scope: Scope::Account,
        window: Window::Month,
        resources: None,
    };
    let removed = fixture.account.change_limit(&removal).unwrap();
    assert!(removed.after.is_none());
    let blocked = fixture.account.reserve(reservation("b2", "openrouter"));
    assert!(matches!(blocked, Err(Error::Limit)), "removal fails closed");
}

#[test]
fn scopes_parse_provider_accounts_and_refuse_paths() {
    let provider = limits::parse_scope("provider:openrouter:KEY").unwrap();
    assert_eq!(provider, Scope::Provider(OPENROUTER.into()));
    assert_eq!(limits::parse_scope("account").unwrap(), Scope::Account);
    for invalid in ["video:../escape", "galaxy:far", "video", "series:"] {
        assert!(limits::parse_scope(invalid).is_err(), "{invalid}");
    }
}
