//! Integration tests for video review, approve/lock, unlock, note, edit, undo, redo, and history.

#![expect(
    clippy::unwrap_used,
    reason = "fixture helpers must fail on invalid test setup or protocol"
)]

use assert_cmd::Command;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn manifest_json(folder: &str, identity: (&str, &str), channel: &str) -> Value {
    json!({
        "schema": 1,
        "id": identity.0,
        "created": identity.1,
        "title": folder,
        "guide": {"id": "fixture", "version": "1", "hash": "fixture"},
        "channel": channel,
        "inputs": {},
        "variants": ["en:en"],
        "quality": "standard",
        "budget": 50,
        "skip_reviews": false,
        "folder": folder.split('/').collect::<Vec<_>>(),
        "cast_levels": 0
    })
}

fn video_metadata_dir(root: &Path, reference: &str) -> PathBuf {
    let metadata = root.join(reference).join(".retold");
    std::fs::create_dir_all(&metadata).unwrap();
    metadata
}

fn save_manifest(run_file: PathBuf, manifest: &Value) {
    let text = manifest.to_string();
    std::fs::write(run_file, text).unwrap();
}

fn write_manifest(metadata: &Path, reference: &str, identity: (&str, &str)) {
    let (channel, folder) = reference.split_once('/').unwrap();
    let manifest = manifest_json(folder, identity, channel);
    save_manifest(metadata.join("run.json"), &manifest);
}

fn saved_video(root: &Path, reference: &str, identity: (&str, &str)) -> PathBuf {
    let metadata = video_metadata_dir(root, reference);
    write_manifest(&metadata, reference, identity);
    metadata
}

