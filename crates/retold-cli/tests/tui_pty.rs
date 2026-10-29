//! PTY integration test suite for `retold`.
//! Verifies terminal lifecycle, alternate screen restoration, interactive navigation across Home,
//! Spend, Providers, and Video screens, key controls, and strict read-only preservation.

#![expect(
    clippy::unwrap_used,
    reason = "fixture helpers in tests must fail on invalid setup"
)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use assert_cmd::cargo::cargo_bin;
use retold_engine::gate::Usd;
use retold_engine::{NewRun, Quality, run};

const VIDEO_CAP: Usd = Usd::from_micros(5_000_000);
const NO_PROVIDERS: &str = "schema_version = 1\n[providers]\n[routing]\n";
const HASH_LEN: usize = 32;
const TERMINAL_ROWS: u16 = 30;
const TERMINAL_COLUMNS: u16 = 110;
const PROVIDER_FORM_FIELDS: [usize; 3] = [6, 5, 3];
const RESIZE_WINDOWS: &[(u16, u16)] = &[
    (1, 1),
    (1, 40),
    (5, 4),
    (20, 8),
    (40, 12),
    (71, 20),
    (72, 20),
    (80, 24),
    (200, 60),
    (0, 0),
    (110, 30),
];
const EXECUTABLE_MODE: u32 = 0o755;

const ENTER_ALT: &[u8] = b"\x1b[?1049h";
const LEAVE_ALT: &[u8] = b"\x1b[?1049l";
const SHOW_CURSOR: &[u8] = b"\x1b[?25h";

type FileHash = [u8; HASH_LEN];

const PTY_SCRIPT: &str = include_str!("fixtures/pty_driver.py");

struct PtySandbox {
    sandbox_dir: tempfile::TempDir,
}

impl PtySandbox {
    fn new() -> Self {
        let sandbox = Self {
            sandbox_dir: tempfile::tempdir().unwrap(),
        };
        sandbox.seed();
        sandbox
    }

    fn seed(&self) {
        std::fs::create_dir_all(self.library_path()).unwrap();
        std::fs::write(self.library_path().join("providers.toml"), NO_PROVIDERS).unwrap();
    }

    fn config_path(&self) -> PathBuf {
        self.library_path()
    }

    fn library_path(&self) -> PathBuf {
        self.sandbox_dir.path().join("library")
    }

    fn run(&self, actions: &str) -> (i32, Vec<u8>) {
        self.run_with(actions, &[])
    }

    fn run_with(&self, actions: &str, environment: &[(&str, &Path)]) -> (i32, Vec<u8>) {
        let bin = cargo_bin("retold");
        let mut command = Command::new("python3");
        command.arg("-c").arg(PTY_SCRIPT);
        command.env("RETOLD_BIN", bin);
        command.env("RETOLD_LIB", self.library_path());
        command.env("RETOLD_CFG", self.config_path());
        command.env("RETOLD_ACTIONS", actions);
        command.env("PYTHONDONTWRITEBYTECODE", "1");
        let mut command = self.discovery_environment(command);
        command
            .env_remove("NO_COLOR")
            .env_remove("RETOLD_WORKSPACE")
            .env_remove("XDG_DATA_HOME");
        command
            .env_remove("CODEX_HOME")
            .env_remove("CLAUDE_CONFIG_DIR");
        command.env("HOME", self.sandbox_dir.path().join("home"));
        command.env("RETOLD_STATE", self.sandbox_dir.path().join("state"));
        command.envs(environment.iter().copied());
        let output = command.output().unwrap();
        let status = output.status;
        let code = status.code().unwrap_or(-1);
        let mut transcript = output.stdout;
        transcript.extend(output.stderr);
        (code, transcript)
    }
}

impl PtySandbox {
    fn discovery_environment(&self, mut command: Command) -> Command {
        command.env_remove("RETOLD_PTY_ARGS");
        let programs = self.sandbox_dir.path().join("programs");
        if programs.is_dir() {
            command.env("RETOLD_TEST_PATH", programs);
        }
        command.env(
            "RETOLD_TEST_PROVIDER_MARKER",
            self.sandbox_dir.path().join("provider-invoked"),
        );
        command
    }

    fn discovery_fixture(&self) {
        std::fs::create_dir_all(self.sandbox_dir.path().join("home/.claude-work")).unwrap();
        create_discovery_program(&self.sandbox_dir.path().join("programs"));
    }
}

fn create_discovery_program(directory: &Path) {
    std::fs::create_dir_all(directory).unwrap();
    fake_discovered_program(&directory.join("claude"));
}

fn fake_discovered_program(binary: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(
        binary,
        "#!/bin/sh\nset -e\nprintf invoked > \"$RETOLD_TEST_PROVIDER_MARKER\"\nexit 91\n",
    )
    .unwrap();
    std::fs::set_permissions(binary, std::fs::Permissions::from_mode(EXECUTABLE_MODE)).unwrap();
}

fn discovery_review_keys() -> String {
    let mut actions = String::from(concat!(
        "KEY:7,KEY:\t,KEY:\t,KEY:\t,KEY:\t,SLEEP:0.4,KEY:\r,SLEEP:0.2,",
        "KEY:\t,KEY:\t,KEY:\r,SLEEP:0.2,KEY:smart=sonnet,"
    ));
    for fields in PROVIDER_FORM_FIELDS {
        actions.push_str(&"KEY:\t,".repeat(fields));
        actions.push_str("KEY:\r,SLEEP:0.2,");
    }
    actions
}

#[test]
fn pty_discovery_cancels_then_adopts_a_reviewed_definition_without_running_it() {
    let sandbox = PtySandbox::new();
    sandbox.route_fixture();
    sandbox.discovery_fixture();
    let before = file_hashes(&sandbox.library_path());
    let review = discovery_review_keys();
    let (exit, raw) = sandbox.run(&format!(
        "{review}KEY:\x1b,SLEEP:0.2,KEY:\x1b,SLEEP:0.2,KEY:q"
    ));
    assert_eq!(exit, 0);
    assert!(screen_contains(&raw, "Discover · Offline"));
    assert!(screen_contains(&raw, "Cancel"));
    assert_eq!(file_hashes(&sandbox.library_path()), before);
    let (exit, raw) = sandbox.run(&format!("{review}KEY:\r,SLEEP:0.6,KEY:q,SLEEP:0.2,KEY:q"));
    assert_eq!(exit, 0);
    assert!(screen_contains(&raw, "Provider added"));
    assert_discovered_provider_saved(&sandbox);
}

fn assert_discovered_provider_saved(sandbox: &PtySandbox) {
    let routes = retold_app::providers::routes(&sandbox.library_path());
    let result = match routes {
        retold_app::protocol::Payload::Result { contents, .. } => Some(contents),
        _ => None,
    };
    let contents = result.unwrap();
    assert_eq!(
        contents["routing"]["text.fast"],
        serde_json::json!(["first:fast", "second:fast"])
    );
    assert_eq!(
        contents["routing"]["text.smart"],
        serde_json::json!(["first:smart"])
    );
    let config = std::fs::read_to_string(sandbox.library_path().join("providers.toml")).unwrap();
    assert!(config.contains("[providers.claude]"), "{config}");
    assert!(config.contains("[providers.first]"), "{config}");
    assert!(!sandbox.sandbox_dir.path().join("provider-invoked").exists());
}

impl PtySandbox {
    fn route_fixture(&self) -> (PathBuf, &'static str) {
        let source = r#"schema_version = 1
[providers.first]
kind = "codex_cli"
binary = "/does-not-exist/first"
max_parallel = 1
[providers.second]
kind = "codex_cli"
binary = "/does-not-exist/second"
max_parallel = 1
[routing]
"text.fast" = ["first:fast", "second:fast"]
"text.smart" = ["first:smart"]
"#;
        let path = self.library_path().join("providers.toml");
        std::fs::write(&path, source).unwrap();
        (path, source)
    }
}

fn file_hashes(root: &Path) -> BTreeMap<PathBuf, FileHash> {
    let mut map = BTreeMap::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return map;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            map.extend(file_hashes(&path));
        } else if let Ok(bytes) = std::fs::read(&path) {
            map.insert(path, *blake3::hash(&bytes).as_bytes());
        }
    }
    map
}

fn init_studio_channel(config: &Path) -> retold_guide::Channel {
    let providers_file = config.join("providers.toml");
    let channel_dest = config.join("channels/test-studio");
    std::fs::write(providers_file, NO_PROVIDERS).unwrap();
    retold_guide::channel::init_from_template("test-studio", &channel_dest).unwrap()
}

fn create_video_run(library: &Path, channel: &retold_guide::Channel) {
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
    run::create(library, new).unwrap();
}

fn setup_video(config: &Path, library: &Path) {
    let channel = init_studio_channel(config);
    create_video_run(library, &channel);
}

fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// Ratatui sends changed cells and cursor moves, not complete strings on every redraw.
/// The last fully drawn screen, for readable failure messages.
fn final_screen(raw: &[u8]) -> String {
    let rows = std::num::NonZeroU16::new(TERMINAL_ROWS).unwrap();
    let columns = std::num::NonZeroU16::new(TERMINAL_COLUMNS).unwrap();
    let mut terminal = vt100::Parser::new(rows, columns, 0);
    terminal.process(raw);
    terminal.screen().contents()
}

fn screen_contains(raw: &[u8], text: &str) -> bool {
    let rows = std::num::NonZeroU16::new(TERMINAL_ROWS).unwrap();
    let columns = std::num::NonZeroU16::new(TERMINAL_COLUMNS).unwrap();
    let mut terminal = vt100::Parser::new(rows, columns, 0);
    raw.iter().any(|byte| {
        terminal.process(std::slice::from_ref(byte));
        terminal.screen().contents().contains(text)
    })
}

#[test]
fn pty_lifecycle_terminal_restoration_and_read_only() {
    let sandbox = PtySandbox::new();
    let before_hashes = file_hashes(&sandbox.library_path());

    let (exit, raw) = sandbox.run("KEY:q");
    assert_eq!(exit, 0);
    assert!(contains_subslice(&raw, ENTER_ALT));
    assert!(contains_subslice(&raw, LEAVE_ALT));
    assert!(contains_subslice(&raw, SHOW_CURSOR));

    let after_hashes = file_hashes(&sandbox.library_path());
    assert_eq!(before_hashes, after_hashes, "library remained untouched");
}

#[test]
fn pty_navigation_spend_providers_and_actions() {
    let sandbox = PtySandbox::new();
    let before_hashes = file_hashes(&sandbox.library_path());

    let actions = concat!(
        "KEY:8,SLEEP:0.4,KEY:7,SLEEP:0.4,KEY::,",
        "KEY:providers add,SLEEP:0.2,KEY:\r,SLEEP:0.3,KEY:\x1b,SLEEP:0.2,KEY:q"
    );
    let (exit, raw) = sandbox.run(actions);
    assert_eq!(exit, 0);
    assert!(contains_subslice(&raw, ENTER_ALT));
    assert!(contains_subslice(&raw, LEAVE_ALT));
    assert!(screen_contains(&raw, "Spend"));
    assert!(screen_contains(&raw, "Providers"));
    assert!(screen_contains(&raw, "Provider"));

    let after_hashes = file_hashes(&sandbox.library_path());
    assert_eq!(before_hashes, after_hashes, "library remained untouched");
}

#[test]
fn pty_video_screen_and_log_toggle() {
    let sandbox = PtySandbox::new();
    setup_video(&sandbox.config_path(), &sandbox.library_path());
    let before_hashes = file_hashes(&sandbox.library_path());

    let actions = concat!(
        "KEY:3,SLEEP:0.3,KEY:\r,SLEEP:0.4,KEY:\t,",
        "SLEEP:0.3,KEY:\t,SLEEP:0.3,KEY:\t,SLEEP:0.3,KEY:\x1b,SLEEP:0.2,KEY:q"
    );
    let (exit, raw) = sandbox.run(actions);
    assert_eq!(exit, 0);
    assert!(contains_subslice(&raw, ENTER_ALT));
    assert!(contains_subslice(&raw, LEAVE_ALT));
    assert!(screen_contains(&raw, "Nosferatu"));
    assert!(screen_contains(&raw, "Evidence"));
    assert!(contains_subslice(&raw, b"Costs"));

    let after_hashes = file_hashes(&sandbox.library_path());
    assert_eq!(before_hashes, after_hashes, "video run remained untouched");
}

#[test]
fn pty_command_palette_and_theme_toggle() {
    let sandbox = PtySandbox::new();
    let before_hashes = file_hashes(&sandbox.library_path());

    let actions = concat!(
        "KEY::,SLEEP:0.4,",
        "KEY:h,SLEEP:0.1,KEY:o,SLEEP:0.1,KEY:m,SLEEP:0.1,KEY:e,SLEEP:0.2,",
        "KEY:\x1b,SLEEP:0.4,",
        "KEY:T,SLEEP:0.3,",
        "KEY:T,SLEEP:0.3,",
        "KEY:q"
    );
    let (exit, raw) = sandbox.run(actions);
    assert_eq!(exit, 0);
    assert!(contains_subslice(&raw, ENTER_ALT));
    assert!(contains_subslice(&raw, LEAVE_ALT));
    assert!(screen_contains(&raw, "Commands"));
    assert!(screen_contains(&raw, "High contrast"));
    assert!(screen_contains(&raw, "Light"));

    let after_hashes = file_hashes(&sandbox.library_path());
    assert_eq!(before_hashes, after_hashes, "library remained untouched");
}

#[test]
fn pty_creates_a_provider_through_the_form_and_confirmation() {
    /// Fields after the first in each later step: models and login, programs and limits, review.
    const STEP_FIELDS: [usize; 3] = [6, 5, 3];
    let sandbox = PtySandbox::new();
    // Step 1: name, then pick the first kind (claude_cli) and continue.
    let mut actions = String::from("KEY:7,KEY:n,SLEEP:0.3,KEY:claude-tui,KEY:\t,KEY:\x1b[C,");
    actions.push_str("KEY:\t,KEY:\r,SLEEP:0.2,");
    // Step 2 starts on the tier field.
    actions.push_str("KEY:smart=sonnet,");
    for fields in STEP_FIELDS {
        actions.push_str(&"KEY:\t,".repeat(fields));
        actions.push_str("KEY:\r,SLEEP:0.2,");
    }
    actions.push_str("SLEEP:0.3,KEY:\r,SLEEP:0.6,KEY:q,SLEEP:0.2,KEY:q");
    let (exit, raw) = sandbox.run(&actions);
    assert_eq!(exit, 0);
    assert!(contains_subslice(&raw, LEAVE_ALT));
    let config = std::fs::read_to_string(sandbox.library_path().join("providers.toml")).unwrap();
    assert!(config.contains("[providers.claude-tui]"), "{config}");
    assert!(config.contains("kind = \"claude_cli\""), "{config}");
    assert!(screen_contains(&raw, "Provider added"));
}

#[test]
fn pty_route_editor_cancels_without_writing_then_saves_reviewed_order() {
    let sandbox = PtySandbox::new();
    let (path, source) = sandbox.route_fixture();
    let (exit, raw) = sandbox.run(concat!(
        "KEY:7,KEY:\t,SLEEP:0.4,KEY:\r,SLEEP:0.2,KEY:J,",
        "SLEEP:0.2,KEY:\x1b,SLEEP:0.2,KEY:q"
    ));
    assert_eq!(exit, 0);
    assert!(screen_contains(&raw, "Route · text.fast"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), source);
    let (exit, raw) = sandbox.run(concat!(
        "KEY:7,KEY:\t,SLEEP:0.4,KEY:\r,SLEEP:0.2,KEY:J,",
        "KEY:\r,SLEEP:0.2,KEY:\r,SLEEP:0.6,KEY:q,SLEEP:0.2,KEY:q"
    ));
    assert_eq!(exit, 0);
    assert!(screen_contains(&raw, "Route updated"));
    assert!(contains_subslice(&raw, LEAVE_ALT));
    let payload = retold_app::providers::routes(&sandbox.library_path());
    let retold_app::protocol::Payload::Result { contents, .. } = payload else {
        panic!("{payload:?}");
    };
    assert_eq!(
        contents["routing"]["text.fast"],
        serde_json::json!(["second:fast", "first:fast"])
    );
    assert_eq!(
        contents["routing"]["text.smart"],
        serde_json::json!(["first:smart"])
    );
}

#[test]
fn pty_provider_overview_displays_recorded_success_without_running_a_provider() {
    use retold_providers::health::Health;

    let sandbox = PtySandbox::new();
    sandbox.route_fixture();
    let config = retold_engine::provider_settings::load(&sandbox.library_path()).unwrap();
    let ledger =
        retold_engine::provider_health::HealthLedger::new(&sandbox.library_path(), &config)
            .unwrap();
    ledger.succeeded("first").unwrap();
    let before = file_hashes(&sandbox.library_path());
    let (exit, raw) = sandbox.run("KEY:7,WAIT:Last check succeeded,WAIT:missing_profile,KEY:\x03");
    assert_eq!(exit, 0, "{}", final_screen(&raw));
    assert!(screen_contains(&raw, "Last check succeeded"));
    assert!(screen_contains(&raw, "Login evidence"));
    assert!(!screen_contains(&raw, "not_checked"));
    assert!(contains_subslice(&raw, LEAVE_ALT));
    assert_eq!(file_hashes(&sandbox.library_path()), before);
}

