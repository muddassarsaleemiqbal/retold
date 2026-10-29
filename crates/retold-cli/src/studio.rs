//! Studio daemon lifecycle CLI commands (`studio start`, `studio stop`, `studio status`).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use clap::{Arg, ArgMatches, Command as ClapCommand};
use retold_app::protocol::{Diagnostic, ErrorCode, Outcome, Payload};
use retold_app::studio::{
    StudioError, StudioServer, query_status, resolve_workspace_state, stop_studio,
};

const STARTUP_WAIT: Duration = Duration::from_millis(25);
const STARTUP_TIMEOUT: Duration = Duration::from_secs(3);

#[must_use]
pub fn command() -> ClapCommand {
    let foreground_arg = Arg::new("foreground")
        .long("foreground")
        .action(clap::ArgAction::SetTrue)
        .help("Run studio in the foreground rather than as a background daemon");
    let start_cmd = ClapCommand::new("start")
        .about("Start studio daemon")
        .arg(foreground_arg);
    let status_cmd =
        ClapCommand::new("status").about("Display studio daemon status and active queue depth");
    let stop_cmd = ClapCommand::new("stop").about("Stop running studio daemon gracefully");

    let mut base = ClapCommand::new("studio");
    base = base.about("Manage background studio worker daemon");
    base = base.subcommand_required(true);
    base = base.arg_required_else_help(true);
    let stop_all = scoped_command(
        "stop-all",
        "Stop this workspace's production queue and durably inhibit new dispatch",
    );
    let enable = scoped_command(
        "enable-dispatch",
        "Clear the specified queue hold; stopped jobs remain stopped",
    );
    base.subcommands([status_cmd, stop_cmd, start_cmd, stop_all, enable])
}

fn scoped_command(name: &'static str, help: &'static str) -> ClapCommand {
    ClapCommand::new(name).about(help).arg(
        Arg::new("request-id")
            .long("request-id")
            .required(true)
            .help("Unique stop request ID; reuse only to retry the same request"),
    )
}

#[must_use]
pub fn query(library: Option<&PathBuf>, matches: &ArgMatches) -> Payload {
    let state_dir = resolve_workspace_state(library.map(PathBuf::as_path));
    match matches.subcommand() {
        Some(("status", _)) => status_payload(&state_dir),
        Some(("stop", _)) => stop_payload(&state_dir),
        Some(("start", args)) => start_payload(&state_dir, library, args),
        Some(("stop-all", args)) => stop_all_payload(&state_dir, args),
        Some(("enable-dispatch", args)) => enable_dispatch_payload(&state_dir, args),
        _ => Payload::Error {
            outcome: Outcome::Failed,
            diagnostic: Diagnostic::new(
                ErrorCode::InputRequired,
                "A studio subcommand is required. See --help.",
            ),
        },
    }
}

fn stop_all_payload(state: &Path, args: &ArgMatches) -> Payload {
    let Some(id) = args.get_one::<String>("request-id") else {
        return Diagnostic::new(ErrorCode::InputRequired, "Supply --request-id.").into_payload();
    };
    match retold_app::studio::stop_workspace_queue(state, id) {
        Ok(stop) => Payload::Result {
            outcome: Outcome::Completed,
            contents: serde_json::json!({
                "text": "Workspace dispatch inhibited. Stop requests saved; active work remains unconfirmed until its outcome is recorded.",
                "stop": stop,
            }),
        },
        Err(error) => error.diagnostic().into_payload(),
    }
}

fn enable_dispatch_payload(state: &Path, args: &ArgMatches) -> Payload {
    let Some(id) = args.get_one::<String>("request-id") else {
        return Diagnostic::new(
            ErrorCode::InputRequired,
            "Supply the current hold's --request-id.",
        )
        .into_payload();
    };
    let result = retold_app::queue::QueueStore::open(&state.join("queue.sqlite"))
        .and_then(|queue| queue.resume_dispatch(id));
    match result {
        Ok(dispatch) => Payload::Result {
            outcome: Outcome::Completed,
            contents: serde_json::json!({
                "text": "Workspace dispatch enabled. Stopped jobs remain stopped.", "dispatch": dispatch,
            }),
        },
        Err(error) => error.diagnostic().into_payload(),
    }
}

fn start_payload(state: &Path, library: Option<&PathBuf>, args: &ArgMatches) -> Payload {
    if let Err(error) = retold_app::studio::validate_startup(library.map(PathBuf::as_path), state) {
        return error.into_payload();
    }
    let status = query_status(state);
    let issue = status.issues.first();
    if let Some(issue) = issue {
        issue.clone().into_payload()
    } else if status.running {
        already_running_payload(status.pid)
    } else if args.get_flag("foreground") {
        run_foreground(state, library.map(PathBuf::as_path))
    } else {
        spawn_background(state, library.map(PathBuf::as_path))
    }
}

fn already_running_payload(pid: Option<u32>) -> Payload {
    Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": "Studio is already running.",
            "running": true,
            "pid": pid,
        }),
    }
}

fn status_payload(state_dir: &Path) -> Payload {
    let status = query_status(state_dir);
    let issue = status.issues.first();
    if let Some(issue) = issue {
        return issue.clone().into_payload();
    }
    if status.running {
        return running_status(&status);
    }
    Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": "Studio is not running.", "running": false,
            "active_jobs": 0, "queue_depth": status.queue_depth,
        }),
    }
}

