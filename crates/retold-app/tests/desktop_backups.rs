//! Native backups preview without writing, create once, verify offline and refuse a stale review.
//! Re-runs itself with an isolated HOME so the real installation account is never used.
#![expect(clippy::unwrap_used, reason = "isolated fixture assertions")]
use std::path::Path;

use retold_app::account::{Account, types::Workspace};
use retold_app::desktop::backups;

const FORGED_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";
const CHILD: &str = "RETOLD_DESKTOP_BACKUP_CHILD";

fn manifest() -> Vec<u8> {
    let manifest = serde_json::json!({"schema":1,"id":"video-fixture","created":"2026-10-19T00:00:00Z","title":"Story",
        "guide":{"id":"fixture","version":"1","hash":"fixture"},"channel":"channel","inputs":{},
        "variants":["en:en"],"quality":"standard","budget":0,"skip_reviews":true,
        "folder":["story"],"cast_levels":0});
    serde_json::to_vec(&manifest).unwrap()
}

fn production(root: &Path) {
    let metadata = root.join("channel/story/.retold");
    std::fs::create_dir_all(&metadata).unwrap();
    std::fs::write(metadata.join("run.json"), manifest()).unwrap();
}

fn register(root: &Path) {
    let directory = retold_app::account::canonical_directory().unwrap();
    let account = Account::initialize(&directory, "UTC").unwrap();
    let registered = Workspace {
        id: "workspace-fixture".into(),
        root: root.into(),
    };
    account.register_workspace(registered).unwrap();
}

fn workspace(home: &Path) -> std::path::PathBuf {
    let root = home.join("workspace-root");
    production(&root);
    register(&root);
    root
}

fn exercise() {
    let home = std::path::PathBuf::from(std::env::var_os("HOME").unwrap());
    let root = workspace(&home);
    let output = backups::destination(&home);
    let reviewed = backups::review_backup(&root, &output).unwrap();
    assert!(!output.exists());
    backups::create_backup(&reviewed).unwrap();
    assert!(!backups::verify(&output).unwrap().execution_authorized);
    assert!(backups::create_backup(&reviewed).is_err());
    let mut forged = backups::review_backup(&root, &home.join("other")).unwrap();
    forged.preview = serde_json::json!({});
    assert!(backups::create_backup(&forged).is_err());
    assert!(!home.join("other").exists());
    assert!(backups::review_activation(&root).is_err());
    let forged = backups::Activation {
        root,
        plan_hash: FORGED_HASH.into(),
        text: String::new(),
    };
    assert!(backups::activate(&forged).is_err());
}

#[test]
fn a_reviewed_backup_is_created_once_and_verifies() {
    if std::env::var_os(CHILD).is_some() {
        exercise();
        return;
    }
    let home = tempfile::tempdir().unwrap();
    let program = std::env::current_exe().unwrap();
    let mut command = std::process::Command::new(program);
    command.args(["--exact", "a_reviewed_backup_is_created_once_and_verifies"]);
    command.env(CHILD, "1");
    command.env("HOME", home.path());
    command.env_remove("XDG_DATA_HOME");
    assert!(command.status().unwrap().success());
}
