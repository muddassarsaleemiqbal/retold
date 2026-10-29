//! `video plan` and `video continue` against a real saved run, with an isolated home, settings
//! folder and library. No provider is configured, so nothing can be dispatched.
#![expect(
    clippy::unwrap_used,
    reason = "fixture helpers must fail on invalid test setup or protocol"
)]

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use retold_engine::gate::Usd;
use retold_engine::store::Store;
use retold_engine::{NewRun, Quality, RunLayout, run};
use serde_json::Value;

const VIDEO_CAP: Usd = Usd::from_micros(5_000_000);
const NO_PROVIDERS: &str = "schema_version = 1\n[providers]\n[routing]\n";
const STALE_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

struct Installation {
    sandbox: tempfile::TempDir,
    video_id: String,
    layout: RunLayout,
}

impl Installation {
    /// One saved video created from the bundled guide and a template channel.
    fn new() -> Self {
        let sandbox = tempfile::tempdir().unwrap();
        let channel = settings(&sandbox.path().join("config"));
        let (video_id, layout) = create_video(&sandbox.path().join("library"), &channel);
        Self {
            sandbox,
            video_id,
            layout,
        }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.sandbox.path().join(name)
    }

    /// The binary, isolated to this installation's home, settings and library.
    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::cargo_bin("retold").unwrap();
        command.env("HOME", self.path("home"));
        command.env("RETOLD_CONFIG_DIR", self.path("config"));
        command.env("RETOLD_LIBRARY", self.path("library"));
        command.env_remove("XDG_DATA_HOME").args(args);
        command
    }

    fn retold(&self, args: &[&str]) -> (Value, i32) {
        let mut command = self.command(&["--json"]);
        command.args(args);
        let output = command.output().unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert_eq!(stdout.lines().count(), 1, "{stdout}");
        (
            serde_json::from_str(&stdout).unwrap(),
            output.status.code().unwrap(),
        )
    }

    fn plain(&self, args: &[&str]) -> String {
        let output = self.command(args).output().unwrap();
        String::from_utf8(output.stdout).unwrap()
    }

    fn export(&self, extra: &[&str]) -> (Value, i32) {
        let mut args = vec!["video", "export", self.video_id.as_str()];
        args.extend_from_slice(extra);
        self.retold(&args)
    }

    fn initialize_account(&self) {
        let (_, exit) = self.retold(&["account", "init", "--timezone", "UTC"]);
        assert_eq!(exit, 0);
    }

    fn account(&self) -> Value {
        self.retold(&["account", "show"]).0["payload"]["data"].clone()
    }

    fn plan(&self, extra: &[&str]) -> Value {
        let mut args = vec!["video", "plan", self.video_id.as_str()];
        args.extend_from_slice(extra);
        let (envelope, exit) = self.retold(&args);
        assert_eq!(exit, 0, "{envelope}");
        envelope["payload"]["data"].clone()
    }

    fn resume(&self, extra: &[&str]) -> (Value, i32) {
        let mut args = vec!["video", "continue", self.video_id.as_str()];
        args.extend_from_slice(extra);
        self.retold(&args)
    }
}

/// A template channel and provider settings with no provider at all.
fn settings(config: &Path) -> retold_guide::Channel {
    let channel_dir = config.join("channels/test-studio");
    let channel = retold_guide::channel::init_from_template("test-studio", &channel_dir).unwrap();
    std::fs::write(config.join("providers.toml"), NO_PROVIDERS).unwrap();
    channel
}

/// A saved video from the bundled guide; returns its immutable ID and layout.
fn create_video(library: &Path, channel: &retold_guide::Channel) -> (String, RunLayout) {
    let guide = retold_guide::catalog::bundled().unwrap().remove(0);
    let raw: indexmap::IndexMap<String, String> = [
        ("title".into(), "Nosferatu".into()),
        ("kind".into(), "film".into()),
        ("year".into(), "1922".into()),
    ]
    .into();
    let new = NewRun {
        guide: &guide,
        channel,
        inputs: retold_guide::inputs::resolve(guide.manifest(), &raw).unwrap(),
        variants: vec!["en".parse().unwrap()],
        quality: Quality::Standard,
        budget: VIDEO_CAP,
        skip_reviews: true,
        source_file: None,
    };
    let (layout, meta) = run::create(library, new).unwrap();
    (meta.id, layout)
}

fn code(envelope: &Value) -> &str {
    envelope["payload"]["code"].as_str().unwrap_or("")
}

fn files(root: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = walk(root);
    found.sort();
    found
}

fn walk(directory: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(directory)
        .into_iter()
        .flatten()
        .flatten()
        .flat_map(|entry| {
            let path = entry.path();
            let mut nested = if path.is_dir() {
                walk(&path)
            } else {
                Vec::new()
            };
            nested.push(path);
            nested
        })
        .collect()
}

