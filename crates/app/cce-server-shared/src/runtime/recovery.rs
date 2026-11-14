//! Startup recovery for data consistency and crash recovery
//!
//! This module handles per-project startup recovery:
//! 1. Loading project metadata (epoch, batch_id, active_epoch)
//! 2. Classifying files based on content hash and state
//! 3. Processing modified files (re-indexing)
//! 4. Processing incomplete files (external storage sync)
//! 5. Collecting relation index recovery information

use std::sync::Arc;

use cce_orchestrator::IndexOrchestrator;
use cce_orchestrator::index::{MetadataStore, in_memory_metadata_store};
use cce_storage_common::MetadataStorage;
use cce_types::StorageError;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use tracing::{debug, info, warn};

mod project_meta;

pub use project_meta::ProjectMeta;

/// File state classification for recovery
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileState {
    /// File content changed on disk (needs re-parsing)
    Modified,
    /// External storage incomplete (needs resync)
    Incomplete,
    /// File was removed from disk and its persisted artifacts must be deleted.
    Deleted,
    /// File is consistent
    Consistent,
}

/// File classification result
#[derive(Debug, Clone)]
pub struct FileClassification {
    pub path: String,
    pub state: FileState,
    pub epoch: i64,
    pub batch_id: i64,
}

/// Low-level recovery operations over the relation contract.
pub struct StartupRecoveryManager {
    store: MetadataStore,
}

impl StartupRecoveryManager {
    /// Create a new recovery manager
    pub fn new(store: MetadataStore) -> Self {
        Self { store }
    }

    /// Get reference to the relation store
    pub fn store(&self) -> &MetadataStore {
        &self.store
    }

    /// Classify all files in a project for recovery
    ///
    /// Uses content hash comparison to determine if a file needs re-parsing.
    /// Files with mismatched hashes are marked as Modified for re-indexing.
    /// batch_id is preserved for logging/tracking but not used for state classification.
    pub async fn classify_files(
        &self,
        project_id: i64,
        meta: &ProjectMeta,
    ) -> Result<Vec<FileClassification>, StorageError> {
        let entries = self
            .store
            .files_epoch_entries(project_id, meta.epoch)
            .await?;

        let classifications = entries
            .into_iter()
            .map(|entry| {
                let state = match Self::compute_file_hash(&entry.path) {
                    Ok(disk_hash) => match &entry.content_hash {
                        Some(stored) if disk_hash != *stored => {
                            debug!(
                                project_id = project_id,
                                path = %entry.path,
                                "File content mismatch detected"
                            );
                            FileState::Modified
                        }
                        Some(_) => FileState::Consistent,
                        None => FileState::Modified,
                    },
                    Err(StorageError::Io(error))
                        if error.0.kind() == std::io::ErrorKind::NotFound =>
                    {
                        FileState::Deleted
                    }
                    Err(e) => {
                        warn!(
                            project_id = project_id,
                            path = %entry.path,
                            error = %e,
                        "Failed to compute file hash, marking storage state incomplete"
                        );
                        FileState::Incomplete
                    }
                };

                FileClassification {
                    path: entry.path,
                    state,
                    epoch: meta.epoch,
                    batch_id: entry.batch_id,
                }
            })
            .collect();

        Ok(classifications)
    }

    /// Compute SHA256 hash of a file
    pub fn compute_file_hash(path: &str) -> Result<String, StorageError> {
        let content = fs::read(path).map_err(StorageError::from)?;
        let mut hasher = Sha256::new();
        hasher.update(&content);
        Ok(hex::encode(hasher.finalize()))
    }

    /// Mark file for re-parsing in SQLite
    pub async fn mark_file_for_reparse(
        &self,
        project_id: i64,
        file_path: &str,
        epoch: i64,
    ) -> Result<(), StorageError> {
        if !self
            .store
            .files_set_epoch(project_id, file_path, epoch)
            .await
            .map_err(|e| {
                StorageError::Query(format!(
                    "Failed to mark file '{file_path}' for re-parse: {e}"
                ))
            })?
        {
            return Err(StorageError::Query(format!(
                "Failed to mark file '{file_path}' for re-parse: no file row"
            )));
        }

        Ok(())
    }

