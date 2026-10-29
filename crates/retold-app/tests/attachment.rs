//! Attachment replays committed progress without dispatching or controlling its job.
#![expect(
    clippy::unwrap_used,
    reason = "fixture and assertion failures must fail these tests"
)]
use std::path::Path;

use retold_app::protocol::{Outcome, Payload};
use retold_app::queue::{EVENT_PAGE_SIZE, JobSpec, JobStatus, QueueStore, TransitionSpec};
use retold_app::studio::{Attachment, ProgressEvent, StudioServer};

const NOW: &str = "2026-10-20T00:00:00Z";
const EXTRA_EVENTS: usize = 7;

fn running_job(queue: &QueueStore) -> String {
    let job = queue
        .enqueue(
            JobSpec {
                idempotency_key: "observed",
                video_ref: "video",
                plan_hash: "hash",
            },
            NOW,
        )
        .unwrap();
    queue
        .transition(
            TransitionSpec {
                id: &job.id,
                target: JobStatus::Running,
                error: Option::default(),
            },
            NOW,
        )
        .unwrap();
    job.id
}

fn progress(id: &str, message: &str) -> ProgressEvent {
    ProgressEvent {
        sequence: 0,
        job_id: id.into(),
        timestamp: NOW.into(),
        stage: "research".into(),
        step: Option::default(),
        message: message.into(),
        is_terminal: false,
    }
}

fn finish(queue: &QueueStore, id: &str) {
    queue
        .finish(
            id,
            &Payload::Result {
                outcome: Outcome::Completed,
                contents: serde_json::json!({"saved": true}),
            },
            progress(id, "Saved outcome"),
        )
        .unwrap();
}

fn database(root: &Path) -> std::path::PathBuf {
    root.join("queue.sqlite")
}

#[test]
fn completed_jobs_replay_every_page_offline_without_touching_storage() {
    let root = tempfile::tempdir().unwrap();
    let queue = QueueStore::open(&database(root.path())).unwrap();
    let id = running_job(&queue);
    for index in 0..EVENT_PAGE_SIZE * 2 + EXTRA_EVENTS {
        queue
            .record_event(progress(&id, &format!("Saved {index}")))
            .unwrap();
    }
    finish(&queue, &id);
    let original = std::fs::read(database(root.path())).unwrap();
    let attachment = Attachment::open(root.path(), &id, 0).unwrap();
    assert!(!attachment.owner_present().unwrap());
    let seen = replay_all(attachment);
    assert_eq!(seen.len(), EVENT_PAGE_SIZE * 2 + EXTRA_EVENTS + 2);
    assert!(seen.windows(2).all(|pair| pair[0] < pair[1]));
    assert_eq!(std::fs::read(database(root.path())).unwrap(), original);
    assert!(!root.path().join("locks").exists());
}

fn replay_all(mut attachment: Attachment) -> Vec<u64> {
    let mut seen = Vec::new();
    loop {
        let page = attachment.poll().unwrap();
        assert!(page.events.len() <= EVENT_PAGE_SIZE);
        assert_eq!(page.job.status, JobStatus::Completed);
        for event in page.events {
            attachment = attachment.acknowledged(&event).unwrap();
            seen.push(event.sequence);
        }
        if page.is_terminal {
            assert!(page.result.is_some());
            break;
        }
    }
    seen
}

#[test]
fn dropping_an_attachment_preserves_the_running_job_and_its_owner() {
    let root = tempfile::tempdir().unwrap();
    let server = StudioServer::bind(root.path()).unwrap();
    let id = running_job(server.queue());
    let attached = Attachment::open(root.path(), "job-", 0).unwrap();
    assert_eq!(attached.job_id(), id);
    assert!(attached.owner_present().unwrap());
    drop(attached);
    assert_eq!(
        server.queue().find(&id).unwrap().unwrap().status,
        JobStatus::Running
    );
    let attached = Attachment::open(root.path(), &id, 0).unwrap();
    drop(server);
    assert!(!attached.owner_present().unwrap());
    assert_eq!(attached.poll().unwrap().job.status, JobStatus::Running);
}

#[test]
fn acknowledgments_accept_only_forward_events_for_the_attached_job() {
    let root = tempfile::tempdir().unwrap();
    let queue = QueueStore::open(&database(root.path())).unwrap();
    let id = running_job(&queue);
    let mut attached = Attachment::open(root.path(), &id, 0).unwrap();
    let mut page = attached.poll().unwrap();
    let event = page.events.remove(0);
    attached = attached.acknowledged(&event).unwrap();
    let cursor = attached.sequence();
    assert!(attached.acknowledged(&event).is_err());
    let mut wrong = event;
    wrong.job_id = "another-job".into();
    wrong.sequence = cursor + 1;
    assert!(attached.acknowledged(&wrong).is_err());
    assert_eq!(attached.sequence(), cursor);
    assert!(Attachment::open(root.path(), &id, cursor + 1).is_err());
    assert!(Attachment::open(root.path(), &id, u64::MAX).is_err());
}

#[test]
fn missing_jobs_leave_a_missing_workspace_untouched() {
    let root = tempfile::tempdir().unwrap();
    let missing = root.path().join("missing");
    let error = Attachment::open(&missing, "job-missing", 0).unwrap_err();
    assert_eq!(
        error.diagnostic().code,
        retold_app::protocol::ErrorCode::RefNotFound
    );
    assert!(!missing.exists());
}
