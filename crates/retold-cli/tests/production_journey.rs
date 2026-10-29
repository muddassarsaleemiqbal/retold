//! A draft reaches `ready` and exports a project and a rendered video through the real CLI.
//!
//! Every provider is a loopback stand-in for the OpenRouter API (`fixtures/fake_openrouter.py`)
//! with zero prices, so the run spends nothing and contacts no network service. Media stages
//! use the host's real ffmpeg.
#![cfg(unix)]
#![expect(clippy::unwrap_used, reason = "isolated fixture setup and assertions")]

use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

use serde_json::Value;

const REFERENCE: &str = "studio/films/nosferatu-1922";
const KEY_VARIABLE: &str = "RETOLD_FAKE_OPENROUTER_KEY";
const SERVER_START: Duration = Duration::from_secs(10);
const ALLOWANCE: [&str; 4] = ["--spend", "1", "--max-requests", "2000"];
/// Plans may ask for missing limits and character approval before they can run.
const PLANNING_ROUNDS: usize = 8;
const FINISHED: [&str; 4] = ["completed", "failed", "blocked", "stopped"];
const POLL: Duration = Duration::from_millis(100);
/// The terminal size the PTY driver opens, rows by columns.
const PTY_SIZE: (u16, u16) = (30, 110);
const PTY_SECONDS: &str = "30";
const JOB_LIMIT: Duration = Duration::from_secs(120);
/// Open the linked series part and review its run with a $1 allowance.
const TUI_SERIES_RUN: &str = concat!(
    "KEY:4,SLEEP:0.8,KEY:\r,SLEEP:0.8,KEY:r,SLEEP:0.8,KEY:\t,KEY:\t,",
    "KEY:\x15,KEY:1,KEY:\t,KEY:2000,KEY:\x13,SLEEP:1,KEY:\r,SLEEP:1.5,KEY:\x03"
);
/// Review tab: approve each of the three character sheets through its confirmation.
const TUI_APPROVE: &str = concat!(
    "KEY:3,SLEEP:0.8,KEY:\r,SLEEP:0.8,KEY:\t,KEY:\t,SLEEP:1.5,",
    "KEY:a,SLEEP:0.4,KEY:\x13,SLEEP:0.4,KEY:\r,SLEEP:1,KEY:\x1b,SLEEP:0.8,KEY:\x1b[B,SLEEP:0.4,",
    "KEY:a,SLEEP:0.4,KEY:\x13,SLEEP:0.4,KEY:\r,SLEEP:1,KEY:\x1b,SLEEP:0.8,KEY:\x1b[B,SLEEP:0.4,",
    "KEY:\x1b[B,SLEEP:0.4,KEY:a,SLEEP:0.4,KEY:\x13,SLEEP:0.4,KEY:\r,SLEEP:1,KEY:\x03"
);
/// A reviewed rendered export of the English 9x16 delivery.
const TUI_EXPORT: &str = concat!(
    "KEY:3,SLEEP:0.8,KEY:\r,SLEEP:0.8,KEY:e,SLEEP:0.8,KEY:\t,KEY:en,KEY:\t,KEY:9x16,",
    "KEY:\t,KEY:\t,KEY:\t,KEY:\x13,SLEEP:0.3,KEY:\x15,KEY:1,KEY:\t,KEY:2000,",
    "KEY:\x13,SLEEP:0.3,KEY:\x13,SLEEP:0.3,KEY:\x13,SLEEP:1.5,KEY:\r,SLEEP:1.5,KEY:\x03"
);
const FONT: [&str; 2] = if cfg!(target_os = "macos") {
    ["Helvetica", "Regular"]
} else {
    ["DejaVu Sans", "Book"]
};

/// A channel voiced and captioned by the stand-in, with a font the host has.
const CHANNEL: [&str; 15] = [
    "channels",
    "create",
    "studio",
    "--language",
    "en",
    "--voice-provider",
    "fixture",
    "--voice-model",
    "fixture/tts",
    "--voice",
    "alloy",
    "--font-family",
    FONT[0],
    "--font-style",
    FONT[1],
];
/// The engine fixture's film: its plan, hooks and script answers are written for it.
const NEW_VIDEO: [&str; 10] = [
    "new",
    "fixture",
    "--channel",
    "studio",
    "--input",
    "title=Nosferatu",
    "--input",
    "kind=film",
    "--input",
    "year=1922",
];

struct Journey {
    sandbox: tempfile::TempDir,
    server: Child,
    state: PathBuf,
    worker: Option<Child>,
}