#[test]
fn pty_provider_health_and_login_inspection_remain_read_only() {
    let sandbox = PtySandbox::new();
    sandbox.route_fixture();
    let before = file_hashes(&sandbox.library_path());
    let (exit, raw) = sandbox.run(concat!(
        "KEY:7,KEY:\t,KEY:\t,SLEEP:0.4,KEY:\r,SLEEP:0.4,",
        "KEY:c,SLEEP:0.2,KEY:\x1b,SLEEP:0.2,KEY:\x1b,SLEEP:0.2,KEY:\t,SLEEP:0.3,",
        "KEY:\r,SLEEP:0.4,KEY:r,SLEEP:0.4,KEY:\x1b,SLEEP:0.2,KEY:q"
    ));
    assert_eq!(exit, 0);
    assert!(contains_subslice(&raw, LEAVE_ALT));
    assert!(screen_contains(&raw, "Provider status · first"));
    assert!(screen_contains(&raw, "Not checked"));
    assert!(screen_contains(&raw, "Login evidence"));
    assert!(screen_contains(&raw, "no provider requests sent"));
    assert_eq!(file_hashes(&sandbox.library_path()), before);
}

/// Routes as the CLI reports them, so the check reads what a run would use.
fn saved_routes(root: &Path) -> serde_json::Value {
    let mut command = Command::new(cargo_bin("retold"));
    command
        .args(["--json", "providers", "route", "--root"])
        .arg(root);
    let output = command.output().unwrap();
    let reply: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    reply["payload"]["data"]["routing"].clone()
}

/// F2, then the provider step: name it, pick Claude, give it a smart tier, run it. Setup moves
/// on to routing by itself, where suggested routes are accepted. Two result pages stay open, so
/// `q` backs out of each before it quits.
const FRESH_SETUP: &str = concat!(
    "KEY:\x1bOQ,SLEEP:0.3,KEY:\x1b[B,KEY:\x1b[B,KEY:\r,SLEEP:0.3,",
    "KEY:claude-tui,KEY:\t,KEY:\x1b[C,KEY:\x13,SLEEP:0.2,KEY:smart=sonnet,",
    "KEY:\x13,KEY:\x13,KEY:\x13,SLEEP:0.3,KEY:\r,SLEEP:1.0,",
    "KEY:\r,SLEEP:0.3,KEY:\x13,SLEEP:0.3,KEY:\r,SLEEP:1.0,",
    "KEY:\x1b,SLEEP:0.2,KEY:q,SLEEP:0.2,KEY:q,SLEEP:0.2,KEY:q"
);

#[test]
fn pty_fresh_setup_adds_a_provider_then_accepts_suggested_routes() {
    let sandbox = PtySandbox::new();
    let (exit, raw) = sandbox.run(FRESH_SETUP);
    assert_eq!(exit, 0);
    assert!(contains_subslice(&raw, LEAVE_ALT));
    let config = std::fs::read_to_string(sandbox.library_path().join("providers.toml")).unwrap();
    assert!(config.contains("[providers.claude-tui]"), "{config}");
    let routing = saved_routes(&sandbox.library_path());
    for tier in ["text.smart", "text.fast", "text.best"] {
        assert_eq!(routing[tier][0], "claude-tui:smart", "{tier}\n{config}");
    }
    assert!(screen_contains(&raw, "Suggested routes saved"));
}

fn initialize_money(sandbox: &PtySandbox) -> PathBuf {
    let home = sandbox.sandbox_dir.path().join("home");
    for args in [
        vec!["account", "init"],
        vec![
            "account",
            "limit",
            "account",
            "day",
            "--usd",
            "5",
            "--requests",
            "100",
        ],
    ] {
        let mut command = Command::new(cargo_bin("retold"));
        command.env("HOME", &home).env_remove("XDG_DATA_HOME");
        let status = command.args(args).output().unwrap();
        assert!(
            status.status.success(),
            "{}",
            String::from_utf8_lossy(&status.stderr)
        );
    }
    money_directory(&home)
}

fn money_directory(home: &Path) -> PathBuf {
    let files = file_hashes(home);
    let database = files
        .keys()
        .find(|path| {
            path.file_name()
                .is_some_and(|name| name == "accounting.sqlite")
        })
        .unwrap();
    database.parent().unwrap().to_path_buf()
}

#[test]
fn pty_money_limit_preview_cancellation_preserves_accounting() {
    let sandbox = PtySandbox::new();
    let home = initialize_money(&sandbox);
    let before = file_hashes(&home);
    let (exit, raw) = sandbox.run(concat!(
        "KEY:8,KEY:\t,SLEEP:0.5,KEY:\r,SLEEP:0.2,",
        "KEY:\t,KEY:\t,KEY:2,KEY:\x13,SLEEP:0.5,",
        "KEY:\x1b,SLEEP:0.2,KEY:\x1b,SLEEP:0.2,KEY:q"
    ));
    assert_eq!(exit, 0);
    assert!(screen_contains(&raw, "Review change"));
    assert_eq!(file_hashes(&home), before);
}