    /// Find cached ParsedFile data from interrupted checkpoints
    ///
    /// Queries `checkpoint_file` for a file whose content hash matches the
    /// current disk content, allowing recovery to reuse previously parsed AST
    /// data instead of re-parsing from scratch.
    pub async fn find_cached_parsed_data(
        &self,
        project_id: i64,
        file_path: &str,
        disk_content_hash: &str,
    ) -> Result<Option<Vec<u8>>, StorageError> {
        self.store
            .checkpoint_cached_parsed_data(project_id, file_path, disk_content_hash)
            .await
    }

    /// Read chunks for a specific file at given epoch
    pub async fn read_chunks_for_file(
        &self,
        project_id: i64,
        file_path: &str,
        epoch: i64,
    ) -> Result<Vec<String>, StorageError> {
        self.store
            .chunks_by_file(project_id, file_path, epoch)
            .await
    }

    /// Collect relation index recovery information
    pub async fn collect_relation_info(
        &self,
        project_id: i64,
        meta: &ProjectMeta,
    ) -> Result<RelationIndexRecoveryInfo, StorageError> {
        // The legacy `relations` table no longer exists; relation counts now
        // come from the canonical snapshot of the active relation epoch.
        let entity_count = self
            .store
            .entities_count(project_id, meta.active_relation_epoch)
            .await
            .unwrap_or(0) as usize;
        let relation_count = self
            .store
            .snapshot_read(project_id, meta.active_relation_epoch)
            .await
            .map(|snapshot| snapshot.relations.len())
            .unwrap_or(0);

        debug!(
            project_id = project_id,
            epoch = meta.active_epoch,
            relation_epoch = meta.active_relation_epoch,
            entity_count = entity_count,
            relation_count = relation_count,
            "Collected relation index recovery info"
        );

        Ok(RelationIndexRecoveryInfo {
            project_id,
            active_epoch: meta.active_epoch,
            active_relation_epoch: meta.active_relation_epoch,
            entity_count,
            symbol_count: entity_count,
            relation_count,
        })
    }
}

/// Recovery information for RelationIndex
#[derive(Debug, Clone)]
pub struct RelationIndexRecoveryInfo {
    pub project_id: i64,
    pub active_epoch: i64,
    pub active_relation_epoch: i64,
    pub entity_count: usize,
    pub symbol_count: usize,
    pub relation_count: usize,
}

/// Business-level recovery coordinator
pub struct StartupRecoveryCoordinator {
    manager: StartupRecoveryManager,
}

impl StartupRecoveryCoordinator {
    /// Create a new recovery coordinator
    pub fn new(store: MetadataStore) -> Self {
        Self {
            manager: StartupRecoveryManager::new(store),
        }
    }

    /// Execute full recovery process for a project
    pub async fn recover_project(
        &self,
        project_id: i64,
        orchestrator: Option<Arc<tokio::sync::Mutex<IndexOrchestrator>>>,
    ) -> Result<RecoveryResult, StorageError> {
        info!(project_id, "Starting project recovery");

        let mut result = RecoveryResult {
            project_id,
            ..Default::default()
        };

        // Load project metadata
        let meta = ProjectMeta::load(self.manager.store(), project_id).await?;
        debug!(
            project_id = project_id,
            epoch = meta.epoch,
            batch_id = meta.batch_id,
            active_epoch = meta.active_epoch,
            "Loaded project metadata"
        );

        // Classify files
        let classifications = self.manager.classify_files(project_id, &meta).await?;
        result.files_classified = classifications.len();

        let reparse_files: Vec<_> = classifications
            .iter()
            .filter(|classification| classification.state == FileState::Modified)
            .cloned()
            .collect();
        let resync_files: Vec<_> = classifications
            .iter()
            .filter(|classification| classification.state == FileState::Incomplete)
            .cloned()
            .collect();
        let deleted_files: Vec<_> = classifications
            .into_iter()
            .filter(|classification| classification.state == FileState::Deleted)
            .collect();

        result.files_to_reparse = reparse_files.len();
        result.files_to_resync = resync_files.len();

        info!(
            project_id = project_id,
            files_classified = result.files_classified,
            files_to_reparse = result.files_to_reparse,
            files_to_resync = result.files_to_resync,
            "File classification completed"
        );

        // Process modified files
        if !reparse_files.is_empty() {
            result.files_reparsed = self
                .process_reparse_files(project_id, reparse_files, orchestrator.clone())
                .await?;
        }

        // Process incomplete files
        if !resync_files.is_empty() {
            result.files_resynced = self
                .process_resync_files(project_id, resync_files, orchestrator.clone())
                .await?;
        }

        if !deleted_files.is_empty() {
            result.files_deleted = self
                .process_deleted_files(project_id, deleted_files, orchestrator)
                .await?;
        }

        // Collect relation index info
        let relation_info = self
            .manager
            .collect_relation_info(project_id, &meta)
            .await?;
        result.entity_count = relation_info.entity_count;
        result.relation_count = relation_info.relation_count;

        info!(
            project_id = project_id,
            files_classified = result.files_classified,
            files_reparsed = result.files_reparsed,
            files_resynced = result.files_resynced,
            entities = result.entity_count,
            relations = result.relation_count,
            "Project recovery completed"
        );

        Ok(result)
    }

