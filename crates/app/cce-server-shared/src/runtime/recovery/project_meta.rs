use cce_storage_metadb_sqlite::SqliteClient;
use cce_types::StorageError;

/// Per-project metadata management
#[derive(Debug, Clone)]
pub struct ProjectMeta {
    pub project_id: i64,
    pub epoch: i64,
    pub batch_id: i64,
    pub active_epoch: i64,
    pub active_relation_epoch: i64,
}

impl ProjectMeta {
    /// Load metadata for a specific project from project_meta table
    pub fn load(sqlite: &SqliteClient, project_id: i64) -> Result<Self, StorageError> {
        let conn = sqlite.read_connection()?;

        let epoch = conn
            .query_row(
                "SELECT value FROM project_meta WHERE project_id = ?1 AND key = 'epoch'",
                [project_id],
                |row| row.get::<_, String>(0),
            )
            .unwrap_or_else(|_| "0".to_string())
            .parse::<i64>()
            .unwrap_or(0);

        let batch_id = conn
            .query_row(
                "SELECT value FROM project_meta WHERE project_id = ?1 AND key = 'batch_id'",
                [project_id],
                |row| row.get::<_, String>(0),
            )
            .unwrap_or_else(|_| "0".to_string())
            .parse::<i64>()
            .unwrap_or(0);

        let active_epoch = conn
            .query_row(
                "SELECT value FROM project_meta WHERE project_id = ?1 AND key = 'active_epoch'",
                [project_id],
                |row| row.get::<_, String>(0),
            )
            .unwrap_or_else(|_| "0".to_string())
            .parse::<i64>()
            .unwrap_or(0);

        let active_relation_epoch = conn
            .query_row(
                "SELECT value FROM project_meta WHERE project_id = ?1 AND key = 'active_relation_epoch'",
                [project_id],
                |row| row.get::<_, String>(0),
            )
            .unwrap_or_else(|_| "0".to_string())
            .parse::<i64>()
            .unwrap_or(0);

        Ok(ProjectMeta {
            project_id,
            epoch,
            batch_id,
            active_epoch,
            active_relation_epoch,
        })
    }

    /// Initialize metadata for a new project
    pub fn init_for_project(sqlite: &SqliteClient, project_id: i64) -> Result<(), StorageError> {
        let conn = sqlite.write_connection()?;
        let now = chrono::Utc::now().timestamp();

        conn.execute(
            "INSERT OR IGNORE INTO project_meta (project_id, key, value, created_at, updated_at)
             VALUES (?1, 'epoch', '0', ?2, ?2)",
            rusqlite::params![project_id, now],
        )
        .map_err(|e| {
            StorageError::Query(format!("Failed to initialize project_meta epoch: {}", e))
        })?;

        conn.execute(
            "INSERT OR IGNORE INTO project_meta (project_id, key, value, created_at, updated_at)
             VALUES (?1, 'batch_id', '0', ?2, ?2)",
            rusqlite::params![project_id, now],
        )
        .map_err(|e| {
            StorageError::Query(format!("Failed to initialize project_meta batch_id: {}", e))
        })?;

        conn.execute(
            "INSERT OR IGNORE INTO project_meta (project_id, key, value, created_at, updated_at)
             VALUES (?1, 'active_epoch', '0', ?2, ?2)",
            rusqlite::params![project_id, now],
        )
        .map_err(|e| {
            StorageError::Query(format!(
                "Failed to initialize project_meta active_epoch: {}",
                e
            ))
        })?;

        conn.execute(
            "INSERT OR IGNORE INTO project_meta (project_id, key, value, created_at, updated_at)
             VALUES (?1, 'active_relation_epoch', '0', ?2, ?2)",
            rusqlite::params![project_id, now],
        )
        .map_err(|e| {
            StorageError::Query(format!(
                "Failed to initialize project_meta active_relation_epoch: {}",
                e
            ))
        })?;

        conn.execute(
            "INSERT OR IGNORE INTO project_meta (project_id, key, value, created_at, updated_at)
             VALUES (?1, 'epoch_ready', '0', ?2, ?2)",
            rusqlite::params![project_id, now],
        )
        .map_err(|e| {
            StorageError::Query(format!(
                "Failed to initialize project_meta epoch_ready: {}",
                e
            ))
        })?;

        Ok(())
    }

    /// Update epoch for project
    pub fn update_epoch(
        sqlite: &SqliteClient,
        project_id: i64,
        epoch: i64,
    ) -> Result<(), StorageError> {
        let conn = sqlite.write_connection()?;
        let now = chrono::Utc::now().timestamp();

        conn.execute(
            "UPDATE project_meta SET value = ?1, updated_at = ?2
             WHERE project_id = ?3 AND key = 'epoch'",
            rusqlite::params![epoch.to_string(), now, project_id],
        )
        .map_err(|e| StorageError::Query(format!("Failed to update epoch: {}", e)))?;

        Ok(())
    }

    /// Update batch_id for project
    pub fn update_batch_id(
        sqlite: &SqliteClient,
        project_id: i64,
        batch_id: i64,
    ) -> Result<(), StorageError> {
        let conn = sqlite.write_connection()?;
        let now = chrono::Utc::now().timestamp();

        conn.execute(
            "UPDATE project_meta SET value = ?1, updated_at = ?2
             WHERE project_id = ?3 AND key = 'batch_id'",
            rusqlite::params![batch_id.to_string(), now, project_id],
        )
        .map_err(|e| StorageError::Query(format!("Failed to update batch_id: {}", e)))?;

        Ok(())
    }

    /// Update active_epoch for project (atomic CAS)
    ///
    /// Uses compare-and-swap to ensure epochs only increase monotonically.
    /// Old epochs cannot overwrite newer ones, preventing race conditions
    /// between concurrent operations.
    pub fn update_active_epoch(
        sqlite: &SqliteClient,
        project_id: i64,
        active_epoch: i64,
    ) -> Result<(), StorageError> {
        let conn = sqlite.write_connection()?;
        let now = chrono::Utc::now().timestamp();

        let rows = conn
            .execute(
                "UPDATE project_meta SET value = ?1, updated_at = ?2
                 WHERE project_id = ?3 AND key = 'active_epoch'
                 AND CAST(value AS INTEGER) < ?1",
                rusqlite::params![active_epoch.to_string(), now, project_id],
            )
            .map_err(|e| StorageError::Query(format!("Failed to update active_epoch: {}", e)))?;

        if rows == 0 {
            tracing::warn!(
                project_id,
                active_epoch,
                "Failed to update active_epoch: current epoch is equal or newer"
            );
        }

        Ok(())
    }
}
