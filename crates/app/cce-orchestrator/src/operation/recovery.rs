//! Recovery manager for resuming interrupted indexing operations
//!
//! Handles:
//! - Loading saved checkpoint state
//! - Validating file list consistency
//! - Determining recovery starting point
//! - Listing files that need reprocessing

use std::sync::Arc;
use tracing::{debug, info, trace, warn};

use crate::index::FileIndexer;

use super::checkpoint::CheckpointManager;
use cce_types::StorageError;
/// Recovery plan for resuming an interrupted operation
#[derive(Debug, Clone)]
pub struct RecoveryPlan {
    /// Operation ID to resume
    pub operation_id: String,
    /// Starting batch index
    pub start_batch: u32,
    /// Total number of batches
    pub total_batches: u32,
    /// Files that need processing (by batch and file)
    pub files_to_process: Vec<(u32, String)>, // (batch_index, file_path)
}

/// Recovery manager for handling interrupted operations
pub struct RecoveryManager {
    file_indexer: Arc<FileIndexer>,
    checkpoint_manager: Arc<CheckpointManager>,
}

impl RecoveryManager {
    /// Create a new recovery manager
    pub fn new(file_indexer: Arc<FileIndexer>, checkpoint_manager: Arc<CheckpointManager>) -> Self {
        Self {
            file_indexer,
            checkpoint_manager,
        }
    }

    /// Recover from an interrupted operation
    ///
    /// This will:
    /// 1. Load the saved checkpoint
    /// 2. Validate file list consistency
    /// 3. Determine the recovery starting point
    /// 4. List all files that need reprocessing
    pub async fn recover_operation(
        &self,
        operation_id: &str,
    ) -> Result<Option<RecoveryPlan>, StorageError> {
        info!(
            operation_id = operation_id,
            "Attempting to recover interrupted operation"
        );

        // 1. Load checkpoint
        let checkpoint = self.checkpoint_manager.get_checkpoint(operation_id).await?;

        let checkpoint = match checkpoint {
            Some(cp) => cp,
            None => {
                info!(
                    operation_id = operation_id,
                    "No checkpoint found, starting fresh"
                );
                return Ok(None);
            }
        };

        // 2. Validate file list consistency
        let start_batch = checkpoint.current_batch_index;
        let boundaries = self
            .load_recovery_boundaries(&checkpoint, start_batch)
            .await?;
        match self
            .file_indexer
            .validate_checkpoint(&checkpoint, &boundaries)
        {
            Ok(_) => {
                debug!(operation_id = operation_id, "Checkpoint validation passed");
            }
            Err(e) => {
                warn!(
                    operation_id = operation_id,
                    error = %e,
                    "Checkpoint validation failed, file list may have changed"
                );
                return Ok(None); // Can't recover, start fresh
            }
        }

        // 3. Determine recovery starting point
        let start_batch = checkpoint.current_batch_index;
        let total_batches = checkpoint.total_files as u32 / checkpoint.batch_size
            + if checkpoint.total_files as u32 % checkpoint.batch_size != 0 {
                1
            } else {
                0
            };

        info!(
            operation_id = operation_id,
            start_batch = start_batch,
            total_batches = total_batches,
            "Recovery plan: resuming from batch"
        );

        // 4. List files to process
        // Query the database for partially processed files that need reprocessing
        let mut files_to_process = Vec::new();

        for batch_idx in start_batch..total_batches {
            match self
                .checkpoint_manager
                .get_batch_files(&checkpoint.operation_id, batch_idx)
                .await
            {
                Ok(batch_files) => {
                    for file_checkpoint in batch_files {
                        files_to_process.push((batch_idx, file_checkpoint.file_path));
                    }
                }
                Err(e) => {
                    trace!(
                        batch_idx = batch_idx,
                        error = %e,
                        "Failed to get batch files from checkpoint, attempting filesystem lookup"
                    );

                    // Fallback to filesystem-based batching if database lookup fails
                    if let Ok(batch_files) = self.file_indexer.get_batch(batch_idx as usize) {
                        for file in batch_files {
                            files_to_process
                                .push((batch_idx, file.path.to_string_lossy().to_string()));
                        }
                    }
                }
            }
        }

        info!(
            operation_id = operation_id,
            files_count = files_to_process.len(),
            "Recovery plan prepared"
        );

        Ok(Some(RecoveryPlan {
            operation_id: checkpoint.operation_id,
            start_batch,
            total_batches,
            files_to_process,
        }))
    }

