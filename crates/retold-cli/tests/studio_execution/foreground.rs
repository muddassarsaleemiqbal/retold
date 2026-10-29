//! Foreground cancellation must finish the engine before returning its terminal envelope.
use std::process::{Child, Output, Stdio};
use std::thread;
use std::time::Instant;

use super::control::{
    assert_provider_exited, install_slow_provider, signal_owner, wait_provider_exit, wait_started,
};
use super::{Installation, POLL_INTERVAL, WAIT_LIMIT};

#[test]
fn foreground_sigint_cancels_and_reaps_the_provider() {
    assert_foreground_interrupt("-INT");
}

#[test]
fn foreground_sigterm_cancels_and_reaps_the_provider() {
    assert_foreground_interrupt("-TERM");
}

fn assert_foreground_interrupt(signal: &str) {
    let fixture = Installation::new();
    install_slow_provider(&fixture);
    let child = spawn_foreground(&fixture);
    wait_started(&fixture);
    signal_owner(signal, u64::from(child.id()));
    let output = wait_foreground(child);
    assert_cancelled_output(&output);
    assert_provider_exited(&fixture);
    assert!(fixture.run.join("work/01-research/source.json").exists());
    assert!(!fixture.state().join("queue.sqlite").exists());
}

fn spawn_foreground(fixture: &Installation) -> Child {
    let mut command = fixture.command(&[
        "video",
        "continue",
        &fixture.video_id,
        "--until",
        "script",
        "--yes",
    ]);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    command.spawn().unwrap()
}

fn wait_foreground(mut child: Child) -> Output {
    let started = Instant::now();
    loop {
        let exited = child.try_wait().unwrap().is_some();
        if exited {
            break;
        }
        assert!(started.elapsed() < WAIT_LIMIT, "foreground did not drain");
        thread::sleep(POLL_INTERVAL);
    }
    child.wait_with_output().unwrap()
}

fn assert_cancelled_output(output: &Output) {
    assert_eq!(output.status.code(), Some(4));
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(envelope["type"], "error");
    assert_eq!(envelope["payload"]["context"]["cancelled"], true);
    assert!(envelope["payload"]["context"]["job_id"].is_string());
}

#[test]
fn foreground_sigkill_is_cleaned_up_by_the_independent_supervisor() {
    let fixture = Installation::new();
    install_slow_provider(&fixture);
    let child = spawn_foreground(&fixture);
    wait_started(&fixture);
    signal_owner("-KILL", u64::from(child.id()));
    let output = wait_foreground(child);
    assert!(!output.status.success());
    wait_provider_exit(&fixture);
    assert!(fixture.run.join("work/01-research/source.json").exists());
}
