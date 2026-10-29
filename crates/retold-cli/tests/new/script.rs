//! Independent CLI and application writers share real content and optimistic version checks.
use super::Fixture;
use retold_engine::editing::{self, SectionEdit};
use retold_engine::{
    RunLayout, Stage, fsx,
    store::{Store, hash},
    story::Script,
};
use serde_json::Value;
use std::path::Path;

fn seeded(fixture: &Fixture) -> String {
    let (created, code) = fixture.create(&["--budget", "0"]);
    assert_eq!(code, 0, "{created}");
    let video = &created["payload"]["data"]["created"];
    let root = fixture.root.join("library").canonicalize().unwrap();
    let directory = Path::new(video["directory"].as_str().unwrap());
    let layout = RunLayout::from_video_dir(&root, "test-studio", directory, 2).unwrap();
    let script: Script = serde_json::from_str(include_str!(
        "../../../retold-engine/tests/fixtures/full/script.json"
    ))
    .unwrap();
    let mut store = Store::open(&layout).unwrap();
    store
        .commit(
            "script",
            &hash(&script).unwrap(),
            &script,
            Option::default(),
        )
        .unwrap();
    fsx::write_json_atomic(&layout.stage(Stage::Script).join("script.json"), &script).unwrap();
    video["ref"]
        .as_str()
        .or_else(|| video["reference"].as_str())
        .unwrap()
        .into()
}

fn layout(fixture: &Fixture, reference: &str) -> RunLayout {
    let library = fixture.root.join("library").canonicalize().unwrap();
    RunLayout::from_video_dir(&library, "test-studio", &library.join(reference), 2).unwrap()
}

fn shown(fixture: &Fixture, reference: &str) -> Value {
    let (document, code) = fixture.invoke(&["video", "script", "show", reference]);
    assert_eq!(code, 0, "{document}");
    document["payload"]["data"]["content"].clone()
}

fn version(document: &Value) -> &str {
    document["version"].as_str().unwrap()
}

#[test]
fn cli_saves_actual_content_and_another_clients_revision_rejects_its_old_approval() {
    let fixture = Fixture::new();
    let reference = seeded(&fixture);
    let original = shown(&fixture, &reference);
    let text = original["script"]["sections"][0]["text"]
        .as_str()
        .unwrap()
        .replace("vampire", "stranger");
    let (saved, code) = fixture.invoke(&[
        "video",
        "script",
        "edit",
        &reference,
        "--expected-version",
        version(&original),
        "--section",
        "hook",
        "--text",
        &text,
    ]);
    assert_eq!(code, 0, "{saved}");
    let saved = shown(&fixture, &reference);
    assert_eq!(saved["script"]["sections"][0]["text"], text);
    approve_and_conflict(&fixture, &reference, &saved);
}

fn approve_and_conflict(fixture: &Fixture, reference: &str, saved: &Value) {
    let (approved, code) = revision_command(fixture, (reference, "approve"), version(saved));
    assert_eq!(code, 0, "{approved}");
    let approved = shown(fixture, reference);
    assert_eq!(approved["approved"], true);
    editing::save_section(
        &layout(fixture, reference),
        SectionEdit {
            expected_version: version(&approved),
            section: "hook",
            text: "English हिन्दी اردو draft",
        },
    )
    .unwrap();
    let (rejected, code) = revision_command(fixture, (reference, "approve"), version(&approved));
    assert_eq!(code, 3, "{rejected}");
    assert_eq!(rejected["payload"]["code"], "RG-PLAN-CHANGED");
    let current = shown(fixture, reference);
    assert_eq!(
        current["script"]["sections"][0]["text"],
        "English हिन्दी اردو draft"
    );
    assert_eq!(current["approved"], false);
}

#[test]
fn cli_revision_navigation_preserves_review_and_queue_boundaries() {
    let fixture = Fixture::new();
    let reference = seeded(&fixture);
    let original = shown(&fixture, &reference);
    let saved = editing::save_section(
        &layout(&fixture, &reference),
        SectionEdit {
            expected_version: version(&original),
            section: "hook",
            text: "Manual draft हिन्दी اردو",
        },
    )
    .unwrap();
    let (undone, code) = revision_command(&fixture, (&reference, "undo"), &saved.version);
    assert_eq!(code, 0, "{undone}");
    let undone = shown(&fixture, &reference);
    assert_eq!(undone["content_hash"], original["content_hash"]);
    assert_redo_and_history(&fixture, &reference, version(&undone));
}

fn assert_redo_and_history(fixture: &Fixture, reference: &str, expected: &str) {
    let (redone, code) = revision_command(fixture, (reference, "redo"), expected);
    assert_eq!(code, 0, "{redone}");
    assert_eq!(shown(fixture, reference)["approved"], false);
    let (history, code) = fixture.invoke(&["video", "script", "history", reference]);
    assert_eq!(code, 0, "{history}");
    assert_eq!(
        history["payload"]["data"]["entries"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(
        !fixture
            .root
            .join("library/.retold/studio/queue.sqlite")
            .exists()
    );
}

fn revision_command(fixture: &Fixture, subject: (&str, &str), expected: &str) -> (Value, i32) {
    let (reference, operation) = subject;
    fixture.invoke(&[
        "video",
        "script",
        operation,
        reference,
        "--expected-version",
        expected,
    ])
}
