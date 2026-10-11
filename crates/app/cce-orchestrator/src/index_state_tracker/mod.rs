//! Index state tracker for unified indexing operations
//!
//! Provides centralized tracking of index states across all files and modules,
//! supporting full index, hot update, and incremental update operations.
//! Features independent retries per module and version control to prevent
//! old updates from overwriting newer ones.

use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use cce_storage_metadb_sqlite::SqliteClient;

use crate::index_state::{FileUpdateState, StateTrackerError};

mod dead_letters;
mod file_states;
mod operations;
mod phases;

/// Update state tracker statistics
#[derive(Debug, Clone)]
pub struct UpdateStateStats {
    /// Total number of files being tracked
    pub total_files: usize,
    /// Number of files that are fully updated
    pub fully_updated: usize,
}

/// Update state tracker manages states for all files being updated
#[derive(Debug, Clone)]
pub struct UpdateStateTracker {
    /// Project ID for multi-project isolation
    project_id: i64,
    /// File path -> Update state mapping
    states: Arc<RwLock<HashMap<String, FileUpdateState>>>,
    /// Global version counter for generating unique version numbers
    version_counter: Arc<RwLock<u64>>,
    /// Operation ID for the current indexing operation
    current_operation_id: Arc<RwLock<Option<String>>>,
    /// Optional durable file-level state projection.
    database: Arc<RwLock<Option<Arc<SqliteClient>>>>,
}

impl UpdateStateTracker {
    /// Create a new state tracker for a specific project
    pub fn new(project_id: i64) -> Self {
        Self {
            project_id,
            states: Arc::new(RwLock::new(HashMap::new())),
            version_counter: Arc::new(RwLock::new(0)),
            current_operation_id: Arc::new(RwLock::new(None)),
            database: Arc::new(RwLock::new(None)),
        }
    }

    /// Attach the checkpoint database used for the durable state projection.
    ///
    /// Binding is required for dead-letter and truncate state to survive a
    /// restart; a failure to acquire the write lock is reported so the caller
    /// can decide whether running with an in-memory-only projection is
    /// acceptable.
    pub fn set_database(&self, database: Arc<SqliteClient>) -> Result<(), StateTrackerError> {
        match self.database.try_write() {
            Ok(mut value) => {
                *value = Some(database);
                Ok(())
            }
            Err(error) => {
                tracing::error!(
                    %error,
                    "Failed to bind durable database to index state tracker"
                );
                Err(StateTrackerError::DatabaseBindBusy)
            }
        }
    }

    /// Restore file states for an operation after a process restart.
    pub async fn restore_operation(&self, operation_id: &str) {
        let database = self.database.read().await.clone();
        let Some(database) = database else { return };
        let (restored, max_version) =
            Self::load_projection(&database, self.project_id, operation_id);

        if restored.is_empty() {
            return;
        }
        *self.states.write().await = restored;
        *self.version_counter.write().await = max_version;
        *self.current_operation_id.write().await = Some(operation_id.to_string());
        tracing::info!(
            project_id = self.project_id,
            operation_id,
            max_version,
            "Restored durable file-level index state"
        );
    }

    fn load_projection(
        database: &Arc<SqliteClient>,
        project_id: i64,
        operation_id: &str,
    ) -> (HashMap<String, FileUpdateState>, u64) {
        let client = database.as_ref();
        let Ok(conn) = client.read_connection() else {
            return (HashMap::new(), 0);
        };
        let mut statement = match conn.prepare(
            "SELECT file_path, version, state_json
             FROM index_state_projection
             WHERE project_id = ?1 AND operation_id = ?2",
        ) {
            Ok(statement) => statement,
            Err(error) => {
                tracing::warn!(%error, "Failed to prepare durable index state recovery");
                return (HashMap::new(), 0);
            }
        };
        let rows = match statement.query_map(rusqlite::params![project_id, operation_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        }) {
            Ok(rows) => rows,
            Err(error) => {
                tracing::warn!(%error, "Failed to query durable index state recovery");
                return (HashMap::new(), 0);
            }
        };

        let mut restored = HashMap::new();
        let mut max_version = 0u64;
        for row in rows {
            let (path, raw_version, json) = match row {
                Ok(value) => value,
                Err(error) => {
                    tracing::warn!(%error, "Failed to read durable index state row");
                    continue;
                }
            };
            let Ok(version) = u64::try_from(raw_version) else {
                tracing::warn!(file = %path, raw_version, "Ignoring invalid durable index state version");
                continue;
            };
            match serde_json::from_str::<FileUpdateState>(&json) {
                Ok(state) if state.project_id == project_id && state.version == version => {
                    max_version = max_version.max(version);
                    restored.insert(path, state);
                }
                Ok(_) | Err(_) => {
                    tracing::warn!(file = %path, "Ignoring incompatible durable index state row");
                }
            }
        }
        (restored, max_version)
    }

