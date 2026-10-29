//! Explicit process launch with bounded readiness checks; opening a frontend never calls this.
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{Receiver, TryRecvError, sync_channel};
use std::time::{Duration, Instant};

use super::{StudioError, StudioStatus, hold_startup, query_status, resolve_workspace_state};

const STARTUP_TIMEOUT: Duration = Duration::from_secs(3);
const STARTUP_POLL: Duration = Duration::from_millis(25);

#[derive(Debug, Clone)]
pub struct WorkerLaunch {
    pub executable: PathBuf,
    pub arguments: Vec<OsString>,
}

/// Starting a worker can execute already queued jobs. Call only from an explicit start action.
pub fn start_worker(library: &Path, launch: &WorkerLaunch) -> Result<StudioStatus, StudioError> {
    let state = resolve_workspace_state(Some(library));
    let _startup = hold_startup(Some(library), &state)?;
    let status = checked_status(&state)?;
    if status.running {
        return Ok(status);
    }
    let mut command = Command::new(&launch.executable);
    command.args(&launch.arguments).env("RETOLD_STATE", &state);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let child = command.spawn()?;
    await_start(child, &state)
}

fn checked_status(state: &Path) -> Result<StudioStatus, StudioError> {
    let status = query_status(state);
    let issue = status.issues.first();
    if let Some(issue) = issue {
        return Err(issue.clone().into());
    }
    Ok(status)
}

fn await_start(child: Child, state: &Path) -> Result<StudioStatus, StudioError> {
    let exit = watch_child(child);
    let started = Instant::now();
    while started.elapsed() < STARTUP_TIMEOUT {
        let status = checked_status(state)?;
        if status.running {
            return Ok(status);
        }
        match exit.try_recv() {
            Ok(exit) => {
                return Err(StudioError::Protocol(format!(
                    "Worker exited during startup ({}). Inspect workspace configuration and its socket path.",
                    exit?
                )));
            }
            Err(TryRecvError::Disconnected) => {
                return Err(StudioError::Protocol(
                    "Worker supervision disconnected; startup is unconfirmed.".into(),
                ));
            }
            Err(TryRecvError::Empty) => (),
        }
        std::thread::sleep(STARTUP_POLL);
    }
    Err(StudioError::Protocol("Worker startup is unconfirmed. Inspect status before retrying; startup may still be in progress.".into()))
}

fn watch_child(mut child: Child) -> Receiver<std::io::Result<ExitStatus>> {
    let (sender, receiver) = sync_channel(1);
    // Reap our own child even when startup times out or the GUI stays open after shutdown.
    std::thread::spawn(move || {
        let _ = sender.send(child.wait());
    });
    receiver
}