fn running_status(status: &retold_app::studio::StudioStatus) -> Payload {
    let pid = status.pid.unwrap_or_default();
    let active = status.active_jobs;
    let depth = status.queue_depth;
    Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": format!("Studio is running (PID {pid}, {active} active jobs, {depth} pending in total)."),
            "running": true, "pid": status.pid, "active_jobs": active,
            "queue_depth": depth, "uptime_seconds": status.uptime_seconds,
        }),
    }
}

fn stop_payload(state_dir: &Path) -> Payload {
    match stop_studio(state_dir) {
        Ok(true) => Payload::Result {
            outcome: Outcome::Completed,
            contents: serde_json::json!({
                "text": "Studio stopped.",
                "stopped": true,
            }),
        },
        Ok(false) => Payload::Result {
            outcome: Outcome::Completed,
            contents: serde_json::json!({
                "text": "Studio is not running.",
                "stopped": false,
            }),
        },
        Err(err) => execution_error(&err.to_string()),
    }
}

fn run_foreground(state_dir: &Path, library: Option<&Path>) -> Payload {
    match bind_admitted(state_dir, library) {
        Ok(server) => server_run_payload(&server),
        Err(StudioError::AlreadyRunning) => Payload::Error {
            outcome: Outcome::Blocked,
            diagnostic: Diagnostic::new(
                ErrorCode::AccountBusy,
                "Another studio process already holds the singleton lock.",
            ),
        },
        Err(err) => err.diagnostic().into_payload(),
    }
}

fn bind_admitted(state: &Path, library: Option<&Path>) -> Result<StudioServer, StudioError> {
    let _startup = retold_app::studio::hold_startup(library, state)?;
    StudioServer::bind(state)
}

fn server_run_payload(server: &StudioServer) -> Payload {
    let interrupt = match crate::interrupt::Interrupt::new() {
        Ok(interrupt) => interrupt,
        Err(error) => return execution_error(&error.to_string()),
    };
    match server.run_until(interrupt.flag()) {
        Ok(()) => Payload::Result {
            outcome: Outcome::Completed,
            contents: serde_json::json!({"text": "Studio stopped cleanly.", "stopped": true}),
        },
        Err(err) => execution_error(&err.to_string()),
    }
}

fn spawn_background(state_dir: &Path, library: Option<&Path>) -> Payload {
    let _startup = match retold_app::studio::hold_startup(library, state_dir) {
        Ok(activity) => activity,
        Err(error) => return error.into_payload(),
    };
    let current_exe = match resolve_executable() {
        Ok(exe) => exe,
        Err(err) => return execution_error(&err.to_string()),
    };
    let mut cmd = build_background_cmd(&current_exe, state_dir, library);
    match cmd.spawn() {
        Ok(child) => wait_for_start(child, state_dir),
        Err(err) => execution_error(&err.to_string()),
    }
}

fn resolve_executable() -> Result<PathBuf, std::io::Error> {
    if let Ok(exe) = std::env::var("CARGO_BIN_EXE_retold") {
        return Ok(PathBuf::from(exe));
    }
    let current = std::env::current_exe()?;
    if let Some(candidate) = test_sibling_binary(&current) {
        return Ok(candidate);
    }
    Ok(current)
}

fn test_sibling_binary(current: &Path) -> Option<PathBuf> {
    let is_test_runner = current.to_string_lossy().contains("/deps/");
    if !is_test_runner {
        return None;
    }
    let parent = current.parent()?.parent()?;
    let candidate = parent.join("retold");
    candidate.exists().then_some(candidate)
}

fn build_background_cmd(exe: &Path, state_dir: &Path, library: Option<&Path>) -> Command {
    let mut cmd = Command::new(exe);
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::null());
    cmd.env("RETOLD_STATE", state_dir);
    if let Some(library) = library {
        cmd.arg("--library").arg(library);
    }
    cmd.arg("studio");
    cmd.arg("start");
    cmd.arg("--foreground");
    cmd
}

fn wait_for_start(mut child: std::process::Child, state: &Path) -> Payload {
    let started = std::time::Instant::now();
    while started.elapsed() < STARTUP_TIMEOUT {
        let status = query_status(state);
        if status.running {
            return background_started_payload(status.pid.unwrap_or_else(|| child.id()));
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                return execution_error(&format!(
                    "Studio exited during startup ({status}). Inspect workspace configuration and its socket path."
                ));
            }
            Err(error) => return execution_error(&error.to_string()),
            Ok(None) => std::thread::sleep(STARTUP_WAIT),
        }
    }
    execution_error(
        "Studio startup was not confirmed. Inspect studio status before retrying; startup may still be in progress.",
    )
}

fn background_started_payload(pid: u32) -> Payload {
    Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": format!("Studio started in background (PID {pid})."),
            "running": true,
            "pid": pid,
        }),
    }
}

fn execution_error(msg: &str) -> Payload {
    Payload::Error {
        outcome: Outcome::Failed,
        diagnostic: Diagnostic::new(ErrorCode::ExecutionFailed, msg),
    }
}
