//! Queue transactions preserve intent, replay and incomplete-owner outcomes.
#![expect(clippy::unwrap_used, reason = "isolated queue fixtures and assertions")]
use retold_app::plan::PlanOptions;
use retold_app::protocol::{Outcome, Payload};
use retold_app::queue::ListOptions;
use retold_app::queue::{
    EVENT_PAGE_SIZE, ExecutionIntent, JobSpec, JobStatus, QueueStore, TransitionSpec,
};
use retold_app::studio::{ProgressEvent, StudioServer};

const NOW: &str = "2026-10-20T00:00:00Z";
const OVERSIZED_EVENT_BYTES: usize = 4096;
const PENDING_CAPACITY: usize = 1024;

fn enqueue(queue: &QueueStore, key: &str) -> retold_app::queue::JobRecord {
    queue
        .enqueue(
            JobSpec {
                idempotency_key: key,
                video_ref: "video",
                plan_hash: "hash",
            },
            NOW,
        )
        .unwrap()
}

fn event(id: &str) -> ProgressEvent {
    ProgressEvent {
        sequence: 0,
        job_id: id.into(),
        timestamp: NOW.into(),
        stage: "test".into(),
        step: None,
        message: "Progress".into(),
        is_terminal: false,
    }
}

#[test]
fn distinct_keys_avoid_collisions_and_ambiguous_prefixes_are_rejected() {
    let queue = QueueStore::in_memory().unwrap();
    let first = enqueue(&queue, "first");
    let second = enqueue(&queue, "second");
    assert_ne!(first.id, second.id);
    assert!(matches!(
        queue.find("job-"),
        Err(retold_app::queue::QueueError::Ambiguous(_))
    ));
    assert!(
        queue.find("job-%").unwrap().is_none(),
        "prefix matching is literal"
    );
    let conflict = queue.enqueue(
        JobSpec {
            idempotency_key: "first",
            video_ref: "other",
            plan_hash: "hash",
        },
        NOW,
    );
    assert!(conflict.is_err());
    let page = queue.list_page(&ListOptions::default()).unwrap();
    assert_eq!(page.total, 2);
}

#[test]
fn intents_are_immutable_and_event_replay_is_bounded_and_survives_reopen() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("queue.sqlite");
    let queue = QueueStore::open(&path).unwrap();
    let intent = ExecutionIntent {
        library: temporary.path().into(),
        plan: PlanOptions::default(),
    };
    let spec = JobSpec {
        idempotency_key: "intent",
        video_ref: "video",
        plan_hash: "hash",
    };
    let job = queue.enqueue_executable(spec, &intent, NOW).unwrap();
    let changed = ExecutionIntent {
        library: temporary.path().join("different"),
        ..intent
    };
    assert!(queue.enqueue_executable(spec, &changed, NOW).is_err());
    for _ in 0..EVENT_PAGE_SIZE.saturating_add(1) {
        queue.record_event(event(&job.id)).unwrap();
    }
    drop(queue);
    let reopened = QueueStore::open_read_only(&path).unwrap();
    let page = reopened.events(&job.id, 0).unwrap();
    assert_eq!(page.len(), EVENT_PAGE_SIZE);
    let cursor = page.last().unwrap().sequence;
    assert_eq!(reopened.events(&job.id, cursor).unwrap().len(), 1);
    assert!(reopened.intent(&job.id).unwrap().is_some());
}

#[test]
fn terminal_state_and_outcome_event_commit_together_and_cannot_be_replaced() {
    let queue = QueueStore::in_memory().unwrap();
    let job = enqueue(&queue, "finish");
    change_status(&queue, &job.id, JobStatus::Running);
    let result = Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({"done":true}),
    };
    let mut oversized = event(&job.id);
    oversized.message = "x".repeat(OVERSIZED_EVENT_BYTES);
    assert!(queue.finish(&job.id, &result, oversized).is_err());
    assert_eq!(
        queue.find(&job.id).unwrap().unwrap().status,
        JobStatus::Running
    );
    assert!(queue.result(&job.id).unwrap().is_none());
    let finished = queue.finish(&job.id, &result, event(&job.id)).unwrap();
    assert_eq!(finished.status, JobStatus::Completed);
    assert!(
        queue
            .events(&job.id, 0)
            .unwrap()
            .last()
            .unwrap()
            .is_terminal
    );
    assert!(queue.finish(&job.id, &result, event(&job.id)).is_err());
}

