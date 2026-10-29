//! Real process cancellation and responsive control, using only a local fake provider.
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::thread;
use std::time::{Duration, Instant};

use retold_app::queue::{JobStatus, QueueStore};
use retold_app::studio::{StudioRequest, StudioResponse, send_request};

use super::{Installation, POLL_INTERVAL, WAIT_LIMIT, reviewed_research_job};

const EXECUTABLE_MODE: u32 = 0o700;
const ACK_LIMIT: Duration = Duration::from_secs(1);
// Concurrent durable research setup precedes dispatch; control acknowledgments keep their own bound.
const PROVIDER_START_LIMIT: Duration = Duration::from_secs(15);
const SCRIPT: &str = include_str!("../fixtures/studio/slow-provider.sh");
const PROVIDERS: &str = include_str!("../fixtures/studio/providers.toml");

pub fn install_slow_provider(fixture: &Installation) {
    let binary = fixture.sandbox.path().join("fake-provider");
    fs::write(&binary, SCRIPT).unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(EXECUTABLE_MODE)).unwrap();
    write_provider_config(fixture, &binary);
}

fn write_provider_config(fixture: &Installation, binary: &std::path::Path) {
    let encoded = serde_json::to_string(binary).unwrap();
    let config = PROVIDERS.replace("__BINARY__", &encoded);
    fs::write(fixture.root.join("config/providers.toml"), config).unwrap();
}

fn submit_script(fixture: &Installation) -> String {
    let result = fixture.succeed(&[
        "video",
        "continue",
        &fixture.video_id,
        "--until",
        "script",
        "--yes",
        "--background",
    ]);
    result["payload"]["data"]["job"]["id"]
        .as_str()
        .unwrap()
        .into()
}

pub fn wait_started(fixture: &Installation) {
    let path = fixture.sandbox.path().join("provider.started");
    let started = Instant::now();
    while !path.exists() {
        assert!(
            started.elapsed() < PROVIDER_START_LIMIT,
            "fake provider did not start; queue: {:?}",
            pending_details(fixture)
        );
        thread::sleep(POLL_INTERVAL);
    }
}

fn pending_details(fixture: &Installation) -> Vec<retold_app::queue::JobRecord> {
    let database = fixture.state().join("queue.sqlite");
    if !database.exists() {
        return Vec::new();
    }
    QueueStore::open_read_only(&fixture.state().join("queue.sqlite"))
        .unwrap()
        .list_page(&retold_app::queue::ListOptions::default())
        .unwrap()
        .jobs
}

fn wait_status(fixture: &Installation, id: &str, expected: JobStatus) {
    let queue = QueueStore::open_read_only(&fixture.state().join("queue.sqlite")).unwrap();
    let started = Instant::now();
    loop {
        let job = queue.find(id).unwrap().unwrap();
        if job.status == expected {
            return;
        }
        assert!(
            started.elapsed() < WAIT_LIMIT,
            "expected {expected:?}, found {job:?}"
        );
        thread::sleep(POLL_INTERVAL);
    }
}

#[test]
fn stop_is_acknowledged_while_a_provider_runs_and_finishes_after_cancellation() {
    let fixture = Installation::new();
    install_slow_provider(&fixture);
    fixture.succeed(&["studio", "start"]);
    let id = submit_script(&fixture);
    wait_started(&fixture);
    let started = Instant::now();
    let response =
        send_request(&fixture.state(), &StudioRequest::StopJob { id: id.clone() }).unwrap();
    assert!(
        started.elapsed() < ACK_LIMIT,
        "control loop blocked on execution"
    );
    assert!(
        matches!(response, StudioResponse::JobTransitioned { job } if job.status == JobStatus::Stopping)
    );
    wait_status(&fixture, &id, JobStatus::Stopped);
    assert_provider_exited(&fixture);
    assert_stopped_attachment(&fixture, &id);
}

fn assert_stopped_attachment(fixture: &Installation, id: &str) {
    let reported = fixture.succeed(&["--quiet", "queue", "attach", id]);
    let outcome = &reported["payload"]["data"];
    assert_eq!(reported["payload"]["outcome"], "completed");
    assert_eq!(outcome["attachment"]["status"], "stopped");
    assert_eq!(
        outcome["execution_result"]["payload"]["context"]["cancelled"],
        true
    );
}

#[test]
fn pause_preserves_completed_work_and_unpause_requires_a_fresh_plan_after_progress() {
    let fixture = Installation::new();
    install_slow_provider(&fixture);
    fixture.succeed(&["studio", "start"]);
    let id = submit_script(&fixture);
    wait_started(&fixture);
    fixture.succeed(&["queue", "pause", &id]);
    wait_status(&fixture, &id, JobStatus::Paused);
    assert_provider_exited(&fixture);
    assert!(fixture.run.join("work/01-research/source.json").exists());
    fixture.succeed(&["queue", "unpause", &id]);
    let finished = fixture.wait(&id);
    assert_eq!(finished.status, JobStatus::Blocked);
    assert_eq!(finished.error_code.as_deref(), Some("RG-PLAN-CHANGED"));
}

