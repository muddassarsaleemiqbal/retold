//! Stop-all invariants apply across connections, restarts and older database writers.
#![expect(clippy::unwrap_used, reason = "test fixture assertions")]

use retold_app::queue::{JobRecord, JobSpec, JobStatus, QueueStore, TransitionSpec};

const NOW: &str = "2026-10-25T00:00:00Z";

fn enqueue(queue: &QueueStore, key: &str) -> JobRecord {
    queue
        .enqueue(
            JobSpec {
                idempotency_key: key,
                video_ref: "fixture/video",
                plan_hash: "reviewed",
            },
            NOW,
        )
        .unwrap()
}

fn transition(queue: &QueueStore, id: &str, target: JobStatus) {
    queue
        .transition(
            TransitionSpec {
                id,
                target,
                error: None,
            },
            NOW,
        )
        .unwrap();
}

fn assert_status(queue: &QueueStore, id: &str, status: JobStatus) {
    assert_eq!(queue.find(id).unwrap().unwrap().status, status);
}

#[test]
fn stop_snapshot_commits_events_and_keeps_active_work_unconfirmed() {
    let queue = QueueStore::in_memory().unwrap();
    let queued = enqueue(&queue, "queued");
    let paused = enqueue(&queue, "paused");
    let active = enqueue(&queue, "active");
    transition(&queue, &paused.id, JobStatus::Paused);
    transition(&queue, &active.id, JobStatus::Running);
    let receipt = queue.stop_all("control-1", NOW).unwrap();
    assert_eq!(receipt.stopped.len(), 2);
    assert_eq!(receipt.stopping, vec![active.id.clone()]);
    assert_status(&queue, &queued.id, JobStatus::Stopped);
    assert_status(&queue, &active.id, JobStatus::Stopping);
    assert!(
        !queue
            .events(&active.id, 0)
            .unwrap()
            .last()
            .unwrap()
            .is_terminal
    );
    assert!(
        queue
            .events(&queued.id, 0)
            .unwrap()
            .last()
            .unwrap()
            .is_terminal
    );
    assert!(queue.next_queued().unwrap().is_none());
}

#[test]
fn receipt_replays_after_restart_without_stopping_new_work_or_reinstating_a_hold() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("queue.sqlite");
    let queue = QueueStore::open(&path).unwrap();
    let stopped = enqueue(&queue, "old");
    let original = queue.stop_all("request", NOW).unwrap();
    drop(queue);
    let queue = QueueStore::open(&path).unwrap();
    assert_eq!(
        queue.dispatch_state().unwrap().inhibited_by.as_deref(),
        Some("request")
    );
    assert_eq!(queue.stop_all("request", "later").unwrap(), original);
    queue.resume_dispatch("request").unwrap();
    let next = enqueue(&queue, "new");
    assert_eq!(queue.stop_all("request", NOW).unwrap(), original);
    assert_eq!(queue.next_queued().unwrap().unwrap().id, next.id);
    assert_eq!(
        queue.find(&stopped.id).unwrap().unwrap().status,
        JobStatus::Stopped
    );
}

#[test]
fn held_queue_rejects_new_admission_and_older_raw_sql_writers() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("queue.sqlite");
    let queue = QueueStore::open(&path).unwrap();
    let job = enqueue(&queue, "old");
    queue.stop_all("hold", NOW).unwrap();
    let other = QueueStore::open(&path).unwrap();
    let spec = JobSpec {
        idempotency_key: "new",
        video_ref: "v",
        plan_hash: "hash",
    };
    assert!(other.enqueue(spec, NOW).is_err());
    let old_writer = rusqlite::Connection::open(&path).unwrap();
    assert!(
        old_writer
            .execute(
                "UPDATE jobs SET status = 'running' WHERE id = ?1",
                [&job.id]
            )
            .is_err()
    );
    assert!(
        old_writer
            .execute("UPDATE jobs SET status = 'queued' WHERE id = ?1", [&job.id])
            .is_err()
    );
    assert!(other.next_queued().unwrap().is_none());
}

#[test]
fn older_resume_request_cannot_clear_a_newer_hold() {
    let queue = QueueStore::in_memory().unwrap();
    queue.stop_all("first", NOW).unwrap();
    queue.stop_all("second", NOW).unwrap();
    assert!(queue.resume_dispatch("first").is_err());
    assert_eq!(
        queue.dispatch_state().unwrap().inhibited_by.as_deref(),
        Some("second")
    );
    queue.resume_dispatch("second").unwrap();
    assert!(queue.dispatch_state().unwrap().inhibited_by.is_none());
}

#[test]
fn invalid_control_does_not_change_jobs_or_dispatch() {
    let queue = QueueStore::in_memory().unwrap();
    let job = enqueue(&queue, "untouched");
    assert!(queue.stop_all("", NOW).is_err());
    assert!(queue.stop_all("control\n", NOW).is_err());
    assert_eq!(queue.next_queued().unwrap().unwrap().id, job.id);
    assert!(queue.dispatch_state().unwrap().inhibited_by.is_none());
}

#[test]
fn offline_stop_records_inhibition_without_starting_a_worker() {
    let directory = tempfile::tempdir().unwrap();
    let stop = retold_app::studio::stop_workspace_queue(directory.path(), "offline").unwrap();
    assert!(!stop.worker_acknowledged);
    assert!(stop.connection_issue.is_some());
    let queue = QueueStore::open_read_only(&directory.path().join("queue.sqlite")).unwrap();
    assert!(queue.dispatch_inhibited().unwrap());
    assert!(!directory.path().join("studio.sock").exists());
}

#[test]
fn daemon_advertises_and_acknowledges_scoped_stopping() {
    use retold_app::studio::{
        StudioRequest, StudioResponse, StudioServer, send_request, stop_studio,
    };
    let directory = tempfile::tempdir().unwrap();
    let server = StudioServer::bind(directory.path()).unwrap();
    server.queue().stop_all("initial", NOW).unwrap();
    let worker = std::thread::spawn(move || server.run().unwrap());
    let request = StudioRequest::Capabilities;
    let response = send_request(directory.path(), &request).unwrap();
    assert!(
        matches!(response, StudioResponse::Capabilities { features, .. } if features.contains(&"durable_stop_all_v1".into()))
    );
    let stop = retold_app::studio::stop_workspace_queue(directory.path(), "ipc").unwrap();
    assert!(stop.worker_acknowledged);
    assert_eq!(stop.receipt.control_id, "ipc");
    stop_studio(directory.path()).unwrap();
    worker.join().unwrap();
}
