//! Delivered files copy verbatim, never overwrite silently, and history detects later changes.
#![expect(clippy::unwrap_used, reason = "isolated fixture assertions")]
use std::path::Path;

use retold_app::desktop::destinations::{self, Overwrite};
use retold_app::desktop::media;

fn save(root: &Path, relative: &str, bytes: &[u8]) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, bytes).unwrap();
}

fn production(root: &Path) {
    let manifest = serde_json::json!({"schema":1,"id":"delivery-fixture", "created":"2026-10-26T00:00:00Z", "title":"Delivery", "guide":{"id":"fixture","version":"1","hash":"fixture"}, "channel":"studio","inputs":{},"variants":["en:en"],"quality":"standard","budget":0,"skip_reviews":false,"folder":["film"],"cast_levels":0});
    save(
        root,
        "studio/film/.retold/run.json",
        &serde_json::to_vec(&manifest).unwrap(),
    );
    save(
        root,
        "studio/film/deliverables/en/story.mp4",
        b"delivered video bytes",
    );
}

fn delivered_item(root: &Path) -> media::Item {
    let query = media::Query {
        edition: Some("en:en".into()),
        offset: 0,
    };
    let page = media::page(root, "studio/film", &query).unwrap();
    page.items
        .into_iter()
        .find(|item| item.relative.ends_with("story.mp4"))
        .unwrap()
}

fn outbox(root: &Path) -> std::path::PathBuf {
    let folder = root.join("outbox");
    std::fs::create_dir_all(&folder).unwrap();
    folder
}

fn deliver(
    root: &Path,
    item: &media::Item,
    overwrite: Overwrite,
) -> Result<destinations::Delivered, retold_app::protocol::Diagnostic> {
    destinations::deliver(root, "studio/film", (item, &root.join("outbox"), overwrite))
}

#[test]
fn a_delivery_copies_once_refuses_silent_overwrite_and_history_detects_changes() {
    let root = tempfile::tempdir().unwrap();
    production(root.path());
    outbox(root.path());
    let item = delivered_item(root.path());
    let first = deliver(root.path(), &item, Overwrite::Refuse).unwrap();
    assert_eq!(
        std::fs::read(&first.destination).unwrap(),
        b"delivered video bytes"
    );
    assert!(deliver(root.path(), &item, Overwrite::Refuse).is_err());
    deliver(root.path(), &item, Overwrite::Replace).unwrap();
    std::fs::write(&first.destination, b"edited elsewhere").unwrap();
    let history = destinations::history(root.path(), "studio/film").unwrap();
    assert_eq!(history.len(), 2);
    assert!(history.iter().all(|entry| !entry.intact));
}