pub fn assert_provider_exited(fixture: &Installation) {
    assert!(!provider_is_alive(fixture), "provider outlived its owner");
    assert!(!fixture.sandbox.path().join("provider.finished").exists());
}

fn provider_is_alive(fixture: &Installation) -> bool {
    let pid = provider_pid(fixture);
    let mut command = std::process::Command::new("/bin/kill");
    command.args(["-0", &pid.to_string()]);
    command
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    command.status().unwrap().success()
}

pub fn wait_provider_exit(fixture: &Installation) {
    let started = Instant::now();
    while provider_is_alive(fixture) {
        assert!(
            started.elapsed() < WAIT_LIMIT,
            "supervisor did not reap provider"
        );
        thread::sleep(POLL_INTERVAL);
    }
    assert_provider_exited(fixture);
}

fn provider_pid(fixture: &Installation) -> u32 {
    let path = fixture.sandbox.path().join("provider.pid");
    let text = fs::read_to_string(path).unwrap();
    parse_pid(&text)
}

fn parse_pid(text: &str) -> u32 {
    text.trim().parse().unwrap()
}

#[test]
fn backup_refuses_active_worker_writes_and_succeeds_after_confirmed_stop() {
    let fixture = Installation::new();
    install_slow_provider(&fixture);
    fixture.succeed(&["studio", "start"]);
    let id = submit_script(&fixture);
    wait_started(&fixture);
    let archive = fixture.sandbox.path().join("snapshot");
    let (busy, code) =
        fixture.invoke(&["storage", "backup", "--output", archive.to_str().unwrap()]);
    assert_eq!(code, 3, "{busy}");
    assert_eq!(busy["payload"]["code"], "RG-ACCOUNT-BUSY");
    assert!(!archive.exists());
    fixture.succeed(&["queue", "stop", &id]);
    wait_status(&fixture, &id, JobStatus::Stopped);
    fixture.succeed(&["storage", "backup", "--output", archive.to_str().unwrap()]);
    assert!(archive.join("index.json").is_file());
    fixture.succeed(&["storage", "verify", archive.to_str().unwrap()]);
}

#[test]
fn studio_reorders_only_pending_work_while_an_existing_provider_remains_active() {
    let fixture = Installation::new();
    install_slow_provider(&fixture);
    fixture.succeed(&["studio", "start"]);
    let active = submit_script(&fixture);
    wait_started(&fixture);
    let pending = reorder_pending(&fixture);
    for id in pending {
        fixture.succeed(&["queue", "stop", &id]);
    }
    fixture.succeed(&["queue", "stop", &active]);
    wait_status(&fixture, &active, JobStatus::Stopped);
    assert_provider_exited(&fixture);
}

fn reorder_pending(fixture: &Installation) -> Vec<String> {
    let first = reviewed_research_job(fixture, "queued-one");
    let second = reviewed_research_job(fixture, "queued-two");
    let queue = QueueStore::open_read_only(&fixture.state().join("queue.sqlite")).unwrap();
    let next = queue.next_queued().unwrap().unwrap();
    let other = if next.id == first.id {
        &second.id
    } else {
        &first.id
    };
    fixture.succeed(&["queue", "move", other, "--before", &next.id]);
    assert_eq!(queue.next_queued().unwrap().unwrap().id, *other);
    vec![first.id, second.id]
}

#[test]
fn sigterm_drains_the_provider_before_releasing_studio_ownership() {
    assert_signal_shutdown("-TERM");
}

#[test]
fn sigint_drains_the_provider_before_releasing_studio_ownership() {
    assert_signal_shutdown("-INT");
}

fn assert_signal_shutdown(signal: &str) {
    let fixture = Installation::new();
    install_slow_provider(&fixture);
    let started = fixture.succeed(&["studio", "start"]);
    let pid = started["payload"]["data"]["pid"].as_u64().unwrap();
    let id = submit_script(&fixture);
    wait_started(&fixture);
    signal_owner(signal, pid);
    wait_status(&fixture, &id, JobStatus::Paused);
    wait_studio_exit(&fixture);
    assert_provider_exited(&fixture);
}

pub fn signal_owner(signal: &str, pid: u64) {
    let status = std::process::Command::new("/bin/kill")
        .args([signal, &pid.to_string()])
        .status()
        .unwrap();
    assert!(status.success());
}

