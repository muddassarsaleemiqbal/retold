//! Workspace backups capture only managed content and immutable accounting evidence, offline.
#![expect(clippy::unwrap_used, reason = "isolated fixtures must succeed")]
use retold_app::account::{
    Account,
    types::{Limit, Scope, Unit, Window, Workspace},
};
use retold_app::storage;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const SECRET_CANARY: &str = "private-provider-secret-canary";
const LIMIT_MICROS: u64 = 1_000_000;
const HASH_LENGTH: usize = 64;

struct Fixture {
    home: tempfile::TempDir,
    root: PathBuf,
    metadata: PathBuf,
    account: Account,
}

fn fixture() -> Fixture {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("workspace-root");
    let metadata = root.join("channel/story/.retold");
    std::fs::create_dir_all(&metadata).unwrap();
    save_manifest(&metadata);
    let account = registered_account(home.path(), &root);
    Fixture {
        home,
        root,
        metadata,
        account,
    }
}

fn registered_account(home: &Path, root: &Path) -> Account {
    let directory = home.join(if cfg!(target_os = "macos") {
        "Library/Application Support/retold-account"
    } else {
        ".local/share/retold-account"
    });
    let account = Account::initialize(&directory, "UTC").unwrap();
    account
        .register_workspace(Workspace {
            id: "workspace-fixture".into(),
            root: root.into(),
        })
        .unwrap();
    account
}

