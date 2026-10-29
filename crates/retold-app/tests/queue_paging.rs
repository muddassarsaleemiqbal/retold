//! Pages stay bounded and stable, and expanding the queue schema preserves old readers/writers.
#![expect(
    clippy::unwrap_used,
    reason = "fixture and assertion failures must fail these tests"
)]
use std::path::Path;

use retold_app::queue::{JobSpec, JobStatus, ListOptions, MAX_PAGE_SIZE, QueueError, QueueStore};
use retold_app::studio::ProgressEvent;
use rusqlite::Connection;

const NOW: &str = "2026-10-20T00:00:00Z";
const LEGACY_SCHEMA: &str = include_str!("fixtures/queue/legacy_schema.sql");
const SEED_SQL: &str = include_str!("fixtures/queue/seed.sql");
const LEGACY_ROWS: usize = 3;
const OVERSIZED_KEY_BYTES: usize = 161;
const OLD_UPDATE: &str = include_str!("fixtures/queue/old_writer_update.sql");
const SCHEMA_INVENTORY_SQL: &str = include_str!("fixtures/queue/schema_inventory.sql");
const INCOMPATIBLE_EVENTS: &str = include_str!("fixtures/queue/incompatible_events.sql");
type SchemaEntry = (String, String, Option<String>);

fn seed(path: &Path, count: i64) {
    Connection::open(path)
        .unwrap()
        .execute(SEED_SQL, [count])
        .unwrap();
}

fn first_cursor(queue: &QueueStore) -> String {
    let page = queue
        .list_page(&ListOptions {
            limit: 1,
            ..ListOptions::default()
        })
        .unwrap();
    page.next_cursor.unwrap()
}

fn after(cursor: String) -> ListOptions {
    ListOptions {
        limit: 1,
        cursor: Some(cursor),
        status: Option::default(),
    }
}

fn enqueue(queue: &QueueStore, key: &str) -> String {
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
        .id
}

fn assert_stale(queue: &QueueStore, cursor: String) {
    assert!(matches!(
        queue.list_page(&after(cursor)),
        Err(QueueError::CursorInvalid)
    ));
}

fn create_legacy_queue(path: &Path) {
    Connection::open(path)
        .unwrap()
        .execute_batch(LEGACY_SCHEMA)
        .unwrap();
    seed(path, i64::try_from(LEGACY_ROWS).unwrap());
}

fn schema_inventory(connection: &Connection) -> Vec<SchemaEntry> {
    let mut statement = connection.prepare(SCHEMA_INVENTORY_SQL).unwrap();
    let rows = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap();
    rows.collect::<rusqlite::Result<Vec<_>>>().unwrap()
}

#[test]
fn ten_thousand_history_rows_are_read_in_complete_bounded_pages() {
    const HISTORY: usize = 10_000;
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("queue.sqlite");
    let queue = QueueStore::open(&path).unwrap();
    seed(&path, i64::try_from(HISTORY).unwrap());
    let mut options = ListOptions {
        limit: MAX_PAGE_SIZE,
        ..ListOptions::default()
    };
    let mut seen = std::collections::HashSet::new();
    loop {
        let page = queue.list_page(&options).unwrap();
        assert_eq!(page.total, HISTORY);
        assert!(page.jobs.len() <= usize::try_from(MAX_PAGE_SIZE).unwrap());
        for job in page.jobs {
            assert!(seen.insert(job.id));
        }
        options.cursor = page.next_cursor;
        let finished = options.cursor.is_none();
        if finished {
            break;
        }
    }
    assert_eq!(seen.len(), HISTORY);
}

#[test]
fn mutations_filters_other_queues_and_forged_offsets_invalidate_cursors() {
    let queue = QueueStore::in_memory().unwrap();
    enqueue(&queue, "first");
    enqueue(&queue, "second");
    let cursor = first_cursor(&queue);
    let filtered = ListOptions {
        status: Some(JobStatus::Queued),
        ..after(cursor.clone())
    };
    assert!(matches!(
        queue.list_page(&filtered),
        Err(QueueError::CursorInvalid)
    ));
    let other = QueueStore::in_memory().unwrap();
    enqueue(&other, "first");
    enqueue(&other, "second");
    assert_stale(&other, cursor.clone());
    let forged = format!("{}:99999", cursor.rsplit_once(':').unwrap().0);
    assert_stale(&queue, forged);
    enqueue(&queue, "third");
    assert_stale(&queue, cursor);
}

