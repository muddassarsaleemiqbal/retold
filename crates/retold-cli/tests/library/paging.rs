//! Native-facing typed pages and CLI pagination have identical, filter-bound observations.
use super::{EARLIER, files, query, saved};
use retold_app::library::{self, ListOptions};
use retold_app::protocol::ErrorCode;

const FIXTURE_VIDEO_COUNT: usize = 4;
const QUERY_VARIATIONS: usize = 4;

#[test]
fn every_effective_filter_and_sort_is_bound_to_the_page_cursor() {
    let root = tempfile::tempdir().unwrap();
    seed_pages(root.path());
    let options = ListOptions {
        limit: Some(1),
        ..ListOptions::default()
    };
    let first = library::page(root.path(), &options).unwrap();
    let cursor = first.next_cursor;
    for mut filters in changed_queries(&options) {
        filters.cursor.clone_from(&cursor);
        let error = library::page(root.path(), &filters).unwrap_err();
        assert_eq!(error.code, ErrorCode::CursorInvalid);
    }
    let next = library::page(root.path(), &ListOptions { cursor, ..options }).unwrap();
    assert_ne!(first.items[0].id, next.items[0].id);
}

fn changed_queries(options: &ListOptions) -> [ListOptions; QUERY_VARIATIONS] {
    [
        ListOptions {
            sort: Some("title".into()),
            ..options.clone()
        },
        ListOptions {
            tag: Some("shared".into()),
            ..options.clone()
        },
        ListOptions {
            archive: true,
            ..options.clone()
        },
        ListOptions {
            limit: Some(2),
            ..options.clone()
        },
    ]
}

fn seed_pages(root: &std::path::Path) {
    for index in 0..FIXTURE_VIDEO_COUNT {
        seed_video(root, index);
    }
}

fn seed_video(root: &std::path::Path, index: usize) {
    let metadata = shared_metadata(root, index);
    if index >= 2 {
        std::fs::write(metadata.parent().unwrap().join(".archived"), "").unwrap();
    }
}

fn shared_metadata(root: &std::path::Path, index: usize) -> std::path::PathBuf {
    let (reference, identity) = story_identity(index);
    let metadata = saved(root, &reference, (&identity, EARLIER));
    std::fs::write(metadata.join("tags.json"), "[\"shared\"]").unwrap();
    metadata
}

fn story_identity(index: usize) -> (String, String) {
    (format!("channel/story-{index}"), format!("id-{index}"))
}

#[test]
fn typed_and_cli_pages_match_and_keep_the_library_read_only() {
    let root = tempfile::tempdir().unwrap();
    seed_pages(root.path());
    let before = files(root.path());
    let options = ListOptions {
        limit: Some(1),
        ..ListOptions::default()
    };
    let typed = library::page(root.path(), &options).unwrap();
    let cli = query(root.path(), &["video", "list", "--limit", "1"]);
    assert_eq!(serde_json::to_value(typed).unwrap(), cli["payload"]["data"]);
    assert_eq!(files(root.path()), before);
}
