//! Short BEGIN IMMEDIATE transactions serialize all limit checks and durable mutations.
use super::{
    Error, Result,
    state::{FORMAT_VERSION, State},
};
use rusqlite::{Connection, OpenFlags, TransactionBehavior};
use std::path::Path;

pub const DATABASE_NAME: &str = "accounting.sqlite";
const MAX_STATE_BYTES: i32 = 64 * 1024 * 1024;
const BUSY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
const SCHEMA: &str = include_str!("schema.sql");

pub fn initialize(directory: &Path, timezone: &str) -> Result<()> {
    let state = State::empty(timezone)?;
    let parent = directory.parent().ok_or(Error::Missing)?;
    std::fs::create_dir_all(parent).map_err(|_| Error::Corrupt)?;
    std::fs::create_dir(directory).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            Error::Exists
        } else {
            Error::Corrupt
        }
    })?;
    let mut connection = connect(
        directory,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
    )?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| sql_error(&error))?;
    transaction
        .execute_batch(SCHEMA)
        .map_err(|error| sql_error(&error))?;
    transaction
        .execute(include_str!("insert.sql"), [encode(&state)?])
        .map_err(|error| sql_error(&error))?;
    transaction.commit().map_err(|error| sql_error(&error))
}

pub fn read(directory: &Path) -> Result<State> {
    let connection = existing(directory, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    load(&connection)
}

/// A consistent copy of the whole database at `target`, which must not exist yet. The caller holds
/// the barrier exclusively, so no transaction is in progress.
pub fn snapshot(directory: &Path, target: &Path) -> Result<()> {
    let target = target
        .to_str()
        .ok_or(Error::Backup("Backup paths must be valid UTF-8."))?;
    let connection = existing(directory, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    connection
        .execute("VACUUM INTO ?1", [target])
        .map_err(|error| sql_error(&error))?;
    Ok(())
}

pub fn transaction<T>(
    directory: &Path,
    now: jiff::Timestamp,
    mutation: impl FnOnce(State) -> Result<(State, T)>,
) -> Result<T> {
    let mut connection = existing(directory, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| sql_error(&error))?;
    let mut state = load(&transaction)?;
    state.last_seen = state.last_seen.max(now);
    let (state, result) = apply_mutation(state, mutation);
    state.validate()?;
    transaction
        .execute(include_str!("update.sql"), [encode(&state)?])
        .map_err(|error| sql_error(&error))?;
    transaction.commit().map_err(|error| sql_error(&error))?;
    result
}

fn existing(directory: &Path, flags: OpenFlags) -> Result<Connection> {
    let metadata = std::fs::symlink_metadata(directory).map_err(|_| Error::Missing)?;
    if !metadata.is_dir() {
        return Err(Error::Corrupt);
    }
    let path = directory.join(DATABASE_NAME);
    let file = std::fs::symlink_metadata(path).map_err(|_| Error::Missing)?;
    if !file.is_file() {
        return Err(Error::Corrupt);
    }
    connect(directory, flags)
}

fn connect(directory: &Path, flags: OpenFlags) -> Result<Connection> {
    let connection = Connection::open_with_flags(directory.join(DATABASE_NAME), flags)
        .map_err(|error| sql_error(&error))?;
    connection
        .busy_timeout(BUSY_TIMEOUT)
        .map_err(|error| sql_error(&error))?;
    connection
        .set_limit(
            rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH,
            MAX_STATE_BYTES,
        )
        .map_err(|error| sql_error(&error))?;
    if flags.contains(OpenFlags::SQLITE_OPEN_READ_WRITE) {
        connection
            .pragma_update(Option::<&str>::default(), "synchronous", "FULL")
            .map_err(|error| sql_error(&error))?;
    }
    Ok(connection)
}

fn load(connection: &Connection) -> Result<State> {
    let version: u32 = connection
        .pragma_query_value(Option::<&str>::default(), "user_version", |row| row.get(0))
        .map_err(|error| sql_error(&error))?;
    if version != FORMAT_VERSION {
        return Err(Error::Corrupt);
    }
    let contents: String = connection
        .query_row(include_str!("select.sql"), [], |row| row.get(0))
        .map_err(|error| sql_error(&error))?;
    let state: State = serde_json::from_str(&contents).map_err(|_| Error::Corrupt)?;
    state.validate()?;
    Ok(state)
}

fn encode(state: &State) -> Result<String> {
    let contents = serde_json::to_string(state).map_err(|_| Error::Corrupt)?;
    if contents.len() > MAX_STATE_BYTES as usize {
        return Err(Error::Limit);
    }
    Ok(contents)
}

fn sql_error(error: &rusqlite::Error) -> Error {
    match error.sqlite_error_code() {
        Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked) => {
            Error::Busy
        }
        _ => Error::Corrupt,
    }
}

// Rejected authority must not let a later backward clock revive an expired grant.
fn apply_mutation<T>(
    state: State,
    mutation: impl FnOnce(State) -> Result<(State, T)>,
) -> (State, Result<T>) {
    let clock_checkpoint = state.clone();
    match mutation(state) {
        Ok((state, result)) => (state, Ok(result)),
        Err(error) => (clock_checkpoint, Err(error)),
    }
}