#[test]
fn plan_pins_finite_authority_and_writes_nothing() {
    let installation = Installation::new();
    installation.initialize_account();
    let before = files(&installation.path("library"));
    let free = installation.plan(&[]);
    assert_eq!(free["plan_kind"], "executable");
    assert_eq!(free["can_execute"], true, "{free}");
    assert_eq!(free["authority"]["usd_micros"], 0);
    assert_eq!(free["authority"]["requests"], 0);
    assert_eq!(free["authority"]["until"], "ready");
    assert!(free["authority"]["export"].is_null());
    assert_eq!(free["plan_hash"], installation.plan(&[])["plan_hash"]);
    let paid = installation.plan(&["--spend", "1", "--until", "script"]);
    assert_eq!(paid["authority"]["usd_micros"], 1_000_000);
    assert_eq!(
        paid["authority"]["stages"],
        serde_json::json!(["research", "script"])
    );
    assert_ne!(paid["plan_hash"], free["plan_hash"]);
    let missing = &paid["blockers"][0]["context"]["missing_limits"];
    assert_eq!(missing.as_array().map(Vec::len), Some(4), "{paid}");
    assert_eq!(files(&installation.path("library")), before);
}

#[test]
fn continue_needs_a_reviewed_plan_and_stops_at_ready() {
    let installation = Installation::new();
    let (unconfirmed, exit) = installation.resume(&[]);
    assert_eq!((code(&unconfirmed), exit), ("RG-INPUT-REQUIRED", 2));
    let (export, exit) = installation.resume(&["--until", "export", "--yes"]);
    assert_eq!((code(&export), exit), ("RG-INPUT-REQUIRED", 2));
    let (no_account, exit) = installation.resume(&["--yes"]);
    assert_eq!((code(&no_account), exit), ("RG-ACCOUNT-REQUIRED", 3));
    installation.initialize_account();
    let (stale, exit) = installation.resume(&["--plan-hash", STALE_HASH]);
    assert_eq!((code(&stale), exit), ("RG-PLAN-CHANGED", 3));
    let (unlimited, exit) = installation.resume(&["--spend", "1", "--yes"]);
    assert_eq!((code(&unlimited), exit), ("RG-ACCOUNT-REQUIRED", 3));
    assert_eq!(installation.account()["jobs"], 0);
}

#[test]
fn reviewed_plan_admits_one_fresh_job() {
    let installation = Installation::new();
    installation.initialize_account();
    let plan = installation.plan(&["--until", "created"]);
    let hash = plan["plan_hash"].as_str().unwrap();
    let (envelope, exit) = installation.resume(&["--until", "created", "--plan-hash", hash]);
    assert_eq!(exit, 0, "{envelope}");
    let continued = &envelope["payload"]["data"]["continued"];
    assert_eq!(continued["plan_hash"], hash);
    assert_eq!(continued["export_authorized"], false);
    assert!(continued["job_id"].as_str().unwrap().starts_with("job-"));
    let account = installation.account();
    assert_eq!(
        (account["jobs"].clone(), account["attempts"].clone()),
        (1.into(), 0.into())
    );
    // The run's saved state moved on, so the reviewed plan no longer applies.
    let (replayed, exit) = installation.resume(&["--until", "created", "--plan-hash", hash]);
    assert_eq!(
        (code(&replayed), exit),
        ("RG-PLAN-CHANGED", 3),
        "{replayed}"
    );
    assert_eq!(installation.account()["jobs"], 1);
}

#[test]
fn engine_failure_is_reported_with_its_job_and_no_dispatch() {
    let installation = Installation::new();
    installation.initialize_account();
    let (envelope, exit) = installation.resume(&["--until", "script", "--yes"]);
    assert_eq!(
        (code(&envelope), exit),
        ("RG-EXECUTION-FAILED", 4),
        "{envelope}"
    );
    let context = &envelope["payload"]["context"];
    assert!(context["job_id"].as_str().unwrap().starts_with("job-"));
    assert_eq!(context["usage"]["requests"]["charged"], 0);
    assert_eq!(installation.account()["attempts"], 0);
}

#[test]
fn plain_plan_names_its_ceilings_and_the_exact_continue_command() {
    let installation = Installation::new();
    installation.initialize_account();
    let hash = installation.plan(&["--spend", "0.015"])["plan_hash"].clone();
    let text = installation.plain(&["video", "plan", &installation.video_id, "--spend", "0.015"]);
    assert!(text.contains("spend up to $0.02"), "{text}");
    assert!(text.contains(hash.as_str().unwrap()), "{text}");
    assert!(text.contains("Blocked: RG-ACCOUNT-REQUIRED"), "{text}");
    let free = installation.plain(&["video", "plan", &installation.video_id]);
    assert!(free.contains("Next: retold video continue"), "{free}");
}

