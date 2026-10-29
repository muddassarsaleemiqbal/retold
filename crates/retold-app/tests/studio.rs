//! Integration tests for Studio server & client lifecycle.

#![expect(clippy::unwrap_used, reason = "test assertions unwrap")]

use std::thread;
use std::time::Duration;

use retold_app::queue::{JobSpec, JobStatus, QueueStore};
use retold_app::studio::{
    StudioRequest, StudioResponse, StudioServer, poll_events, poll_events_after, query_status,
    send_request, stop_studio,
};

const STARTUP_WAIT: Duration = Duration::from_millis(50);
const TERMINAL_POLL: Duration = Duration::from_millis(10);

#[test]
fn offline_status_is_clean() {
    let workspace_dir = tempfile::tempdir().unwrap();
    let status = query_status(workspace_dir.path());
    assert!(!status.running);
    assert_eq!(status.active_jobs, 0);
    assert_eq!(status.queue_depth, 0);

    let db_path = workspace_dir.path().join("queue.sqlite");
    assert!(
        !db_path.exists(),
        "offline inspection must not create queue storage"
    );
    let store = QueueStore::open(&db_path).unwrap();
    store
        .enqueue(
            JobSpec {
                idempotency_key: "offline-1",
                video_ref: "video-1",
                plan_hash: "hash-1",
            },
            "2026-10-19T00:00:00Z",
        )
        .unwrap();

    let updated_status = query_status(workspace_dir.path());
    assert!(!updated_status.running);
    assert_eq!(updated_status.queue_depth, 1);
}

#[test]
fn server_lifecycle_and_ipc() {
    let workspace_dir = tempfile::tempdir().unwrap();
    let server = StudioServer::bind(workspace_dir.path()).unwrap();

    let second = StudioServer::bind(workspace_dir.path());
    assert!(second.is_err());

    let handle = thread::spawn(move || {
        server.run().unwrap();
    });

    thread::sleep(STARTUP_WAIT);

    let status = query_status(workspace_dir.path());
    assert!(status.running);
    assert!(status.pid.is_some());

    let response = send_request(workspace_dir.path(), &StudioRequest::Ping).unwrap();
    assert!(matches!(response, StudioResponse::Pong { .. }));
    assert_legacy_listing(workspace_dir.path());

    let stopped = stop_studio(workspace_dir.path()).unwrap();
    assert!(stopped);

    handle.join().unwrap();

    let after_status = query_status(workspace_dir.path());
    assert!(!after_status.running);
}

fn assert_legacy_listing(state: &std::path::Path) {
    let legacy = serde_json::from_str::<StudioRequest>(r#"{"action":"list_queue"}"#).unwrap();
    let listing = serde_json::to_value(send_request(state, &legacy).unwrap()).unwrap();
    assert_eq!(listing["jobs"], serde_json::json!([]));
    assert_eq!(listing["total"], 0);
}

#[test]
fn worker_rejects_jobs_without_executable_intents_instead_of_fabricating_completion() {
    let workspace_dir = tempfile::tempdir().unwrap();
    let server = StudioServer::bind(workspace_dir.path()).unwrap();
    let queue = server.queue();

    let job = queue
        .enqueue(
            JobSpec {
                idempotency_key: "worker-job-1",
                video_ref: "drishyam/1",
                plan_hash: "hash-xyz",
            },
            "2026-10-19T00:00:00Z",
        )
        .unwrap();

    assert_eq!(job.status, JobStatus::Queued);
    server.tick_worker().unwrap();

    let finished = wait_for_terminal(queue, &job.id);
    assert_eq!(finished.status, JobStatus::Failed);
    assert_eq!(finished.error_code.as_deref(), Some("RG-INPUT-REQUIRED"));
    assert!(queue.result(&job.id).unwrap().is_some());
}

fn wait_for_terminal(queue: &QueueStore, id: &str) -> retold_app::queue::JobRecord {
    let started = std::time::Instant::now();
    loop {
        let job = queue.find(id).unwrap().unwrap();
        let terminal = job.status.is_terminal();
        if terminal {
            return job;
        }
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "worker did not finish"
        );
        thread::sleep(TERMINAL_POLL);
    }
}

fn run_job_in_studio(server: StudioServer) -> (String, thread::JoinHandle<()>) {
    let job = server
        .queue()
        .enqueue(
            JobSpec {
                idempotency_key: "event-job-1",
                video_ref: "v1",
                plan_hash: "h1",
            },
            "2026-10-19T00:00:00Z",
        )
        .unwrap();
    server.tick_worker().unwrap();
    let handle = thread::spawn(move || {
        server.run().unwrap();
    });
    (job.id, handle)
}

#[test]
fn studio_events_stream_and_replay() {
    let workspace_dir = tempfile::tempdir().unwrap();
    let server = StudioServer::bind(workspace_dir.path()).unwrap();
    let (job_id, handle) = run_job_in_studio(server);
    thread::sleep(STARTUP_WAIT);

    let (events, is_terminal) = poll_events(workspace_dir.path(), &job_id).unwrap();
    assert!(is_terminal);
    assert_ne!(events.len(), 0);
    assert_saved_failure(workspace_dir.path(), &job_id);

    let (replay, _) = poll_events_after(workspace_dir.path(), &job_id, 1).unwrap();
    assert_eq!(replay.len(), events.len() - 1);

    stop_studio(workspace_dir.path()).unwrap();
    handle.join().unwrap();
}

fn assert_saved_failure(state: &std::path::Path, id: &str) {
    let response = send_request(state, &StudioRequest::GetJob { id: id.into() }).unwrap();
    assert!(
        matches!(response, StudioResponse::Job { job: Some(job), result: Some(_), .. }
        if job.status == JobStatus::Failed)
    );
}

#[test]
fn malformed_clients_receive_an_error_and_leave_the_studio_running() {
    use std::io::{BufRead, BufReader, Write};
    let directory = tempfile::tempdir().unwrap();
    let server = StudioServer::bind(directory.path()).unwrap();
    let handle = thread::spawn(move || server.run().unwrap());
    let mut stream =
        std::os::unix::net::UnixStream::connect(directory.path().join("studio.sock")).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    stream.write_all(b"not valid JSON\n").unwrap();
    let mut response = String::new();
    BufReader::new(stream).read_line(&mut response).unwrap();
    assert!(matches!(
        serde_json::from_str::<StudioResponse>(&response).unwrap(),
        StudioResponse::Error { .. }
    ));
    assert!(query_status(directory.path()).running);
    stop_studio(directory.path()).unwrap();
    handle.join().unwrap();
}
