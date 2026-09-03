use crate::hot_update::error::{HotUpdateError, Result};
use cce_storage_sqlite::SqliteClient;
use std::sync::Arc;

/// Create a temporary database connection for the change detector.
///
/// Prefers an in-memory database and falls back to a temp file. The error is
/// propagated instead of panicking when neither can be created.
pub(crate) fn create_temp_db() -> Result<Arc<SqliteClient>> {
    match SqliteClient::in_memory() {
        Ok(db) => Ok(Arc::new(db)),
        Err(_) => {
            tracing::warn!("Failed to create in-memory database, falling back to temp file");
            SqliteClient::new_in_temp().map(Arc::new).map_err(|e| {
                HotUpdateError::config(format!("Cannot create database for change detector: {e}"))
            })
        }
    }
}