fn wait_studio_exit(fixture: &Installation) {
    let started = Instant::now();
    while retold_app::studio::query_status(&fixture.state()).running {
        assert!(started.elapsed() < WAIT_LIMIT, "studio did not drain");
        thread::sleep(POLL_INTERVAL);
    }
}

#[test]
fn shutdown_reaps_the_provider_even_when_queue_writes_are_rejected() {
    let fixture = Installation::new();
    install_slow_provider(&fixture);
    let started = fixture.succeed(&["studio", "start"]);
    let pid = started["payload"]["data"]["pid"].as_u64().unwrap();
    let id = submit_script(&fixture);
    wait_started(&fixture);
    retire_queue(&fixture);
    signal_owner("-TERM", pid);
    wait_studio_exit(&fixture);
    assert_provider_exited(&fixture);
    assert_unsettled_queue(&fixture, &id);
}

fn retire_queue(fixture: &Installation) {
    fs::write(fixture.state().join(".retold-restore.json"), b"{}").unwrap();
}

fn assert_unsettled_queue(fixture: &Installation, id: &str) {
    let queue = QueueStore::open_read_only(&fixture.state().join("queue.sqlite")).unwrap();
    assert_eq!(queue.find(id).unwrap().unwrap().status, JobStatus::Running);
    assert!(queue.details(id).unwrap().unwrap().result.is_none());
}

#[test]
fn studio_sigkill_reaps_the_provider_and_recovers_without_redispatch() {
    let fixture = Installation::new();
    install_slow_provider(&fixture);
    let started = fixture.succeed(&["studio", "start"]);
    let pid = started["payload"]["data"]["pid"].as_u64().unwrap();
    let id = submit_script(&fixture);
    wait_started(&fixture);
    signal_owner("-KILL", pid);
    wait_provider_exit(&fixture);
    fixture.succeed(&["studio", "start"]);
    let recovered = fixture.wait(&id);
    assert_eq!(recovered.status, JobStatus::Blocked);
    assert_eq!(recovered.error_code.as_deref(), Some("RG-PLAN-CHANGED"));
    assert_provider_exited(&fixture);
}

struct SuspendedSupervisor(u64);

impl Drop for SuspendedSupervisor {
    fn drop(&mut self) {
        let _ = std::process::Command::new("/bin/kill")
            .args(["-CONT", &self.0.to_string()])
            .status();
    }
}

fn suspend_supervisor(fixture: &Installation) -> SuspendedSupervisor {
    let preview = fixture.succeed(&["account", "init", "--timezone", "UTC", "--dry-run"]);
    let directory =
        std::path::PathBuf::from(preview["payload"]["data"]["directory"].as_str().unwrap());
    let owners = retold_app::account::barrier::owners(&directory).unwrap();
    let supervisor = owners
        .iter()
        .find(|owner| owner.purpose == "supervised provider process")
        .unwrap();
    let pid = u64::from(supervisor.pid);
    signal_owner("-STOP", pid);
    SuspendedSupervisor(pid)
}

#[test]
fn backup_stays_blocked_after_owner_death_until_supervisor_cleanup() {
    let fixture = Installation::new();
    install_slow_provider(&fixture);
    let started = fixture.succeed(&["studio", "start"]);
    let pid = started["payload"]["data"]["pid"].as_u64().unwrap();
    submit_script(&fixture);
    wait_started(&fixture);
    let suspended = suspend_supervisor(&fixture);
    signal_owner("-KILL", pid);
    let archive = fixture.sandbox.path().join("after-crash");
    let args = ["storage", "backup", "--output", archive.to_str().unwrap()];
    let (busy, code) = fixture.invoke(&args);
    assert_eq!(code, 3, "{busy}");
    assert_eq!(busy["payload"]["code"], "RG-ACCOUNT-BUSY");
    assert!(!archive.exists());
    drop(suspended);
    wait_provider_exit(&fixture);
    wait_backup_after_cleanup(&fixture, &args);
    fixture.succeed(&["storage", "verify", archive.to_str().unwrap()]);
}

#[test]
fn simultaneous_owner_and_supervisor_death_retains_backup_and_admission_barrier() {
    let fixture = Installation::new();
    install_slow_provider(&fixture);
    let started = fixture.succeed(&["studio", "start"]);
    let pid = started["payload"]["data"]["pid"].as_u64().unwrap();
    submit_script(&fixture);
    wait_started(&fixture);
    let suspended = suspend_supervisor(&fixture);
    signal_owner("-KILL", pid);
    signal_owner("-KILL", suspended.0);
    // The provider survives both owners; PID liveness cannot release claims.
    assert!(provider_is_alive(&fixture));
    assert_orphan_blocks_work(&fixture);
    // Fixture cleanup must not reconcile production uncertainty.
    let group = format!("-{}", provider_pid(&fixture));
    let _ = std::process::Command::new("/bin/kill")
        .args(["-KILL", "--", &group])
        .status();
    std::mem::forget(suspended);
}

