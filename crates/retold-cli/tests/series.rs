//! Integration tests for multi-part series lifecycle commands.
#![expect(clippy::unwrap_used, reason = "test helpers")]

use std::path::Path;

use assert_cmd::Command;
use serde_json::Value;

fn query_series_command(root: &Path, args: &[&str]) -> Value {
    let mut command = Command::cargo_bin("retold").unwrap();
    let installation = root.with_extension("installation");
    command.env("HOME", &installation);
    command.env("RETOLD_CONFIG_DIR", installation.join("config"));
    command.env_remove("XDG_DATA_HOME");
    command.arg("--json");
    command.arg("--library");
    command.arg(root);
    let assert_cmd = command.args(args).assert();
    let stdout_bytes = &assert_cmd.get_output().stdout;
    let stdout = std::str::from_utf8(stdout_bytes).unwrap();
    serde_json::from_str(stdout).unwrap()
}

#[test]
fn series_creation_and_listing_lifecycle() {
    let root = tempfile::tempdir().unwrap();
    let created = query_series_command(
        root.path(),
        &[
            "series",
            "new",
            "story-recap",
            "--work",
            "Dark Knight",
            "--parts",
            "3",
            "--budget",
            "15.00",
        ],
    );
    assert_eq!(created["type"], "result");
    let series_data = &created["payload"]["data"]["series"];
    assert_eq!(series_data["id"], "dark-knight");
    assert_eq!(series_data["parts"].as_array().unwrap().len(), 3);

    let listing = query_series_command(root.path(), &["series", "list"]);
    assert_eq!(listing["type"], "result");
    let items = listing["payload"]["data"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"], "dark-knight");
    assert_eq!(items[0]["part_count"], 3);
}

#[test]
fn series_show_lifecycle() {
    let root = tempfile::tempdir().unwrap();
    query_series_command(
        root.path(),
        &[
            "series",
            "new",
            "story-recap",
            "--work",
            "Inception",
            "--parts",
            "2",
        ],
    );

    let shown = query_series_command(root.path(), &["series", "show", "inception"]);
    assert_eq!(shown["type"], "result");
    assert_eq!(
        shown["payload"]["data"]["series"]["parts"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn series_update_lifecycle() {
    let root = tempfile::tempdir().unwrap();
    query_series_command(
        root.path(),
        &[
            "series",
            "new",
            "story-recap",
            "--work",
            "Inception",
            "--parts",
            "2",
        ],
    );

    let updated = query_series_command(
        root.path(),
        &["series", "update", "inception", "--add-parts", "2"],
    );
    assert_eq!(updated["type"], "result");
    assert_eq!(
        updated["payload"]["data"]["series"]["parts"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    assert_eq!(updated["payload"]["data"]["series"]["version"], 2);
}

#[test]
fn series_plan_lifecycle() {
    let root = tempfile::tempdir().unwrap();
    query_series_command(
        root.path(),
        &[
            "series",
            "new",
            "story-recap",
            "--work",
            "Interstellar",
            "--parts",
            "4",
        ],
    );

    let plan = query_series_command(
        root.path(),
        &["series", "plan", "interstellar", "--parts", "P01,P02"],
    );
    assert_eq!(plan["type"], "result");
    assert_eq!(
        plan["payload"]["data"]["selected_parts"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn series_run_lifecycle() {
    let root = tempfile::tempdir().unwrap();
    query_series_command(
        root.path(),
        &[
            "series",
            "new",
            "story-recap",
            "--work",
            "Interstellar",
            "--parts",
            "4",
        ],
    );
    assert_unreviewed_run_is_refused(root.path());
    assert_unlinked_part_blocks_the_plan(root.path());
}

/// Nothing is submitted without a reviewed plan.
fn assert_unreviewed_run_is_refused(root: &std::path::Path) {
    let run = query_series_command(root, &["series", "run", "interstellar", "--parts", "P01"]);
    assert_eq!(run["type"], "error");
    assert_eq!(run["payload"]["code"], "RG-INPUT-REQUIRED");
}

/// A part without a linked video is a blocker, so even an accepted plan submits nothing.
fn assert_unlinked_part_blocks_the_plan(root: &std::path::Path) {
    let plan = query_series_command(root, &["series", "plan", "interstellar", "--parts", "P01"]);
    let plan = &plan["payload"]["data"];
    assert_eq!(plan["can_execute"], false);
    let message = plan["blockers"][0]["message"].as_str().unwrap();
    assert!(message.contains("link it"), "{message}");
    let words = ["series", "run", "interstellar", "--parts", "P01", "--yes"];
    assert_eq!(query_series_command(root, &words)["type"], "error");
}
