use rusqlite::Connection;

use cce_types::StorageError;

/// The schema version represented by the table definitions in `schema/tables`.
pub(crate) const LATEST_SCHEMA_VERSION: i64 = 2;

/// Reject databases written by an incompatible schema state.
pub fn run(conn: &Connection) -> Result<(), StorageError> {
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|error| StorageError::Table(format!("Failed to read schema version: {error}")))?;
    if version != LATEST_SCHEMA_VERSION {
        let message = format!(
            "Incompatible database schema version {version} (expected {LATEST_SCHEMA_VERSION}); \
             run a full rebuild with force_reindex=true or delete the data directory"
        );
        tracing::error!(%message, "Refusing to open outdated index database");
        return Err(StorageError::Table(message));
    }
    Ok(())
}
