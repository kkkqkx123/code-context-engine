//! Telemetry table: metrics_aggregated.

use rusqlite::Connection;

use cce_types::StorageError;

pub fn create_tables(conn: &Connection) -> Result<(), StorageError> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS metrics_aggregated (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp DATETIME NOT NULL,
            metric_name TEXT NOT NULL,
            metric_type TEXT NOT NULL DEFAULT 'counter',
            labels_json TEXT,
            count INTEGER NOT NULL,
            avg REAL,
            median REAL,
            max REAL,
            p90 REAL,
            p99 REAL,
            project_id INTEGER,
            operation_type TEXT,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
        )",
        [],
    )
    .map_err(|e| {
        StorageError::Table(format!("Failed to create metrics_aggregated table: {}", e))
    })?;

    Ok(())
}
