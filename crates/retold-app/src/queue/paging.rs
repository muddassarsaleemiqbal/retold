//! Bounded pages are pinned to queue mutations, without scanning normal job history.
use std::path::Path;

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use super::store::parse_row;
use super::{JobRecord, JobStatus, QueueError, QueueStore};

pub const DEFAULT_PAGE_SIZE: u32 = 64;
pub const MAX_PAGE_SIZE: u32 = 128;
const MAX_CURSOR_BYTES: usize = 160;
const MAX_LEGACY_JOBS: usize = 100_000;
const SELECT_PAGE: &str = include_str!("select_page.sql");
const COUNT_FILTERED: &str = include_str!("count_filtered.sql");
const SELECT_REVISION: &str = include_str!("select_revision.sql");
const SELECT_LEGACY_REVISION: &str = include_str!("select_legacy_revision.sql");

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ListOptions {
    pub limit: u32,
    pub cursor: Option<String>,
    pub status: Option<JobStatus>,
}

impl Default for ListOptions {
    fn default() -> Self {
        Self {
            limit: DEFAULT_PAGE_SIZE,
            cursor: Option::default(),
            status: Option::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobPage {
    pub jobs: Vec<JobRecord>,
    pub total: usize,
    #[serde(default)]
    pub next_cursor: Option<String>,
}

impl QueueStore {
    pub fn list_page(&self, options: &ListOptions) -> Result<JobPage, QueueError> {
        validate_limit(options.limit)?;
        let mut connection = self.lock()?;
        let transaction = connection.transaction()?;
        let page = PageReader {
            connection: &transaction,
            options,
        }
        .read()?;
        transaction.commit()?;
        Ok(page)
    }
}

struct PageReader<'a> {
    connection: &'a Connection,
    options: &'a ListOptions,
}

impl PageReader<'_> {
    fn read(&self) -> Result<JobPage, QueueError> {
        let snapshot = self.snapshot()?;
        let status = self.options.status.map(JobStatus::as_str);
        let total: i64 = self
            .connection
            .query_row(COUNT_FILTERED, [status], |row| row.get(0))?;
        let total = usize::try_from(total).map_err(|_| corrupt_count())?;
        let offset = cursor_offset(self.options.cursor.as_deref(), &snapshot, total)?;
        let jobs = self.jobs(offset)?;
        let end = offset.saturating_add(jobs.len());
        Ok(JobPage {
            jobs,
            total,
            next_cursor: (end < total).then(|| format!("v1:{snapshot}:{end}")),
        })
    }

    fn snapshot(&self) -> Result<String, QueueError> {
        let migrated = super::schema::has_table(self.connection, "queue_revision")?;
        let revision = if migrated {
            current_revision(self.connection)?
        } else {
            legacy_revision(self.connection)?
        };
        let path = self.connection.path().unwrap_or(":memory:");
        let path = Path::new(path)
            .canonicalize()
            .unwrap_or_else(|_| path.into());
        let identity = (path, revision, self.options.status);
        let bytes = serde_json::to_vec(&identity)?;
        Ok(blake3::hash(&bytes).to_hex().to_string())
    }

    fn jobs(&self, offset: usize) -> Result<Vec<JobRecord>, QueueError> {
        let offset = i64::try_from(offset).map_err(|_| QueueError::CursorInvalid)?;
        let status = self.options.status.map(JobStatus::as_str);
        let mut statement = self.connection.prepare(SELECT_PAGE)?;
        let rows = statement.query_map(params![status, self.options.limit, offset], parse_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(QueueError::from)
    }
}

fn current_revision(connection: &Connection) -> Result<String, QueueError> {
    let (identity, revision): (String, i64) =
        connection.query_row(SELECT_REVISION, [], |row| Ok((row.get(0)?, row.get(1)?)))?;
    if revision < 0 || identity.is_empty() {
        return Err(QueueError::Corrupt("Invalid queue revision."));
    }
    Ok(format!("{identity}:{revision}"))
}

/// Old databases remain inspectable without writing a schema upgrade from a query.
fn legacy_revision(connection: &Connection) -> Result<String, QueueError> {
    let mut statement = connection.prepare(SELECT_LEGACY_REVISION)?;
    let rows = statement.query_map([], parse_row)?;
    let mut digest = blake3::Hasher::new();
    for (index, row) in rows.enumerate() {
        if index >= MAX_LEGACY_JOBS {
            return Err(QueueError::Corrupt(
                "Start studio to upgrade this large queue before paginating its history.",
            ));
        }
        digest.update(&serde_json::to_vec(&row?)?);
    }
    Ok(digest.finalize().to_hex().to_string())
}

fn cursor_offset(cursor: Option<&str>, snapshot: &str, total: usize) -> Result<usize, QueueError> {
    let Some(cursor) = cursor else { return Ok(0) };
    if cursor.len() > MAX_CURSOR_BYTES {
        return Err(QueueError::CursorInvalid);
    }
    let (identity, offset) = cursor
        .strip_prefix("v1:")
        .and_then(|suffix| suffix.split_once(':'))
        .ok_or(QueueError::CursorInvalid)?;
    let offset = offset
        .parse::<usize>()
        .map_err(|_| QueueError::CursorInvalid)?;
    if identity != snapshot || offset >= total {
        return Err(QueueError::CursorInvalid);
    }
    Ok(offset)
}

fn validate_limit(limit: u32) -> Result<(), QueueError> {
    if (1..=MAX_PAGE_SIZE).contains(&limit) {
        Ok(())
    } else {
        Err(QueueError::Invalid(
            "Queue page size must be between 1 and 128.",
        ))
    }
}

const fn corrupt_count() -> QueueError {
    QueueError::Corrupt("Invalid queue job count.")
}