fn save_manifest(metadata: &Path) {
    let manifest = json!({"schema":1,"id":"video-fixture","created":"2026-10-19T00:00:00Z","title":"Story",
        "guide":{"id":"fixture","version":"1","hash":"fixture"},"channel":"channel","inputs":{},
        "variants":["en:en"],"quality":"standard","budget":0,"skip_reviews":true,
        "folder":["story"],"cast_levels":0});
    std::fs::write(
        metadata.join("run.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
}

impl Fixture {
    fn output(&self) -> PathBuf {
        self.home.path().join("backup")
    }

    fn invoke(&self, extra: &[&str]) -> (Value, i32) {
        let executable = assert_cmd::cargo::cargo_bin("retold");
        let mut command = std::process::Command::new(executable);
        command
            .env("HOME", self.home.path())
            .env_remove("XDG_DATA_HOME")
            .env("PATH", "");
        command.args(["--json", "--library"]).arg(&self.root);
        command
            .args(["storage", "backup", "--output"])
            .arg(self.output())
            .args(extra);
        let output = command.output().unwrap();
        assert_eq!(output.stderr, Vec::<u8>::new());
        (
            serde_json::from_slice(&output.stdout).unwrap(),
            output.status.code().unwrap(),
        )
    }

    fn assert_no_staging(&self) {
        let entries = std::fs::read_dir(self.home.path()).unwrap();
        for entry in entries {
            let name = entry.unwrap().file_name();
            assert!(!name.to_string_lossy().starts_with(".retold-backup-"));
        }
    }
}

#[test]
fn preview_is_read_only_and_created_backup_preserves_files_and_account_limits() {
    let fixture = fixture();
    let limits = vec![Limit {
        scope: Scope::Account,
        window: Window::Day,
        resources: [(Unit::UsdMicros, LIMIT_MICROS)].into(),
    }];
    fixture.account.set_limits(limits).unwrap();
    let (preview, exit) = fixture.invoke(&["--dry-run"]);
    assert_eq!(exit, 0, "{preview}");
    assert!(!fixture.output().exists());
    assert!(!fixture.metadata.join("lock").exists());
    let (created, exit) = fixture.invoke(&[]);
    assert_eq!(exit, 0, "{created}");
    verify_financial_snapshot(&fixture);
    assert_eq!(fixture.invoke(&[]).1, 2, "never overwrite");
    fixture.assert_no_staging();
}

fn verify_financial_snapshot(fixture: &Fixture) {
    let backup = fixture.output();
    let verified = storage::verify(&backup).unwrap();
    assert_eq!(verified.index.workspace_id, "workspace-fixture");
    assert!(!verified.execution_authorized);
    let financial =
        retold_app::account::backup::verify(&backup.join("account"), Some(&fixture.account))
            .unwrap();
    assert_eq!(financial.same_account, Some(true));
    assert_eq!(financial.manifest.contents.limits, 1);
    let original = std::fs::read(fixture.metadata.join("run.json")).unwrap();
    assert_eq!(
        std::fs::read(backup.join("workspace/channel/story/.retold/run.json")).unwrap(),
        original
    );
}

#[test]
fn busy_accounts_report_owners_without_publishing() {
    let fixture = fixture();
    let _held = fixture.account.hold("fixture running job").unwrap();
    let (blocked, exit) = fixture.invoke(&[]);
    assert_eq!(exit, 3, "{blocked}");
    assert_eq!(
        blocked["payload"]["context"]["owners"][0]["purpose"],
        "fixture running job"
    );
    assert!(!fixture.output().exists());
    fixture.assert_no_staging();
}

#[test]
fn run_owners_block_snapshots_even_without_account_ownership() {
    let fixture = fixture();
    let lock = std::fs::File::create(fixture.metadata.join("lock")).unwrap();
    lock.lock().unwrap();
    assert_eq!(fixture.invoke(&[]).1, 3);
    assert!(!fixture.output().exists());
    drop(lock);
    assert_eq!(fixture.invoke(&[]).1, 0);
}

#[test]
fn secrets_and_unmanaged_files_are_excluded_and_omissions_are_visible() {
    let fixture = fixture();
    seed_secrets(&fixture.root, &fixture.metadata);
    let (created, exit) = fixture.invoke(&[]);
    assert_eq!(exit, 0, "{created}");
    check_secret_omissions(&fixture);
}

#[cfg(unix)]
#[test]
fn symlinks_in_managed_content_fail_without_publication() {
    let fixture = fixture();
    std::os::unix::fs::symlink(
        fixture.metadata.join("run.json"),
        fixture.metadata.join("linked.json"),
    )
    .unwrap();
    assert_eq!(fixture.invoke(&[]).1, 4);
    assert!(!fixture.output().exists());
    fixture.assert_no_staging();
}

#[test]
fn invalid_manifests_and_missing_account_binding_fail_closed() {
    let fixture = fixture();
    std::fs::write(fixture.metadata.join("run.json"), b"bad manifest").unwrap();
    assert_eq!(fixture.invoke(&[]).1, 4);
    let other = fixture.home.path().join("unregistered");
    std::fs::create_dir(&other).unwrap();
    let refused = storage::creation::apply(
        &fixture.account,
        storage::creation::Request {
            root: &other,
            output: &fixture.output(),
            mode: storage::creation::Mode::Create,
            target: storage::creation::Target::Workspace,
        },
    );
    assert!(refused.is_err());
    assert!(!fixture.output().exists());
}

fn check_secret_omissions(fixture: &Fixture) {
    let index = storage::verify(&fixture.output()).unwrap().index;
    for entry in index.files {
        let contents = std::fs::read(fixture.output().join(entry.path)).unwrap();
        assert!(
            !contents
                .windows(SECRET_CANARY.len())
                .any(|window| window == SECRET_CANARY.as_bytes())
        );
    }
    let mapping = std::fs::read_to_string(fixture.output().join("workspace.json")).unwrap();
    assert!(mapping.contains(".env") && mapping.contains("auth.json"));
}

#[test]
fn nested_cast_assets_are_copied_once_and_shared_cast_is_included() {
    let fixture = fixture();
    let nested = fixture
        .metadata
        .parent()
        .unwrap()
        .join("work/04-characters/sheets");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(nested.join("cast.json"), b"nested cast").unwrap();
    add_shared_part(&fixture);
    let (created, exit) = fixture.invoke(&[]);
    assert_eq!(exit, 0, "{created}");
    let index = storage::verify(&fixture.output()).unwrap().index;
    assert_eq!(
        index
            .files
            .iter()
            .filter(|entry| entry.path.ends_with("cast.json"))
            .count(),
        2
    );
}

fn add_shared_part(fixture: &Fixture) {
    let metadata = fixture.root.join("channel/series/part/.retold");
    std::fs::create_dir_all(&metadata).unwrap();
    let manifest = shared_manifest(read_manifest(&fixture.metadata));
    std::fs::write(
        metadata.join("run.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    save_shared_cast(&fixture.root);
}

#[test]
fn missing_cache_evidence_blocks_instead_of_publishing_incomplete_history() {
    let fixture = fixture();
    let digest = "a".repeat(HASH_LENGTH);
    let state = json!({"schema":1,"active":null,"error":null,"completed": {
        "research": {"key":digest,"output_hash":digest,"receipt":null}
    }});
    std::fs::write(
        fixture.metadata.join("state.json"),
        serde_json::to_vec(&state).unwrap(),
    )
    .unwrap();
    assert_eq!(fixture.invoke(&[]).1, 4);
    assert!(!fixture.output().exists());
    fixture.assert_no_staging();
}

fn write_canary(path: PathBuf) {
    std::fs::write(path, SECRET_CANARY).unwrap();
}

fn shared_manifest(mut manifest: Value) -> Value {
    manifest["id"] = json!("shared-part");
    manifest["folder"] = json!(["series", "part"]);
    manifest["cast_levels"] = json!(1);
    manifest
}

fn save_shared_cast(root: &Path) {
    let cast = root.join("channel/series/cast");
    std::fs::create_dir(&cast).unwrap();
    std::fs::write(cast.join("cast.json"), b"shared cast").unwrap();
}

fn seed_secrets(root: &Path, metadata: &Path) {
    write_canary(root.join("providers.toml"));
    for name in [".env", "auth.json"] {
        write_canary(metadata.join(name));
    }
}

fn read_manifest(metadata: &Path) -> Value {
    let bytes = std::fs::read(metadata.join("run.json")).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[test]
fn selected_video_backup_excludes_unrelated_videos() {
    let fixture = fixture();
    let second_metadata = fixture.root.join("channel/other-story/.retold");
    std::fs::create_dir_all(&second_metadata).unwrap();
    let mut second_manifest = read_manifest(&fixture.metadata);
    second_manifest["id"] = json!("other-video");
    second_manifest["folder"] = json!(["other-story"]);
    std::fs::write(
        second_metadata.join("run.json"),
        serde_json::to_vec(&second_manifest).unwrap(),
    )
    .unwrap();

    let (created, exit) = fixture.invoke(&["--video", "video-fixture"]);
    assert_eq!(exit, 0, "{created}");
    let verified = storage::verify(&fixture.output()).unwrap();
    let paths: Vec<String> = verified
        .index
        .files
        .into_iter()
        .map(|entry| entry.path)
        .collect();
    assert!(paths.iter().any(|entry| entry.contains("story")));
    assert!(!paths.iter().any(|entry| entry.contains("other-story")));

    let fixture2 = self::fixture();
    let (_failed, exit) = fixture2.invoke(&["--video", "nonexistent-video"]);
    assert_ne!(exit, 0);
}

fn write_second_manifest(dir: &Path, source: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    let mut manifest = read_manifest(source);
    manifest["id"] = json!("second-video");
    manifest["folder"] = json!(["second"]);
    let bytes = serde_json::to_vec(&manifest).unwrap();
    std::fs::write(dir.join("run.json"), bytes).unwrap();
}

fn setup_second_workspace(fixture: &Fixture) {
    let second_root = fixture.home.path().join("second-workspace");
    let second_meta = second_root.join("channel/second/.retold");
    write_second_manifest(&second_meta, &fixture.metadata);
    let workspace = Workspace {
        id: "second-workspace".into(),
        root: second_root,
    };
    fixture.account.register_workspace(workspace).unwrap();
}

#[test]
fn multi_workspace_backup_all_captures_all_registered_roots() {
    let fixture = fixture();
    setup_second_workspace(&fixture);

    let (preview, exit) = fixture.invoke(&["--all", "--dry-run"]);
    assert_eq!(exit, 0, "{preview}");
    assert_eq!(
        preview["payload"]["data"]["multi_backup_preview"]["workspace_count"],
        2
    );

    let (created, exit) = fixture.invoke(&["--all"]);
    assert_eq!(exit, 0, "{created}");
    assert!(fixture.output().join("workspace-fixture").is_dir());
    assert!(fixture.output().join("second-workspace").is_dir());
}

#[path = "storage_backup/semantics.rs"]
mod semantics;

#[path = "storage_backup/restoration.rs"]
mod restoration;

fn backed_up() -> Fixture {
    let fixture = fixture();
    let (result, exit) = fixture.invoke(&[]);
    assert_eq!(exit, 0, "{result}");
    fixture
}

#[path = "storage_backup/activation.rs"]
mod activation;
