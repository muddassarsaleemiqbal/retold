//! A real worker assertion exercises the process-wide panic hook while the TUI is active.
use std::process::Command;

use super::super::{Dispatch, Worker};
use crate::tui::{Session, Sources};

const CANARY: &str = "PRIVATE-WORKER-PANIC-CANARY";
const WORKER_FIXTURE: &str =
    "tui::runtime::tests::terminal::worker_panic_preserves_input_and_redacts_detail";
const FATAL_FIXTURE: &str =
    "tui::runtime::tests::terminal::fatal_ui_panic_restores_every_terminal_mode";
const DRIVER: &str = include_str!("../../../tests/fixtures/pty_driver.py");
const PROVIDERS: &str = "[providers.first]\nkind = 'codex_cli'\nbinary = '/no-such-provider'\nmax_parallel = 1\n[routing]\n";

fn panic_worker() -> Worker {
    let root = std::path::PathBuf::from(std::env::var_os("RETOLD_CFG").unwrap());
    let scope = crate::tui::scope::Scope::new(Some(root));
    let dispatch: Dispatch = std::sync::Arc::new(move |words: Vec<String>| {
        let failing = words.ends_with(&["providers".into(), "health".into()]);
        assert!(!failing, "{CANARY}");
        scope.request(words)
    });
    Worker::start(Vec::new(), dispatch, || {})
}

fn worker_panic_fixture() {
    let mut session = interactive_session();
    session.worker = panic_worker();
    while !session.app.quit {
        session = session.step().unwrap();
    }
    crate::tui::release_input();
    ratatui::restore();
}

fn interactive_session() -> Session {
    let terminal = ratatui::init();
    ratatui::crossterm::execute!(
        std::io::stdout(),
        ratatui::crossterm::event::EnableMouseCapture,
        ratatui::crossterm::event::EnableBracketedPaste
    )
    .unwrap();
    Session::start(terminal, &Sources::default()).unwrap()
}

fn fatal_ui_panic_fixture() {
    let _session = interactive_session();
    let raw = ratatui::crossterm::terminal::is_raw_mode_enabled().unwrap();
    assert!(!raw, "intentional UI failure probe");
}

fn isolated_command(root: &std::path::Path) -> Command {
    let mut command = Command::new("python3");
    command.arg("-c").arg(DRIVER);
    command.env("RETOLD_BIN", std::env::current_exe().unwrap());
    command.env("RETOLD_LIB", root).env("RETOLD_CFG", root);
    command.env("HOME", root.join("home"));
    command.env("RETOLD_STATE", root.join("state"));
    command.env("PYTHONDONTWRITEBYTECODE", "1");
    for variable in [
        "RETOLD_WORKSPACE",
        "XDG_DATA_HOME",
        "CODEX_HOME",
        "CLAUDE_CONFIG_DIR",
    ] {
        command.env_remove(variable);
    }
    command
}

fn fixture_command(root: &std::path::Path, fixture: &str) -> Command {
    let mut command = isolated_command(root);
    let arguments = ["--exact", fixture, "--nocapture"];
    command.env(
        "RETOLD_PTY_ARGS",
        serde_json::to_string(&arguments).unwrap(),
    );
    command.env("RETOLD_ACTIONS", "");
    command.env("RETOLD_PTY_CHILD", "1");
    command
}

#[test]
fn worker_panic_preserves_input_and_redacts_detail() {
    let child = std::env::var_os("RETOLD_PTY_CHILD").is_some();
    if child {
        worker_panic_fixture();
    } else {
        verify_worker_panic();
    }
}

fn verify_worker_panic() {
    let root = tempfile::tempdir().unwrap();
    let config = root.path().join("providers.toml");
    std::fs::write(&config, PROVIDERS).unwrap();
    let mut command = fixture_command(root.path(), WORKER_FIXTURE);
    command.env(
        "RETOLD_ACTIONS",
        "KEY:7,WAIT:unexpectedly,ASSERT_RAW,KEY:8,WAIT:Spend,KEY:\x03",
    );
    let output = command.output().unwrap();
    let raw = [output.stdout, output.stderr].concat();
    let text = String::from_utf8_lossy(&raw);
    assert!(
        !text.contains(CANARY),
        "worker panic detail leaked into the TUI: {text}"
    );
    assert!(output.status.success(), "{text}");
    assert!(text.contains("unexpectedly"));
    assert_eq!(std::fs::read_to_string(config).unwrap(), PROVIDERS);
}

#[test]
fn fatal_ui_panic_restores_every_terminal_mode() {
    let child = std::env::var_os("RETOLD_PTY_CHILD").is_some();
    if child {
        fatal_ui_panic_fixture();
    } else {
        verify_fatal_panic();
    }
}

fn verify_fatal_panic() {
    let root = tempfile::tempdir().unwrap();
    let output = fixture_command(root.path(), FATAL_FIXTURE)
        .output()
        .unwrap();
    let raw = [output.stdout, output.stderr].concat();
    let text = String::from_utf8_lossy(&raw);
    assert!(!output.status.success());
    assert!(text.contains("intentional UI failure probe"), "{text}");
    assert!(
        !text.contains("terminal attributes were not restored"),
        "{text}"
    );
    for sequence in ["\x1b[?1049l", "\x1b[?1000l", "\x1b[?2004l"] {
        assert!(
            text.contains(sequence),
            "missing cleanup {sequence:?}: {text}"
        );
    }
}
