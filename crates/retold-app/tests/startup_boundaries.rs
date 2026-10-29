//! Inactive roots remain readable; existing handles cannot acquire new queue write authority.
#![expect(clippy::unwrap_used, reason = "fixture helpers fail on invalid setup")]
use retold_app::queue::{JobSpec, JobStatus, ListOptions, QueueError, QueueStore, TransitionSpec};
use retold_app::studio::{Attachment, ProgressEvent, StudioServer};

const NOW: &str = "2026-10-20T00:00:00Z";
const INACTIVE: &str = ".retold-restore.json";

const fn spec(key: &str) -> JobSpec<'_> {
    JobSpec {
        idempotency_key: key,
        video_ref: "video",
        plan_hash: "hash",
    }
}

#[test]
fn inactive_startup_rejects_new_queue_directories_and_locks() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join(INACTIVE), b"{}").unwrap();
    let state = root.path().join("future/state");
    let server = StudioServer::bind(&state).unwrap_err();
    assert_eq!(
        server.diagnostic().code,
        retold_app::protocol::ErrorCode::NotReady
    );
    assert!(matches!(
        QueueStore::open(&state.join("queue.sqlite")),
        Err(QueueError::Inactive)
    ));
    assert!(!root.path().join("future").exists());
}

#[test]
fn existing_queue_handles_reject_mutations_after_the_root_becomes_inactive() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("queue.sqlite");
    let queue = QueueStore::open(&path).unwrap();
    let job = queue.enqueue(spec("original"), NOW).unwrap();
    let original = std::fs::read(&path).unwrap();
    std::fs::write(root.path().join(INACTIVE), b"{}").unwrap();
    assert!(matches!(
        queue.enqueue(spec("new"), NOW),
        Err(QueueError::Inactive)
    ));
    let transition = TransitionSpec {
        id: &job.id,
        target: JobStatus::Stopped,
        error: Option::default(),
    };
    assert!(matches!(
        queue.transition(transition, NOW),
        Err(QueueError::Inactive)
    ));
    let event = progress_event(&job.id);
    assert!(matches!(
        queue.record_event(event),
        Err(QueueError::Inactive)
    ));
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert_eq!(queue.list_page(&ListOptions::default()).unwrap().total, 1);
    assert!(Attachment::open(root.path(), &job.id, 0).is_ok());
}

fn progress_event(id: &str) -> ProgressEvent {
    ProgressEvent {
        sequence: 0,
        job_id: id.into(),
        timestamp: NOW.into(),
        stage: "research".into(),
        step: Option::default(),
        message: "uncommitted".into(),
        is_terminal: false,
    }
}

#[cfg(unix)]
#[test]
fn aliases_cannot_hide_an_inactive_state_ancestor() {
    let root = tempfile::tempdir().unwrap();
    let retired = root.path().join("retired");
    mark_retired(&retired);
    let alias = root.path().join("alias");
    std::os::unix::fs::symlink(&retired, &alias).unwrap();
    assert!(matches!(
        QueueStore::open(&alias.join("queue.sqlite")),
        Err(QueueError::Inactive)
    ));
    assert!(!retired.join("queue.sqlite").exists());
}

fn mark_retired(path: &std::path::Path) {
    std::fs::create_dir(path).unwrap();
    std::fs::write(path.join(INACTIVE), b"{}").unwrap();
}
