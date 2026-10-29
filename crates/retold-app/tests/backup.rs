//! The maintenance barrier lets jobs and changes share the account, lets a backup capture it only
//! when nobody holds it, and survives owners that crash. Backups verify offline. No providers.
#![expect(clippy::unwrap_used, reason = "Isolated test fixtures must succeed")]
use retold_app::account::backup::{self, Manifest};
use retold_app::account::{Account, Error, barrier, types::*};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

const QUOTE_MICROS: u64 = 1_000_000;
const LIMIT_MICROS: u64 = 10_000_000;
const LIMIT_REQUESTS: u64 = 10;
const SNAPSHOT_PAUSE: Duration = Duration::from_millis(200);
const CHILD_WAIT: Duration = Duration::from_secs(30);
const CHILD_POLL: Duration = Duration::from_millis(20);

struct Fixture {
    sandbox: tempfile::TempDir,
    directory: PathBuf,
    account: Account,
}

fn fixture() -> Fixture {
    let sandbox = tempfile::tempdir().unwrap();
    let directory = sandbox.path().join("account");
    let account = Account::initialize(&directory, "UTC").unwrap();
    Fixture {
        sandbox,
        directory,
        account,
    }
}

impl Fixture {
    fn output(&self, name: &str) -> PathBuf {
        self.sandbox.path().join(name)
    }

    /// Entries beside the backups, to prove refused or failed backups leave nothing behind.
    fn entries(&self) -> Vec<String> {
        let entries = std::fs::read_dir(self.sandbox.path()).unwrap();
        let mut names: Vec<_> = entries
            .map(|entry| {
                let name = entry.unwrap().file_name();
                name.to_string_lossy().into_owned()
            })
            .collect();
        names.sort();
        names
    }
}

fn money(usd_micros: u64, requests: u64) -> Resources {
    BTreeMap::from([(Unit::UsdMicros, usd_micros), (Unit::Requests, requests)])
}

fn hash(value: &str) -> String {
    retold_engine::store::hash(&value).unwrap()
}

/// One admitted job with one dispatched attempt that still holds its quote.
fn record_attempt(fixture: &Fixture) {
    let account = &fixture.account;
    let root = fixture.directory.parent().unwrap();
    let workspace = Workspace {
        id: "workspace".into(),
        root: root.canonicalize().unwrap(),
    };
    account.register_workspace(workspace.clone()).unwrap();
    account.register_route("route", "provider").unwrap();
    let limits = [
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
    account.set_limits(limits.into()).unwrap();
    account.admit(job(workspace)).unwrap();
    account.reserve(reservation()).unwrap();
    account.mark_dispatched("attempt", &hash("plan")).unwrap();
}

fn job(workspace: Workspace) -> Job {
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
        operations: BTreeMap::from([(hash("attempt"), money(QUOTE_MICROS, 1))]),
        deadline: None,
    }
}

fn reservation() -> Reservation {
    Reservation {
        attempt_id: "attempt".into(),
        job_id: "job".into(),
        operation_key: hash("attempt"),
        node: None,
        route: "route".into(),
        capability: Capability::Text,
        plan_hash: hash("plan"),
        resources: money(QUOTE_MICROS, 1),
    }
}

fn day_limit(usd_micros: u64) -> Vec<Limit> {
    vec![Limit {
        scope: Scope::Account,
        window: Window::Day,
        resources: money(usd_micros, LIMIT_REQUESTS),
    }]
}

#[test]
fn backups_are_refused_at_once_while_a_job_holds_the_account() {
    let fixture = fixture();
    let before = fixture.entries();
    let job = fixture.account.hold("video continue job-1").unwrap();
    job.set_limits(day_limit(LIMIT_MICROS)).unwrap();
    fixture.account.set_limits(day_limit(LIMIT_MICROS)).unwrap();
    let refused = fixture.account.backup(&fixture.output("busy"));
    let Err(Error::Occupied(owners)) = refused else {
        panic!("expected the running job to be reported: {refused:?}");
    };
    assert_eq!(owners[0].purpose, "video continue job-1");
    assert_eq!(owners[0].pid, std::process::id());
    assert_eq!(fixture.entries(), before, "nothing is staged or published");
    drop(job);
    fixture.account.backup(&fixture.output("idle")).unwrap();
}

#[test]
fn changes_wait_for_a_snapshot_instead_of_failing() {
    let fixture = fixture();
    let exclusive = barrier::exclusive(&fixture.directory).unwrap();
    let directory = fixture.directory.clone();
    let change = std::thread::spawn(move || {
        let account = Account::open(&directory).unwrap();
        account.set_limits(day_limit(QUOTE_MICROS))
    });
    std::thread::sleep(SNAPSHOT_PAUSE);
    assert!(
        !change.is_finished(),
        "admission is closed during a snapshot"
    );
    drop(exclusive);
    change.join().unwrap().unwrap();
    assert_eq!(fixture.account.summary().unwrap().limits.len(), 1);
}

