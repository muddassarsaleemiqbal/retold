//! SQLite storage implementation for durable jobs.

use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior, params};

use super::types::{JobRecord, JobSpec, JobStatus, QueueError, TransitionSpec, can_transition};

const SCHEMA: &str = include_str!("schema.sql");
const SELECT_NEXT_QUEUED: &str = include_str!("select_next_queued.sql");
const SELECT_BY_ID: &str = include_str!("select_by_id.sql");
const SELECT_BY_PREFIX: &str = include_str!("select_by_prefix.sql");
const SELECT_BY_KEY: &str = include_str!("select_by_key.sql");
const INSERT: &str = include_str!("insert.sql");
const UPDATE_PRIORITY: &str = include_str!("update_priority.sql");
const UPDATE_TRANSITION: &str = include_str!("update_transition.sql");

const COL_ID: usize = 0;
const COL_KEY: usize = 1;
const COL_VIDEO: usize = 2;
const COL_PLAN: usize = 3;
const COL_STATUS: usize = 4;
const COL_PRIORITY: usize = 5;
const COL_CREATED: usize = 6;
const COL_UPDATED: usize = 7;
const COL_ERR_CODE: usize = 8;
const COL_ERR_MSG: usize = 9;
const MAX_RECORD_BYTES: i32 = 2 * 1024 * 1024;
const MAX_PENDING_JOBS: i64 = 1024;
const COUNT_PENDING: &str = include_str!("count_pending.sql");
const SELECT_PENDING_ORDER: &str = include_str!("select_pending_order.sql");
const JOB_ID_HEX: usize = 24;
const SELECT_INTERRUPTED: &str = include_str!("select_interrupted.sql");

#[derive(Debug, Clone)]
pub struct QueueStore {
    conn: Arc<Mutex<Connection>>,
}

impl QueueStore {
    pub(super) fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>, QueueError> {
        self.conn.lock().map_err(|_| QueueError::LockPoisoned)
    }

