//! Integration tests for durable queue store and transitions.

#![expect(clippy::unwrap_used, reason = "test assertions unwrap")]

use retold_app::queue::ListOptions;
use retold_app::queue::{JobSpec, JobStatus, QueueStore, TransitionSpec};

const NOW: &str = "2026-10-19T12:00:00Z";
const LATER: &str = "2026-10-19T12:05:00Z";
const PREFIX_LEN: usize = 8;

#[test]
fn enqueue_and_idempotency() {
    let store = QueueStore::in_memory().unwrap();
    let spec = JobSpec {
        idempotency_key: "idem-key-1",
        video_ref: "drishyam/1",
        plan_hash: "hash-123",
    };
    let job1 = store.enqueue(spec, NOW).unwrap();
    assert_eq!(job1.status, JobStatus::Queued);
    assert_eq!(job1.video_ref, "drishyam/1");

    let job2 = store.enqueue(spec, LATER).unwrap();
    assert_eq!(job1.id, job2.id);
    assert_eq!(job2.created_at, NOW);
}

#[test]
fn valid_state_transitions() {
    let store = QueueStore::in_memory().unwrap();
    let spec = JobSpec {
        idempotency_key: "idem-key-2",
        video_ref: "drishyam/2",
        plan_hash: "hash-456",
    };
    let job = store.enqueue(spec, NOW).unwrap();
    assert_running_transition(&store, &job.id);
    assert_completed_transition(&store, &job.id);
}

fn assert_running_transition(store: &QueueStore, job_id: &str) {
    let spec = TransitionSpec {
        id: job_id,
        target: JobStatus::Running,
        error: None,
    };
    let running = store.transition(spec, LATER).unwrap();
    assert_eq!(running.status, JobStatus::Running);
}

fn assert_completed_transition(store: &QueueStore, job_id: &str) {
    let spec = TransitionSpec {
        id: job_id,
        target: JobStatus::Completed,
        error: None,
    };
    let completed = store.transition(spec, LATER).unwrap();
    assert_eq!(completed.status, JobStatus::Completed);
}

#[test]
fn invalid_state_transitions_fail() {
    let store = QueueStore::in_memory().unwrap();
    let spec = JobSpec {
        idempotency_key: "idem-key-3",
        video_ref: "drishyam/3",
        plan_hash: "hash-789",
    };
    let job = store.enqueue(spec, NOW).unwrap();

    let invalid = store.transition(
        TransitionSpec {
            id: &job.id,
            target: JobStatus::Completed,
            error: None,
        },
        LATER,
    );
    assert!(invalid.is_err());
}

#[test]
fn priority_reordering() {
    let store = QueueStore::in_memory().unwrap();
    let (id_a, id_b) = enqueue_two_jobs(&store);

    let initial = store.list_page(&ListOptions::default()).unwrap().jobs;
    assert_eq!(initial[0].id, id_a);

    store.move_before(&id_b, &id_a).unwrap();

    let updated = store.list_page(&ListOptions::default()).unwrap().jobs;
    assert_eq!(updated[0].id, id_b);
}

fn enqueue_two_jobs(store: &QueueStore) -> (String, String) {
    let spec_a = JobSpec {
        idempotency_key: "idem-key-4",
        video_ref: "video-a",
        plan_hash: "hash-a",
    };
    let job_a = store.enqueue(spec_a, NOW).unwrap();

    let spec_b = JobSpec {
        idempotency_key: "idem-key-5",
        video_ref: "video-b",
        plan_hash: "hash-b",
    };
    let job_b = store.enqueue(spec_b, LATER).unwrap();
    (job_a.id, job_b.id)
}

#[test]
fn find_by_prefix() {
    let store = QueueStore::in_memory().unwrap();
    let job = store
        .enqueue(
            JobSpec {
                idempotency_key: "idem-key-6",
                video_ref: "video-c",
                plan_hash: "hash-c",
            },
            NOW,
        )
        .unwrap();

    let found = store.find(&job.id[..PREFIX_LEN]).unwrap();
    assert!(found.is_some());
}