#[test]
fn backups_capture_coherent_accounting_and_verify_offline() {
    let fixture = fixture();
    record_attempt(&fixture);
    let output = fixture.output("backup");
    let published = fixture.account.backup(&output).unwrap();
    let contents = &published.manifest.contents;
    assert_eq!((contents.attempts, contents.unresolved), (1, 1));
    assert_eq!(contents.usd_micros.held, QUOTE_MICROS);
    let verified = backup::verify(&output, Some(&fixture.account)).unwrap();
    assert_eq!(verified.manifest, published.manifest);
    assert_eq!(verified.same_account, Some(true));
    let other = crate::fixture();
    let foreign = backup::verify(&output, Some(&other.account)).unwrap();
    assert_eq!(foreign.same_account, Some(false));
    assert_eq!(backup::verify(&output, None).unwrap().same_account, None);
    assert!(
        !fixture
            .entries()
            .iter()
            .any(|name| name.contains("partial"))
    );
}

#[test]
fn tampered_backups_fail_verification() {
    let fixture = fixture();
    record_attempt(&fixture);
    let output = fixture.output("backup");
    fixture.account.backup(&output).unwrap();
    let manifest_path = output.join("backup.json");
    let original = std::fs::read(&manifest_path).unwrap();
    let mut manifest: Manifest = serde_json::from_slice(&original).unwrap();
    manifest.contents.unresolved = 0;
    std::fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    assert!(matches!(
        backup::verify(&output, None),
        Err(Error::BackupInvalid(_))
    ));
    std::fs::write(&manifest_path, original).unwrap();
    tamper_database(&output);
}

fn tamper_database(output: &Path) {
    let database = output.join("accounting.sqlite");
    let mut bytes = std::fs::read(&database).unwrap();
    bytes.pop();
    std::fs::write(&database, bytes).unwrap();
    assert!(matches!(
        backup::verify(output, None),
        Err(Error::BackupInvalid(_))
    ));
}

#[test]
fn backups_never_overwrite_or_nest_inside_the_account() {
    let fixture = fixture();
    let existing = fixture.output("existing");
    std::fs::create_dir(&existing).unwrap();
    let refused = [existing, fixture.directory.join("inside")];
    for output in refused {
        let attempt = fixture.account.backup(&output);
        assert!(matches!(attempt, Err(Error::Invalid(_))), "{output:?}");
    }
    let preview = fixture.account.preview_backup(&fixture.output("new"));
    assert_eq!(preview.unwrap().busy, Vec::new());
    assert!(!fixture.output("new").exists(), "previews write nothing");
}

#[test]
fn hold_in_child() {
    let Ok(directory) = std::env::var("RETOLD_TEST_ACCOUNT") else {
        return;
    };
    let directory = Path::new(&directory);
    let account = Account::open(directory).unwrap();
    let _held = account.hold("crashing job").unwrap();
    std::fs::write(directory.join("held.ready"), b"ready").unwrap();
    std::thread::sleep(CHILD_WAIT);
}

#[test]
fn owners_that_crash_release_the_account() {
    let fixture = fixture();
    let executable = std::env::current_exe().unwrap();
    let mut command = std::process::Command::new(executable);
    command
        .args(["--exact", "hold_in_child", "--nocapture"])
        .env("RETOLD_TEST_ACCOUNT", &fixture.directory)
        .stdout(std::process::Stdio::null());
    let mut child = command.spawn().unwrap();
    wait_until(|| fixture.directory.join("held.ready").exists());
    let busy = fixture.account.backup(&fixture.output("busy"));
    let Err(Error::Occupied(owners)) = busy else {
        panic!("expected the child to hold the account: {busy:?}");
    };
    assert_eq!(owners[0].pid, child.id());
    child.kill().unwrap();
    child.wait().unwrap();
    assert_eq!(barrier::owners(&fixture.directory).unwrap(), Vec::new());
    fixture.account.set_limits(day_limit(LIMIT_MICROS)).unwrap();
    fixture.account.backup(&fixture.output("after")).unwrap();
}

fn wait_until(ready: impl Fn() -> bool) {
    let deadline = std::time::Instant::now() + CHILD_WAIT;
    while !ready() {
        assert!(
            std::time::Instant::now() < deadline,
            "child never held the account"
        );
        std::thread::sleep(CHILD_POLL);
    }
}

#[test]
fn manifest_timezone_must_match_the_snapshot() {
    let fixture = fixture();
    let output = fixture.output("backup");
    fixture.account.backup(&output).unwrap();
    let path = output.join("backup.json");
    let bytes = std::fs::read(&path).unwrap();
    let mut manifest: Manifest = serde_json::from_slice(&bytes).unwrap();
    manifest.timezone = "Asia/Karachi".into();
    std::fs::write(path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    assert!(matches!(
        backup::verify(&output, None),
        Err(Error::BackupInvalid(_))
    ));
}