impl Drop for Journey {
    fn drop(&mut self) {
        self.cli(&["studio", "stop"]);
        if let Some(worker) = &mut self.worker {
            worker.kill().ok();
            worker.wait().ok();
        }
        self.server.kill().ok();
        self.server.wait().ok();
    }
}

impl Journey {
    fn start() -> Self {
        let sandbox = tempfile::tempdir().unwrap();
        let port_file = sandbox.path().join("port");
        let script = repository("crates/retold-cli/tests/fixtures/fake_openrouter.py");
        let fixtures = repository("crates/retold-engine/tests/fixtures/full");
        let mut server = Command::new("python3");
        server.args([script, fixtures, port_file.clone()]);
        let server = server.spawn().unwrap();
        let journey = Self::with_server(sandbox, server);
        let port = wait_for_port(&port_file);
        std::fs::create_dir_all(journey.library()).unwrap();
        std::fs::write(journey.library().join("providers.toml"), providers(&port)).unwrap();
        journey
    }

    fn with_server(sandbox: tempfile::TempDir, server: Child) -> Self {
        let state = sandbox.path().join("state");
        Self {
            sandbox,
            server,
            state,
            worker: None,
        }
    }

    /// Acceptance renders the full-HD layout at preview frame rate, without a throughput benchmark.
    fn preview_frame_rate(&self) {
        let guide = self.library().join("guides/fixture/guide.toml");
        let source = std::fs::read_to_string(&guide).unwrap();
        let preview = preview_guide(&source);
        std::fs::write(guide, preview).unwrap();
    }

    fn library(&self) -> PathBuf {
        self.sandbox.path().join("library")
    }

    fn cli(&self, args: &[&str]) -> Value {
        let mut command = Command::new(env!("CARGO_BIN_EXE_retold"));
        command.args(["--json", "--library"]);
        command.arg(self.library());
        command.args(args);
        command.envs(self.environment());
        command.env("RETOLD_CONFIG_DIR", self.library());
        command.env_remove("RETOLD_WORKSPACE");
        let output = command.output().unwrap();
        let envelope: Value = serde_json::from_slice(&output.stdout).unwrap();
        envelope["payload"].clone()
    }

    fn draft(&self) {
        self.prepare_draft_resources();
        let source = repository("crates/retold-engine/tests/fixtures/full/source.txt");
        let created =
            self.cli(&[&NEW_VIDEO[..], &["--source-file", source.to_str().unwrap()]].concat());
        assert_eq!(
            created["data"]["created"]["reference"], REFERENCE,
            "{created}"
        );
    }

    fn prepare_draft_resources(&self) {
        self.cli(&["account", "init", "--timezone", "UTC"]);
        let guide = repository("crates/retold-guide/tests/fixtures/guides/narrative-fixture");
        self.cli(&[
            "guides",
            "create",
            "fixture",
            "--from",
            guide.to_str().unwrap(),
        ]);
        self.preview_frame_rate();
        self.cli(&CHANNEL);
    }
}

/// Planning, then running, until the video is ready: limits and approval are added as asked.
impl Journey {
    fn continue_to_ready(&self) -> Value {
        for _ in 0..PLANNING_ROUNDS {
            let plan = self.cli(&[&["video", "plan", REFERENCE][..], &ALLOWANCE].concat());
            let added = self.add_missing_limits(&plan["data"]);
            if added {
                continue;
            }
            let hash = plan["data"]["plan_hash"].as_str().unwrap();
            let run = self.continue_with(hash);
            let review = run["code"] == "RG-REVIEW-REQUIRED";
            if !review {
                return run;
            }
            let approved = self.cli(&["video", "approve", REFERENCE, "characters"]);
            assert_eq!(approved["outcome"], "completed", "{approved}");
        }
        unreachable!("the video did not reach ready within {PLANNING_ROUNDS} reviewed plans");
    }

    fn continue_with(&self, hash: &str) -> Value {
        let words = [
            &["video", "continue", REFERENCE][..],
            &ALLOWANCE,
            &["--plan-hash", hash],
        ];
        self.cli(&words.concat())
    }

    /// Paid routes need finite limits on every scope; returns whether any were added.
    fn add_missing_limits(&self, plan: &Value) -> bool {
        let blockers = plan["blockers"].as_array().unwrap();
        let missing = blockers
            .iter()
            .filter(|blocker| blocker["code"] == "RG-ACCOUNT-REQUIRED")
            .flat_map(|blocker| blocker["context"]["missing_limits"].as_array().unwrap());
        let limits: Vec<_> = missing.map(limit_words).collect();
        for (scope, window) in &limits {
            let words = [
                "account",
                "limit",
                scope,
                window,
                "--usd",
                "5",
                "--requests",
                "5000",
            ];
            self.cli(&words);
        }
        !limits.is_empty()
    }

