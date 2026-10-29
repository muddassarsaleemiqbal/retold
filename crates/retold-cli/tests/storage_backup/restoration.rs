//! Restore is offline, non-overwriting and leaves every restored run inactive.
use super::*;
use retold_engine::recovery::INACTIVE_MARKER;

#[test]
fn preview_writes_nothing_and_restore_preserves_history_without_installing_accounting() {
    let fixture = backed_up();
    let output = fixture.home.path().join("restored");
    let before = std::fs::read(fixture.output().join("index.json")).unwrap();
    let (preview, exit) = invoke(&fixture, &output, &["--dry-run"]);
    assert_eq!(exit, 0, "{preview}");
    assert!(!output.exists());
    fixture.assert_no_staging();
    let (restored, exit) = invoke(&fixture, &output, &[]);
    assert_eq!(exit, 0, "{restored}");
    assert_eq!(
        restored["payload"]["data"]["workspace_restored"]["inactive"]["execution_authorized"],
        false
    );
    verify_restored(&fixture, &output);
    assert_eq!(
        std::fs::read(fixture.output().join("index.json")).unwrap(),
        before
    );
    assert!(
        storage::verify(&fixture.output())
            .unwrap()
            .managed_workspace_verified
    );
    fixture.assert_no_staging();
}

#[test]
fn restore_does_not_roll_back_newer_canonical_limits() {
    let fixture = backed_up();
    fixture
        .account
        .set_limits(vec![Limit {
            scope: Scope::Account,
            window: Window::Month,
            resources: [(Unit::UsdMicros, LIMIT_MICROS)].into(),
        }])
        .unwrap();
    let before = serde_json::to_value(fixture.account.summary().unwrap()).unwrap();
    let output = fixture.home.path().join("restored");
    assert_eq!(invoke(&fixture, &output, &[]).1, 0);
    let after = serde_json::to_value(fixture.account.summary().unwrap()).unwrap();
    assert_eq!(before, after);
    let archived = retold_app::account::backup::verify(
        &output.join(".retold-recovery/account"),
        Some(&fixture.account),
    )
    .unwrap();
    assert_eq!(archived.manifest.contents.limits, 0);
}

#[test]
fn restored_active_tasks_are_stopped_with_original_state_retained() {
    let fixture = fixture();
    let original = save_active_state(&fixture);
    assert_eq!(fixture.invoke(&[]).1, 0);
    let output = fixture.home.path().join("restored");
    assert_eq!(invoke(&fixture, &output, &[]).1, 0);
    let stopped = read_json(&output.join("channel/story/.retold/state.json"));
    assert_eq!(stopped["active"], Value::Null);
    assert_eq!(stopped["active_tasks"], json!([]));
    assert_eq!(
        std::fs::read(
            output.join(".retold-recovery/original-state/channel/story/.retold/state.json")
        )
        .unwrap(),
        original
    );
    assert_eq!(
        std::fs::read(fixture.metadata.join("state.json")).unwrap(),
        original
    );
}

#[test]
fn restore_works_without_original_root_or_local_account_and_remains_inspectable() {
    let fixture = backed_up();
    std::fs::rename(&fixture.root, fixture.home.path().join("moved-original")).unwrap();
    let output = fixture.home.path().join("restored");
    let empty_home = tempfile::tempdir().unwrap();
    let command = restore_command(&fixture, &output);
    let result = command_with_home(command, empty_home.path());
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stdout)
    );
    assert_eq!(std::fs::read_dir(empty_home.path()).unwrap().count(), 0);
    let mut listing = std::process::Command::new(assert_cmd::cargo::cargo_bin("retold"));
    listing.env("HOME", empty_home.path()).env("PATH", "");
    listing.args(["--json", "--library"]).arg(&output);
    let result = listing.args(["video", "list"]).output().unwrap();
    assert!(result.status.success());
    assert!(String::from_utf8_lossy(&result.stdout).contains("video-fixture"));
}

