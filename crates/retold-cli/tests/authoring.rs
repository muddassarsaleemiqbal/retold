//! Fresh workspace authoring is offline, explicit and preserves existing resources.
#![expect(clippy::unwrap_used, reason = "isolated fixture setup and assertions")]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use serde_json::Value;

struct Fixture {
    home: tempfile::TempDir,
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join("workspace");
        let fixture = Self { home, root };
        let (result, code) = fixture.invoke(&["workspace", "init", fixture.root.to_str().unwrap()]);
        assert_eq!(code, 0, "{result}");
        fixture
    }

    fn invoke(&self, args: &[&str]) -> (Value, i32) {
        let mut command = Command::cargo_bin("retold").unwrap();
        command.env("HOME", self.home.path());
        command.env("RETOLD_CONFIG_DIR", self.home.path().join("installation"));
        command
            .env_remove("RETOLD_WORKSPACE")
            .env_remove("RETOLD_LIBRARY");
        command.env_remove("XDG_DATA_HOME").env("PATH", "");
        command
            .args(["--json", "--workspace"])
            .arg(&self.root)
            .args(args);
        let output = command.output().unwrap();
        let value = serde_json::from_slice(&output.stdout).unwrap();
        (value, output.status.code().unwrap())
    }

    fn succeed(&self, args: &[&str]) -> Value {
        let (result, code) = self.invoke(args);
        assert_eq!(code, 0, "{args:?}: {result}");
        result
    }
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

fn create_guide(fixture: &Fixture) {
    fixture.succeed(&[
        "guides",
        "create",
        "explain",
        "--language",
        "en",
        "--purpose",
        "Source-backed explanations",
        "--style",
        "Clear flat diagrams",
        "--delivery",
        "Calm and direct",
        "--writing",
        "Use source facts and short sentences",
        "--packaging",
        "Accurate titles and source disclosures",
        "--seconds",
        "90",
    ]);
}

fn create_channel(fixture: &Fixture) {
    fixture.succeed(&[
        "channels",
        "create",
        "science",
        "--language",
        "en",
        "--voice-provider",
        "local-voice",
        "--voice-model",
        "narrator",
        "--voice",
        "speaker",
        "--font-family",
        "Noto Sans",
        "--font-style",
        "Regular",
        "--handle",
        "@science",
        "--accent",
        "#123456",
    ]);
}

#[test]
fn fresh_workspace_can_author_check_and_create_a_non_recap_draft() {
    let fixture = Fixture::new();
    create_guide(&fixture);
    create_channel(&fixture);
    fixture.succeed(&["guides", "check", "explain"]);
    fixture.succeed(&["channels", "check", "science"]);
    let source = fixture.home.path().join("facts.txt");
    fs::write(
        &source,
        "Water evaporates, condenses into droplets and falls as rain.",
    )
    .unwrap();
    let result = fixture.succeed(&[
        "new",
        "explain",
        "--topic",
        "How rain forms",
        "--source-file",
        source.to_str().unwrap(),
    ]);
    assert_draft_source(&result);
    assert!(!fixture.root.join("config/providers.toml").exists());
}

fn assert_draft_source(result: &Value) {
    let run = Path::new(
        result["payload"]["data"]["created"]["directory"]
            .as_str()
            .unwrap(),
    );
    let manifest = fs::read_to_string(run.join(".retold/guide/guide.toml")).unwrap();
    assert!(manifest.contains("Source-backed explanations"));
    assert!(!manifest.contains("Nosferatu"));
    assert!(run.join("work/01-research/source-file.txt").is_file());
}

#[test]
fn scaffolds_are_schema_valid_but_report_unfinished_configuration() {
    let fixture = Fixture::new();
    for kind in ["guides", "channels"] {
        fixture.succeed(&[kind, "create", "unfinished", "--language", "ur"]);
        let (checked, code) = fixture.invoke(&[kind, "check", "unfinished"]);
        assert_eq!(code, 3, "{checked}");
        assert_eq!(checked["payload"]["code"], "RG-NOT-READY");
        let schema = fixture.succeed(&[kind, "schema"]);
        assert!(schema["payload"]["data"]["schema"]["properties"]["id"].is_object());
    }
}

#[test]
fn dry_run_invalid_and_duplicate_authoring_leave_files_unchanged() {
    let fixture = Fixture::new();
    let before = files(fixture.home.path());
    for kind in ["guides", "channels"] {
        fixture.succeed(&[kind, "create", "preview", "--language", "en", "--dry-run"]);
        for args in [
            vec![kind, "create", "missing-language"],
            vec![kind, "create", "../escape", "--language", "en"],
        ] {
            let (result, code) = fixture.invoke(&args);
            assert_eq!(code, 2, "{result}");
        }
    }
    assert_eq!(files(fixture.home.path()), before);
    create_guide(&fixture);
    let before = files(fixture.home.path());
    let (result, code) = fixture.invoke(&["guides", "create", "explain", "--language", "en"]);
    assert_eq!(code, 2, "{result}");
    assert_eq!(files(fixture.home.path()), before);
}

