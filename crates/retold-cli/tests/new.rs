//! A new draft is a real engine run, with no implicit provider calls or export authority.
#![expect(clippy::unwrap_used, reason = "isolated fixture setup and assertions")]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use serde_json::Value;

#[path = "new/script.rs"]
mod script_tests;

const VIDEO_BUDGET_MICROS: u64 = 2_500_000;

struct Fixture {
    sandbox: tempfile::TempDir,
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let sandbox = tempfile::tempdir().unwrap();
        let root = sandbox.path().join("workspace");
        let fixture = Self { sandbox, root };
        let (initialized, code) =
            fixture.invoke(&["workspace", "init", fixture.root.to_str().unwrap()]);
        assert_eq!(code, 0, "{initialized}");
        install_resources(&fixture.root);
        fixture
    }

    fn invoke(&self, args: &[&str]) -> (Value, i32) {
        let mut command = Command::cargo_bin("retold").unwrap();
        command.env("HOME", self.sandbox.path());
        command.env(
            "RETOLD_CONFIG_DIR",
            self.sandbox.path().join("installation"),
        );
        command
            .env_remove("RETOLD_WORKSPACE")
            .env_remove("RETOLD_LIBRARY")
            .env_remove("XDG_DATA_HOME");
        command.env("PATH", "");
        command
            .args(["--json", "--workspace"])
            .arg(&self.root)
            .args(args);
        let output = command.output().unwrap();
        (
            serde_json::from_slice(&output.stdout).unwrap(),
            output.status.code().unwrap(),
        )
    }

    fn create(&self, extra: &[&str]) -> (Value, i32) {
        let mut args = vec![
            "new",
            "narrative-fixture",
            "--work",
            "Nosferatu",
            "--kind",
            "film",
        ];
        args.extend_from_slice(extra);
        self.invoke(&args)
    }
}

fn install_resources(root: &Path) {
    install_guide(root);
    retold_guide::channel::init_from_template(
        "test-studio",
        &root.join("config/channels/test-studio"),
    )
    .unwrap();
}

fn install_guide(root: &Path) {
    let guide = fixture_guide();
    guide
        .write_to(&root.join("config/guides").join(guide.id()))
        .unwrap();
}

fn fixture_guide() -> retold_guide::Guide {
    retold_guide::catalog::bundled().unwrap().remove(0)
}

fn files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut result = BTreeMap::new();
    for entry in fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            result.extend(files(&path));
        } else {
            result.insert(path.clone(), fs::read(path).unwrap());
        }
    }
    result
}

#[test]
fn new_saves_manifest_snapshots_and_source_and_appears_in_the_library() {
    let fixture = Fixture::new();
    let source = fixture.sandbox.path().join("facts.txt");
    fs::write(&source, "The film was released in 1922.").unwrap();
    let (created, code) = fixture.create(&[
        "--source-file",
        source.to_str().unwrap(),
        "--budget",
        "2.50",
    ]);
    assert_eq!(code, 0, "{created}");
    assert_saved_run(&created, "The film was released in 1922.");
    let (listed, code) = fixture.invoke(&["video", "list"]);
    assert_eq!(code, 0, "{listed}");
    assert_eq!(
        listed["payload"]["data"]["items"][0]["id"],
        created["payload"]["data"]["created"]["video_id"]
    );
    assert!(!fixture.root.join("library/series").exists());
}

fn assert_saved_run(created: &Value, source: &str) {
    let saved_video = &created["payload"]["data"]["created"];
    let directory = Path::new(saved_video["directory"].as_str().unwrap());
    let manifest: Value =
        serde_json::from_slice(&fs::read(directory.join(".retold/run.json")).unwrap()).unwrap();
    assert_eq!(manifest["id"], saved_video["video_id"]);
    assert_eq!(manifest["budget"], VIDEO_BUDGET_MICROS);
    assert_eq!(saved_video["budget_usd_micros"], VIDEO_BUDGET_MICROS);
    assert_eq!(manifest["skip_reviews"], false);
    assert!(directory.join(".retold/guide/guide.toml").is_file());
    assert!(directory.join(".retold/channel.toml").is_file());
    assert_eq!(
        fs::read_to_string(directory.join("work/01-research/source-file.txt")).unwrap(),
        source
    );
    assert_eq!(saved_video["stage"], "created");
}

#[test]
fn dry_run_validates_without_any_filesystem_changes() {
    let fixture = Fixture::new();
    let before = files(fixture.sandbox.path());
    let (preview, code) = fixture.create(&["--dry-run"]);
    assert_eq!(code, 0, "{preview}");
    assert_eq!(preview["payload"]["data"]["dry_run"], true);
    let routes = preview["payload"]["data"]["routes"].as_array().unwrap();
    assert_eq!(routes.len(), 12, "every guide role is listed: {routes:?}");
    assert!(routes.iter().all(|route| route["chain"].is_array()));
    assert_eq!(files(fixture.sandbox.path()), before);
}

#[test]
fn invalid_inputs_languages_budgets_and_export_flags_create_nothing() {
    let fixture = Fixture::new();
    let before = files(fixture.sandbox.path());
    for args in [
        vec!["--input", "unknown=value"],
        vec!["--input", "title=Another film"],
        vec!["--input", "year=not-a-number"],
        vec!["--input", "year=1922", "--input", "year=1923"],
        vec!["--lang", "en", "--lang", "en:en"],
        vec!["--lang", "not_a_language"],
        vec!["--budget=-1"],
        vec!["--until", "export"],
        vec!["--allow-video"],
    ] {
        let (result, code) = fixture.create(&args);
        assert_eq!(code, 2, "{args:?}: {result}");
        assert_eq!(files(fixture.sandbox.path()), before);
    }
}

#[test]
fn missing_source_and_missing_configuration_fail_without_a_partial_run() {
    let fixture = Fixture::new();
    let before = files(fixture.sandbox.path());
    let (missing_source, code) =
        fixture.create(&["--source-file", "/definitely/missing/facts.txt"]);
    assert_eq!(code, 2, "{missing_source}");
    let (missing_guide, code) = fixture.invoke(&["new", "missing-guide", "--work", "Absent"]);
    assert_eq!(code, 2, "{missing_guide}");
    let (missing_channel, code) = fixture.create(&["--channel", "missing-channel"]);
    assert_eq!(code, 2, "{missing_channel}");
    assert_eq!(files(fixture.sandbox.path()), before);
}

#[test]
fn duplicate_creation_preserves_the_existing_run() {
    let fixture = Fixture::new();
    let (created, code) = fixture.create(&[]);
    assert_eq!(code, 0, "{created}");
    let before = files(fixture.sandbox.path());
    let (duplicate, code) = fixture.create(&[]);
    assert_eq!(code, 2, "{duplicate}");
    assert_eq!(files(fixture.sandbox.path()), before);
}

#[test]
fn nested_new_and_single_guide_selection_create_the_same_kind_of_run() {
    let fixture = Fixture::new();
    let (created, code) = fixture.invoke(&[
        "video",
        "new",
        "--work",
        "An ordinary garden",
        "--kind",
        "film",
    ]);
    assert_eq!(code, 0, "{created}");
    let directory = Path::new(
        created["payload"]["data"]["created"]["directory"]
            .as_str()
            .unwrap(),
    );
    assert!(directory.join(".retold/run.json").is_file());
    assert!(!directory.join("deliverables/en/video.mp4").exists());
}