#[test]
fn reordering_invalidates_pages_while_progress_preserves_them() {
    let queue = QueueStore::in_memory().unwrap();
    let first = enqueue(&queue, "first");
    let second = enqueue(&queue, "second");
    let cursor = first_cursor(&queue);
    queue
        .record_event(ProgressEvent {
            sequence: 0,
            job_id: first.clone(),
            timestamp: NOW.into(),
            stage: "research".into(),
            step: Option::default(),
            message: "Saved progress".into(),
            is_terminal: false,
        })
        .unwrap();
    assert!(queue.list_page(&after(cursor.clone())).is_ok());
    queue.move_before(&second, &first).unwrap();
    assert_stale(&queue, cursor);
}

#[test]
fn legacy_queries_preserve_schema_and_history() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("queue.sqlite");
    create_legacy_queue(&path);
    let original = std::fs::read(&path).unwrap();
    let queue = QueueStore::open_read_only(&path).unwrap();
    let cursor = first_cursor(&queue);
    assert_eq!(queue.list_page(&after(cursor)).unwrap().jobs.len(), 1);
    let details = queue.details("job-00001").unwrap().unwrap();
    assert!(details.result.is_none());
    assert!(details.execution.is_none());
    assert_eq!(std::fs::read(&path).unwrap(), original);
}

#[test]
fn upgrade_is_idempotent_and_old_writers_still_invalidate_new_cursors() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("queue.sqlite");
    create_legacy_queue(&path);
    let legacy = QueueStore::open_read_only(&path).unwrap();
    let old_cursor = first_cursor(&legacy);
    let upgraded = QueueStore::open(&path).unwrap();
    let cursor = first_cursor(&upgraded);
    let repeated = QueueStore::open(&path).unwrap();
    assert!(repeated.list_page(&after(cursor.clone())).is_ok());
    assert_stale(&legacy, old_cursor);
    Connection::open(&path)
        .unwrap()
        .execute_batch(OLD_UPDATE)
        .unwrap();
    assert_stale(&upgraded, cursor);
    assert_eq!(
        upgraded.list_page(&ListOptions::default()).unwrap().total,
        LEGACY_ROWS
    );
    assert_eq!(upgraded.find("job-00003").unwrap().unwrap().priority, 7);
}

#[test]
fn failed_upgrade_rolls_back_every_schema_change_and_preserves_old_jobs() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("queue.sqlite");
    create_legacy_queue(&path);
    let old = Connection::open(&path).unwrap();
    old.execute_batch(INCOMPATIBLE_EVENTS).unwrap();
    let original = schema_inventory(&old);
    assert!(QueueStore::open(&path).is_err());
    assert_eq!(schema_inventory(&old), original);
    let reader = QueueStore::open_read_only(&path).unwrap();
    assert_eq!(
        reader.list_page(&ListOptions::default()).unwrap().total,
        LEGACY_ROWS
    );
}

#[test]
fn invalid_queue_inputs_and_page_sizes_are_refused_without_writes() {
    let queue = QueueStore::in_memory().unwrap();
    for key in [
        String::new(),
        "bad\u{1b}[2J".into(),
        "x".repeat(OVERSIZED_KEY_BYTES),
    ] {
        let result = queue.enqueue(
            JobSpec {
                idempotency_key: &key,
                video_ref: "video",
                plan_hash: "hash",
            },
            NOW,
        );
        assert!(result.is_err());
    }
    assert_eq!(queue.pending_count().unwrap(), 0);
    for limit in [0, MAX_PAGE_SIZE + 1] {
        assert!(
            queue
                .list_page(&ListOptions {
                    limit,
                    ..ListOptions::default()
                })
                .is_err()
        );
    }
}