#[test]
fn restart_blocks_interrupted_pause_and_stop_without_fabricating_quiescence() {
    let temporary = tempfile::tempdir().unwrap();
    let queue = QueueStore::open(&temporary.path().join("queue.sqlite")).unwrap();
    let ids = interrupted_jobs(&queue);
    let server = StudioServer::bind(temporary.path()).unwrap();
    for id in ids {
        assert_recovered(&server, &id);
    }
}

fn interrupted_jobs(queue: &QueueStore) -> Vec<String> {
    let mut ids = Vec::new();
    for (key, status) in [
        ("running", JobStatus::Running),
        ("pausing", JobStatus::Pausing),
        ("stopping", JobStatus::Stopping),
    ] {
        let job = enqueue(queue, key);
        change_status(queue, &job.id, JobStatus::Running);
        if status != JobStatus::Running {
            change_status(queue, &job.id, status);
        }
        ids.push(job.id);
    }
    ids
}

fn change_status(queue: &QueueStore, id: &str, target: JobStatus) {
    let spec = TransitionSpec {
        id,
        target,
        error: None,
    };
    queue.transition(spec, NOW).unwrap();
}

fn assert_recovered(server: &StudioServer, id: &str) {
    assert_eq!(
        server.queue().find(id).unwrap().unwrap().status,
        JobStatus::Blocked
    );
    assert!(server.queue().result(id).unwrap().is_some());
    assert!(
        server
            .queue()
            .events(id, 0)
            .unwrap()
            .last()
            .unwrap()
            .is_terminal
    );
}

#[test]
fn moving_pending_work_preserves_every_other_relative_position_and_rejects_active_work() {
    let queue = QueueStore::in_memory().unwrap();
    let (first, second, third, fourth) = pending_ids(&queue);
    queue.move_before(&third, &second).unwrap();
    let expected = vec![first.clone(), third, second, fourth.clone()];
    assert_eq!(
        queue
            .list_page(&ListOptions::default())
            .unwrap()
            .jobs
            .into_iter()
            .map(|job| job.id)
            .collect::<Vec<_>>(),
        expected
    );
    change_status(&queue, &first, JobStatus::Running);
    assert!(queue.move_before(&first, &fourth).is_err());
    assert_eq!(
        queue
            .list_page(&ListOptions::default())
            .unwrap()
            .jobs
            .into_iter()
            .map(|job| job.id)
            .collect::<Vec<_>>(),
        expected
    );
}

fn pending_ids(queue: &QueueStore) -> (String, String, String, String) {
    for key in ["first", "second", "third", "fourth"] {
        enqueue(queue, key);
    }
    let mut initial = queue
        .list_page(&ListOptions::default())
        .unwrap()
        .jobs
        .into_iter()
        .map(|job| job.id);
    let first = initial.next().unwrap();
    let second = initial.next().unwrap();
    let third = initial.next().unwrap();
    let fourth = initial.next().unwrap();
    (first, second, third, fourth)
}

#[test]
fn queue_capacity_refuses_new_work_without_losing_existing_jobs() {
    let queue = QueueStore::in_memory().unwrap();
    for index in 0..PENDING_CAPACITY {
        enqueue(&queue, &format!("job-{index}"));
    }
    let refused = queue.enqueue(
        JobSpec {
            idempotency_key: "overflow",
            video_ref: "video",
            plan_hash: "hash",
        },
        NOW,
    );
    assert!(refused.is_err());
    assert_eq!(queue.pending_count().unwrap(), PENDING_CAPACITY);
    assert_eq!(
        queue.list_page(&ListOptions::default()).unwrap().total,
        PENDING_CAPACITY
    );
}
