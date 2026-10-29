//! Activation never rolls back finance or admits a saved job.
use super::*;
use retold_app::account::types::Summary;
use retold_app::storage::{activation, restoration};
use retold_engine::recovery::{INACTIVE_MARKER, ensure_active};

fn restored() -> (Fixture, PathBuf) {
    let fixture = backed_up();
    let output = fixture.home.path().join("restored");
    restoration::apply(&fixture.output(), &output, restoration::Mode::Restore).unwrap();
    (fixture, output)
}

fn preview(fixture: &Fixture, root: &Path) -> Value {
    activation::apply(&fixture.account, root, activation::Mode::Preview).unwrap()
}

fn hash(plan: &Value) -> &str {
    plan["activation_plan"]["plan_hash"].as_str().unwrap()
}

fn assert_limits_preserved(fixture: &Fixture, before: &Summary) {
    let current = fixture.account.summary().unwrap();
    assert_eq!(current.limits, before.limits);
    assert_eq!(current.attempts, before.attempts);
    assert_eq!(current.jobs, before.jobs);
}

fn assert_roots_transitioned(fixture: &Fixture, root: &Path) {
    let account = &fixture.account;
    let registered = account.workspace_at(root).unwrap();
    let workspace = registered.unwrap();
    assert_eq!(workspace.id, "workspace-fixture");
    assert!(ensure_active(root).is_ok());
    assert!(ensure_active(&fixture.root).is_err());
    let reregister = account.register_workspace(Workspace {
        id: "new-original-id".into(),
        root: fixture.root.clone(),
    });
    assert!(reregister.is_err());
}

#[test]
fn reviewed_activation_preserves_newer_limits_and_retires_original_root() {
    let (fixture, root) = restored();
    fixture
        .account
        .set_limits(vec![Limit {
            scope: Scope::Account,
            window: Window::Month,
            resources: [(Unit::UsdMicros, LIMIT_MICROS)].into(),
        }])
        .unwrap();
    let before = fixture.account.summary().unwrap();
    let plan = preview(&fixture, &root);
    assert!(!fixture.root.join(INACTIVE_MARKER).exists());
    assert!(!root.join("channel/story/.retold/lock").exists());
    activation::apply(
        &fixture.account,
        &root,
        activation::Mode::Activate(hash(&plan)),
    )
    .unwrap();
    assert_limits_preserved(&fixture, &before);
    assert_roots_transitioned(&fixture, &root);
    let again = activation::apply(
        &fixture.account,
        &root,
        activation::Mode::Activate(hash(&plan)),
    );
    assert!(again.is_ok(), "{again:?}");
}

#[test]
fn stale_plan_and_busy_account_never_change_ownership() {
    let (fixture, root) = restored();
    let plan = preview(&fixture, &root);
    fixture.account.set_limits(vec![]).unwrap();
    assert!(
        activation::apply(
            &fixture.account,
            &root,
            activation::Mode::Activate(hash(&plan))
        )
        .is_err()
    );
    assert!(root.join(INACTIVE_MARKER).is_file());
    assert!(!fixture.root.join(INACTIVE_MARKER).exists());
    let plan = preview(&fixture, &root);
    let _busy = fixture.account.hold("test live execution").unwrap();
    assert!(
        activation::apply(
            &fixture.account,
            &root,
            activation::Mode::Activate(hash(&plan))
        )
        .is_err()
    );
    assert!(!fixture.root.join(INACTIVE_MARKER).exists());
}

#[test]
fn another_account_and_missing_original_remain_inactive() {
    let (fixture, root) = restored();
    let other = Account::initialize(&fixture.home.path().join("other-account"), "UTC").unwrap();
    assert!(activation::apply(&other, &root, activation::Mode::Preview).is_err());
    std::fs::rename(&fixture.root, fixture.home.path().join("missing-original")).unwrap();
    assert!(activation::apply(&fixture.account, &root, activation::Mode::Preview).is_err());
    assert!(root.join(INACTIVE_MARKER).is_file());
}

