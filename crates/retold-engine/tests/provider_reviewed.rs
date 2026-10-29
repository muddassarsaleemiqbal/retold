//! Exact-source reviews share the CLI writer lock and never dispatch a provider.
use retold_engine::provider_settings::{
    AddError, AddRequest, add,
    reviewed::{self, Change},
};

const VOICE_DEFINITION: &str = "kind = 'local_voice'\nmax_parallel = 1\n";

#[cfg(unix)]
#[test]
fn changed_missing_ancestor_cannot_retarget_a_reviewed_save() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("future/config");
    let snapshot = reviewed::inspect(&root).unwrap();
    let review = reviewed::preview(&root, &snapshot.version, &voice("voice")).unwrap();
    let other = temporary.path().join("other");
    retarget_future(temporary.path(), &other).unwrap();
    assert!(matches!(reviewed::apply(&review), Err(AddError::Changed)));
    assert!(!other.join("config").exists());
}

#[cfg(unix)]
fn retarget_future(root: &std::path::Path, other: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir(other)?;
    std::os::unix::fs::symlink(other, root.join("future"))
}
fn voice(id: &str) -> Change {
    Change::Add {
        id: id.into(),
        definition: VOICE_DEFINITION.into(),
    }
}

#[test]
fn missing_configuration_inspection_and_preview_are_read_only() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("missing/config");
    let before = reviewed::inspect(&root).unwrap();
    assert!(before.providers.is_empty());
    let review = reviewed::preview(&root, &before.version, &voice("voice")).unwrap();
    assert!(!root.exists());
    assert!(review.before().contains("No configuration"));
    assert!(review.after().contains("local_voice"));
    assert!(reviewed::apply(&review).unwrap().is_none());
    let after = reviewed::inspect(&root).unwrap();
    assert_eq!(after.providers[0].id, "voice");
    assert_ne!(before.version, after.version);
    assert!(matches!(reviewed::apply(&review), Err(AddError::Changed)));
}

#[test]
fn another_cli_writer_invalidates_the_review_without_touching_their_bytes() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let before = reviewed::inspect(root).unwrap();
    let review = reviewed::preview(root, &before.version, &voice("native")).unwrap();
    add(&AddRequest::inline(root, "cli", VOICE_DEFINITION)).unwrap();
    let expected = std::fs::read(root.join("providers.toml")).unwrap();
    assert!(matches!(reviewed::apply(&review), Err(AddError::Changed)));
    assert_eq!(
        std::fs::read(root.join("providers.toml")).unwrap(),
        expected
    );
    assert_eq!(std::fs::read_dir(root).unwrap().count(), 2);
}

#[test]
fn reviewed_routing_preserves_providers_and_blocks_referenced_removal() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    add(&AddRequest::inline(root, "voice", VOICE_DEFINITION)).unwrap();
    let snapshot = reviewed::inspect(root).unwrap();
    let change = Change::Route {
        tier: "tts".into(),
        order: "voice:default".into(),
    };
    let review = reviewed::preview(root, &snapshot.version, &change).unwrap();
    let original = std::fs::read(root.join("providers.toml")).unwrap();
    let backup = reviewed::apply(&review).unwrap().unwrap();
    assert_eq!(std::fs::read(backup).unwrap(), original);
    let snapshot = reviewed::inspect(root).unwrap();
    assert_eq!(snapshot.routing["tts"], ["voice:default"]);
    let remove = Change::Remove { id: "voice".into() };
    assert!(reviewed::preview(root, &snapshot.version, &remove).is_err());
    let unsupported = Change::Route {
        tier: "text.smart".into(),
        order: "voice:default".into(),
    };
    assert!(reviewed::preview(root, &snapshot.version, &unsupported).is_err());
}

#[test]
fn exact_comments_and_invalid_definitions_invalidate_without_leaking_input() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    add(&AddRequest::inline(root, "voice", VOICE_DEFINITION)).unwrap();
    let snapshot = reviewed::inspect(root).unwrap();
    let change = Change::Replace {
        id: "voice".into(),
        definition: "secret = 'PRIVATE'".into(),
    };
    let error = reviewed::preview(root, &snapshot.version, &change).unwrap_err();
    assert!(!error.to_string().contains("PRIVATE"));
    let review = reviewed::preview(root, &snapshot.version, &voice("second")).unwrap();
    let text = std::fs::read_to_string(root.join("providers.toml")).unwrap();
    std::fs::write(
        root.join("providers.toml"),
        format!("# another writer\n{text}"),
    )
    .unwrap();
    assert!(matches!(reviewed::apply(&review), Err(AddError::Changed)));
}

#[test]
fn active_writer_blocks_apply_then_releases_for_the_same_review() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let snapshot = reviewed::inspect(root).unwrap();
    let review = reviewed::preview(root, &snapshot.version, &voice("voice")).unwrap();
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true);
    options.create(true).truncate(false);
    let file = options.open(root.join(".providers.lock")).unwrap();
    fs2::FileExt::lock_exclusive(&file).unwrap();
    assert!(matches!(reviewed::apply(&review), Err(AddError::Busy)));
    assert!(!root.join("providers.toml").exists());
    fs2::FileExt::unlock(&file).unwrap();
    reviewed::apply(&review).unwrap();
}

#[test]
fn clearing_a_route_then_removing_its_provider_requires_two_current_reviews() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    add(&AddRequest::inline(root, "voice", VOICE_DEFINITION)).unwrap();
    let initial = reviewed::inspect(root).unwrap();
    let route = Change::Route {
        tier: "tts".into(),
        order: "voice:default".into(),
    };
    let review = reviewed::preview(root, &initial.version, &route).unwrap();
    reviewed::apply(&review).unwrap();
    let routed = reviewed::inspect(root).unwrap();
    let clear = Change::ClearRoute { tier: "tts".into() };
    let review = reviewed::preview(root, &routed.version, &clear).unwrap();
    reviewed::apply(&review).unwrap();
    let cleared = reviewed::inspect(root).unwrap();
    assert!(!cleared.routing.contains_key("tts"));
    let remove = Change::Remove { id: "voice".into() };
    let review = reviewed::preview(root, &cleared.version, &remove).unwrap();
    reviewed::apply(&review).unwrap();
    assert!(reviewed::inspect(root).unwrap().providers.is_empty());
}