#[test]
fn imports_preserve_user_text_and_assets_and_reject_mixed_options() {
    let fixture = Fixture::new();
    create_guide(&fixture);
    create_channel(&fixture);
    let (guide, channel) = import_sources(&fixture);
    fixture.succeed(&[
        "guides",
        "create",
        "copy-guide",
        "--from",
        guide.to_str().unwrap(),
    ]);
    fixture.succeed(&[
        "channels",
        "create",
        "copy-channel",
        "--from",
        channel.to_str().unwrap(),
    ]);
    assert_copied_music(&fixture);
    assert_mixed_import_preserved(&fixture, &guide);
}

fn import_sources(fixture: &Fixture) -> (PathBuf, PathBuf) {
    let guide = fixture.root.join("config/guides/explain");
    let channel = fixture.root.join("config/channels/science");
    write_music(&channel);
    (guide, channel)
}

fn write_music(channel: &Path) {
    fs::write(channel.join("music/bed.wav"), b"user music bytes").unwrap();
}

fn assert_copied_music(fixture: &Fixture) {
    let original = fixture.root.join("config/channels/science/music/bed.wav");
    let copy = fixture
        .root
        .join("config/channels/copy-channel/music/bed.wav");
    assert_eq!(fs::read(original).unwrap(), fs::read(copy).unwrap());
}

fn assert_mixed_import_preserved(fixture: &Fixture, guide: &Path) {
    let before = files(fixture.home.path());
    let (result, code) = fixture.invoke(&[
        "guides",
        "create",
        "mixed",
        "--from",
        guide.to_str().unwrap(),
        "--language",
        "en",
    ]);
    assert_eq!(code, 2, "{result}");
    assert_eq!(files(fixture.home.path()), before);
}

#[test]
#[cfg(unix)]
fn imports_reject_symlink_cycles_and_never_publish_partial_resources() {
    let fixture = Fixture::new();
    create_guide(&fixture);
    create_channel(&fixture);
    for kind in ["guides", "channels"] {
        assert_cycle_rejected(&fixture, kind);
    }
}

#[cfg(unix)]
fn assert_cycle_rejected(fixture: &Fixture, kind: &str) {
    let source = create_cycle(fixture, kind);
    let (result, code) =
        fixture.invoke(&[kind, "create", "copy", "--from", source.to_str().unwrap()]);
    assert_eq!(code, 2, "{result}");
    let destination = fixture.root.join(format!("config/{kind}/copy"));
    assert!(!destination.exists());
}

#[cfg(unix)]
fn create_cycle(fixture: &Fixture, kind: &str) -> PathBuf {
    let original = if kind == "guides" {
        "explain"
    } else {
        "science"
    };
    let source = fixture.root.join(format!("config/{kind}/{original}"));
    std::os::unix::fs::symlink(&source, source.join("cycle")).unwrap();
    source
}

#[test]
fn bundled_example_is_explicit_portable_and_preserves_its_sidecars() {
    let fixture = Fixture::new();
    let before = files(&fixture.root);
    fixture.succeed(&[
        "guides",
        "create",
        "recap",
        "--example",
        "story-recap",
        "--dry-run",
    ]);
    assert_eq!(files(&fixture.root), before);
    fixture.succeed(&[
        "guides",
        "create",
        "recap",
        "--example",
        "story-recap",
        "--name",
        "My recap",
    ]);
    fixture.succeed(&["guides", "check", "recap"]);
    assert_example_sidecars(&fixture);
    let created = files(&fixture.root);
    let (_, code) = fixture.invoke(&["guides", "create", "recap", "--example", "story-recap"]);
    assert_ne!(code, 0);
    assert_eq!(files(&fixture.root), created);
}

#[test]
fn example_rejects_import_and_scaffold_conflicts_without_writing() {
    let fixture = Fixture::new();
    let before = files(&fixture.root);
    for options in [
        vec!["--example", "unknown"],
        vec!["--example", "story-recap", "--from", "/missing"],
        vec!["--example", "story-recap", "--language", "en"],
    ] {
        let args = [vec!["guides", "create", "recap"], options].concat();
        let (_, code) = fixture.invoke(&args);
        assert_ne!(code, 0);
        assert_eq!(files(&fixture.root), before);
    }
}

fn assert_example_sidecars(fixture: &Fixture) {
    let directory = fixture.root.join("config/guides/recap");
    for name in [
        "voice.md",
        "voice.hi.md",
        "voice.ur.md",
        "examples/the-shining.md",
        "examples/the-notebook.md",
    ] {
        assert!(directory.join(name).is_file(), "{name}");
    }
}