#[test]
fn modified_history_and_injected_hooks_block_activation() {
    let (fixture, root) = restored();
    std::fs::create_dir(root.join("channel/story/.retold/hooks")).unwrap();
    std::fs::write(
        root.join("channel/story/.retold/hooks/run.sh"),
        b"never execute",
    )
    .unwrap();
    assert!(activation::apply(&fixture.account, &root, activation::Mode::Preview).is_err());
    assert!(!fixture.root.join(INACTIVE_MARKER).exists());
}

#[test]
fn canonical_cli_requires_review_and_keeps_workspace_identity_in_new_plans() {
    let (fixture, root) = restored();
    let (plan, exit) = invoke(&fixture, &root, &["--dry-run"]);
    assert_eq!(exit, 0, "{plan}");
    let reviewed = plan["payload"]["data"]["activation_plan"]["plan_hash"]
        .as_str()
        .unwrap();
    let (activated, exit) = invoke(&fixture, &root, &["--plan-hash", reviewed]);
    assert_eq!(exit, 0, "{activated}");
    assert_eq!(
        fixture.account.workspace_at(&root).unwrap().unwrap().id,
        "workspace-fixture"
    );
}

fn setup_other_account(path: &Path) -> (Account, Summary) {
    let other_account = Account::initialize(path, "UTC").unwrap();
    other_account
        .set_limits(vec![Limit {
            scope: Scope::Account,
            window: Window::Month,
            resources: [(Unit::UsdMicros, LIMIT_MICROS)].into(),
        }])
        .unwrap();
    let summary = other_account.summary().unwrap();
    (other_account, summary)
}

#[test]
fn cross_installation_reconciliation_binds_workspace_to_target_account() {
    let (fixture, root) = restored();
    let (other_account, before_summary) =
        setup_other_account(&fixture.home.path().join("cross-machine-account"));

    assert!(activation::apply(&other_account, &root, activation::Mode::Preview).is_err());

    let plan =
        activation::apply(&other_account, &root, activation::Mode::ReconcilePreview).unwrap();
    let hash = plan["reconciliation_plan"]["plan_hash"].as_str().unwrap();

    verify_unattested_reconcile_refused(&other_account, &root, hash);
    verify_attested_reconcile_succeeds(&other_account, &root, hash);

    let after_summary = other_account.summary().unwrap();
    assert_eq!(after_summary.limits, before_summary.limits);
    let registered = other_account.workspace_at(&root).unwrap().unwrap();
    assert_eq!(registered.id, "workspace-fixture");
}

fn verify_unattested_reconcile_refused(account: &Account, root: &Path, hash: &str) {
    let refused = activation::apply(
        account,
        root,
        activation::Mode::ReconcileActivate(activation::ReconcileConfirmation {
            plan_hash: hash,
            attest_prior_stopped: false,
        }),
    );
    assert!(refused.is_err());
    assert!(root.join(INACTIVE_MARKER).is_file());
}

fn verify_attested_reconcile_succeeds(account: &Account, root: &Path, hash: &str) {
    let activated = activation::apply(
        account,
        root,
        activation::Mode::ReconcileActivate(activation::ReconcileConfirmation {
            plan_hash: hash,
            attest_prior_stopped: true,
        }),
    )
    .unwrap();
    assert_eq!(
        activated["workspace_reconciled"]["workspace_id"],
        "workspace-fixture"
    );
    assert!(!root.join(INACTIVE_MARKER).exists());
}

fn invoke(fixture: &Fixture, root: &Path, extra: &[&str]) -> (Value, i32) {
    let bin = assert_cmd::cargo::cargo_bin("retold");
    let mut command = std::process::Command::new(bin);
    command.env("HOME", fixture.home.path());
    command.env_remove("XDG_DATA_HOME");
    command.env("PATH", "");
    command.args(["--json", "storage", "activate"]);
    command.arg(root);
    command.args(extra);
    let output = command.output().unwrap();
    assert_eq!(output.stderr, Vec::<u8>::new());
    (
        serde_json::from_slice(&output.stdout).unwrap(),
        output.status.code().unwrap(),
    )
}
