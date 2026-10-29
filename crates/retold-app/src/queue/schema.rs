//! Read compatibility checks never install schemas or initialize databases.
use rusqlite::Connection;

use super::QueueError;

const TABLE_EXISTS_SQL: &str = include_str!("has_table.sql");

pub(super) fn has_table(connection: &Connection, table: &str) -> Result<bool, QueueError> {
    connection
        .query_row(TABLE_EXISTS_SQL, [table], |row| row.get(0))
        .map_err(QueueError::from)
}
