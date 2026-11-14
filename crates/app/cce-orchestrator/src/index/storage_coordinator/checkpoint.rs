//! Checkpoint queries.

use cce_storage_common::RelationStorage;

use crate::error::OrchestratorError;

use super::StorageCoordinator;

impl StorageCoordinator {
    // ===== Checkpoint Management =====

    /// Get the latest incomplete checkpoint for recovery, filtered by operation type and root dir
    pub async fn get_latest_checkpoint(
        &self,
        operation_type: &str,
        root_dir: &str,
    ) -> Result<Option<cce_storage_common::metadb::CheckpointRecord>, OrchestratorError> {
        let Some(store) = self.metadata_store.as_ref() else {
            return Ok(None);
        };
        store
            .checkpoint_latest_incomplete(self.project_id, operation_type, root_dir)
            .await
            .map_err(OrchestratorError::Storage)
    }
}