    fn export(&self, scope: &[&str]) -> Value {
        let base = [
            &["video", "export", REFERENCE, "--edition", "en"][..],
            scope,
            &ALLOWANCE,
        ];
        let preview = self.cli(&[&base.concat()[..], &["--dry-run"]].concat());
        assert_eq!(preview["data"]["can_execute"], true, "{preview}");
        let hash = preview["data"]["plan_hash"].as_str().unwrap();
        self.cli(&[&base.concat()[..], &["--plan-hash", hash]].concat())
    }

    fn deliverable(&self, name: &str) -> PathBuf {
        self.library()
            .join(REFERENCE)
            .join("deliverables/en/9x16")
            .join(name)
    }
}

/// The TUI in a pseudo-terminal, typing `keys` (comma-separated `KEY:`/`SLEEP:` actions).
impl Journey {
    fn tui(&self, keys: &str) -> Vec<u8> {
        let mut command = Command::new("python3");
        command.args(["-c", include_str!("fixtures/pty_driver.py")]);
        command.envs(self.environment());
        command.env("RETOLD_BIN", env!("CARGO_BIN_EXE_retold"));
        command.envs([
            ("RETOLD_LIB", self.library()),
            ("RETOLD_CFG", self.library()),
        ]);
        command.envs([
            ("RETOLD_ACTIONS", keys),
            ("RETOLD_PTY_SECONDS", PTY_SECONDS),
        ]);
        command.env_remove("NO_COLOR");
        command.env_remove("RETOLD_WORKSPACE");
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stderr),
            final_screen(&output.stdout)
        );
        output.stdout
    }

    /// An isolated home and studio state, with the stand-in's key and no ambient workspace.
    fn environment(&self) -> Vec<(String, std::ffi::OsString)> {
        let sandbox = self.sandbox.path();
        vec![
            ("HOME".into(), sandbox.join("home").into()),
            ("RETOLD_STATE".into(), self.state.clone().into()),
            (KEY_VARIABLE.into(), "fixture".into()),
            ("XDG_DATA_HOME".into(), sandbox.join("data").into()),
        ]
    }
}

/// Waits for the newest studio job to settle and returns it.
impl Journey {
    fn settled_job(&self, count: usize) -> Value {
        let started = Instant::now();
        loop {
            let jobs = self.cli(&["queue", "list"])["data"]["jobs"].clone();
            let jobs = jobs.as_array().unwrap();
            let newest = jobs
                .iter()
                .max_by_key(|job| job["created_at"].as_str().map(str::to_owned));
            let done =
                newest.filter(|job| FINISHED.contains(&job["status"].as_str().unwrap_or("")));
            if jobs.len() == count
                && let Some(job) = done
            {
                return job.clone();
            }
            assert!(
                started.elapsed() < JOB_LIMIT,
                "job did not settle: {jobs:?}"
            );
            std::thread::sleep(POLL);
        }
    }
}

/// The last screen drawn before the TUI left its alternate screen.
fn final_screen(raw: &[u8]) -> String {
    let rows = std::num::NonZeroU16::new(PTY_SIZE.0).unwrap();
    let columns = std::num::NonZeroU16::new(PTY_SIZE.1).unwrap();
    let mut terminal = vt100::Parser::new(rows, columns, 0);
    let mut last = String::new();
    for byte in raw {
        terminal.process(std::slice::from_ref(byte));
        let screen = terminal.screen().contents();
        let drawn = !screen.trim().is_empty();
        if drawn {
            last = screen;
        }
    }
    last
}

fn preview_guide(source: &str) -> String {
    assert!(source.contains("fps = 30"));
    source.replace("fps = 30", "fps = 5")
}

fn limit_words(limit: &Value) -> (String, String) {
    let scope = &limit["scope"];
    let kind = scope["kind"].as_str().unwrap();
    let named = scope["id"].as_str().map(|id| format!("{kind}:{id}"));
    let window = limit["window"].as_str().unwrap().to_owned();
    (named.unwrap_or_else(|| kind.to_owned()), window)
}

