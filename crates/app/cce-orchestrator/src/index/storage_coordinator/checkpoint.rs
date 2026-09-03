//! Checkpoint queries.

use crate::error::OrchestratorError;

use super::StorageCoordinator;

impl StorageCoordinator {
    // ===== Checkpoint Management =====

    /// Get the latest incomplete checkpoint for recovery, filtered by operation type and root dir
    pub async fn get_latest_checkpoint(
        &self,
        operation_type: &str,
        root_dir: &str,
    ) -> Result<Option<cce_storage_sqlite::CheckpointRecord>, OrchestratorError> {
        use cce_storage_sqlite::CheckpointRepository;

        let Some(client) = self.metadata_store.as_deref() else {
            return Ok(None);
        };
        let conn = client
            .read_connection()
            .map_err(OrchestratorError::Storage)?;
        CheckpointRepository::get_latest_incomplete_by_type(
            &conn,
            self.project_id,
            operation_type,
            root_dir,
        )
        .map_err(OrchestratorError::Storage)
    }
}
