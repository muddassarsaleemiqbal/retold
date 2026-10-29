//! Integration tests for video organizational tags and tag-based filtering.
#![expect(clippy::unwrap_used, reason = "test helpers")]

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use serde_json::{Value, json};

struct TaggedVideoSpec<'a> {
    reference: &'a str,
    id: &'a str,
    title: &'a str,
}

fn build_tag_manifest(spec: &TaggedVideoSpec<'_>) -> (String, Value) {
    let (channel, folder) = spec.reference.split_once('/').unwrap();
    let manifest = json!({
        "schema": 1,
        "id": spec.id,
        "created": "2026-10-18T00:00:00Z",
        "title": spec.title,
        "guide": {"id": "fixture", "version": "1", "hash": "fixture"},
        "channel": channel,
        "inputs": {},
        "variants": ["en:en"],
        "quality": "standard",
        "budget": 50,
        "skip_reviews": false,
        "folder": folder.split('/').collect::<Vec<_>>(),
        "cast_levels": 0
    });
    (spec.reference.to_string(), manifest)
}

fn write_manifest_file(target_path: &Path, content: &Value) {
    let serialized = content.to_string();
    fs::write(target_path, serialized).unwrap();
}

fn ensure_meta_dir(library: &Path, reference: &str) -> PathBuf {
    let meta = library.join(reference).join(".retold");
    fs::create_dir_all(&meta).unwrap();
    meta
}

fn seed_tagged_video(library: &Path, spec: &TaggedVideoSpec<'_>) {
    let (reference, manifest) = build_tag_manifest(spec);
    let meta_dir = ensure_meta_dir(library, &reference);
    write_manifest_file(&meta_dir.join("run.json"), &manifest);
}

fn query_tags_json(library: &Path, arguments: &[&str]) -> Value {
    let mut command = Command::cargo_bin("retold").unwrap();
    command.arg("--json").arg("--library").arg(library);
    command.args(arguments);
    let assert_result = command.assert();
    let output = assert_result.get_output();
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn video_tag_add_and_remove() {
    let fixture_directory = tempfile::tempdir().unwrap();
    let library = fixture_directory.path();
    let spec = TaggedVideoSpec {
        reference: "main/episode-1",
        id: "video-id-1",
        title: "Episode One",
    };
    seed_tagged_video(library, &spec);

    let add_result = query_tags_json(
        library,
        &["video", "tag", "video-id-1", "--add", "highlight"],
    );
    assert_eq!(add_result["payload"]["outcome"], "completed");
    let added_tags = add_result["payload"]["data"]["tags"].as_array().unwrap();
    assert_eq!(added_tags, &[json!("highlight")]);

    let remove_result = query_tags_json(
        library,
        &["video", "tag", "video-id-1", "--remove", "highlight"],
    );
    assert_eq!(remove_result["payload"]["outcome"], "completed");
    let removed_tags = remove_result["payload"]["data"]["tags"].as_array().unwrap();
    assert_eq!(removed_tags.len(), 0);
}

#[test]
fn video_list_filters_by_tag() {
    let fixture_directory = tempfile::tempdir().unwrap();
    let library = fixture_directory.path();
    let alpha_spec = TaggedVideoSpec {
        reference: "main/alpha",
        id: "vid-alpha",
        title: "Alpha Video",
    };
    seed_tagged_video(library, &alpha_spec);

    let beta_spec = TaggedVideoSpec {
        reference: "main/beta",
        id: "vid-beta",
        title: "Beta Video",
    };
    seed_tagged_video(library, &beta_spec);

    let _ = query_tags_json(library, &["video", "tag", "vid-alpha", "--add", "reviewed"]);

    let filtered = query_tags_json(library, &["video", "list", "--tag", "reviewed"]);
    assert_eq!(filtered["payload"]["outcome"], "completed");
    let items = filtered["payload"]["data"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"], "vid-alpha");
}

#[test]
fn video_list_sorts_by_title() {
    let fixture_directory = tempfile::tempdir().unwrap();
    let library = fixture_directory.path();
    let second_spec = TaggedVideoSpec {
        reference: "main/second",
        id: "vid-2",
        title: "Zeta Story",
    };
    seed_tagged_video(library, &second_spec);

    let first_spec = TaggedVideoSpec {
        reference: "main/first",
        id: "vid-1",
        title: "Alpha Story",
    };
    seed_tagged_video(library, &first_spec);

    let sorted = query_tags_json(library, &["video", "list", "--sort", "title"]);
    assert_eq!(sorted["payload"]["outcome"], "completed");
    let items = sorted["payload"]["data"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["title"], "Alpha Story");
    assert_eq!(items[1]["title"], "Zeta Story");
}
