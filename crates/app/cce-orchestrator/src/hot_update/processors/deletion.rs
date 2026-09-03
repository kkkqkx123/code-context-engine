//! Shared deletion handling for update processors
//!
//! This module extracts the common file-deletion pattern that was duplicated
//! across EmbeddingUpdateProcessor, Bm25UpdateProcessor, SummaryUpdateProcessor,
//! and NlDocumentUpdateProcessor.

use std::path::Path;

use crate::hot_update::FileChangeType;
use crate::hot_update::error::{HotUpdateError, Result};
use crate::index::StorageCoordinator;
use crate::operation::ModuleFailure;

/// Remove a file from a specific storage backend.
///
/// Each processor calls this with its corresponding storage method to handle
/// deleted files consistently.
pub async fn remove_file_from_storage(
    storage: &StorageCoordinator,
    file_path: &Path,
    module_name: &str,
) -> Result<()> {
    match module_name {
        "embedding" => storage
            .remove_file_from_vectors(file_path)
            .await
            .map_err(|e| HotUpdateError::embedding(e.to_string())),
        "bm25" => storage
            .remove_file_from_bm25(file_path)
            .await
            .map_err(|e| HotUpdateError::bm25(e.to_string())),
        "summary" => storage
            .remove_file_from_summary(file_path)
            .await
            .map_err(|e| HotUpdateError::summary(e.to_string())),
        _ => Err(HotUpdateError::hot_update(format!(
            "Unsupported module: {}",
            module_name
        ))),
    }
}

/// Process deleted files from a batch result and collect failures.
///
/// Returns the count of successfully processed deletions.
pub async fn process_deletions(
    storage: &StorageCoordinator,
    batch_result: &crate::hot_update::BatchChangeResult,
    module_name: &str,
    failed_modules: &mut Vec<ModuleFailure>,
) -> usize {
    let mut processed = 0;

    for file_change in &batch_result.file_changes {
        if file_change.change_type == FileChangeType::Deleted {
            let path = &file_change.path;
            match remove_file_from_storage(storage, path, module_name).await {
                Ok(_) => {
                    processed += 1;
                }
                Err(e) => {
                    failed_modules.push(ModuleFailure {
                        file_path: path.to_string_lossy().to_string(),
                        module_name: module_name.to_string(),
                        error: e.to_string(),
                        retry_count: 0,
                        next_retry_time: None,
                    });
                    tracing::warn!(
                        file = %path.display(),
                        error = %e,
                        module = module_name,
                        "Failed to remove file from module"
                    );
                }
            }
        }
    }

    processed
}