fn repository(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

fn wait_for_port(port_file: &Path) -> String {
    let started = Instant::now();
    loop {
        let port = std::fs::read_to_string(port_file).unwrap_or_default();
        if !port.is_empty() {
            return port;
        }
        assert!(
            started.elapsed() < SERVER_START,
            "fake OpenRouter did not start"
        );
        std::thread::sleep(POLL);
    }
}

/// Every task tier routes to the stand-in's matching zero-priced model.
fn providers(port: &str) -> String {
    let routes = [
        ("text.fast", "text"),
        ("text.smart", "text"),
        ("text.best", "text"),
        ("vision.fast", "vision"),
        ("vision.smart", "vision"),
        ("image", "image"),
        ("image.no_refs", "image"),
        ("tts", "tts"),
        ("transcribe", "whisper"),
    ];
    let routing = routes
        .iter()
        .fold(String::new(), |mut table, (tier, model)| {
            use std::fmt::Write;
            let _ = writeln!(table, "\"{tier}\" = [\"fixture:fixture/{model}\"]");
            table
        });
    format!(
        "schema_version = 1\n[providers.fixture]\nkind = \"openrouter\"\nkey_env = \"{KEY_VARIABLE}\"\nmax_parallel = 4\nbase_url = \"http://127.0.0.1:{port}\"\n[routing]\n{routing}"
    )
}

#[test]
fn draft_reaches_ready_after_character_approval_and_exports_project_and_video() {
    let journey = Journey::start();
    journey.draft();
    let ready = journey.continue_to_ready();
    assert_eq!(ready["outcome"], "completed", "{ready}");
    let project = journey.export(&["--format", "9x16", "--project-only"]);
    assert_eq!(project["outcome"], "completed", "{project}");
    assert!(journey.deliverable("nosferatu-1922.en.srt").is_file());
    let rendered = journey.export(&["--format", "9x16"]);
    assert_eq!(rendered["outcome"], "completed", "{rendered}");
    let video = std::fs::metadata(journey.deliverable("nosferatu-1922.en.mp4")).unwrap();
    assert!(video.len() > 0);
}

/// The standalone desktop gate supplies its compiled native interaction driver.
#[test]
#[cfg(feature = "native-acceptance")]
fn native_desktop_exports_ready_video() {
    let driver = std::env::var_os("RETOLD_NATIVE_DRIVER").unwrap();
    let mut journey = Journey::start();
    journey.state = journey.library().join(".retold/studio");
    journey.draft();
    assert_eq!(journey.continue_to_ready()["outcome"], "completed");
    journey.cli(&["studio", "start"]);
    let output = journey.native_driver(&driver, "--ready-export-fixture");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(journey.settled_job(1)["status"], "completed");
    journey.assert_rendered_video();
}

#[test]
#[cfg(feature = "native-acceptance")]
fn native_desktop_runs_through_character_review_and_exports() {
    let driver = std::env::var_os("RETOLD_NATIVE_DRIVER").unwrap();
    let mut journey = Journey::start();
    journey.state = journey.library().join(".retold/studio");
    journey.prepare_draft_resources();
    let created = journey.native_driver(&driver, "--source-draft-fixture");
    assert!(
        created.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&created.stdout),
        String::from_utf8_lossy(&created.stderr)
    );
    let plan = journey.cli(&[&["video", "plan", REFERENCE][..], &ALLOWANCE].concat());
    journey.add_missing_limits(&plan["data"]);
    journey.start_packaged_worker();
    let output = journey.native_driver(&driver, "--run-review-fixture");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(journey.settled_job(3)["status"], "completed");
    journey.assert_rendered_video();
    journey.assert_character_approval_uses_generation_lock();
}

#[cfg(feature = "native-acceptance")]
impl Journey {
    fn start_packaged_worker(&mut self) {
        let binary = std::env::var_os("RETOLD_NATIVE_WORKER").unwrap();
        let mut command = Command::new(binary);
        command.args(["--worker", "--library"]).arg(self.library());
        command.envs(self.environment());
        command.env("RETOLD_CONFIG_DIR", self.library());
        command.env_remove("RETOLD_WORKSPACE");
        self.worker = Some(command.spawn().unwrap());
        let started = Instant::now();
        while !retold_app::studio::query_status(&self.state).running {
            assert!(
                started.elapsed() < SERVER_START,
                "Packaged worker did not start"
            );
            std::thread::sleep(POLL);
        }
    }