    pub(crate) async fn persist_state(&self, state: &FileUpdateState) {
        let database = self.database.read().await.clone();
        let Some(database) = database else { return };
        let client = database.as_ref();
        let operation_id = self
            .current_operation_id
            .read()
            .await
            .clone()
            .unwrap_or_else(|| "state".to_string());
        let state_json = match serde_json::to_string(state) {
            Ok(value) => value,
            Err(error) => {
                tracing::warn!(%error, file = %state.file_path, "Failed to serialize index state");
                return;
            }
        };
        let Ok(version) = i64::try_from(state.version) else {
            tracing::warn!(file = %state.file_path, "Index state version exceeds SQLite range");
            return;
        };
        let result = client.with_transaction(|tx| {
            tx.execute(
                "INSERT INTO index_state_projection
                    (project_id, operation_id, file_path, version, state_json, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(project_id, operation_id, file_path) DO UPDATE SET
                    version = excluded.version, state_json = excluded.state_json,
                    updated_at = excluded.updated_at
                 WHERE excluded.version >= index_state_projection.version",
                rusqlite::params![
                    self.project_id,
                    operation_id,
                    state.file_path,
                    version,
                    state_json,
                    Utc::now().to_rfc3339(),
                ],
            )
            .map(|_| ())
            .map_err(|error| {
                cce_types::StorageError::insert("index_state_projection", error.to_string())
            })
        });
        if let Err(error) = result {
            tracing::warn!(%error, file = %state.file_path, "Failed to persist index state");
        }
    }

    pub(crate) async fn delete_persisted_state(&self, file_path: &str) {
        let database = self.database.read().await.clone();
        let Some(database) = database else { return };
        let client = database.as_ref();
        let operation_id = self
            .current_operation_id
            .read()
            .await
            .clone()
            .unwrap_or_else(|| "state".to_string());
        let result = client.with_transaction(|tx| {
            tx.execute(
                "DELETE FROM index_state_projection
                 WHERE project_id = ?1 AND operation_id = ?2 AND file_path = ?3",
                rusqlite::params![self.project_id, operation_id, file_path],
            )
            .map(|_| ())
            .map_err(|error| {
                cce_types::StorageError::delete("index_state_projection", error.to_string())
            })
        });
        if let Err(error) = result {
            tracing::warn!(%error, file = %file_path, "Failed to delete persisted index state");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index_state::{ModuleType, ModuleUpdateState};
    use crate::operation::CheckpointManager;
    use crate::operation::checkpoint::CreateCheckpointParams;
    use cce_types::OperationKind;
    use std::path::{Path, PathBuf};

    #[tokio::test]
    async fn durable_projection_restores_file_state_after_restart() {
        let database = Arc::new(SqliteClient::in_memory().expect("in-memory database"));
        let checkpoint_manager = CheckpointManager::new_for_project(1, database.clone());
        checkpoint_manager
            .create_checkpoint(CreateCheckpointParams {
                operation_id: "full-operation",
                operation_type: OperationKind::FullIndex,
                root_dir: "/tmp/project",
                total_files: 1,
                batch_size: 1,
                file_list_hash: "files",
            })
            .await
            .expect("checkpoint should be created");

        let tracker = UpdateStateTracker::new(1);
        tracker
            .set_database(database.clone())
            .expect("database binding");
        tracker
            .start_full_index_for_operation(
                "full-operation".to_string(),
                1,
                1,
                "/tmp/project".to_string(),
            )
            .await;
        tracker
            .create_full_index_batch(
                &[PathBuf::from("src/lib.rs")],
                0,
                1,
                1,
                "/tmp/project".to_string(),
            )
            .await;
        tracker
            .mark_success(Path::new("src/lib.rs"), ModuleType::Relation)
            .await
            .expect("state should update");

        let restored = UpdateStateTracker::new(1);
        restored.set_database(database).expect("database binding");
        restored.restore_operation("full-operation").await;
        let state = restored
            .get_state(Path::new("src/lib.rs"))
            .await
            .expect("state should be restored");
        assert_eq!(state.version, 1);
        assert!(matches!(
            state.get_module_state(ModuleType::Relation).state,
            ModuleUpdateState::Success
        ));
    }
}