fn assert_orphan_blocks_work(fixture: &Installation) {
    let archive = fixture.sandbox.path().join("uncertain-snapshot");
    let (busy, code) =
        fixture.invoke(&["storage", "backup", "--output", archive.to_str().unwrap()]);
    assert_eq!(code, 3, "{busy}");
    assert_eq!(busy["payload"]["code"], "RG-ACCOUNT-BUSY");
    assert!(!archive.exists());
    let (blocked, code) = fixture.invoke(&["studio", "start"]);
    assert_eq!(code, 3, "{blocked}");
    assert_eq!(blocked["payload"]["code"], "RG-ACCOUNT-BUSY");
}

fn wait_backup_after_cleanup(fixture: &Installation, args: &[&str]) {
    let started = Instant::now();
    loop {
        let (response, code) = fixture.invoke(args);
        if code == 0 {
            return;
        }
        assert_eq!(response["payload"]["code"], "RG-ACCOUNT-BUSY", "{response}");
        assert!(
            started.elapsed() < PROVIDER_START_LIMIT,
            "cleanup retained ownership: {response}"
        );
        thread::sleep(POLL_INTERVAL);
    }
}

#[test]
fn failed_provider_spawn_clears_the_prelaunch_claim() {
    let fixture = Installation::new();
    assert!(!spawn_missing_provider(&fixture).status.success());
    fixture.succeed(&["studio", "start"]);
}

fn spawn_missing_provider(fixture: &Installation) -> std::process::Output {
    use std::os::unix::net::UnixListener;
    let endpoint = fixture.sandbox.path().join("launch.sock");
    let _listener = UnixListener::bind(&endpoint).unwrap();
    let mut command = inherit_environment(&fixture.command(&[]));
    let missing = fixture.sandbox.path().join("missing-provider");
    command
        .arg("__retold_process_supervisor_v1")
        .arg(endpoint)
        .arg(missing);
    command.output().unwrap()
}

fn inherit_environment(environment: &std::process::Command) -> std::process::Command {
    let mut command = std::process::Command::new(environment.get_program());
    for (name, value) in environment.get_envs() {
        if let Some(value) = value {
            command.env(name, value);
        } else {
            command.env_remove(name);
        }
    }
    command
}

#[test]
fn provider_exec_retains_the_identity_persisted_before_release() {
    let fixture = Installation::new();
    install_slow_provider(&fixture);
    fixture.succeed(&["studio", "start"]);
    let job = submit_script(&fixture);
    wait_started(&fixture);
    let inventory = fixture.succeed(&["account", "processes", "--limit", "1"]);
    let claims = inventory["payload"]["data"]["processes"]
        .as_array()
        .unwrap();
    assert_eq!(claims.len(), 1);
    let claim = &claims[0];
    assert_eq!(claim["schema_version"], 2);
    assert_eq!(claim["worker"]["pid"], provider_pid(&fixture));
    assert_eq!(claim["worker"]["group"], provider_pid(&fixture));
    assert_eq!(claim["worker"]["boot"], claim["supervisor"]["boot"]);
    assert_eq!(claim["worker_observation"], "matching");
    assert_eq!(claim["supervisor_observation"], "matching");
    fixture.succeed(&["queue", "stop", &job]);
    wait_status(&fixture, &job, JobStatus::Stopped);
    let settled = fixture.succeed(&["account", "processes"]);
    assert_eq!(
        settled["payload"]["data"]["processes"],
        serde_json::json!([])
    );
}

#[test]
fn gated_worker_rejects_a_missing_release_token() {
    let fixture = Installation::new();
    let endpoint = fixture.sandbox.path().join("gated.sock");
    let listener = std::os::unix::net::UnixListener::bind(&endpoint).unwrap();
    listener.set_nonblocking(true).unwrap();
    let marker = fixture.sandbox.path().join("executed-provider");
    let mut command = inherit_environment(&fixture.command(&[]));
    command.arg("__retold_gated_worker_v1").arg(endpoint);
    command.arg("/usr/bin/touch").arg(&marker);
    command
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let child = command.spawn().unwrap();
    let connection = accept_worker(&listener).unwrap();
    assert!(!marker.exists());
    drop(connection);
    assert!(!child.wait_with_output().unwrap().status.success());
    assert!(!marker.exists());
}

fn accept_worker(
    listener: &std::os::unix::net::UnixListener,
) -> std::io::Result<std::os::unix::net::UnixStream> {
    let started = Instant::now();
    loop {
        match listener.accept() {
            Ok((stream, _)) => return Ok(stream),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => (),
            Err(error) => return Err(error),
        }
        assert!(started.elapsed() < WAIT_LIMIT);
        thread::sleep(POLL_INTERVAL);
    }
}
