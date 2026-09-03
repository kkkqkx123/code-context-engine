use rusqlite::Connection;

use cce_types::StorageError;

use crate::migration;

pub mod checkpoint;
pub mod indexes;
pub mod tables;

pub fn create_all(conn: &Connection) -> Result<(), StorageError> {
    let fresh_database = is_fresh_database(conn)?;

    tables::create_all(conn)?;
    checkpoint::create_all(conn)?;
    indexes::create_all(conn)?;

    if fresh_database {
        conn.pragma_update(None, "user_version", migration::LATEST_SCHEMA_VERSION)
            .map_err(|error| {
                StorageError::Table(format!("Failed to set schema version: {error}"))
            })?;
    } else {
        migration::run(conn)?;
    }
    Ok(())
}

fn is_fresh_database(conn: &Connection) -> Result<bool, StorageError> {
    let has_user_tables: bool = conn
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM sqlite_master
                WHERE type IN ('table', 'view') AND name NOT LIKE 'sqlite_%'
            )",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::Table(format!("Failed to inspect schema: {error}")))?;
    Ok(!has_user_tables)
}