    pub fn open(path: &Path) -> Result<Self, QueueError> {
        writable_parent(path)?;
        let parent = path.parent();
        if let Some(dir) = parent {
            std::fs::create_dir_all(dir)?;
        }
        let conn = initialized(Connection::open(path)?)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn open_read_only(path: &Path) -> Result<Self, QueueError> {
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        configure(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn in_memory() -> Result<Self, QueueError> {
        let conn = initialized(Connection::open_in_memory()?)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn enqueue(&self, spec: JobSpec<'_>, now: &str) -> Result<JobRecord, QueueError> {
        let mut conn = self.write_lock()?;
        let transaction = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let job = enqueue_locked(&transaction, spec, now)?;
        transaction.commit()?;
        Ok(job)
    }

    pub fn pending_count(&self) -> Result<usize, QueueError> {
        let connection = self.lock()?;
        let count: i64 = connection.query_row(COUNT_PENDING, [], |row| row.get(0))?;
        usize::try_from(count).map_err(|_| QueueError::Invalid("Unsupported queue depth."))
    }

    pub fn interrupted(&self) -> Result<Vec<JobRecord>, QueueError> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(SELECT_INTERRUPTED)?;
        let rows = statement.query_map([], parse_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(QueueError::from)
    }

    pub fn next_queued(&self) -> Result<Option<JobRecord>, QueueError> {
        let conn = self.lock()?;
        let state = super::dispatch::read_dispatch(&conn)?;
        let inhibited = state.inhibited_by.is_some();
        if inhibited {
            return Ok(Option::default());
        }
        let next = conn
            .query_row(SELECT_NEXT_QUEUED, [], parse_row)
            .optional()?;
        Ok(next)
    }

    pub fn find(&self, id_or_prefix: &str) -> Result<Option<JobRecord>, QueueError> {
        let conn = self.lock()?;
        read_by_reference(&conn, id_or_prefix)
    }

    pub fn find_by_key(&self, key: &str) -> Result<Option<JobRecord>, QueueError> {
        let connection = self.lock()?;
        find_by_idempotency(&connection, key)
    }

    pub fn transition(&self, spec: TransitionSpec<'_>, now: &str) -> Result<JobRecord, QueueError> {
        transition_job(self, spec, now)
    }

    pub fn move_before(&self, id: &str, before_id: &str) -> Result<(), QueueError> {
        let mut conn = self.write_lock()?;
        let transaction = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let source = queued_job(&transaction, id)?;
        let target = queued_job(&transaction, before_id)?;
        let ordered = pending_order(&transaction)?;
        let reordered = moved_order(ordered, &source.id, &target.id)?;
        write_priorities(&transaction, &reordered)?;
        transaction.commit()?;
        Ok(())
    }
}

impl QueueStore {
    pub(super) fn write_lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>, QueueError> {
        let connection = self.lock()?;
        let disk_path = connection.path().filter(|path| !path.is_empty());
        if let Some(path) = disk_path {
            writable_parent(Path::new(path))?;
        }
        Ok(connection)
    }
}

fn writable_parent(path: &Path) -> Result<(), QueueError> {
    let parent = path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    retold_engine::recovery::ensure_active(parent).map_err(|_| QueueError::Inactive)
}

fn transition_job(
    store: &QueueStore,
    spec: TransitionSpec<'_>,
    now: &str,
) -> Result<JobRecord, QueueError> {
    let mut conn = store.write_lock()?;
    let transaction = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let current = read_job_by_id(&transaction, spec.id)?;
    let valid = can_transition(current.status, spec.target);
    if !valid {
        return Err(QueueError::InvalidTransition(current.status, spec.target));
    }
    apply_transition(&transaction, spec, now)?;
    super::durable::record_transition(&transaction, spec, now)?;
    let job = read_job_by_id(&transaction, spec.id)?;
    transaction.commit()?;
    Ok(job)
}

fn generate_job_id(spec: JobSpec<'_>, now: &str) -> String {
    let raw = format!("{}:{}:{now}", spec.video_ref, spec.idempotency_key);
    let hash = blake3::hash(raw.as_bytes()).to_hex();
    format!("job-{}", &hash[..JOB_ID_HEX])
}

struct RawJobRow {
    id: String,
    idempotency_key: String,
    video_ref: String,
    plan_hash: String,
    status_str: String,
    priority: i64,
    created_at: String,
    updated_at: String,
    error_code: Option<String>,
    error_message: Option<String>,
}

fn read_raw_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawJobRow> {
    Ok(RawJobRow {
        id: row.get(COL_ID)?,
        idempotency_key: row.get(COL_KEY)?,
        video_ref: row.get(COL_VIDEO)?,
        plan_hash: row.get(COL_PLAN)?,
        status_str: row.get(COL_STATUS)?,
        priority: row.get(COL_PRIORITY)?,
        created_at: row.get(COL_CREATED)?,
        updated_at: row.get(COL_UPDATED)?,
        error_code: row.get(COL_ERR_CODE)?,
        error_message: row.get(COL_ERR_MSG)?,
    })
}

fn raw_to_record(raw: RawJobRow) -> rusqlite::Result<JobRecord> {
    let status = JobStatus::parse(&raw.status_str).ok_or_else(invalid_status)?;
    let record = JobRecord {
        id: raw.id,
        idempotency_key: raw.idempotency_key,
        video_ref: raw.video_ref,
        plan_hash: raw.plan_hash,
        status,
        priority: raw.priority,
        created_at: raw.created_at,
        updated_at: raw.updated_at,
        error_code: raw.error_code,
        error_message: raw.error_message,
    };
    record.validate().map_err(super::validation::invalid_row)?;
    Ok(record)
}

pub(super) fn parse_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<JobRecord> {
    read_raw_row(row).and_then(raw_to_record)
}

fn invalid_status() -> rusqlite::Error {
    let error = std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "Unsupported queue job status.",
    );
    rusqlite::Error::FromSqlConversionFailure(
        COL_STATUS,
        rusqlite::types::Type::Text,
        Box::new(error),
    )
}

fn find_by_idempotency(conn: &Connection, key: &str) -> Result<Option<JobRecord>, QueueError> {
    let record = conn
        .query_row(SELECT_BY_KEY, params![key], parse_row)
        .optional()?;
    Ok(record)
}

pub(super) fn read_job_by_id(conn: &Connection, id: &str) -> Result<JobRecord, QueueError> {
    let record = conn
        .query_row(SELECT_BY_ID, params![id], parse_row)
        .optional()?
        .ok_or_else(|| QueueError::NotFound(id.to_owned()))?;
    Ok(record)
}

#[derive(Debug, Clone, Copy)]
struct JobInsertSpec<'a> {
    id: &'a str,
    spec: JobSpec<'a>,
    now: &'a str,
}

fn insert_job(conn: &Connection, target: JobInsertSpec<'_>) -> Result<(), QueueError> {
    conn.execute(
        INSERT,
        params![
            target.id,
            target.spec.idempotency_key,
            target.spec.video_ref,
            target.spec.plan_hash,
            spec_status_str(),
            target.now
        ],
    )?;
    Ok(())
}

fn spec_status_str() -> &'static str {
    JobStatus::Queued.as_str()
}

pub(super) fn apply_transition(
    conn: &Connection,
    spec: TransitionSpec<'_>,
    now: &str,
) -> Result<(), QueueError> {
    let (code, message) = match spec.error {
        Some((err_code, err_msg)) => (Some(err_code), Some(err_msg)),
        None => (None, None),
    };
    conn.execute(
        UPDATE_TRANSITION,
        params![spec.target.as_str(), now, code, message, spec.id],
    )?;
    Ok(())
}

pub(super) fn enqueue_locked(
    conn: &Connection,
    spec: JobSpec<'_>,
    now: &str,
) -> Result<JobRecord, QueueError> {
    spec.validate()?;
    if let Some(record) = find_by_idempotency(conn, spec.idempotency_key)? {
        let unchanged = record.video_ref == spec.video_ref && record.plan_hash == spec.plan_hash;
        return if unchanged {
            Ok(record)
        } else {
            Err(QueueError::Invalid(
                "Idempotency keys cannot replace an existing job's plan or video.",
            ))
        };
    }
    require_capacity(conn)?;
    super::dispatch::require_open(conn)?;
    let job_id = generate_job_id(spec, now);
    let insert_spec = JobInsertSpec {
        id: &job_id,
        spec,
        now,
    };
    insert_job(conn, insert_spec)?;
    read_job_by_id(conn, &job_id)
}

fn initialized(mut connection: Connection) -> Result<Connection, QueueError> {
    configure(&connection)?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    transaction.execute_batch(SCHEMA)?;
    transaction.commit()?;
    Ok(connection)
}

fn configure(connection: &Connection) -> Result<(), QueueError> {
    connection.busy_timeout(std::time::Duration::from_millis(100))?;
    connection.set_limit(
        rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH,
        MAX_RECORD_BYTES,
    )?;
    connection.pragma_update(Option::<&str>::default(), "foreign_keys", "ON")?;
    connection.pragma_update(Option::<&str>::default(), "synchronous", "FULL")?;
    Ok(())
}

pub(super) fn read_by_reference(
    connection: &Connection,
    reference: &str,
) -> Result<Option<JobRecord>, QueueError> {
    super::validation::reference(reference)?;
    let exact = connection
        .query_row(SELECT_BY_ID, params![reference], parse_row)
        .optional()?;
    if exact.is_some() {
        return Ok(exact);
    }
    let mut statement = connection.prepare(SELECT_BY_PREFIX)?;
    let candidates = statement
        .query_map(params![reference], parse_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if candidates.len() > 1 {
        return Err(QueueError::Ambiguous(reference.into()));
    }
    Ok(candidates.into_iter().next())
}

fn queued_job(connection: &Connection, reference: &str) -> Result<JobRecord, QueueError> {
    let job = read_by_reference(connection, reference)?
        .ok_or_else(|| QueueError::NotFound(reference.into()))?;
    if job.status != JobStatus::Queued {
        return Err(QueueError::Invalid(
            "Only pending queued jobs can be reordered.",
        ));
    }
    Ok(job)
}

fn pending_order(connection: &Connection) -> Result<Vec<String>, QueueError> {
    let mut statement = connection.prepare(SELECT_PENDING_ORDER)?;
    let rows = statement.query_map([], |row| row.get(0))?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(QueueError::from)
}

fn moved_order(
    mut jobs: Vec<String>,
    source: &str,
    target: &str,
) -> Result<Vec<String>, QueueError> {
    if source == target {
        return Ok(jobs);
    }
    let from = jobs
        .iter()
        .position(|id| id == source)
        .ok_or_else(|| QueueError::NotFound(source.into()))?;
    let source = jobs.remove(from);
    let before = jobs
        .iter()
        .position(|id| id == target)
        .ok_or_else(|| QueueError::NotFound(target.into()))?;
    jobs.insert(before, source);
    Ok(jobs)
}

fn write_priorities(connection: &Connection, jobs: &[String]) -> Result<(), QueueError> {
    for (index, id) in jobs.iter().enumerate() {
        let priority = i64::try_from(jobs.len().saturating_sub(index))
            .map_err(|_| QueueError::Invalid("Queue order exceeds the supported range."))?;
        connection.execute(UPDATE_PRIORITY, params![priority, id])?;
    }
    Ok(())
}

fn require_capacity(connection: &Connection) -> Result<(), QueueError> {
    let pending: i64 = connection.query_row(COUNT_PENDING, [], |row| row.get(0))?;
    if pending >= MAX_PENDING_JOBS {
        return Err(QueueError::Invalid(
            "Queue capacity reached (1024 pending jobs). Wait, pause or stop other work before submitting.",
        ));
    }
    Ok(())
}
