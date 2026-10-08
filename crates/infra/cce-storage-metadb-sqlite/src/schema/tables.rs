//! Table creation orchestration.
//!
//! This module delegates table creation to domain-specific sub-modules.

use rusqlite::Connection;

use cce_types::StorageError;

mod core_tables;
mod detail_tables;
mod relation_tables;
mod telemetry_tables;

pub fn create_all(conn: &Connection) -> Result<(), StorageError> {
    core_tables::create_tables(conn)?;
    detail_tables::create_tables(conn)?;
    relation_tables::create_tables(conn)?;
    telemetry_tables::create_tables(conn)?;
    Ok(())
}