#[test]
fn pty_structured_guide_field_cancellation_preserves_configuration() {
    let sandbox = PtySandbox::new();
    retold_app::authoring::create_guide(
        &sandbox.config_path(),
        &retold_app::authoring::GuideOptions {
            id: "example".into(),
            language: Some("en".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let before = file_hashes(&sandbox.config_path());
    let (exit, raw) = sandbox.run(concat!(
        "KEY:9,SLEEP:0.5,KEY:\r,SLEEP:0.4,KEY:\r,SLEEP:0.2,",
        "KEY:\x1b,SLEEP:0.2,KEY:q,SLEEP:0.2,KEY:q"
    ));
    assert_eq!(exit, 0);
    assert!(screen_contains(&raw, "Edit configuration field"));
    assert_eq!(file_hashes(&sandbox.config_path()), before);
}

#[test]
fn pty_structured_guide_field_applies_the_reviewed_change() {
    let sandbox = PtySandbox::new();
    retold_app::authoring::create_guide(
        &sandbox.config_path(),
        &retold_app::authoring::GuideOptions {
            id: "example".into(),
            language: Some("en".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let sidecar = sandbox.config_path().join("guides/example/voice.md");
    let original = std::fs::read(&sidecar).unwrap();
    let (exit, raw) = sandbox.run(concat!(
        "KEY:9,SLEEP:0.5,KEY:\r,SLEEP:0.4,KEY:\r,SLEEP:0.2,",
        "KEY:\x15,KEY:Reviewed title,KEY:\r,SLEEP:0.5,KEY:\r,SLEEP:0.5,",
        "KEY:q,SLEEP:0.2,KEY:q,SLEEP:0.2,KEY:q"
    ));
    assert_eq!(exit, 0);
    assert!(screen_contains(&raw, "Review change"));
    let path = sandbox.config_path().join("guides/example/guide.toml");
    let manifest = std::fs::read_to_string(path).unwrap();
    assert!(manifest.contains("name = \"Reviewed title\""), "{manifest}");
    assert_eq!(std::fs::read(sidecar).unwrap(), original);
}

#[test]
fn pty_money_limit_applies_the_reviewed_ceiling() {
    let sandbox = PtySandbox::new();
    initialize_money(&sandbox);
    let (exit, raw) = sandbox.run(concat!(
        "KEY:8,KEY:\t,SLEEP:0.5,KEY:\r,SLEEP:0.2,",
        "KEY:\t,KEY:\t,KEY:2,KEY:\x13,SLEEP:0.5,",
        "KEY:\r,SLEEP:0.5,KEY:q,SLEEP:0.2,KEY:q"
    ));
    assert_eq!(exit, 0);
    assert!(screen_contains(&raw, "Review change"));
    let binary = cargo_bin("retold");
    let mut command = Command::new(binary);
    command.args(["--json", "account", "show"]);
    command.env("HOME", sandbox.sandbox_dir.path().join("home"));
    command.env_remove("XDG_DATA_HOME");
    let output = command.output().unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("2000000"), "{text}");
}

const AUTHOR_CONFIGURATION: &str = concat!(
    "KEY:9,SLEEP:0.4,KEY:n,SLEEP:0.2,KEY:recap,",
    "KEY:\t,KEY:\t,KEY:\t,KEY:\t,KEY: ,",
    "KEY:\x13,KEY:\x13,KEY:\x13,KEY:\x13,SLEEP:0.2,KEY:\r,SLEEP:0.5,",
    "KEY:q,SLEEP:0.4,KEY::,KEY:guides check,KEY:\r,SLEEP:0.4,KEY:q,",
    "KEY:\t,KEY:n,SLEEP:0.2,KEY:studio,KEY:\t,KEY:\t,KEY:\t,KEY:en,KEY:\x13,",
    "KEY:local-voice,KEY:\t,KEY:narrator,KEY:\t,KEY:speaker,KEY:\x13,",
    "KEY:#123456,KEY:\t,KEY:Noto Sans,KEY:\t,KEY:Regular,",
    "KEY:\x13,KEY:\x13,SLEEP:0.2,KEY:\r,SLEEP:0.5,KEY:q,SLEEP:0.4,",
    "KEY::,KEY:channels check,KEY:\r,SLEEP:0.4,KEY:\x03"
);

#[test]
fn pty_creates_and_validates_a_bundled_guide_and_explicit_channel() {
    let sandbox = PtySandbox::new();
    let (exit, raw) = sandbox.run(AUTHOR_CONFIGURATION);
    assert_eq!(exit, 0, "{}", String::from_utf8_lossy(&raw));
    assert!(contains_subslice(&raw, LEAVE_ALT));
    assert!(screen_contains(
        &raw,
        "recap: schema and explicit configuration checks passed"
    ));
    assert!(screen_contains(
        &raw,
        "studio: schema and explicit configuration checks passed"
    ));
    let guide = sandbox.config_path().join("guides/recap");
    assert!(guide.join("examples/the-shining.md").is_file());
    let channel = sandbox.config_path().join("channels/studio/channel.toml");
    let saved = std::fs::read_to_string(channel).unwrap();
    assert!(saved.contains("local-voice"));
    assert!(!saved.contains("CONFIGURE"));
    assert!(!sandbox.sandbox_dir.path().join("provider-invoked").exists());
}

#[test]
fn pty_studio_lifecycle_starts_and_stops_through_reviewed_controls() {
    let sandbox = PtySandbox::new();
    let state = sandbox.sandbox_dir.path().join("state");
    let (started, start_screen) =
        sandbox.run("KEY:5,KEY:\t,SLEEP:0.4,KEY:s,KEY:\x13,SLEEP:0.2,KEY:\r,SLEEP:1,KEY:\x03");
    let running = retold_app::studio::query_status(&state).running;
    let (stopped, stop_screen) =
        sandbox.run("KEY:5,KEY:\t,SLEEP:0.4,KEY:x,KEY:\r,SLEEP:0.7,KEY:\x03");
    let after = retold_app::studio::query_status(&state).running;
    retold_app::studio::stop_studio(&state).ok();
    assert_eq!(started, 0, "{}", String::from_utf8_lossy(&start_screen));
    assert!(running);
    assert_eq!(stopped, 0, "{}", String::from_utf8_lossy(&stop_screen));
    assert!(!after);
    assert!(contains_subslice(&stop_screen, LEAVE_ALT));
}

fn saved_review_image(sandbox: &PtySandbox) -> (PathBuf, PathBuf) {
    setup_video(&sandbox.config_path(), &sandbox.library_path());
    let root = saved_video_root(&sandbox.library_path());
    let folder = root.join("work/04-images");
    std::fs::create_dir_all(&folder).unwrap();
    let image = folder.join("frame.png");
    image::RgbImage::new(1, 1).save(&image).unwrap();
    (root, image)
}
fn saved_video_root(library: &Path) -> PathBuf {
    let files = file_hashes(library);
    let manifest = files
        .keys()
        .find(|path| path.file_name().is_some_and(|name| name == "run.json"))
        .unwrap();
    let metadata = manifest.parent().unwrap();
    metadata.parent().unwrap().to_path_buf()
}

#[test]
fn pty_saved_artifact_review_records_note_approval_and_unlock_without_changing_media() {
    let sandbox = PtySandbox::new();
    let (root, image) = saved_review_image(&sandbox);
    let before = std::fs::read(&image).unwrap();
    let (exit, raw) = sandbox.run(concat!(
        "KEY:3,SLEEP:0.4,KEY:\r,SLEEP:0.4,KEY:\t,KEY:\t,SLEEP:0.5,",
        "KEY:n,KEY:Check framing,KEY:\x13,SLEEP:0.2,KEY:\r,SLEEP:0.4,KEY:q,",
        "KEY:a,KEY:\x13,SLEEP:0.2,KEY:\r,SLEEP:0.4,KEY:q,",
        "KEY:u,KEY:\x13,SLEEP:0.2,KEY:\r,SLEEP:0.4,KEY:q,KEY:\x03"
    ));
    assert_eq!(exit, 0, "{}", String::from_utf8_lossy(&raw));
    let saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("meta/reviews.json")).unwrap()).unwrap();
    let item = "work/04-images/frame.png";
    assert_eq!(saved["approvals"][item]["locked"], false);
    assert_saved_review_note(&sandbox, &root, item);
    assert_eq!(std::fs::read(image).unwrap(), before);
}

fn assert_saved_review_note(sandbox: &PtySandbox, root: &Path, item: &str) {
    let library = sandbox.library_path();
    let relative = root.strip_prefix(&library).unwrap();
    let reference = relative.to_str().unwrap();
    let query = retold_app::notes::Query {
        target: Some(item.into()),
        offset: 0,
    };
    let comments = retold_app::notes::document(&library, reference, &query).unwrap();
    assert_eq!(comments.total, 1);
    assert_eq!(comments.notes[0].text, "Check framing");
}

#[test]
fn pty_language_filter_explains_empty_results_and_preserves_saved_media() {
    let sandbox = PtySandbox::new();
    saved_review_image(&sandbox);
    let before = file_hashes(&sandbox.library_path());
    let (exit, raw) = sandbox.run(concat!(
        "KEY:3,SLEEP:0.4,KEY:\r,SLEEP:0.4,KEY:\t,KEY:\t,SLEEP:0.5,",
        "KEY:L,SLEEP:0.5,KEY:L,SLEEP:0.5,KEY:\x03"
    ));
    assert_eq!(exit, 0, "{}", String::from_utf8_lossy(&raw));
    assert!(screen_contains(&raw, "language en"));
    assert!(screen_contains(&raw, "No artifacts match"));
    assert_eq!(file_hashes(&sandbox.library_path()), before);
}

impl PtySandbox {
    fn cli(&self, args: &[&str]) -> serde_json::Value {
        let mut command = Command::new(cargo_bin("retold"));
        command
            .args(["--json", "--library"])
            .arg(self.library_path())
            .args(args);
        command.env("HOME", self.sandbox_dir.path().join("home"));
        command.env("RETOLD_CONFIG_DIR", self.config_path());
        command.env("RETOLD_STATE", self.sandbox_dir.path().join("state"));
        command
            .env_remove("RETOLD_WORKSPACE")
            .env_remove("XDG_DATA_HOME");
        let output = command.output().unwrap();
        let result = serde_json::from_slice(&output.stdout).unwrap();
        assert!(output.status.success(), "{result}");
        result
    }
    fn research_draft(&self) -> PathBuf {
        self.cli(&["account", "init", "--timezone", "UTC"]);
        self.cli(&["guides", "create", "explain", "--language", "en"]);
        self.cli(&["channels", "create", "science", "--language", "en"]);
        let source = self.sandbox_dir.path().join("source.txt");
        std::fs::write(&source, "Clouds consist of suspended water droplets.").unwrap();
        self.cli(&[
            "new",
            "explain",
            "--channel",
            "science",
            "--topic",
            "Clouds",
            "--source-file",
            source.to_str().unwrap(),
        ]);
        saved_video_root(&self.library_path())
    }
}

#[test]
fn pty_background_research_executes_and_attaches_to_saved_events() {
    let sandbox = PtySandbox::new();
    let root = sandbox.research_draft();
    sandbox.cli(&["studio", "start"]);
    let (exit, raw) = sandbox.run(concat!(
        "KEY:3,SLEEP:0.5,KEY:\r,SLEEP:0.5,KEY:c,SLEEP:0.2,",
        "KEY:\t,KEY:\x15,KEY:research,KEY:\x13,KEY:\x13,",
        "KEY: ,KEY:\x13,KEY:\x13,SLEEP:0.7,KEY:\r,SLEEP:2,KEY:q,KEY:\x1b,KEY:\x1b,",
        "KEY:5,SLEEP:0.5,KEY:\r,SLEEP:0.5,KEY:\x03"
    ));
    retold_app::studio::stop_studio(&sandbox.sandbox_dir.path().join("state")).ok();
    assert_eq!(exit, 0, "{}", String::from_utf8_lossy(&raw));
    assert!(
        root.join("work/01-research/source.json").is_file(),
        "{}",
        String::from_utf8_lossy(&raw)
    );
    assert!(screen_contains(&raw, "Studio events"));
}

#[test]
fn pty_restore_reviews_a_backup_and_leaves_the_new_workspace_inactive() {
    let sandbox = PtySandbox::new();
    let archive = backed_up_workspace(&sandbox);
    let output = sandbox.sandbox_dir.path().join("restored");
    let keys = format!(
        "KEY:0,KEY:\t,KEY:\t,SLEEP:0.5,KEY:r,SLEEP:0.2,KEY:{},KEY:\t,KEY:{},KEY:\x13,KEY:\x13,KEY:\x13,SLEEP:0.8,KEY:\r,SLEEP:1,KEY:\x03",
        archive.display(),
        output.display()
    );
    let (exit, raw) = sandbox.run(&keys);
    assert_eq!(exit, 0, "{}", String::from_utf8_lossy(&raw));
    assert!(
        screen_contains(&raw, "Review change"),
        "{}",
        String::from_utf8_lossy(&raw)
    );
    assert!(
        output
            .join(retold_engine::recovery::INACTIVE_MARKER)
            .exists(),
        "{}",
        String::from_utf8_lossy(&raw)
    );
    activate_restored_from_tui(&sandbox, &output);
}

fn backed_up_workspace(sandbox: &PtySandbox) -> PathBuf {
    sandbox.cli(&["account", "init", "--timezone", "UTC"]);
    sandbox.cli(&[
        "workspace",
        "init",
        sandbox.library_path().to_str().unwrap(),
    ]);
    let archive = sandbox.sandbox_dir.path().join("backup");
    sandbox.cli(&["storage", "backup", "--output", archive.to_str().unwrap()]);
    archive
}

fn activate_restored_from_tui(sandbox: &PtySandbox, root: &Path) {
    let keys = format!(
        "KEY:0,KEY:\t,KEY:\t,SLEEP:0.5,KEY:a,SLEEP:0.2,KEY:{},KEY:\x13,KEY:\x13,KEY:\x13,KEY:\x13,SLEEP:0.8,KEY:\r,SLEEP:1,KEY:\x03",
        root.display()
    );
    let (exit, raw) = sandbox.run(&keys);
    assert_eq!(exit, 0, "{}", String::from_utf8_lossy(&raw));
    assert!(
        !root.join(retold_engine::recovery::INACTIVE_MARKER).exists(),
        "{}",
        String::from_utf8_lossy(&raw)
    );
    assert!(
        sandbox
            .library_path()
            .join(retold_engine::recovery::INACTIVE_MARKER)
            .exists()
    );
}

#[test]
fn pty_new_video_reviews_providers_then_saves_a_draft() {
    let sandbox = PtySandbox::new();
    sandbox.cli(&["account", "init", "--timezone", "UTC"]);
    sandbox.cli(&["guides", "create", "explain", "--language", "en"]);
    sandbox.cli(&["channels", "create", "science", "--language", "en"]);
    let (exit, raw) = sandbox.run(concat!(
        "KEY:3,SLEEP:0.6,KEY:n,SLEEP:0.6,KEY:explain,KEY:\t,KEY:science,KEY:\x13,SLEEP:0.3,",
        "KEY:Rain,KEY:\x13,SLEEP:0.3,KEY:\x13,SLEEP:0.3,KEY:\x13,SLEEP:1.2,",
        "KEY:\r,SLEEP:1.5,KEY:\x03"
    ));
    let screen = String::from_utf8_lossy(&raw);
    assert_eq!(exit, 0, "{screen}");
    assert!(screen_contains(&raw, "Review new draft"), "{screen}");
    assert!(screen_contains(&raw, "Providers used"), "{screen}");
    assert!(screen_contains(&raw, "not routed (image)"), "{screen}");
    let draft = sandbox.library_path().join("science/rain/.retold/run.json");
    assert!(draft.is_file(), "{screen}");
}

#[test]
fn pty_music_import_is_confirmed_then_listed_with_mood_coverage() {
    let sandbox = PtySandbox::new();
    sandbox.cli(&["account", "init", "--timezone", "UTC"]);
    let track = sandbox.sandbox_dir.path().join("night.wav");
    std::fs::write(&track, include_bytes!("fixtures/tui/silence.wav")).unwrap();
    let keys = format!(
        "KEY:6,SLEEP:0.5,KEY:\t,SLEEP:0.5,KEY:n,SLEEP:0.5,KEY:{},KEY:\t,KEY:Night,KEY:\t,KEY:CC0,\
         KEY:\t,KEY:\t,KEY:\t,KEY:tense,KEY:\x13,SLEEP:0.6,KEY:\r,SLEEP:1.2,KEY:\x1b,SLEEP:1,KEY:\x03",
        track.display()
    );
    let (exit, raw) = sandbox.run(&keys);
    let screen = String::from_utf8_lossy(&raw);
    assert_eq!(exit, 0, "{screen}");
    let listed = sandbox.cli(&["library", "music", "list"]);
    assert!(listed.to_string().contains("Night"), "{listed}");
    assert!(screen_contains(&raw, "Mood coverage"), "{screen}");
    assert!(screen_contains(&raw, "✓ tense"), "{screen}");
}

#[test]
fn pty_series_matrix_adds_a_part_through_the_reviewed_update() {
    let sandbox = PtySandbox::new();
    sandbox.create_two_part_series();
    let (exit, raw) = sandbox.run(concat!(
        "KEY:4,SLEEP:0.6,KEY:\r,SLEEP:0.8,KEY:a,SLEEP:0.5,KEY:\t,KEY:\t,KEY:\t,KEY:1,KEY:\x13,SLEEP:0.6,",
        "KEY:\r,SLEEP:1.2,KEY:\x03"
    ));
    let screen = String::from_utf8_lossy(&raw);
    assert_eq!(exit, 0, "{screen}");
    assert!(screen_contains(&raw, "0/2 parts complete"), "{screen}");
    assert!(
        screen_contains(&raw, "No video linked"),
        "{}",
        final_screen(&raw)
    );
    let shown = sandbox.cli(&["series", "show", "saga"]);
    let parts = shown["payload"]["data"]["series"]["parts"]
        .as_array()
        .unwrap();
    assert_eq!(parts.len(), 3, "{shown}");
}

impl PtySandbox {
    fn create_two_part_series(&self) {
        self.cli(&["account", "init", "--timezone", "UTC"]);
        self.cli(&["guides", "create", "explain", "--language", "en"]);
        self.cli(&["channels", "create", "science", "--language", "en"]);
        self.cli(&[
            "series",
            "new",
            "explain",
            "--work",
            "Saga",
            "--channel",
            "science",
            "--parts",
            "2",
        ]);
    }

    /// A local fake provider that marks `started` and then holds its request.
    fn install_slow_provider(&self, started: &Path) {
        let binary = self.sandbox_dir.path().join("fake-provider");
        std::fs::write(&binary, slow_provider_script(started)).unwrap();
        let mode = std::os::unix::fs::PermissionsExt::from_mode(EXECUTABLE_MODE);
        std::fs::set_permissions(&binary, mode).unwrap();
        let providers = providers_for(&binary);
        std::fs::write(self.config_path().join("providers.toml"), providers).unwrap();
    }

    /// A script job held by the slow provider, so it can be controlled while it runs.
    fn running_job(&self) -> String {
        let root = self.research_draft();
        let started = self.sandbox_dir.path().join("provider.started");
        self.install_slow_provider(&started);
        self.cli(&["studio", "start"]);
        let reference = format!("science/{}", root.file_name().unwrap().to_str().unwrap());
        let job = self.cli(&[
            "video",
            "continue",
            &reference,
            "--until",
            "script",
            "--yes",
            "--background",
        ]);
        wait_for(|| started.exists());
        job["payload"]["data"]["job"]["id"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    fn job_status(&self, id: &str) -> String {
        let shown = self.cli(&["queue", "show", id]);
        shown["payload"]["data"]["job"]["status"]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    }
}

fn wait_for(ready: impl Fn() -> bool) {
    let started = std::time::Instant::now();
    while !ready() {
        assert!(
            started.elapsed() < std::time::Duration::from_secs(20),
            "timed out"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

fn slow_provider_script(started: &Path) -> String {
    format!("#!/bin/sh\n: > '{}'\n/bin/sleep 30\n", started.display())
}

fn providers_for(binary: &Path) -> String {
    let encoded = serde_json::to_string(binary).unwrap();
    include_str!("fixtures/studio/providers.toml").replace("__BINARY__", &encoded)
}

fn queue_control(key: &str) -> String {
    format!("KEY:5,SLEEP:0.8,KEY:{key},SLEEP:0.5,KEY:\x13,SLEEP:0.5,KEY:\r,SLEEP:1,KEY:\x03")
}

#[test]
fn pty_queue_pauses_a_running_job_then_unpauses_it_through_reviewed_controls() {
    let sandbox = PtySandbox::new();
    let id = sandbox.running_job();
    let (exit, raw) = sandbox.run(&queue_control("p"));
    assert_eq!(exit, 0, "{}", String::from_utf8_lossy(&raw));
    wait_for(|| sandbox.job_status(&id) == "paused");
    let (exit, raw) = sandbox.run(&queue_control("r"));
    retold_app::studio::stop_studio(&sandbox.sandbox_dir.path().join("state")).ok();
    assert_eq!(exit, 0, "{}", String::from_utf8_lossy(&raw));
    assert_ne!(sandbox.job_status(&id), "paused");
}

#[test]
fn pty_export_reviews_its_plan_and_refuses_to_apply_a_blocked_one() {
    let sandbox = PtySandbox::new();
    let root = sandbox.research_draft();
    let before = file_hashes(&root);
    let (exit, raw) = sandbox.run(concat!(
        "KEY:3,SLEEP:0.6,KEY:\r,SLEEP:0.6,KEY:e,SLEEP:0.6,KEY:\t,KEY:en,KEY:\t,KEY:9x16,",
        "KEY:\x13,KEY:\x13,KEY:\x13,KEY:\x13,SLEEP:1.5,KEY:\r,SLEEP:1,KEY:\x03"
    ));
    let screen = String::from_utf8_lossy(&raw);
    assert_eq!(exit, 0, "{screen}");
    assert!(screen_contains(&raw, "Review production plan"), "{screen}");
    assert!(
        screen_contains(&raw, "Blocked · correct the issue"),
        "{screen}"
    );
    assert!(
        !screen_contains(&raw, "Review video plan first"),
        "{screen}"
    );
    assert_eq!(file_hashes(&root), before);
}

impl PtySandbox {
    /// A stand-in for a host program on the TUI's `PATH` that records its arguments and input.
    fn shim(&self, name: &str) -> PathBuf {
        let (program, record) = self.shim_paths(name);
        install_shim(&program, &record);
        record
    }

    /// Where a stand-in program lives and where it records what it received.
    fn shim_paths(&self, name: &str) -> (PathBuf, PathBuf) {
        let sandbox = self.sandbox_dir.path();
        let record = sandbox.join(format!("{name}.record"));
        (sandbox.join("programs").join(name), record)
    }
}

fn install_shim(program: &Path, record: &Path) {
    std::fs::create_dir_all(program.parent().unwrap()).unwrap();
    std::fs::write(program, recording_script(record)).unwrap();
    let mode = std::os::unix::fs::PermissionsExt::from_mode(EXECUTABLE_MODE);
    std::fs::set_permissions(program, mode).unwrap();
    warm(program, record);
}

/// Launch a new stand-in once so the operating system's first-run scan happens before the
/// timed interaction, then remove what that warm-up recorded.
fn warm(program: &Path, record: &Path) {
    let status = std::process::Command::new(program)
        .stdin(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
    std::fs::remove_file(record).unwrap();
}

/// Publishes complete arguments and piped input atomically; terminal stdin is never read.
fn recording_script(record: &Path) -> String {
    format!(
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{0}.pending'\n[ -t 0 ] || /bin/cat >> '{0}.pending'\n/bin/mv '{0}.pending' '{0}'\n",
        record.display()
    )
}

fn read_record(record: &Path) -> String {
    wait_for(|| record.exists());
    std::fs::read_to_string(record).unwrap()
}

const CLIPBOARD: &str = if cfg!(target_os = "macos") {
    "pbcopy"
} else {
    "wl-copy"
};
const PLAYER: &str = if cfg!(target_os = "macos") {
    "afplay"
} else {
    "ffplay"
};

#[test]
fn pty_clipboard_copy_sends_the_shown_command_to_the_host_program() {
    let sandbox = PtySandbox::new();
    let record = sandbox.shim(CLIPBOARD);
    let actions = format!(
        "KEY:3,SLEEP:0.6,KEY:y,WAIT_FILE:{},WAIT:Command copied to clipboard.,KEY:\x03",
        record.display()
    );
    let (exit, raw) = sandbox.run(&actions);
    assert_eq!(exit, 0, "{}", String::from_utf8_lossy(&raw));
    assert!(read_record(&record).contains("retold video list"));
}

#[test]
fn pty_bracketed_paste_stays_in_the_form_without_writing() {
    let sandbox = PtySandbox::new();
    let before = file_hashes(&sandbox.library_path());
    let actions =
        "KEY:7,KEY:n,KEY:\x1b[200~paste-canary\r\ny\x13\x1b[201~,WAIT:paste-canary,KEY:\x03";
    let (exit, raw) = sandbox.run(actions);
    assert_eq!(exit, 0, "{}", String::from_utf8_lossy(&raw));
    assert!(contains_subslice(&raw, b"\x1b[?2004h"));
    assert!(contains_subslice(&raw, b"\x1b[?2004l"));
    assert!(
        screen_contains(&raw, "paste-canary y"),
        "{}",
        String::from_utf8_lossy(&raw)
    );
    assert_eq!(file_hashes(&sandbox.library_path()), before);
}

fn resize_cycle() -> String {
    let commands: Vec<_> = RESIZE_WINDOWS
        .iter()
        .map(|(width, height)| format!("RESIZE:{width}x{height},SLEEP:0.08"))
        .collect();
    commands.join(",")
}

#[test]
fn pty_tiny_resizes_preserve_the_open_form_and_palette() {
    let sandbox = PtySandbox::new();
    let before = file_hashes(&sandbox.library_path());
    let cycle = resize_cycle();
    let actions = format!(
        concat!(
            "KEY:7,KEY:n,KEY:\x1b[200~resize-draft\x1b[201~,WAIT:resize-draft,",
            "{cycle},WAIT:resize-draft,KEY:\x0b,WAIT:Commands,",
            "{cycle},WAIT:Commands,KEY:\x1b,RESIZE:109x30,SLEEP:0.08,",
            "RESIZE:110x30,WAIT:resize-draft,KEY:\x03"
        ),
        cycle = cycle
    );
    let (exit, raw) = sandbox.run(&actions);
    assert_eq!(exit, 0, "{}", String::from_utf8_lossy(&raw));
    assert!(screen_contains(&raw, "resize-draft"));
    assert_eq!(file_hashes(&sandbox.library_path()), before);
}

#[test]
fn pty_tiny_resizes_keep_the_review_cancel_safe() {
    let sandbox = PtySandbox::new();
    sandbox.route_fixture();
    let before = file_hashes(&sandbox.library_path());
    let cycle = resize_cycle();
    let actions = format!(
        concat!(
            "KEY:7,WAIT:first,KEY:\x0b,KEY:providers remove,KEY:\r,KEY:\x13,WAIT:Run,",
            "{cycle},WAIT:Run,KEY:\x1b,KEY:\x03"
        ),
        cycle = cycle
    );
    let (exit, raw) = sandbox.run(&actions);
    assert_eq!(exit, 0, "{}", String::from_utf8_lossy(&raw));
    assert!(screen_contains(&raw, "Cancel"));
    assert_eq!(file_hashes(&sandbox.library_path()), before);
}

fn assert_external_signal_restores_terminal(signal: &str) {
    let sandbox = PtySandbox::new();
    let before = file_hashes(&sandbox.library_path());
    let actions = format!("KEY:7,WAIT:Health,ASSERT_RAW,SIGNAL:{signal}");
    let (exit, raw) = sandbox.run(&actions);
    assert_eq!(exit, 0, "{}", String::from_utf8_lossy(&raw));
    assert!(contains_subslice(&raw, LEAVE_ALT));
    assert!(contains_subslice(&raw, b"\x1b[?2004l"));
    assert_eq!(file_hashes(&sandbox.library_path()), before);
}

#[test]
fn pty_external_sigint_restores_terminal_modes() {
    assert_external_signal_restores_terminal("SIGINT");
}

#[test]
fn pty_external_sigterm_restores_terminal_modes() {
    assert_external_signal_restores_terminal("SIGTERM");
}

#[test]
fn pty_external_sighup_restores_terminal_modes() {
    assert_external_signal_restores_terminal("SIGHUP");
}

#[test]
fn pty_stalled_clipboard_keeps_navigation_responsive_and_times_out() {
    let sandbox = PtySandbox::new();
    let record = sandbox.shim(CLIPBOARD);
    let (program, _) = sandbox.shim_paths(CLIPBOARD);
    let script = format!(
        "#!/bin/sh\nprintf started > '{}'\nprintf CLIPBOARD-STDOUT\nprintf CLIPBOARD-STDERR >&2\nexec /bin/sleep 8\n",
        record.display()
    );
    std::fs::write(program, script).unwrap();
    let actions = format!(
        "KEY:3,KEY:y,WAIT_FILE:{},KEY:7,WAIT:Health,KEY:\x03,KEY:\r",
        record.display()
    );
    let (exit, raw) = sandbox.run(&actions);
    let transcript = String::from_utf8_lossy(&raw);
    assert_eq!(exit, 0, "{transcript}");
    assert!(!transcript.contains("CLIPBOARD-STDOUT"));
    assert!(!transcript.contains("CLIPBOARD-STDERR"));
    assert!(screen_contains(&raw, "Clipboard timed out"), "{transcript}");
}

#[test]
fn pty_sigterm_drains_and_reaps_a_pending_clipboard() {
    let sandbox = PtySandbox::new();
    let record = sandbox.shim(CLIPBOARD);
    let (program, _) = sandbox.shim_paths(CLIPBOARD);
    let script = format!(
        "#!/bin/sh\nprintf '%s' \"$$\" > '{}'\nexec /bin/sleep 8\n",
        record.display()
    );
    std::fs::write(program, script).unwrap();
    let actions = format!("KEY:3,KEY:y,WAIT_FILE:{},SIGNAL:SIGTERM", record.display());
    let (exit, raw) = sandbox.run(&actions);
    assert_eq!(exit, 0, "{}", String::from_utf8_lossy(&raw));
    assert!(screen_contains(&raw, "Clipboard timed out"));
    let mut check = Command::new("/bin/kill");
    check.args(["-0", read_record(&record).trim()]);
    check
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    assert!(
        !check.status().unwrap().success(),
        "clipboard child survived shutdown"
    );
}

#[test]
fn pty_workspace_switch_clears_old_drafts_and_reads_the_selected_root() {
    let sandbox = PtySandbox::new();
    let destination = switch_destination(&sandbox);
    let before = file_hashes(&destination);
    let providers = std::fs::read(sandbox.library_path().join("providers.toml")).unwrap();
    let actions = format!(
        "KEY:7,KEY:n,KEY:old-draft-canary,KEY:\x0b,KEY:Go to System,KEY:\r,KEY:\x0b,KEY:workspace switch,KEY:\r,KEY:{},KEY:\x13,WAIT:Run,KEY:\r,WAIT:SecondWorkspace,KEY:7,SLEEP:0.3,KEY:\x03",
        destination.display()
    );
    let (exit, raw) = sandbox.run(&actions);
    let shown = last_tui_screen(&raw);
    assert_eq!(exit, 0, "{}", String::from_utf8_lossy(&raw));
    assert!(!shown.contains("old-draft-canary"), "{shown}");
    assert!(shown.contains("Health"), "{shown}");
    assert!(shown.contains("destination-only"), "{shown}");
    assert!(shown.contains("SecondWorkspace"), "{shown}");
    assert_eq!(file_hashes(&destination), before);
    assert_eq!(
        std::fs::read(sandbox.library_path().join("providers.toml")).unwrap(),
        providers
    );
}

fn switch_destination(sandbox: &PtySandbox) -> PathBuf {
    let destination = sandbox.sandbox_dir.path().join("second-workspace");
    sandbox.cli(&[
        "workspace",
        "init",
        destination.to_str().unwrap(),
        "--name",
        "SecondWorkspace",
    ]);
    let context = retold_app::workspace::context::Context::resolve(Some(&destination)).unwrap();
    retold_app::setup::ensure_config(&context).unwrap();
    std::fs::write(context.config.join("providers.toml"), "schema_version = 1\n[providers.destination-only]\nkind = 'codex_cli'\nmax_parallel = 1\n[routing]\n").unwrap();
    destination
}

fn last_tui_screen(raw: &[u8]) -> String {
    let end = raw
        .windows(LEAVE_ALT.len())
        .rposition(|window| window == LEAVE_ALT);
    final_screen(&raw[..end.unwrap_or(raw.len())])
}

#[test]
fn pty_workspace_switch_loads_new_defaults_without_reusing_an_old_form() {
    let sandbox = PtySandbox::new();
    let destination = switch_destination(&sandbox);
    write_draft_defaults(&sandbox.library_path(), "old-guide");
    write_draft_defaults(&destination.join("config"), "destination-guide");
    let before = file_hashes(&destination);
    let actions = format!(
        "KEY:3,KEY:n,WAIT:old-guide,KEY:\x0b,KEY:Go to System,KEY:\r,KEY:\x0b,KEY:workspace switch,KEY:\r,KEY:{},KEY:\x13,WAIT:Run,KEY:\r,WAIT:SecondWorkspace,KEY:7,WAIT:missing_profile,KEY:3,KEY:n,WAIT:destination-guide,KEY:\x03",
        destination.display()
    );
    let (exit, raw) = sandbox.run(&actions);
    let shown = last_tui_screen(&raw);
    assert_eq!(exit, 0, "{}", String::from_utf8_lossy(&raw));
    assert!(shown.contains("destination-guide"), "{shown}");
    assert!(!shown.contains("old-guide"), "{shown}");
    assert_eq!(file_hashes(&destination), before);
}

fn write_draft_defaults(config: &Path, guide: &str) {
    let saved = serde_json::json!({"last":{"guide":guide}, "pairs":{}});
    std::fs::write(config.join(".tui-defaults.json"), saved.to_string()).unwrap();
}

#[test]
fn pty_cancelling_workspace_switch_preserves_the_old_draft_and_both_roots() {
    let sandbox = PtySandbox::new();
    let destination = switch_destination(&sandbox);
    let original = file_hashes(&sandbox.library_path());
    let target = file_hashes(&destination);
    let actions = format!(
        "KEY:7,KEY:n,KEY:old-draft-canary,KEY:\x0b,KEY:Go to System,KEY:\r,KEY:\x0b,KEY:workspace switch,KEY:\r,KEY:{},KEY:\x13,WAIT:Run,KEY:\x1b,WAIT:Path,KEY:\x0b,KEY:Go to Providers,KEY:\r,WAIT:old-draft-canary,KEY:\x03",
        destination.display()
    );
    let (exit, raw) = sandbox.run(&actions);
    assert_eq!(exit, 0, "{}", String::from_utf8_lossy(&raw));
    assert!(last_tui_screen(&raw).contains("old-draft-canary"));
    assert_eq!(file_hashes(&sandbox.library_path()), original);
    assert_eq!(file_hashes(&destination), target);
}

#[test]
fn pty_external_default_switch_cannot_redirect_an_open_managed_session() {
    let sandbox = PtySandbox::new();
    let destination = switch_destination(&sandbox);
    let source = sandbox.sandbox_dir.path().join("source-workspace");
    sandbox.cli(&[
        "workspace",
        "init",
        source.to_str().unwrap(),
        "--name",
        "SourceWorkspace",
    ]);
    let config = source.join("config");
    source_provider(&config, "source-only");
    sandbox.cli(&["workspace", "switch", source.to_str().unwrap()]);
    assert_session_stays_on_source(&sandbox, (&config, &destination), Option::default());
}

#[test]
fn pty_external_default_switch_keeps_separate_legacy_config_and_media() {
    let sandbox = PtySandbox::new();
    let destination = switch_destination(&sandbox);
    let media = sandbox.sandbox_dir.path().join("separate-media");
    std::fs::create_dir(&media).unwrap();
    let settings = format!("library = {:?}\n", media.to_str().unwrap());
    std::fs::write(sandbox.config_path().join("settings.toml"), settings).unwrap();
    source_provider(&sandbox.config_path(), "source-only");
    let channel = init_studio_channel(&sandbox.config_path());
    create_video_run(&media, &channel);
    source_provider(&sandbox.config_path(), "source-only");
    assert_session_stays_on_source(
        &sandbox,
        (&sandbox.config_path(), &destination),
        Some("Nosferatu"),
    );
}

fn source_provider(config: &Path, id: &str) {
    let text = format!(
        "schema_version = 1\n[providers.{id}]\nkind = 'codex_cli'\nmax_parallel = 1\n[routing]\n"
    );
    std::fs::write(config.join("providers.toml"), text).unwrap();
}

fn assert_session_stays_on_source(
    sandbox: &PtySandbox,
    roots: (&Path, &Path),
    title: Option<&str>,
) {
    let (config, destination) = roots;
    let replacement = sandbox.sandbox_dir.path().join("replacement");
    std::fs::create_dir(&replacement).unwrap();
    source_provider(&replacement, "source-updated");
    let before = file_hashes(destination);
    let ending = title.map_or_else(
        || "KEY:\x03".into(),
        |title| format!("KEY:3,WAIT:{title},KEY:\x03"),
    );
    let actions = format!(
        "KEY:7,WAIT:source-only,EXTERNAL_SWITCH:{},COPY_FILE:{}|{},KEY:\x12,WAIT:source-updated,KEY:\x0b,KEY:providers remove,KEY:\r,KEY:\x13,WAIT:Run,KEY:\r,WAIT:removed,{ending}",
        destination.display(),
        replacement.join("providers.toml").display(),
        config.join("providers.toml").display()
    );
    let environment = [("RETOLD_PTY_DISCOVER_LIBRARY", Path::new("1"))];
    let (exit, raw) = sandbox.run_with(&actions, &environment);
    let shown = last_tui_screen(&raw);
    assert_eq!(exit, 0, "{}", String::from_utf8_lossy(&raw));
    assert!(!shown.contains("destination-only"), "{shown}");
    let saved = std::fs::read_to_string(config.join("providers.toml")).unwrap();
    assert!(!saved.contains("source-updated"), "{saved}");
    assert_eq!(file_hashes(destination), before);
}

#[test]
fn pty_music_preview_plays_the_saved_file_with_the_host_player() {
    let sandbox = PtySandbox::new();
    sandbox.cli(&["account", "init", "--timezone", "UTC"]);
    let track = sandbox.sandbox_dir.path().join("night.wav");
    std::fs::write(&track, include_bytes!("fixtures/tui/silence.wav")).unwrap();
    sandbox.cli(&[
        "library",
        "music",
        "import",
        track.to_str().unwrap(),
        "--title",
        "Night",
    ]);
    let record = sandbox.shim(PLAYER);
    let actions = format!(
        "KEY:6,KEY:\t,WAIT:Night,KEY: ,WAIT_FILE:{},KEY:\x03",
        record.display()
    );
    let (exit, raw) = sandbox.run(&actions);
    assert_eq!(exit, 0, "{}", String::from_utf8_lossy(&raw));
    assert!(read_record(&record).contains(".wav"));
}

#[test]
fn pty_editor_handoff_restores_the_screen_and_checks_the_edited_guide() {
    let sandbox = PtySandbox::new();
    sandbox.cli(&["account", "init", "--timezone", "UTC"]);
    sandbox.cli(&["guides", "create", "explain", "--language", "en"]);
    let record = sandbox.shim("fake-editor");
    let editor = sandbox.sandbox_dir.path().join("programs/fake-editor");
    let actions = format!(
        "KEY:9,WAIT:explain,KEY:\x1b[17~,WAIT:guide.toml,KEY:guide.toml,KEY:\x1b[17~,WAIT_FILE:{},WAIT:retold guides check explain,KEY:\x03",
        record.display()
    );
    let (exit, raw) = sandbox.run_with(&actions, &[("EDITOR", &editor), ("VISUAL", &editor)]);
    let screen = String::from_utf8_lossy(&raw);
    assert_eq!(exit, 0, "{screen}");
    assert!(read_record(&record).contains("guide.toml"), "{screen}");
    let shown = final_screen(&raw);
    assert!(
        screen_contains(&raw, "retold guides check explain"),
        "{shown}"
    );
}

#[test]
fn pty_failed_editor_restores_modes_it_changed_before_exiting() {
    let sandbox = PtySandbox::new();
    sandbox.cli(&["account", "init", "--timezone", "UTC"]);
    sandbox.cli(&["guides", "create", "explain", "--language", "en"]);
    sandbox.shim("failed-editor");
    let (editor, _) = sandbox.shim_paths("failed-editor");
    std::fs::write(&editor, "#!/bin/sh\n/bin/stty -echo -icanon\nexit 7\n").unwrap();
    let before = file_hashes(&sandbox.library_path());
    let actions = concat!(
        "KEY:9,WAIT:explain,KEY:\x1b[17~,WAIT:guide.toml,KEY:guide.toml,",
        "KEY:\x1b[17~,WAIT:unsuccessfully,ASSERT_RAW,KEY:\x03"
    );
    let (exit, raw) = sandbox.run_with(actions, &[("EDITOR", &editor), ("VISUAL", &editor)]);
    assert_eq!(exit, 0, "{}", String::from_utf8_lossy(&raw));
    assert!(screen_contains(&raw, "unsuccessfully"));
    assert_eq!(file_hashes(&sandbox.library_path()), before);
}

#[test]
fn pty_shutdown_preserves_a_stalled_editors_unsaved_work_until_it_exits() {
    for signal in ["SIGINT", "SIGTERM", "SIGHUP"] {
        assert_stalled_editor_shutdown(signal);
    }
}

fn assert_stalled_editor_shutdown(signal: &str) {
    let sandbox = PtySandbox::new();
    sandbox.cli(&["account", "init", "--timezone", "UTC"]);
    sandbox.cli(&["guides", "create", "explain", "--language", "en"]);
    sandbox.shim("stalled-editor");
    let (editor, _) = sandbox.shim_paths("stalled-editor");
    let root = sandbox.sandbox_dir.path();
    let (draft, release, source) = stalled_editor_paths(root);
    std::fs::write(&source, "release").unwrap();
    write_stalled_editor(&editor);
    let actions = stalled_editor_actions(signal, root);
    let (exit, raw) = sandbox.run_with(
        &actions,
        &[
            ("EDITOR", &editor),
            ("VISUAL", &editor),
            ("RETOLD_EDITOR_DRAFT", &draft),
            ("RETOLD_EDITOR_RELEASE", &release),
        ],
    );
    assert_eq!(exit, 0, "{signal}: {}", String::from_utf8_lossy(&raw));
    assert_editor_saved(&sandbox, &draft);
}

fn stalled_editor_paths(root: &Path) -> (PathBuf, PathBuf, PathBuf) {
    (
        root.join("unsaved.toml"),
        root.join("release"),
        root.join("release-source"),
    )
}

fn assert_editor_saved(sandbox: &PtySandbox, draft: &Path) {
    let saved = sandbox.library_path().join("guides/explain/guide.toml");
    assert_eq!(std::fs::read(saved).unwrap(), std::fs::read(draft).unwrap());
}

fn stalled_editor_actions(signal: &str, root: &Path) -> String {
    format!(
        concat!(
            "KEY:9,WAIT:explain,KEY:\x1b[17~,WAIT:guide.toml,KEY:guide.toml,",
            "KEY:\x1b[17~,WAIT_FILE:{},SIGNAL:{signal},SLEEP:0.3,COPY_FILE:{}|{}"
        ),
        root.join("unsaved.toml").display(),
        root.join("release-source").display(),
        root.join("release").display(),
        signal = signal
    )
}

fn write_stalled_editor(editor: &Path) {
    std::fs::write(
        editor,
        concat!(
            "#!/bin/sh\ntrap '' INT TERM HUP\n/bin/stty -echo -icanon\n",
            "/bin/cp \"$1\" \"$RETOLD_EDITOR_DRAFT\"\n",
            "printf '\\n# recovered unsaved draft\\n' >> \"$RETOLD_EDITOR_DRAFT\"\n",
            "while [ ! -f \"$RETOLD_EDITOR_RELEASE\" ]; do /bin/sleep 0.05; done\n",
            "/bin/cp \"$RETOLD_EDITOR_DRAFT\" \"$1\"\n"
        ),
    )
    .unwrap();
}

const OPENER: &str = if cfg!(target_os = "macos") {
    "open"
} else {
    "xdg-open"
};

#[test]
fn pty_browser_opens_the_selected_file_with_the_host_viewer() {
    let sandbox = PtySandbox::new();
    sandbox.cli(&["account", "init", "--timezone", "UTC"]);
    sandbox.cli(&["guides", "create", "explain", "--language", "en"]);
    let record = sandbox.shim(OPENER);
    let actions = format!(
        "KEY:9,WAIT:explain,KEY:\x1b[17~,WAIT:guide.toml,KEY:o,WAIT_FILE:{},KEY:\x03",
        record.display()
    );
    let (exit, raw) = sandbox.run(&actions);
    assert_eq!(exit, 0, "{}", final_screen(&raw));
    assert!(
        read_record(&record).contains("guides/explain/"),
        "{}",
        final_screen(&raw)
    );
}

#[cfg(not(target_os = "macos"))]
#[test]
fn pty_clipboard_falls_back_to_xclip_without_wayland() {
    let sandbox = PtySandbox::new();
    let record = sandbox.shim("xclip");
    let actions = format!(
        "KEY:3,SLEEP:0.6,KEY:y,WAIT_FILE:{},WAIT:Command copied to clipboard.,KEY:\x03",
        record.display()
    );
    let (exit, raw) = sandbox.run(&actions);
    assert_eq!(exit, 0, "{}", final_screen(&raw));
    assert!(read_record(&record).contains("-selection"));
}

#[test]
fn pty_supervision_inspection_and_backup_cancel_preserve_the_account() {
    let sandbox = PtySandbox::new();
    let account = initialize_money(&sandbox);
    let before = file_hashes(&account);
    let destination = sandbox.sandbox_dir.path().join("cancelled-backup");
    let actions = format!(
        "KEY:8,KEY:\t,KEY:\t,KEY:\t,KEY:\t,SLEEP:0.5,KEY:>,KEY:<,KEY:\t,SLEEP:0.3,KEY:b,KEY:{},KEY:\x13,SLEEP:0.5,KEY:\x1b,SLEEP:0.2,KEY:\x1b,SLEEP:0.2,KEY:q",
        destination.display()
    );
    let (exit, raw) = sandbox.run(&actions);
    assert_eq!(exit, 0, "{}", final_screen(&raw));
    assert!(screen_contains(&raw, "No supervised processes"));
    assert!(screen_contains(&raw, "Account recovery backup"));
    assert!(screen_contains(&raw, "Review change"));
    assert!(!destination.exists());
    assert_eq!(file_hashes(&account), before);
}

#[test]
fn pty_account_backup_is_created_after_review_and_verified_from_backup_tab() {
    let sandbox = PtySandbox::new();
    let account = initialize_money(&sandbox);
    let before = file_hashes(&account);
    let destination = sandbox.sandbox_dir.path().join("reviewed-backup");
    let actions = format!(
        "KEY:8,KEY:\t,KEY:\t,KEY:\t,KEY:\t,KEY:\t,SLEEP:0.4,KEY:b,KEY:{0},KEY:\x13,SLEEP:0.5,KEY:\r,SLEEP:0.5,KEY:q,KEY:v,KEY:{0},KEY:\x13,SLEEP:0.5,KEY:q,KEY:q",
        destination.display()
    );
    let (exit, raw) = sandbox.run(&actions);
    assert_eq!(exit, 0);
    assert!(screen_contains(&raw, "Review change"));
    assert!(screen_contains(&raw, "Backup verified"));
    assert!(destination.join("backup.json").is_file());
    assert_eq!(file_hashes(&account), before);
}