    fn assert_character_approval_uses_generation_lock(&self) {
        let layout = retold_engine::RunLayout::from_video_dir(
            &self.library(),
            "studio",
            &self.library().join(REFERENCE),
            2,
        )
        .unwrap();
        let approvals = layout
            .stage(retold_engine::Stage::Characters)
            .join("approved.json");
        let original = std::fs::read(&approvals).unwrap();
        let _owner = retold_engine::store::Store::open(&layout).unwrap();
        let denied = self.cli(&["video", "approve", REFERENCE, "characters"]);
        assert_eq!(denied["code"], "RG-INPUT-REQUIRED", "{denied}");
        assert_eq!(std::fs::read(approvals).unwrap(), original);
    }

    fn native_driver(&self, driver: &std::ffi::OsStr, mode: &str) -> std::process::Output {
        let mut command = Command::new(driver);
        command.arg(mode).arg(self.library()).arg(REFERENCE);
        command.envs(self.environment());
        command.env(
            "RETOLD_NATIVE_SOURCE",
            repository("crates/retold-engine/tests/fixtures/full/source.txt"),
        );
        command.env("RETOLD_CONFIG_DIR", self.library());
        command.env(
            "RETOLD_GUI_EVIDENCE",
            repository("target/gpui-g0/evidence/native-production"),
        );
        command.env_remove("RETOLD_WORKSPACE");
        command.output().unwrap()
    }
}

#[test]
fn the_tui_runs_a_series_part_through_review_to_ready_and_a_rendered_video() {
    let journey = Journey::series_fixture();
    let screen = final_screen(&journey.tui(TUI_SERIES_RUN));
    let job = journey.settled_job(1);
    assert_eq!(job["error_code"], "RG-REVIEW-REQUIRED", "{job}\n{screen}");
    let screen = final_screen(&journey.tui(TUI_APPROVE));
    assert!(screen.contains("Approved character sheets"), "{screen}");
    let screen = final_screen(&journey.tui(TUI_SERIES_RUN));
    assert_eq!(journey.settled_job(2)["status"], "completed", "{screen}");
    let screen = final_screen(&journey.tui(TUI_EXPORT));
    assert_eq!(journey.settled_job(3)["status"], "completed", "{screen}");
    journey.assert_rendered_video();
    journey.assert_series_complete();
    journey.cli(&["studio", "stop"]);
    assert!(
        journey.deliverable("nosferatu-1922.en.srt").is_file(),
        "{screen}"
    );
}

impl Journey {
    fn series_fixture() -> Self {
        let journey = Self::start();
        journey.draft();
        let plan = journey.cli(&[&["video", "plan", REFERENCE][..], &ALLOWANCE].concat());
        journey.add_missing_limits(&plan["data"]);
        journey.link_series();
        journey.cli(&["studio", "start"]);
        journey
    }

    fn link_series(&self) {
        let created = self.cli(&[
            "series",
            "new",
            "fixture",
            "--work",
            "Saga",
            "--channel",
            "studio",
            "--parts",
            "1",
        ]);
        assert_eq!(created["outcome"], "completed", "{created}");
        let linked = self.cli(&[
            "series", "update", "saga", "--part", "P01", "--video", REFERENCE,
        ]);
        assert_eq!(linked["outcome"], "completed", "{linked}");
    }

    fn assert_series_complete(&self) {
        let shown = self.cli(&["series", "show", "saga"]);
        assert_eq!(shown["data"]["completed_parts"], 1, "{shown}");
        let screen = final_screen(&self.tui("KEY:4,SLEEP:0.8,KEY:\r,SLEEP:0.8,KEY:\x03"));
        assert!(screen.contains("1/1 parts complete"), "{screen}");
        let manifest = self
            .library()
            .join(REFERENCE)
            .join("deliverables/manifest.json");
        std::fs::write(manifest, b"invalid delivery evidence").unwrap();
        let broken = self.cli(&["series", "show", "saga"]);
        assert_eq!(broken["data"]["completed_parts"], 0, "{broken}");
    }

    fn probe_video(&self) -> Value {
        let mut command = Command::new("ffprobe");
        command.args([
            "-v",
            "error",
            "-show_streams",
            "-show_format",
            "-of",
            "json",
        ]);
        let output = command
            .arg(self.deliverable("nosferatu-1922.en.mp4"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    }

    fn assert_rendered_video(&self) {
        let probe = self.probe_video();
        let streams = probe["streams"].as_array().unwrap();
        assert!(
            streams.iter().any(|stream| stream["codec_type"] == "video"),
            "{probe}"
        );
        assert!(
            streams.iter().any(|stream| stream["codec_type"] == "audio"),
            "{probe}"
        );
        let duration: f64 = probe["format"]["duration"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        assert!(duration > 0.0, "{probe}");
    }
}