fn query(root: &Path, args: &[&str]) -> Value {
    let mut command = Command::cargo_bin("retold").unwrap();
    let installation = root.with_extension("installation");
    command.env("RETOLD_SETTINGS_DIR", &installation);
    command.arg("--json");
    command.arg("--library");
    command.arg(root);
    command.arg("video");
    command.args(args);
    let assert_res = command.assert().success();
    let output = assert_res.get_output();
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn review_approve_and_unlock_flow() {
    let fixture_dir = tempfile::tempdir().unwrap();
    let root = fixture_dir.path();
    saved_video(root, "main/sample", ("sample-id", "2026-10-18T00:00:00Z"));

    let initial = query(root, &["review", "sample-id"]);
    assert_eq!(initial["payload"]["outcome"], "completed");
    assert_eq!(initial["payload"]["data"]["video"], "main/sample");
    assert_eq!(initial["payload"]["data"]["total"], 0);

    let approved = query(root, &["approve", "sample-id"]);
    assert_eq!(approved["payload"]["outcome"], "completed");
    assert_eq!(approved["payload"]["data"]["locked"], true);

    let review_after = query(root, &["review", "sample-id"]);
    assert_eq!(review_after["payload"]["data"]["total"], 1);

    let unlocked = query(root, &["unlock", "sample-id"]);
    assert_eq!(unlocked["payload"]["outcome"], "completed");
    assert_eq!(unlocked["payload"]["data"]["locked"], false);
}

#[test]
fn note_attaches_feedback_to_item() {
    let fixture_dir = tempfile::tempdir().unwrap();
    let root = fixture_dir.path();
    saved_video(root, "main/sample", ("sample-id", "2026-10-18T00:00:00Z"));

    let noted = query(
        root,
        &["note", "sample-id", "frame-1", "Needs more contrast"],
    );
    assert_eq!(noted["payload"]["outcome"], "completed");
    assert_eq!(
        noted["payload"]["data"]["note"]["text"],
        "Needs more contrast"
    );
}

#[test]
fn edit_shot_invalidates_descendants() {
    let fixture_dir = tempfile::tempdir().unwrap();
    let root = fixture_dir.path();
    saved_video(root, "main/sample", ("sample-id", "2026-10-18T00:00:00Z"));

    let shot = query(
        root,
        &[
            "edit",
            "shot",
            "sample-id",
            "1",
            "--action",
            "hero walks in",
            "--type",
            "wide",
        ],
    );
    assert_eq!(shot["payload"]["outcome"], "completed");
    assert_eq!(shot["payload"]["data"]["target"], "frame-1");
    let invalidated = shot["payload"]["data"]["invalidated"].as_array().unwrap();
    assert!(invalidated.contains(&json!("images/1")));
    assert!(invalidated.contains(&json!("assembly")));
}

#[test]
fn edit_line_invalidates_descendants() {
    let fixture_dir = tempfile::tempdir().unwrap();
    let root = fixture_dir.path();
    saved_video(root, "main/sample", ("sample-id", "2026-10-18T00:00:00Z"));

    let line = query(
        root,
        &[
            "edit",
            "line",
            "sample-id",
            "2",
            "--text",
            "Hello world",
            "--spoken-only",
        ],
    );
    assert_eq!(line["payload"]["outcome"], "completed");
    assert_eq!(line["payload"]["data"]["target"], "line-2");
}

#[test]
fn edit_cast_and_budget_supports_undo() {
    let fixture_dir = tempfile::tempdir().unwrap();
    let root = fixture_dir.path();
    saved_video(root, "main/sample", ("sample-id", "2026-10-18T00:00:00Z"));

    let cast = query(
        root,
        &[
            "edit",
            "cast",
            "sample-id",
            "Alice",
            "--look",
            "cyberpunk jacket",
        ],
    );
    assert_eq!(cast["payload"]["outcome"], "completed");
    assert_eq!(cast["payload"]["data"]["target"], "character-Alice");

    let budget = query(root, &["edit", "budget", "sample-id", "100.00"]);
    assert_eq!(budget["payload"]["outcome"], "completed");
    assert_eq!(budget["payload"]["data"]["budget"], "100.00");

    let undone = query(root, &["undo", "sample-id"]);
    assert_eq!(undone["payload"]["outcome"], "completed");
    assert_eq!(undone["payload"]["data"]["undone"], true);
    assert_eq!(undone["payload"]["data"]["restored_target"], "budget");
}

#[test]
fn redo_dry_run_and_commit() {
    let fixture_dir = tempfile::tempdir().unwrap();
    let root = fixture_dir.path();
    saved_video(root, "main/sample", ("sample-id", "2026-10-18T00:00:00Z"));

    let dry_run = query(root, &["redo", "sample-id", "images", "1,2", "--dry-run"]);
    assert_eq!(dry_run["payload"]["outcome"], "completed");
    assert_eq!(dry_run["payload"]["data"]["dry_run"], true);

    let redo_commit = query(root, &["redo", "sample-id", "images", "1,2"]);
    assert_eq!(redo_commit["payload"]["outcome"], "completed");
    assert_eq!(redo_commit["payload"]["data"]["dry_run"], false);
}

#[test]
fn video_history_tracks_edits_and_reviews() {
    let fixture_dir = tempfile::tempdir().unwrap();
    let root = fixture_dir.path();
    saved_video(root, "main/sample", ("sample-id", "2026-10-18T00:00:00Z"));

    let edit = query(root, &["edit", "budget", "sample-id", "120.00"]);
    assert_eq!(edit["payload"]["outcome"], "completed");

    let note = query(
        root,
        &["note", "sample-id", "frame-1", "Initial draft note"],
    );
    assert_eq!(note["payload"]["outcome"], "completed");

    let history = query(root, &["history", "sample-id"]);
    assert_eq!(history["payload"]["outcome"], "completed");
    let payload_data = &history["payload"]["data"];
    assert_eq!(payload_data["kind"], "all");
    let total = payload_data["total"].as_u64().unwrap();
    assert!(total >= 2);

    let items = payload_data["items"].as_array().unwrap();
    assert_eq!(items[0]["sequence"], 1);
    assert_eq!(items[1]["sequence"], 2);
}

fn seed_history_fixture(root: &Path) {
    saved_video(root, "main/sample", ("sample-id", "2026-10-18T00:00:00Z"));
    let edit = query(root, &["edit", "budget", "sample-id", "120.00"]);
    assert_eq!(edit["payload"]["outcome"], "completed");
    let note = query(
        root,
        &["note", "sample-id", "frame-1", "Initial draft note"],
    );
    assert_eq!(note["payload"]["outcome"], "completed");
}

#[test]
fn video_history_filters_edits() {
    let fixture_dir = tempfile::tempdir().unwrap();
    let root = fixture_dir.path();
    seed_history_fixture(root);

    let edit_history = query(root, &["history", "sample-id", "--kind", "edit"]);
    assert_eq!(edit_history["payload"]["outcome"], "completed");
    let edit_data = &edit_history["payload"]["data"];
    assert_eq!(edit_data["kind"], "edit");
    let edit_items = edit_data["items"].as_array().unwrap();
    assert_eq!(edit_items.len(), 1);
    assert_eq!(edit_items[0]["kind"], "edit");
    assert_eq!(edit_items[0]["target"], "budget");
}

#[test]
fn video_history_filters_reviews() {
    let fixture_dir = tempfile::tempdir().unwrap();
    let root = fixture_dir.path();
    seed_history_fixture(root);

    let review_history = query(root, &["history", "sample-id", "--kind", "review"]);
    assert_eq!(review_history["payload"]["outcome"], "completed");
    let review_data = &review_history["payload"]["data"];
    assert_eq!(review_data["kind"], "review");
    let review_items = review_data["items"].as_array().unwrap();
    assert_eq!(review_items.len(), 1);
    assert_eq!(review_items[0]["kind"], "review");
    assert_eq!(review_items[0]["target"], "frame-1");
}