    /// Load the persisted batch boundary records needed to validate the
    /// resume-start batch and its predecessor.
    async fn load_recovery_boundaries(
        &self,
        checkpoint: &cce_storage_sqlite::types::CheckpointRecord,
        start_batch: u32,
    ) -> Result<Vec<cce_storage_sqlite::types::BatchCheckpointRecord>, StorageError> {
        let mut boundaries = Vec::new();
        for batch_index in [start_batch, start_batch.saturating_sub(1)] {
            if let Some(record) = self
                .checkpoint_manager
                .get_batch_checkpoint(&checkpoint.operation_id, batch_index)
                .await?
            {
                boundaries.push(record);
            }
        }
        Ok(boundaries)
    }

    /// Get the recovery status for an operation
    pub async fn get_recovery_status(
        &self,
        operation_id: &str,
    ) -> Result<Option<String>, StorageError> {
        let checkpoint = self.checkpoint_manager.get_checkpoint(operation_id).await?;

        Ok(checkpoint.map(|cp| {
            format!(
                "Operation: {}, Status: {}, Current Batch: {}/{}",
                cp.operation_id, cp.status, cp.current_batch_index, cp.total_files
            )
        }))
    }

    /// Get failed files for an operation
    pub async fn get_failed_files(
        &self,
        operation_id: &str,
    ) -> Result<Vec<(String, String)>, StorageError> {
        self.checkpoint_manager.get_failed_files(operation_id).await
    }

    /// Validate checkpoint and detect corruption
    pub async fn validate_checkpoint_integrity(
        &self,
        operation_id: &str,
    ) -> Result<bool, StorageError> {
        info!(
            operation_id = operation_id,
            "Validating checkpoint integrity"
        );

        let checkpoint = self.checkpoint_manager.get_checkpoint(operation_id).await?;

        match checkpoint {
            Some(cp) => {
                let op_kind: cce_types::OperationKind = match cp.operation_type.parse() {
                    Ok(kind) => kind,
                    Err(error) => {
                        warn!(
                            operation_id = operation_id,
                            error = %error,
                            "Checkpoint has an invalid operation type"
                        );
                        return Ok(false);
                    }
                };
                match self
                    .checkpoint_manager
                    .validate_and_recover_checkpoint(operation_id, op_kind, &cp.root_dir)
                    .await
                {
                    Ok(Some(_)) => {
                        info!(operation_id = operation_id, "Checkpoint validation passed");
                        Ok(true)
                    }
                    Ok(None) => {
                        warn!(
                            operation_id = operation_id,
                            "Checkpoint validation failed or corrupted"
                        );
                        Ok(false)
                    }
                    Err(e) => {
                        warn!(
                            operation_id = operation_id,
                            error = %e,
                            "Checkpoint validation error"
                        );
                        Err(e)
                    }
                }
            }
            None => {
                info!(operation_id = operation_id, "No checkpoint found");
                Ok(false)
            }
        }
    }

    /// Cleanup all artifacts associated with a completed operation
    pub async fn cleanup_operation_artifacts(
        &self,
        operation_id: &str,
    ) -> Result<(), StorageError> {
        // Delete all checkpoint files for this operation
        self.checkpoint_manager
            .delete_checkpoint_files_by_operation_id(operation_id)
            .await?;

        // Delete all checkpoint batches for this operation
        self.checkpoint_manager
            .delete_checkpoint_batches_by_operation_id(operation_id)
            .await?;

        // Delete all work unit checkpoints for this operation
        self.checkpoint_manager
            .delete_work_units_by_operation_id(operation_id)
            .await?;

        info!(
            operation_id = %operation_id,
            "Cleaned up operation artifacts"
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recovery_plan_creation() {
        let plan = RecoveryPlan {
            operation_id: "test_op".to_string(),
            start_batch: 5,
            total_batches: 10,
            files_to_process: vec![
                (5, "/path/to/file1.rs".to_string()),
                (5, "/path/to/file2.rs".to_string()),
            ],
        };

        assert_eq!(plan.operation_id, "test_op");
        assert_eq!(plan.start_batch, 5);
        assert_eq!(plan.total_batches, 10);
        assert_eq!(plan.files_to_process.len(), 2);
    }
}
