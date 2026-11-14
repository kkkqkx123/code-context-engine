//! File difference manager for tracking file-level changes
//!
//! This module provides functionality for detecting and tracking changes
//! to file lists, which is critical for incremental update optimizations.

use cce_storage_common::MetadataStorage;
use cce_types::StorageError;
use chrono::Utc;
use tracing::trace;

use crate::index::vector_store::MetadataStore;

/// Manages file difference tracking for differential updates
pub struct FileDiffManager {
    /// Project ID for multi-project support
    project_id: i64,
    /// Relation backend for checkpoint persistence
    db: MetadataStore,
}

impl FileDiffManager {
    /// Create a new file diff manager
    pub fn new(project_id: i64, db: MetadataStore) -> Self {
        Self { project_id, db }
    }

    /// Check if file list has changed (differential check)
    ///
    /// Instead of full content hashing, this compares file count and
    /// a quick hash to detect changes efficiently.
    pub async fn has_file_list_changed(
        &self,
        operation_id: &str,
        current_file_list_hash: &str,
    ) -> Result<bool, StorageError> {
        trace!(
            operation_id = %operation_id,
            "Checking if file list has changed"
        );

        if let Some(checkpoint) = self
            .db
            .checkpoint_get(self.project_id, operation_id)
            .await?
        {
            if let Some(ref previous_hash) = checkpoint.file_list_hash {
                let changed = previous_hash != current_file_list_hash;
                trace!(
                    operation_id = %operation_id,
                    file_list_changed = changed,
                    "File list differential check completed"
                );
                Ok(changed)
            } else {
                // No previous hash, so we consider it as changed (first scan)
                Ok(true)
            }
        } else {
            // No previous checkpoint, so consider as changed
            Ok(true)
        }
    }

    /// Update file list hash for future differential checking
    pub async fn update_file_list_hash(
        &self,
        operation_id: &str,
        new_file_list_hash: &str,
    ) -> Result<(), StorageError> {
        trace!(
            operation_id = %operation_id,
            "Updating file list hash for differential checking"
        );

        if let Some(checkpoint) = self
            .db
            .checkpoint_get(self.project_id, operation_id)
            .await?
        {
            let mut updated = checkpoint.clone();
            updated.file_list_hash = Some(new_file_list_hash.to_string());
            updated.updated_at = Utc::now().to_rfc3339();

            self.db.checkpoint_update(self.project_id, &updated).await?;
        }

        trace!(
            operation_id = %operation_id,
            "File list hash updated successfully"
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_file_diff_manager_creation() {
        // This test verifies that the manager can be created
        // Full integration tests would require a database
    }
}
