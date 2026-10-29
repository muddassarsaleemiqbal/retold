//! Native lifecycle changes archive, restore, copy and trash a video and refuse during generation.
#![expect(clippy::unwrap_used, reason = "isolated fixture assertions")]
use std::path::Path;

use retold_app::desktop::lifecycle::{self, Change};

fn save(root: &Path, relative: &str, bytes: &[u8]) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, bytes).unwrap();
}

fn production(root: &Path) {
    let manifest = serde_json::json!({"schema":1,"id":"lifecycle-fixture", "created":"2026-10-26T00:00:00Z", "title":"Lifecycle", "guide":{"id":"fixture","version":"1","hash":"fixture"}, "channel":"studio","inputs":{},"variants":["en:en"],"quality":"standard","budget":0,"skip_reviews":false,"folder":["film"],"cast_levels":0});
    save(
        root,
        "studio/film/.retold/run.json",
        &serde_json::to_vec(&manifest).unwrap(),
    );
}

#[test]
fn archive_restore_copy_and_trash_change_only_the_named_video() {
    let root = tempfile::tempdir().unwrap();
    production(root.path());
    let video = "studio/film";
    lifecycle::apply(root.path(), video, &Change::Archive).unwrap();
    assert!(root.path().join("studio/film/.archived").exists());
    lifecycle::apply(root.path(), video, &Change::Unarchive).unwrap();
    lifecycle::apply(root.path(), video, &Change::AddTag("favourite".into())).unwrap();
    lifecycle::apply(root.path(), video, &Change::RemoveTag("favourite".into())).unwrap();
    assert!(!root.path().join("studio/film/.archived").exists());
    let copy = Change::Copy {
        title: "Lifecycle copy".into(),
    };
    assert!(
        lifecycle::apply(root.path(), video, &copy)
            .unwrap()
            .created
            .is_some()
    );
    lifecycle::apply(root.path(), video, &Change::Trash).unwrap();
    assert!(!root.path().join("studio/film").exists());
    assert!(
        root.path()
            .join(".trash")
            .read_dir()
            .unwrap()
            .next()
            .is_some()
    );
}

#[test]
fn a_locked_video_is_not_changed() {
    let root = tempfile::tempdir().unwrap();
    production(root.path());
    let directory = root.path().join("studio/film");
    let layout =
        retold_engine::RunLayout::from_video_dir(root.path(), "studio", &directory, 0).unwrap();
    let _held = retold_engine::store::Store::open(&layout).unwrap();
    assert!(lifecycle::apply(root.path(), "studio/film", &Change::Trash).is_err());
    assert!(root.path().join("studio/film").exists());
}