#[test]
fn refuses_existing_destinations_and_byte_only_backups() {
    let fixture = backed_up();
    let output = fixture.home.path().join("restored");
    std::fs::create_dir(&output).unwrap();
    assert_eq!(invoke(&fixture, &output, &[]).1, 2);
    assert_eq!(std::fs::read_dir(&output).unwrap().count(), 0);
    refuse_invalid_archives(&fixture);
}

fn refuse_invalid_archives(fixture: &Fixture) {
    let new_output = fixture.home.path().join("new-restored");
    let path = fixture.output().join("index.json");
    let mut index = read_json(&path);
    index["schema_version"] = json!(1);
    save_json(&path, &index);
    assert_eq!(invoke(fixture, &new_output, &[]).1, 4);
    assert!(!new_output.exists());
    fixture.assert_no_staging();
}

#[test]
fn corrupt_backup_is_refused_without_staging() {
    let fixture = backed_up();
    let output = fixture.home.path().join("restored");
    std::fs::write(fixture.output().join("workspace.json"), b"{}").unwrap();
    assert_eq!(invoke(&fixture, &output, &[]).1, 4);
    assert!(!output.exists());
    fixture.assert_no_staging();
}

#[test]
fn restored_roots_and_individual_runs_refuse_account_registration_and_engine_writes() {
    let fixture = backed_up();
    let output = fixture.home.path().join("restored");
    assert_eq!(invoke(&fixture, &output, &[]).1, 0);
    let run = output.join("channel/story");
    for root in [&output, &run] {
        let workspace = Workspace {
            id: "fresh-clone".into(),
            root: root.clone(),
        };
        assert!(fixture.account.register_workspace(workspace).is_err());
        assert!(root.join(INACTIVE_MARKER).is_file());
    }
    let layout = retold_engine::RunLayout::from_video_dir(&output, "channel", &run, 0).unwrap();
    assert!(retold_engine::store::Store::open(&layout).is_err());
    assert!(!layout.lock_file().exists());
}

fn verify_restored(fixture: &Fixture, output: &Path) {
    let original = std::fs::read(fixture.metadata.join("run.json")).unwrap();
    assert_eq!(
        std::fs::read(output.join("channel/story/.retold/run.json")).unwrap(),
        original
    );
    let account = output.join(".retold-recovery/account");
    let verified = retold_app::account::backup::verify(&account, Some(&fixture.account)).unwrap();
    assert_eq!(verified.same_account, Some(true));
    assert!(!output.join("accounting.sqlite").exists());
    assert_eq!(fixture.account.summary().unwrap().workspaces, 1);
}

fn restore_command(fixture: &Fixture, output: &Path) -> std::process::Command {
    let mut command = std::process::Command::new(assert_cmd::cargo::cargo_bin("retold"));
    command
        .env("HOME", fixture.home.path())
        .env_remove("XDG_DATA_HOME")
        .env("PATH", "");
    command
        .args(["--json", "storage", "restore"])
        .arg(fixture.output());
    command.arg("--into").arg(output);
    command
}

fn invoke(fixture: &Fixture, output: &Path, extra: &[&str]) -> (Value, i32) {
    let output = restore_command(fixture, output)
        .args(extra)
        .output()
        .unwrap();
    assert_eq!(output.stderr, Vec::<u8>::new());
    (
        serde_json::from_slice(&output.stdout).unwrap(),
        output.status.code().unwrap(),
    )
}

fn command_with_home(mut command: std::process::Command, home: &Path) -> std::process::Output {
    command.env("HOME", home).output().unwrap()
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn save_json(path: &Path, value: &Value) {
    std::fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
}

fn save_active_state(fixture: &Fixture) -> Vec<u8> {
    let state = json!({"schema":1,"active":"research","active_tasks":["research"],"error":null,"completed":{}});
    let original = serde_json::to_vec(&state).unwrap();
    std::fs::write(fixture.metadata.join("state.json"), &original).unwrap();
    original
}
