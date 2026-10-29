//! Integration tests for saved views CLI commands.
#![expect(clippy::unwrap_used, reason = "test helpers")]

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use serde_json::{Value, json};

struct ViewVideoSpec<'a> {
    channel: &'a str,
    folder: &'a str,
    id: &'a str,
    title: &'a str,
}

fn build_view_manifest(spec: &ViewVideoSpec<'_>) -> Value {
    json!({
        "schema": 1,
        "id": spec.id,
        "created": "2026-10-18T00:00:00Z",
        "title": spec.title,
        "guide": {"id": "sample-guide", "version": "1", "hash": "h1"},
        "channel": spec.channel,
        "inputs": {},
        "variants": ["en:en"],
        "quality": "standard",
        "budget": 50,
        "skip_reviews": false,
        "folder": [spec.folder],
        "cast_levels": 0
    })
}

fn write_view_run_manifest(target_file: &Path, content: &Value) {
    let text = content.to_string();
    fs::write(target_file, text).unwrap();
}

fn view_meta_path(library: &Path, channel: &str, folder: &str) -> PathBuf {
    let relative = format!("{channel}/{folder}/.retold");
    library.join(relative)
}

fn ensure_view_meta_dir(library: &Path, channel: &str, folder: &str) -> PathBuf {
    let meta = view_meta_path(library, channel, folder);
    fs::create_dir_all(&meta).unwrap();
    meta
}

fn seed_view_video(library_path: &Path, spec: &ViewVideoSpec<'_>) {
    let meta_dir = ensure_view_meta_dir(library_path, spec.channel, spec.folder);
    let manifest_content = build_view_manifest(spec);
    write_view_run_manifest(&meta_dir.join("run.json"), &manifest_content);
}

fn seed_two_sample_videos(library: &Path) {
    let first = ViewVideoSpec {
        channel: "main",
        folder: "one",
        id: "id-1",
        title: "Pilot Episode",
    };
    seed_view_video(library, &first);
    let second = ViewVideoSpec {
        channel: "main",
        folder: "two",
        id: "id-2",
        title: "Finale Story",
    };
    seed_view_video(library, &second);
}

struct ViewsFixture {
    library: PathBuf,
}

impl ViewsFixture {
    fn new(dir: &Path) -> Self {
        Self {
            library: dir.to_path_buf(),
        }
    }

    fn run(&self, subcommands: &[&str]) -> Value {
        let mut invocation = Command::cargo_bin("retold").unwrap();
        invocation.arg("--json").arg("--library").arg(&self.library);
        invocation.args(subcommands);
        let assert_result = invocation.assert();
        let output = assert_result.get_output();
        serde_json::from_slice(&output.stdout).unwrap()
    }
}

fn write_filter_json(directory: &Path, filename: &str, content: &str) -> PathBuf {
    let file_path = directory.join(filename);
    fs::write(&file_path, content.as_bytes()).unwrap();
    file_path
}

#[test]
fn view_save_and_list() {
    let fixture_directory = tempfile::tempdir().unwrap();
    let fixture = ViewsFixture::new(fixture_directory.path());
    let filter_path = write_filter_json(
        fixture_directory.path(),
        "search-view.json",
        r#"{"search": "pilot", "limit": 25}"#,
    );

    let saved = fixture.run(&[
        "views",
        "save",
        "pilot-view",
        "--filter-file",
        filter_path.to_str().unwrap(),
    ]);
    assert_eq!(saved["payload"]["outcome"], "completed");

    let listed = fixture.run(&["views", "list"]);
    assert_eq!(listed["payload"]["outcome"], "completed");
    let view_items = listed["payload"]["data"]["items"].as_array().unwrap();
    assert_eq!(view_items.len(), 1);
    assert_eq!(view_items[0]["name"], "pilot-view");
}

#[test]
fn view_show_inspects_filters() {
    let fixture_directory = tempfile::tempdir().unwrap();
    let fixture = ViewsFixture::new(fixture_directory.path());
    let filter_path = write_filter_json(
        fixture_directory.path(),
        "search-view.json",
        r#"{"search": "pilot"}"#,
    );

    let _ = fixture.run(&[
        "views",
        "save",
        "pilot-view",
        "--filter-file",
        filter_path.to_str().unwrap(),
    ]);

    let shown = fixture.run(&["views", "show", "pilot-view"]);
    assert_eq!(shown["payload"]["outcome"], "completed");
    assert_eq!(
        shown["payload"]["data"]["view"]["filters"]["search"],
        "pilot"
    );
}

#[test]
fn view_removal_requires_confirmation() {
    let fixture_directory = tempfile::tempdir().unwrap();
    let fixture = ViewsFixture::new(fixture_directory.path());
    let filter_path = write_filter_json(
        fixture_directory.path(),
        "search-view.json",
        r#"{"search": "pilot"}"#,
    );

    let _ = fixture.run(&[
        "views",
        "save",
        "pilot-view",
        "--filter-file",
        filter_path.to_str().unwrap(),
    ]);

    let unconfirmed_remove = fixture.run(&["views", "remove", "pilot-view"]);
    assert_eq!(unconfirmed_remove["payload"]["outcome"], "failed");

    let confirmed_remove = fixture.run(&["views", "remove", "pilot-view", "--yes"]);
    assert_eq!(confirmed_remove["payload"]["outcome"], "completed");
}

#[test]
fn video_list_applies_saved_view() {
    let fixture_directory = tempfile::tempdir().unwrap();
    let library = fixture_directory.path();
    let fixture = ViewsFixture::new(library);
    seed_two_sample_videos(library);

    let filter_path = write_filter_json(library, "filter.json", r#"{"search": "Pilot"}"#);

    let _ = fixture.run(&[
        "views",
        "save",
        "pilot-filter",
        "--filter-file",
        filter_path.to_str().unwrap(),
    ]);

    let query_result = fixture.run(&["video", "list", "--view", "pilot-filter"]);
    assert_eq!(query_result["payload"]["outcome"], "completed");
    let result_items = query_result["payload"]["data"]["items"].as_array().unwrap();
    assert_eq!(result_items.len(), 1);
    assert_eq!(result_items[0]["id"], "id-1");
}