#[test]
fn export_names_its_deliveries_and_needs_a_ready_video() {
    let installation = Installation::new();
    installation.initialize_account();
    let (unnamed, exit) = installation.export(&["--yes"]);
    assert_eq!((code(&unnamed), exit), ("RG-INPUT-REQUIRED", 2));
    let scope = ["--edition", "en", "--format", "both"];
    let (staged, exit) =
        installation.export(&[&scope[..], &["--until", "ready", "--yes"]].concat());
    assert_eq!((code(&staged), exit), ("RG-INPUT-REQUIRED", 2));
    let (preview, exit) = installation.export(&[&scope[..], &["--dry-run"]].concat());
    assert_eq!(exit, 0, "{preview}");
    let plan = &preview["payload"]["data"];
    let export = &plan["authority"]["export"];
    assert_eq!(export["formats"], serde_json::json!(["16x9", "9x16"]));
    assert_eq!(plan["blockers"][0]["code"], "RG-NOT-READY");
    assert_eq!(plan["next"][2], "export");
    assert_ne!(plan["plan_hash"], installation.plan(&[])["plan_hash"]);
    let (unready, exit) = installation.export(&[&scope[..], &["--yes"]].concat());
    assert_eq!((code(&unready), exit), ("RG-NOT-READY", 3), "{unready}");
    assert_eq!(installation.account()["jobs"], 0);
}

fn setup_portrait_preset(installation: &Installation) {
    let presets_dir = installation.path("library").join("presets");
    fs::create_dir_all(&presets_dir).unwrap();
    let preset_json = r#"{
      "aspects": ["9:16", "1:1"],
      "container": "mp4",
      "video_codec": "h264",
      "audio_codec": "aac",
      "caption_mode": "burned"
    }"#;
    fs::write(presets_dir.join("portrait.json"), preset_json).unwrap();
}

#[test]
fn export_resolves_named_preset() {
    let installation = Installation::new();
    installation.initialize_account();
    setup_portrait_preset(&installation);

    let (missing_preset, exit) =
        installation.export(&["--edition", "en", "--preset", "nonexistent", "--dry-run"]);
    assert_eq!((code(&missing_preset), exit), ("RG-REF-NOT-FOUND", 2));

    let (preset_preview, exit) =
        installation.export(&["--edition", "en", "--preset", "portrait", "--dry-run"]);
    assert_eq!(exit, 0, "{preset_preview}");
    let plan = &preset_preview["payload"]["data"];
    let export = &plan["authority"]["export"];
    assert_eq!(export["formats"], serde_json::json!(["1x1", "9x16"]));
    assert_eq!(export["preset"], "portrait");
}

#[test]
fn export_rejects_unsupported_preset_aspect() {
    let installation = Installation::new();
    installation.initialize_account();
    setup_portrait_preset(&installation);

    let (bad_aspect, exit) = installation.export(&[
        "--edition",
        "en",
        "--preset",
        "portrait",
        "--aspect",
        "16:9",
        "--dry-run",
    ]);
    assert_eq!((code(&bad_aspect), exit), ("RG-INPUT-REQUIRED", 2));
}

#[test]
fn export_filters_to_supported_preset_aspect() {
    let installation = Installation::new();
    installation.initialize_account();
    setup_portrait_preset(&installation);

    let (aspect_preview, exit) = installation.export(&[
        "--edition",
        "en",
        "--preset",
        "portrait",
        "--aspect",
        "9:16",
        "--dry-run",
    ]);
    assert_eq!(exit, 0, "{aspect_preview}");
    let plan = &aspect_preview["payload"]["data"];
    let export = &plan["authority"]["export"];
    assert_eq!(export["formats"], serde_json::json!(["9x16"]));
    assert_eq!(export["preset"], "portrait");
}

#[test]
fn cached_stages_qualify_plan_reuse_and_report_zero_cost() {
    let installation = Installation::new();
    installation.initialize_account();

    let mut store = Store::open(&installation.layout).unwrap();
    let key = retold_engine::store::hash(&"evidence-key").unwrap();
    store
        .commit(
            "research",
            &key,
            &serde_json::json!({"summary": "facts"}),
            None,
        )
        .unwrap();
    drop(store);

    let plan_json = installation.plan(&["--until", "research"]);
    let plain_text = installation.plain(&[
        "video",
        "plan",
        &installation.video_id,
        "--until",
        "research",
    ]);
    assert_cached_plan(&plan_json, &plain_text);
}

fn assert_cached_plan(plan_json: &Value, plain_text: &str) {
    assert_eq!(plan_json["all_cached"], true, "{plan_json}");
    assert_eq!(plan_json["dispatches"], 0);
    assert_eq!(plan_json["estimates"]["usd_micros"], 0);
    assert_eq!(plan_json["estimates"]["requests"], 0);
    assert_eq!(plan_json["estimates"]["source"], "qualified_cache");

    let stages = plan_json["stages"].as_array().unwrap();
    let research_stage = stages
        .iter()
        .find(|stage_item| stage_item["stage"] == "research")
        .unwrap();
    assert_eq!(research_stage["decision"], "reused_from_cache");
    assert_eq!(research_stage["envelope_verified"], 1);
    assert!(plain_text.contains("0 steps · $0"), "{plain_text}");
}