    /// Process files that need re-parsing
    ///
    /// Before re-parsing, checks for cached ParsedFile artifacts from
    /// interrupted operations. If a matching checkpoint entry exists with
    /// the same content hash, the file's parsed data is reused instead of
    /// re-parsing from disk.
    async fn process_reparse_files(
        &self,
        project_id: i64,
        reparse_files: Vec<FileClassification>,
        orchestrator: Option<Arc<tokio::sync::Mutex<IndexOrchestrator>>>,
    ) -> Result<usize, StorageError> {
        let mut processed = 0;

        for classification in reparse_files {
            if let Err(e) = self
                .manager
                .mark_file_for_reparse(project_id, &classification.path, classification.epoch)
                .await
            {
                warn!(
                    project_id = project_id,
                    file = %classification.path,
                    error = %e,
                    "Failed to mark file for re-parse"
                );
                continue;
            }

            processed += 1;

            // Check for cached parsed data from interrupted operations
            let has_cached = match StartupRecoveryManager::compute_file_hash(&classification.path) {
                Ok(hash) => self
                    .manager
                    .find_cached_parsed_data(project_id, &classification.path, &hash)
                    .await
                    .ok()
                    .flatten()
                    .is_some(),
                Err(_) => false,
            };

            if has_cached {
                debug!(
                    project_id = project_id,
                    file = %classification.path,
                    "Skipping re-parse, using cached ParsedFile from interrupted operation"
                );
                continue;
            }

            if let Some(ref orch) = orchestrator {
                match orch
                    .lock()
                    .await
                    .index_file(Path::new(&classification.path))
                    .await
                {
                    Ok(_) => {
                        debug!(
                            project_id = project_id,
                            file = %classification.path,
                            "Successfully re-indexed file"
                        );
                    }
                    Err(e) => {
                        warn!(
                            project_id = project_id,
                            file = %classification.path,
                            error = %e,
                            "Failed to re-index file, will retry on next startup"
                        );
                    }
                }
            } else {
                debug!(
                    project_id = project_id,
                    file = %classification.path,
                    "File marked for re-parse (orchestrator not available)"
                );
            }
        }

        Ok(processed)
    }

