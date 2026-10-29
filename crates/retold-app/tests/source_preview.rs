//! Source review is read-only, bounded and never publishes a draft from changed text.
use retold_app::creation::{self, Options};
const LONG_SOURCE_REPEATS: usize = 3000;
const OVERSIZED_SOURCE: u64 = 2_000_001;

fn selected_source(root: &std::path::Path, name: &str) -> Options {
    Options {
        source_file: Some(root.join(name)),
        ..Options::default()
    }
}

#[test]
fn unicode_preview_is_bounded_and_leaves_source_bytes_unchanged() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("हिंदी source.txt");
    let original = "हिंदी اردو English\n".repeat(LONG_SOURCE_REPEATS);
    std::fs::write(&path, &original).unwrap();
    let preview = creation::preview_source(&path).unwrap();
    assert!(preview.truncated);
    assert_eq!(preview.bytes, original.len());
    assert!(preview.text.starts_with("हिंदी اردو English"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
}

#[test]
fn changed_source_or_choice_cannot_publish_any_draft() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("source.txt");
    std::fs::write(&path, "Original source").unwrap();
    let preview = creation::preview_source(&path).unwrap();
    let options = selected_source(root.path(), "source.txt");
    std::fs::write(&path, "Changed source").unwrap();
    let destination = root.path().join("library");
    let changed = creation::save_draft_from_source(&destination, &options, &preview).unwrap_err();
    assert!(changed.message.contains("changed"), "{changed:?}");
    assert!(!destination.exists());
    let options = selected_source(root.path(), "other.txt");
    assert!(creation::save_draft_from_source(&destination, &options, &preview).is_err());
    assert!(!destination.exists());
}

#[test]
fn invalid_sources_fail_without_creating_or_repairing_them() {
    let root = tempfile::tempdir().unwrap();
    assert!(creation::preview_source(root.path()).is_err());
    assert!(creation::preview_source(&root.path().join("missing")).is_err());
    for bytes in [&b"  "[..], &b"\xff\xfe"[..]] {
        let path = root.path().join("invalid.txt");
        std::fs::write(&path, bytes).unwrap();
        assert!(creation::preview_source(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
    let large = root.path().join("large.txt");
    std::fs::File::create(&large)
        .unwrap()
        .set_len(OVERSIZED_SOURCE)
        .unwrap();
    assert!(creation::preview_source(&large).is_err());
    assert_eq!(std::fs::metadata(large).unwrap().len(), OVERSIZED_SOURCE);
}
