//! The shipped binary's worker mode runs without display initialization and preserves holds.
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context as _, ensure};
use retold_app::studio::{StudioRequest, StudioResponse, query_status, send_request, stop_studio};

struct Worker {
    child: Child,
    state: PathBuf,
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = stop_studio(&self.state);
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn launch(root: &Path, library: &Path, state: &Path) -> anyhow::Result<Worker> {
    let child = Command::new(env!("CARGO_BIN_EXE_retold-studio"))
        .args(["--worker", "--library"])
        .arg(library)
        .env("HOME", root.join("isolated-home"))
        .env("RETOLD_CONFIG_DIR", root.join("isolated-installation"))
        .env("RETOLD_STATE", state)
        .env_remove("RETOLD_WORKSPACE")
        .env_remove("RETOLD_LIBRARY")
        .env_remove("XDG_DATA_HOME")
        .env("DISPLAY", "")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut worker = Worker {
        child,
        state: state.into(),
    };
    let started = Instant::now();
    while !query_status(state).running {
        if let Some(exit) = worker.child.try_wait()? {
            let mut message = String::new();
            if let Some(stderr) = worker.child.stderr.take() {
                stderr.take(16_384).read_to_string(&mut message)?;
            }
            anyhow::bail!("Packaged worker exited before becoming ready ({exit}): {message}");
        }
        ensure!(
            started.elapsed() < Duration::from_secs(5),
            "Packaged worker startup timed out"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    Ok(worker)
}

#[test]
fn packaged_worker_negotiates_controls_and_retains_inhibition_across_restart() -> anyhow::Result<()>
{
    let fixture = tempfile::tempdir()?;
    let library = fixture.path().join("custom-library");
    std::fs::create_dir_all(&library)?;
    let state = library.join(".retold/studio");
    let queue = retold_app::queue::QueueStore::open(&state.join("queue.sqlite"))?;
    queue.stop_all("before-launch", "2026-10-25T00:00:00Z")?;
    let mut worker = launch(fixture.path(), &library, &state)?;
    let response = send_request(&state, &StudioRequest::Capabilities)?;
    ensure!(
        matches!(response, StudioResponse::Capabilities { features, .. } if features.contains(&"durable_stop_all_v1".into())),
        "Worker has no durable control capability"
    );
    ensure!(
        stop_studio(&state)?,
        "Worker did not confirm graceful shutdown"
    );
    ensure!(
        worker.child.wait()?.success(),
        "Worker failed on graceful shutdown"
    );
    drop(worker);
    let mut restarted = launch(fixture.path(), &library, &state)?;
    ensure!(
        queue.dispatch_inhibited()?,
        "Restart silently cleared the hold"
    );
    ensure!(
        queue.next_queued()?.is_none(),
        "Held worker can dequeue work"
    );
    stop_studio(&state)?;
    ensure!(
        restarted.child.wait()?.success(),
        "Restarted worker failed on shutdown"
    );
    Ok(())
}

#[test]
fn worker_mode_rejects_cross_workspace_state_before_creating_it() -> anyhow::Result<()> {
    let fixture = tempfile::tempdir()?;
    let library = fixture.path().join("library");
    std::fs::create_dir(&library)?;
    let other = fixture.path().join("other-state");
    let output = Command::new(env!("CARGO_BIN_EXE_retold-studio"))
        .args(["--worker", "--library"])
        .arg(library)
        .env("RETOLD_STATE", &other)
        .output()?;
    ensure!(
        !output.status.success(),
        "Worker accepted another workspace's state"
    );
    ensure!(
        !other.exists(),
        "Rejected worker created another workspace's state"
    );
    let message = std::str::from_utf8(&output.stderr).context("worker diagnostic was not UTF-8")?;
    ensure!(
        message.contains("RETOLD_STATE"),
        "Scope rejection lacks an actionable diagnostic"
    );
    Ok(())
}
