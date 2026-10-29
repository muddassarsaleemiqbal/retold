//! Native-facing snapshots are typed, read-only and retain per-section failures.
#![expect(clippy::unwrap_used, reason = "fixture assertions")]
use std::path::Path;

use retold_app::desktop::open_workspace;

fn workspace(root: &Path) {
    std::fs::create_dir_all(root.join("config")).unwrap();
    std::fs::create_dir_all(root.join("library")).unwrap();
    write_manifest(root);
}

fn write_manifest(root: &Path) {
    let manifest = retold_app::workspace::WorkspaceToml::new(
        "desktop-fixture".into(),
        "Desktop fixture".into(),
    );
    std::fs::write(
        root.join("workspace.toml"),
        toml::to_string(&manifest).unwrap(),
    )
    .unwrap();
}

#[test]
fn opening_empty_workspace_does_not_create_queue_or_configuration() {
    let root = tempfile::tempdir().unwrap();
    workspace(root.path());
    let snapshot = open_workspace(root.path()).unwrap();
    assert!(snapshot.context.managed);
    assert!(snapshot.videos.is_empty());
    assert_eq!(snapshot.total_videos, 0);
    assert!(snapshot.choices.unwrap().guides.is_empty());
    let activity = snapshot.activity.unwrap();
    assert_eq!(activity.page.total, 0);
    assert!(!activity.state_directory.exists());
    assert_eq!(
        std::fs::read_dir(root.path().join("config"))
            .unwrap()
            .count(),
        0
    );
}

#[test]
fn malformed_queue_does_not_hide_saved_library_or_claim_empty_activity() {
    let root = tempfile::tempdir().unwrap();
    workspace(root.path());
    let state = retold_app::studio::resolve_workspace_state(Some(&root.path().join("library")));
    std::fs::create_dir_all(&state).unwrap();
    let database = state.join("queue.sqlite");
    std::fs::write(&database, b"broken queue").unwrap();
    let snapshot = open_workspace(root.path()).unwrap();
    assert!(snapshot.activity.is_err());
    assert!(snapshot.choices.is_ok());
    assert_eq!(std::fs::read(&database).unwrap(), b"broken queue");
}

#[test]
fn missing_workspace_is_not_initialized_by_opening() {
    let root = tempfile::tempdir().unwrap();
    let missing = root.path().join("missing");
    assert!(open_workspace(&missing).is_err());
    assert!(!missing.exists());
}

#[test]
fn delivery_choices_use_engine_ids_instead_of_caption_specifications() {
    let choices = retold_app::desktop::delivery_editions(&[
        "en:en.srt".into(),
        "en".into(),
        "hi:hi-Latn".into(),
    ])
    .unwrap();
    assert_eq!(
        choices
            .iter()
            .map(|choice| choice.id.as_str())
            .collect::<Vec<_>>(),
        ["en", "hi-hi-Latn"]
    );
    assert!(retold_app::desktop::delivery_editions(&["invalid::edition".into()]).is_err());
}
