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
use cce_storage_sqlite::SqliteClient;
use cce_types::StorageError;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use tracing::{debug, info, warn};

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

/// Low-level recovery operations
pub struct StartupRecoveryManager {
    sqlite: SqliteClient,
}

impl StartupRecoveryManager {
    /// Create a new recovery manager
    pub fn new(sqlite: SqliteClient) -> Self {
        Self { sqlite }
    }

    /// Get reference to SQLite client
    pub fn sqlite(&self) -> &SqliteClient {
        &self.sqlite
    }

    /// Classify all files in a project for recovery
    ///
    /// Uses content hash comparison to determine if a file needs re-parsing.
    /// Files with mismatched hashes are marked as Modified for re-indexing.
    /// batch_id is preserved for logging/tracking but not used for state classification.
    pub fn classify_files(
        &self,
        project_id: i64,
        meta: &ProjectMeta,
    ) -> Result<Vec<FileClassification>, StorageError> {
        let conn = self.sqlite.read_connection()?;
        let mut stmt = conn
            .prepare(
                "SELECT path, epoch, batch_id, content_hash
                 FROM files
                 WHERE project_id = ?1 AND epoch = ?2
                 ORDER BY path",
            )
            .map_err(|e| {
                StorageError::Query(format!(
                    "Failed to prepare file classification query: {}",
                    e
                ))
            })?;

        let classifications = stmt
            .query_map(rusqlite::params![project_id, meta.epoch], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            })
            .map_err(|e| {
                StorageError::Query(format!("Failed to query files for classification: {}", e))
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| {
                StorageError::Query(format!("Failed to collect file classifications: {}", e))
            })?
            .into_iter()
            .map(|(path, epoch, batch_id, stored_hash)| {
                let state = match Self::compute_file_hash(&path) {
                    Ok(disk_hash) => match &stored_hash {
                        Some(stored) if disk_hash != *stored => {
                            debug!(
                                project_id = project_id,
                                path = %path,
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
                            path = %path,
                            error = %e,
                        "Failed to compute file hash, marking storage state incomplete"
                        );
                        FileState::Incomplete
                    }
                };

                FileClassification {
                    path,
                    state,
                    epoch,
                    batch_id,
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
    pub fn mark_file_for_reparse(
        &self,
        project_id: i64,
        file_path: &str,
    ) -> Result<(), StorageError> {
        let conn = self.sqlite.write_connection()?;

        conn.execute(
            "UPDATE files SET epoch = (SELECT epoch FROM project_meta WHERE project_id = ?1 AND key = 'epoch')
             WHERE project_id = ?1 AND path = ?2",
            rusqlite::params![project_id, file_path],
        )
        .map_err(|e| {
            StorageError::Query(format!(
                "Failed to mark file '{}' for re-parse: {}",
                file_path, e
            ))
        })?;

        Ok(())
    }

    /// Find cached ParsedFile data from interrupted checkpoints
    ///
    /// Queries `checkpoint_file` for a file whose content hash matches the
    /// current disk content, allowing recovery to reuse previously parsed AST
    /// data instead of re-parsing from scratch.
    pub fn find_cached_parsed_data(
        &self,
        project_id: i64,
        file_path: &str,
        disk_content_hash: &str,
    ) -> Result<Option<Vec<u8>>, StorageError> {
        let conn = self.sqlite.read_connection()?;

        let mut stmt = conn
            .prepare(
                "SELECT cf.parsed_data
                 FROM checkpoint_file cf
                 JOIN checkpoint c ON cf.operation_id = c.operation_id
                   AND c.project_id = cf.project_id
                 WHERE cf.project_id = ?1
                   AND cf.file_path = ?2
                   AND cf.content_hash = ?3
                   AND cf.parsed_data IS NOT NULL
                   AND c.status = 'in_progress'
                 ORDER BY cf.id DESC
                 LIMIT 1",
            )
            .map_err(|e| {
                StorageError::Query(format!("Failed to prepare cached parsed data query: {}", e))
            })?;

        let result: Option<Vec<u8>> = stmt
            .query_row(
                rusqlite::params![project_id, file_path, disk_content_hash],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .ok();

        Ok(result)
    }

    /// Read chunks for a specific file at given epoch
    pub fn read_chunks_for_file(
        &self,
        project_id: i64,
        file_path: &str,
        epoch: i64,
    ) -> Result<Vec<String>, StorageError> {
        let conn = self.sqlite.read_connection()?;

        let mut stmt = conn
            .prepare(
                "SELECT chunk_id FROM chunks
                 WHERE project_id = ?1 AND file_path = ?2 AND epoch = ?3
                 ORDER BY chunk_id",
            )
            .map_err(|e| {
                StorageError::Query(format!(
                    "Failed to prepare chunks query for file '{}': {}",
                    file_path, e
                ))
            })?;

        let chunk_ids: Vec<String> = stmt
            .query_map(rusqlite::params![project_id, file_path, epoch], |row| {
                row.get::<_, String>(0)
            })
            .map_err(|e| {
                StorageError::Query(format!(
                    "Failed to query chunks for file '{}': {}",
                    file_path, e
                ))
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| {
                StorageError::Query(format!(
                    "Failed to collect chunks for file '{}': {}",
                    file_path, e
                ))
            })?;

        Ok(chunk_ids)
    }

    /// Collect relation index recovery information
    pub fn collect_relation_info(
        &self,
        project_id: i64,
        meta: &ProjectMeta,
    ) -> Result<RelationIndexRecoveryInfo, StorageError> {
        let conn = self.sqlite.read_connection()?;

        let mut relation_stmt = conn
            .prepare(
                "SELECT COUNT(DISTINCT relation_id) FROM relations
                 WHERE project_id = ?1 AND relation_epoch = ?2",
            )
            .map_err(|e| {
                StorageError::Query(format!("Failed to prepare relation count query: {}", e))
            })?;

        let relation_count: usize = relation_stmt
            .query_row(
                rusqlite::params![project_id, meta.active_relation_epoch],
                |row| Ok(row.get::<_, i64>(0).map(|v| v as usize).unwrap_or(0)),
            )
            .unwrap_or(0);

        let mut entity_stmt = conn
            .prepare(
                "SELECT COUNT(*) FROM entities
                 WHERE project_id = ?1 AND epoch = ?2",
            )
            .map_err(|e| {
                StorageError::Query(format!("Failed to prepare entity count query: {}", e))
            })?;

        let entity_count: usize = entity_stmt
            .query_row(
                rusqlite::params![project_id, meta.active_relation_epoch],
                |row| Ok(row.get::<_, i64>(0).map(|v| v as usize).unwrap_or(0)),
            )
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
    pub fn new(sqlite: SqliteClient) -> Self {
        Self {
            manager: StartupRecoveryManager::new(sqlite),
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

        // Phase 1: Load project metadata
        let meta = ProjectMeta::load(self.manager.sqlite(), project_id)?;
        debug!(
            project_id = project_id,
            epoch = meta.epoch,
            batch_id = meta.batch_id,
            active_epoch = meta.active_epoch,
            "Loaded project metadata"
        );

        // Phase 2: Classify files
        let classifications = self.manager.classify_files(project_id, &meta)?;
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

        // Phase 3: Process modified files
        if !reparse_files.is_empty() {
            result.files_reparsed = self
                .process_reparse_files(project_id, reparse_files, orchestrator.clone())
                .await?;
        }

        // Phase 4: Process incomplete files
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

        // Phase 5: Collect relation index info
        let relation_info = self.manager.collect_relation_info(project_id, &meta)?;
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
                .mark_file_for_reparse(project_id, &classification.path)
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
            let has_cached = StartupRecoveryManager::compute_file_hash(&classification.path)
                .ok()
                .and_then(|hash| {
                    self.manager
                        .find_cached_parsed_data(project_id, &classification.path, &hash)
                        .ok()
                        .flatten()
                })
                .is_some();

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

            self.manager.sqlite().with_transaction(|tx| {
                cce_storage_sqlite::FileRepository::delete_by_path(
                    tx,
                    &classification.path,
                    project_id,
                )
            })?;
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
        let conn = self.manager.sqlite().write_connection()?;

        // Delete all checkpoint files for this operation
        let files_deleted = cce_storage_sqlite::repo::CheckpointRepository::delete_checkpoint_files_by_operation_id(
            &conn,
            project_id,
            operation_id,
        )?;

        // Delete all checkpoint batches for this operation
        let batches_deleted = cce_storage_sqlite::repo::CheckpointRepository::delete_checkpoint_batches_by_operation_id(
            &conn,
            project_id,
            operation_id,
        )?;

        // Delete all work unit checkpoints for this operation
        let work_units_deleted =
            cce_storage_sqlite::repo::CheckpointRepository::delete_work_units_by_operation_id(
                &conn,
                project_id,
                operation_id,
            )?;

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
        let sqlite = SqliteClient::in_memory()?;
        let _coordinator = StartupRecoveryCoordinator::new(sqlite);
        Ok(())
    }
}
