//! Interruption hardening: event floods stay bounded, observer restarts never change job state,
//! and a damaged queue journal is reported instead of being repaired or recreated.
#![expect(
    clippy::unwrap_used,
    reason = "fixture and assertion failures must fail these tests"
)]
use std::path::Path;

use retold_app::queue::{EVENT_PAGE_SIZE, JobSpec, JobStatus, QueueStore, TransitionSpec};
use retold_app::studio::ProgressEvent;

const NOW: &str = "2026-10-20T00:00:00Z";
const FLOOD: usize = 20_000;
const JOBS: usize = 1_024;
/// The activity snapshot feeds the UI thread's next frame; it must stay well under a second.
const SNAPSHOT_BUDGET: std::time::Duration = std::time::Duration::from_millis(500);

fn running_job(queue: &QueueStore) -> String {
    let spec = JobSpec {
        idempotency_key: "flooded",
        video_ref: "video",
        plan_hash: "hash",
    };
    let job = queue.enqueue(spec, NOW).unwrap();
    let running = TransitionSpec {
        id: &job.id,
        target: JobStatus::Running,
        error: Option::default(),
    };
    queue.transition(running, NOW).unwrap();
    job.id
}

fn progress(id: &str, index: usize) -> ProgressEvent {
    ProgressEvent {
        sequence: 0,
        job_id: id.into(),
        timestamp: NOW.into(),
        stage: "images".into(),
        step: Option::default(),
        message: format!("Frame {index}"),
        is_terminal: false,
    }
}

fn flooded(path: &Path) -> String {
    let queue = QueueStore::open(path).unwrap();
    let id = running_job(&queue);
    for index in 0..FLOOD {
        queue.record_event(progress(&id, index)).unwrap();
    }
    id
}

#[test]
fn an_event_flood_is_read_in_bounded_pages_and_a_restarted_observer_resumes_without_changing_the_job()
 {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("queue.sqlite");
    let id = flooded(&path);
    let first = QueueStore::open_read_only(&path)
        .unwrap()
        .replay(&id, 0)
        .unwrap();
    assert_eq!(first.events.len(), EVENT_PAGE_SIZE);
    let restarted = QueueStore::open_read_only(&path).unwrap();
    let resumed = restarted.replay(&id, first.next_sequence).unwrap();
    assert!(resumed.events.len() <= EVENT_PAGE_SIZE);
    assert!(
        resumed
            .events
            .iter()
            .all(|event| event.sequence > first.next_sequence - 1)
    );
    assert_eq!(resumed.job.status, JobStatus::Running);
    assert!(!resumed.is_terminal);
}

#[test]
fn a_damaged_queue_journal_is_reported_and_left_untouched() {
    let root = tempfile::tempdir().unwrap();
    let state = root.path().join(".retold/studio");
    std::fs::create_dir_all(&state).unwrap();
    let database = state.join(retold_app::studio::server::QUEUE_DB_NAME);
    std::fs::write(&database, b"this is not a queue journal").unwrap();
    let before = std::fs::read(&database).unwrap();
    let observed =
        retold_app::desktop::activity(root.path(), &retold_app::queue::ListOptions::default());
    assert!(observed.is_err());
    assert_eq!(std::fs::read(&database).unwrap(), before);
}

fn many_jobs(root: &Path) {
    let state = root.join(".retold/studio");
    std::fs::create_dir_all(&state).unwrap();
    let queue = QueueStore::open(&state.join(retold_app::studio::server::QUEUE_DB_NAME)).unwrap();
    for index in 0..JOBS {
        let key = format!("job-{index}");
        let spec = JobSpec {
            idempotency_key: &key,
            video_ref: "video",
            plan_hash: "hash",
        };
        queue.enqueue(spec, NOW).unwrap();
    }
}

#[test]
fn the_activity_snapshot_of_a_large_queue_is_bounded_and_fast() {
    let root = tempfile::tempdir().unwrap();
    many_jobs(root.path());
    let started = std::time::Instant::now();
    let activity =
        retold_app::desktop::activity(root.path(), &retold_app::queue::ListOptions::default())
            .unwrap();
    let elapsed = started.elapsed();
    assert!(activity.page.jobs.len() <= usize::try_from(retold_app::queue::MAX_PAGE_SIZE).unwrap());
    assert_eq!(activity.page.total, JOBS);
    assert!(
        elapsed < SNAPSHOT_BUDGET,
        "activity snapshot took {elapsed:?}"
    );
}