    /// Process files that need external storage resync
    ///
    /// For files that couldn't be read during classification (disk I/O errors,
    /// permission issues), attempt to re-read. If the file is now readable,
    /// re-index it. Otherwise, remove external storage artifacts to maintain
    /// consistency.
    async fn process_resync_files(
        &self,
        project_id: i64,
        resync_files: Vec<FileClassification>,
        orchestrator: Option<Arc<tokio::sync::Mutex<IndexOrchestrator>>>,
    ) -> Result<usize, StorageError> {
        let mut processed = 0;

        for classification in resync_files {
            let disk_hash = StartupRecoveryManager::compute_file_hash(&classification.path);
            match disk_hash {
                Ok(hash) => {
                    // File is now readable — re-index it
                    if let Some(ref orch) = orchestrator {
                        match orch
                            .lock()
                            .await
                            .index_file(Path::new(&classification.path))
                            .await
                        {
                            Ok(_) => {
                                debug!(
                                    project_id = project_id,
                                    file = %classification.path,
                                    "Successfully re-indexed previously incomplete file"
                                );
                                processed += 1;
                            }
                            Err(e) => {
                                warn!(
                                    project_id = project_id,
                                    file = %classification.path,
                                    error = %e,
                                    "Failed to re-index resync file"
                                );
                            }
                        }
                    } else {
                        debug!(
                            project_id = project_id,
                            file = %classification.path,
                            hash = %hash,
                            "File is readable but orchestrator not available for re-index"
                        );
                        processed += 1;
                    }
                }
                Err(e) => {
                    // File still unreadable — remove external artifacts
                    warn!(
                        project_id = project_id,
                        file = %classification.path,
                        error = %e,
                        "File still unreadable, removing external storage artifacts"
                    );
                    if let Some(ref orch) = orchestrator {
                        if let Err(remove_err) = orch
                            .lock()
                            .await
                            .remove_file(Path::new(&classification.path))
                            .await
                        {
                            warn!(
                                project_id = project_id,
                                file = %classification.path,
                                error = %remove_err,
                                "Failed to remove external artifacts for resync file"
                            );
                        }
                    }
                    processed += 1;
                }
            }
        }

        info!(
            project_id = project_id,
            synced_count = processed,
            "Processed incomplete files during startup recovery"
        );

        Ok(processed)
    }

    async fn process_deleted_files(
        &self,
        project_id: i64,
        deleted_files: Vec<FileClassification>,
        orchestrator: Option<Arc<tokio::sync::Mutex<IndexOrchestrator>>>,
    ) -> Result<usize, StorageError> {
        let mut deleted = 0;
        for classification in deleted_files {
            if let Some(ref orchestrator) = orchestrator
                && let Err(error) = orchestrator
                    .lock()
                    .await
                    .remove_file(Path::new(&classification.path))
                    .await
            {
                warn!(file = %classification.path, %error, "Failed to remove external file artifacts");
                continue;
            }

            self.manager
                .store()
                .files_delete_by_path(project_id, &classification.path)
                .await?;
            deleted += 1;
        }
        Ok(deleted)
    }

    /// Cleanup all artifacts associated with a completed operation
    pub async fn cleanup_operation_artifacts(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<(), StorageError> {
        let store = self.manager.store();

        // Delete all checkpoint files for this operation
        let files_deleted = store
            .checkpoint_files_delete_by_operation(project_id, operation_id)
            .await?;

        // Delete all checkpoint batches for this operation
        let batches_deleted = store
            .checkpoint_batches_delete_by_operation(project_id, operation_id)
            .await?;

        // Delete all work unit checkpoints for this operation
        let work_units_deleted = store
            .work_units_delete_by_operation(project_id, operation_id)
            .await?;

        info!(
            operation_id = %operation_id,
            files_deleted = files_deleted,
            batches_deleted = batches_deleted,
            work_units_deleted = work_units_deleted,
            "Cleaned up operation artifacts"
        );

        Ok(())
    }
}

/// Result of recovery for a project
#[derive(Debug, Clone, Default)]
pub struct RecoveryResult {
    pub project_id: i64,
    pub files_classified: usize,
    pub files_to_reparse: usize,
    pub files_to_resync: usize,
    pub files_reparsed: usize,
    pub files_resynced: usize,
    pub files_deleted: usize,
    pub entity_count: usize,
    pub symbol_count: usize,
    pub relation_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_meta_creation() {
        let meta = ProjectMeta {
            project_id: 1,
            epoch: 0,
            batch_id: 0,
            active_epoch: 0,
            active_relation_epoch: 0,
        };
        assert_eq!(meta.project_id, 1);
    }

    #[tokio::test]
    async fn test_recovery_coordinator_creation() -> Result<(), StorageError> {
        let store = in_memory_metadata_store()?;
        let _coordinator = StartupRecoveryCoordinator::new(store);
        Ok(())
    }
}
