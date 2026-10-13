//! Checkpoint manager for unified checkpoint operations
//!
//! This module provides a high-level API for managing checkpoints:
//! - Creating and updating checkpoints
//! - Recording file-level progress
//! - Querying checkpoint states
//! - Recovery support

use cce_storage_metadb_sqlite::CheckpointRepository;
use cce_storage_metadb_sqlite::SqliteClient;
use cce_storage_metadb_sqlite::types::{
    BatchCheckpointRecord, CheckpointRecord, CheckpointStatus, FileCheckpointRecord,
    WorkUnitCheckpointRecord, WorkUnitStatus,
};
use cce_types::{OperationKind, StorageError};
use chrono::Utc;
use std::sync::Arc;
use tracing::{debug, info, trace};

use super::file_diff::FileDiffManager;

mod payload;

pub(crate) use payload::plugin_fingerprint_for;
pub use payload::{
    PARSED_CHECKPOINT_SCHEMA_VERSION, ParsedCheckpointEnvelope, ParsedCheckpointPayload,
    SUMMARY_CHECKPOINT_SCHEMA_VERSION, SummaryCheckpointPayload, decode_parsed_checkpoint,
    decode_summary_checkpoint, encode_parsed_checkpoint, encode_summary_checkpoint,
    persist_summaries_to_checkpoints,
};

/// Checkpoint manager for CRUD operations
///
/// This manager provides a facade for checkpoint operations, delegating
/// file difference tracking
/// to FileDiffManager while keeping core checkpoint operations here.
pub struct CheckpointManager {
    /// Project ID for multi-project support: 0 for global project, >0 for user projects
    project_id: i64,
    /// SQLite database for checkpoint persistence
    db: Arc<SqliteClient>,
    /// Manager for file difference tracking
    file_diff_manager: Arc<FileDiffManager>,
}

/// Parameters for creating a new checkpoint
pub struct CreateCheckpointParams<'a> {
    pub operation_id: &'a str,
    pub operation_type: OperationKind,
    pub root_dir: &'a str,
    pub total_files: u32,
    pub batch_size: u32,
    pub file_list_hash: &'a str,
}

impl CheckpointManager {
    /// Create a checkpoint manager for a specific project (project_id must be > 0)
    pub fn new_for_project(project_id: i64, db: Arc<SqliteClient>) -> Self {
        assert!(
            project_id > 0,
            "project_id must be > 0 for explicit project"
        );
        let file_diff_manager = Arc::new(FileDiffManager::new(project_id, db.clone()));

        Self {
            project_id,
            db,
            file_diff_manager,
        }
    }

    /// Get file diff manager for direct access
    pub fn file_diff_manager(&self) -> Arc<FileDiffManager> {
        self.file_diff_manager.clone()
    }

    /// Get project ID (0 for global project, >0 for user projects)
    pub fn project_id(&self) -> i64 {
        self.project_id
    }

    /// Return the shared SQLite database used by checkpoints.
    ///
    /// File-level progress uses the same durable store so a recovered
    /// operation cannot observe a different state projection from its
    /// operation checkpoint.
    pub fn database(&self) -> Arc<SqliteClient> {
        self.db.clone()
    }

    /// Create a new operation checkpoint
    pub async fn create_checkpoint(
        &self,
        params: CreateCheckpointParams<'_>,
    ) -> Result<CheckpointRecord, StorageError> {
        let priority = match params.operation_type {
            OperationKind::FullIndex => 3,
            OperationKind::Incremental => 2,
            OperationKind::HotUpdate => 1,
            OperationKind::ConfigChange => 1,
        };
        let now = chrono::Utc::now().to_rfc3339();
        let checkpoint = CheckpointRecord {
            id: None,
            project_id: self.project_id,
            operation_id: params.operation_id.to_string(),
            operation_type: params.operation_type.to_string(),
            root_dir: params.root_dir.to_string(),
            total_files: params.total_files,
            batch_size: params.batch_size,
            current_batch_index: 0,
            current_phase: "Scanning".to_string(),
            file_list_hash: Some(params.file_list_hash.to_string()),
            created_at: now.clone(),
            updated_at: now.clone(),
            last_error: None,
            failure_count: 0,
            status: CheckpointStatus::InProgress,
            active_flag: false,
            priority,
            last_heartbeat: Some(now),
            failed_at: None,
        };

        let mut conn = self.db.write_connection()?;
        let tx = conn
            .transaction()
            .map_err(|e| StorageError::sqlite(format!("Failed to create transaction: {}", e)))?;

        CheckpointRepository::create_checkpoint(&tx, self.project_id, &checkpoint)?;

        tx.commit()
            .map_err(|e| StorageError::sqlite(format!("Failed to commit transaction: {}", e)))?;

        trace!(
            operation_id = %params.operation_id,
            total_files = params.total_files,
            batch_size = params.batch_size,
            "Checkpoint created and persisted"
        );

        Ok(checkpoint)
    }

    /// Create a batch checkpoint
    pub async fn create_batch_checkpoint(
        &self,
        operation_id: &str,
        batch_index: u32,
        first_file: &str,
        last_file: &str,
        file_count: u32,
    ) -> Result<BatchCheckpointRecord, StorageError> {
        let batch = BatchCheckpointRecord {
            id: None,
            operation_id: operation_id.to_string(),
            batch_index,
            first_file: first_file.to_string(),
            last_file: last_file.to_string(),
            file_count,
            processed_files: 0,
            failed_files: 0,
            entities_extracted: 0,
            relations_found: 0,
            chunks_generated: 0,
            vectors_stored: 0,
            start_time: chrono::Utc::now().to_rfc3339(),
            end_time: None,
            duration_ms: None,
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        };

        let mut conn = self.db.write_connection()?;
        let tx = conn
            .transaction()
            .map_err(|e| StorageError::sqlite(format!("Failed to create transaction: {}", e)))?;

        CheckpointRepository::insert_batch_checkpoint(&tx, self.project_id, &batch)?;

        tx.commit()
            .map_err(|e| StorageError::sqlite(format!("Failed to commit transaction: {}", e)))?;

        trace!(
            operation_id = %operation_id,
            batch_index = batch_index,
            "Batch checkpoint created and persisted"
        );

        Ok(batch)
    }

    /// Create a file checkpoint
    pub async fn create_file_checkpoint(
        &self,
        operation_id: &str,
        batch_index: u32,
        file_path: &str,
    ) -> Result<FileCheckpointRecord, StorageError> {
        let file = FileCheckpointRecord {
            id: None,
            operation_id: operation_id.to_string(),
            batch_index,
            file_path: file_path.to_string(),
            file_id: None,
            language: None,
            file_size: None,
            content_hash: None,
            parsed_data: None,
            parse_error: None,
            summary_data: None,
            embedding_count: 0,
            bm25_doc_id: None,
            export_path: None,
            render_fingerprint: None,
            module_progress: None,
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        };

        trace!(
            operation_id = %operation_id,
            file_path = file_path,
            "File checkpoint created"
        );

        Ok(file)
    }

    /// Save a file checkpoint to the database
    ///
    /// This is called after processing each file through the pipeline.
    /// It persists parsed data artifacts.
    pub async fn save_file_checkpoint(
        &self,
        file_checkpoint: &FileCheckpointRecord,
    ) -> Result<(), StorageError> {
        trace!(
            file_path = %file_checkpoint.file_path,
            "Saving file checkpoint"
        );

        let mut conn = self.db.write_connection()?;
        let tx = conn
            .transaction()
            .map_err(|e| StorageError::sqlite(format!("Failed to create transaction: {}", e)))?;

        CheckpointRepository::upsert_file_checkpoint(&tx, self.project_id, file_checkpoint)?;

        tx.commit()
            .map_err(|e| StorageError::sqlite(format!("Failed to commit transaction: {}", e)))?;

        trace!(
            file_path = %file_checkpoint.file_path,
            "File checkpoint saved successfully"
        );

        Ok(())
    }

    /// Get a single file checkpoint by operation ID and file path.
    pub async fn get_file_checkpoint(
        &self,
        operation_id: &str,
        file_path: &str,
    ) -> Result<Option<FileCheckpointRecord>, StorageError> {
        let conn = self.db.read_connection()?;
        CheckpointRepository::get_file_checkpoint(&conn, self.project_id, operation_id, file_path)
    }

    /// Get a checkpoint by operation ID
    pub async fn get_checkpoint(
        &self,
        operation_id: &str,
    ) -> Result<Option<CheckpointRecord>, StorageError> {
        trace!(
            operation_id = %operation_id,
            "Retrieving checkpoint"
        );

        let conn = self.db.read_connection()?;
        CheckpointRepository::get_checkpoint(&conn, self.project_id, operation_id)
    }

    /// Get all files in a batch
    pub async fn get_batch_files(
        &self,
        operation_id: &str,
        batch_index: u32,
    ) -> Result<Vec<FileCheckpointRecord>, StorageError> {
        trace!(
            operation_id = %operation_id,
            batch_index = batch_index,
            "Retrieving batch files"
        );

        let conn = self.db.read_connection()?;
        CheckpointRepository::get_batch_files(&conn, self.project_id, operation_id, batch_index)
    }

    /// Get the batch checkpoint record of a specific batch.
    ///
    /// The record carries the `first_file` / `last_file` boundary that the
    /// batch was processed against; recovery compares them with the current
    /// deterministic batching to detect boundary drift.
    pub async fn get_batch_checkpoint(
        &self,
        operation_id: &str,
        batch_index: u32,
    ) -> Result<Option<BatchCheckpointRecord>, StorageError> {
        let conn = self.db.read_connection()?;
        CheckpointRepository::get_batch_checkpoint(
            &conn,
            self.project_id,
            operation_id,
            batch_index,
        )
    }

    /// Get failed files in an operation
    pub async fn get_failed_files(
        &self,
        operation_id: &str,
    ) -> Result<Vec<(String, String)>, StorageError> {
        trace!(
            operation_id = %operation_id,
            "Retrieving failed files"
        );

        let conn = self.db.read_connection()?;
        CheckpointRepository::get_failed_files(&conn, self.project_id, operation_id)
    }

    /// Update checkpoint status
    pub async fn update_checkpoint_status(
        &self,
        operation_id: &str,
        status: CheckpointStatus,
    ) -> Result<(), StorageError> {
        trace!(
            operation_id = %operation_id,
            status = %status,
            "Updating checkpoint status"
        );

        let mut conn = self.db.write_connection()?;
        let tx = conn
            .transaction()
            .map_err(|e| StorageError::sqlite(format!("Failed to create transaction: {}", e)))?;

        CheckpointRepository::update_checkpoint_status(&tx, self.project_id, operation_id, status)?;

        tx.commit()
            .map_err(|e| StorageError::sqlite(format!("Failed to commit transaction: {}", e)))?;

        Ok(())
    }

    /// Update current batch index
    pub async fn update_current_batch_index(
        &self,
        operation_id: &str,
        batch_index: u32,
    ) -> Result<(), StorageError> {
        trace!(
            operation_id = %operation_id,
            batch_index = batch_index,
            "Updating current batch index"
        );

        let mut conn = self.db.write_connection()?;
        let tx = conn
            .transaction()
            .map_err(|e| StorageError::sqlite(format!("Failed to create transaction: {}", e)))?;

        CheckpointRepository::update_current_batch_index(
            &tx,
            self.project_id,
            operation_id,
            batch_index,
        )?;

        tx.commit()
            .map_err(|e| StorageError::sqlite(format!("Failed to commit transaction: {}", e)))?;

        Ok(())
    }

    /// Validate and recover from checkpoint
    ///
    /// Only a checkpoint whose operation type and root directory match the
    /// caller's are considered: a hot-update recovery must never adopt the
    /// unfinished checkpoint of a full-index (or of another watched root).
    pub async fn validate_and_recover_checkpoint(
        &self,
        operation_id: &str,
        operation_type: OperationKind,
        root_dir: &str,
    ) -> Result<Option<CheckpointRecord>, StorageError> {
        let conn = self.db.read_connection()?;

        let checkpoint = CheckpointRepository::get_latest_incomplete_by_type(
            &conn,
            self.project_id,
            operation_type.as_str(),
            root_dir,
        )?;

        if let Some(checkpoint) = checkpoint {
            CheckpointRepository::validate_checkpoint(&checkpoint)?;
            info!(
                requested_operation_id = %operation_id,
                recovered_operation_id = %checkpoint.operation_id,
                operation_type = %operation_type,
                root_dir = %root_dir,
                "Checkpoint validation passed"
            );
            return Ok(Some(checkpoint));
        }

        Ok(None)
    }

    /// Mark an operation checkpoint as completed
    ///
    /// Updates the checkpoint status to Completed in the database.
    /// This prevents the operation from being re-recovered on process restart.
    pub async fn mark_operation_completed(&self, operation_id: &str) -> Result<(), StorageError> {
        trace!(
            operation_id = %operation_id,
            "Marking operation checkpoint as completed"
        );

        let mut conn = self.db.write_connection()?;
        let tx = conn
            .transaction()
            .map_err(|e| StorageError::sqlite(format!("Failed to create transaction: {}", e)))?;

        // Get current checkpoint
        let checkpoint = CheckpointRepository::get_checkpoint(&tx, self.project_id, operation_id)?;

        if let Some(mut cp) = checkpoint {
            cp.status = CheckpointStatus::Completed;
            cp.updated_at = Utc::now().to_rfc3339();

            CheckpointRepository::update_checkpoint(&tx, self.project_id, &cp)?;

            trace!(
                operation_id = %operation_id,
                "Operation checkpoint marked as completed"
            );
        } else {
            trace!(
                operation_id = %operation_id,
                "No checkpoint found for operation, skipping completion mark"
            );
        }

        tx.commit()
            .map_err(|e| StorageError::sqlite(format!("Failed to commit transaction: {}", e)))?;

        Ok(())
    }

    /// Mark an operation checkpoint as failed
    ///
    /// Updates the checkpoint status to Failed in the database and records the failure timestamp.
    /// This enables TTL-based cleanup of failed operations.
    pub async fn mark_operation_failed(
        &self,
        operation_id: &str,
        error: &str,
    ) -> Result<(), StorageError> {
        trace!(
            operation_id = %operation_id,
            error = %error,
            "Marking operation checkpoint as failed"
        );

        let mut conn = self.db.write_connection()?;
        let tx = conn
            .transaction()
            .map_err(|e| StorageError::sqlite(format!("Failed to create transaction: {}", e)))?;

        // Get current checkpoint
        let checkpoint = CheckpointRepository::get_checkpoint(&tx, self.project_id, operation_id)?;

        if let Some(mut cp) = checkpoint {
            cp.status = CheckpointStatus::Failed;
            cp.last_error = Some(error.to_string());
            cp.updated_at = Utc::now().to_rfc3339();
            cp.failed_at = Some(Utc::now().to_rfc3339());
            // A terminal checkpoint must never be picked up as the active
            // operation again.
            cp.active_flag = false;

            CheckpointRepository::update_checkpoint(&tx, self.project_id, &cp)?;

            trace!(
                operation_id = %operation_id,
                "Operation checkpoint marked as failed"
            );
        } else {
            trace!(
                operation_id = %operation_id,
                "No checkpoint found for operation, skipping failure mark"
            );
        }

        tx.commit()
            .map_err(|e| StorageError::sqlite(format!("Failed to commit transaction: {}", e)))?;

        Ok(())
    }

    /// Delete all checkpoint files for an operation
    pub async fn delete_checkpoint_files_by_operation_id(
        &self,
        operation_id: &str,
    ) -> Result<usize, StorageError> {
        let conn = self.db.write_connection()?;
        CheckpointRepository::delete_checkpoint_files_by_operation_id(
            &conn,
            self.project_id,
            operation_id,
        )
    }

    /// Delete all checkpoint batches for an operation
    pub async fn delete_checkpoint_batches_by_operation_id(
        &self,
        operation_id: &str,
    ) -> Result<usize, StorageError> {
        let conn = self.db.write_connection()?;
        CheckpointRepository::delete_checkpoint_batches_by_operation_id(
            &conn,
            self.project_id,
            operation_id,
        )
    }

    /// Delete all work unit checkpoints for an operation
    pub async fn delete_work_units_by_operation_id(
        &self,
        operation_id: &str,
    ) -> Result<usize, StorageError> {
        let conn = self.db.write_connection()?;
        CheckpointRepository::delete_work_units_by_operation_id(
            &conn,
            self.project_id,
            operation_id,
        )
    }

    /// Get all unfinished operations for recovery
    pub async fn get_unfinished_operations(&self) -> Result<Vec<CheckpointRecord>, StorageError> {
        debug!("Retrieving unfinished operations for recovery");

        let conn = self.db.read_connection()?;
        let operations = CheckpointRepository::get_unfinished_operations(&conn, self.project_id)?;

        info!(
            unfinished_count = operations.len(),
            "Unfinished operations retrieved for recovery"
        );

        Ok(operations)
    }

    /// Clear the module progress markers for every file checkpoint of an
    /// operation.
    ///
    /// Module markers are only valid while the operation's candidate
    /// generation survives. A resume whose candidate can no longer be adopted
    /// must clear them so all modules re-execute against the fresh clone;
    /// a successful adoption keeps them and skips already-completed modules.
    pub async fn clear_module_progress(&self, operation_id: &str) -> Result<usize, StorageError> {
        let conn = self.db.write_connection()?;
        CheckpointRepository::clear_module_progress(&conn, self.project_id, operation_id)
    }

    // ---------------------------------------------------------------------------
    // Work unit checkpoint
    // ---------------------------------------------------------------------------

    /// Insert a work unit checkpoint
    pub async fn insert_work_unit(
        &self,
        record: &WorkUnitCheckpointRecord,
    ) -> Result<i64, StorageError> {
        if record.project_id != self.project_id {
            return Err(StorageError::validation(format!(
                "project_id mismatch: expected {} got {}",
                self.project_id, record.project_id
            )));
        }
        let mut conn = self.db.write_connection()?;
        let tx = conn
            .transaction()
            .map_err(|e| StorageError::sqlite(format!("Failed to create transaction: {}", e)))?;

        let id = CheckpointRepository::insert_work_unit(&tx, record)?;

        tx.commit()
            .map_err(|e| StorageError::sqlite(format!("Failed to commit transaction: {}", e)))?;

        Ok(id)
    }

    /// Insert a batch of work unit checkpoints in a single transaction.
    ///
    /// Microbatch loops accumulate their Running records and flush once
    /// instead of paying one transaction per microbatch.
    pub async fn insert_work_units(
        &self,
        records: &[WorkUnitCheckpointRecord],
    ) -> Result<(), StorageError> {
        if records.is_empty() {
            return Ok(());
        }
        for record in records {
            if record.project_id != self.project_id {
                return Err(StorageError::validation(format!(
                    "project_id mismatch: expected {} got {}",
                    self.project_id, record.project_id
                )));
            }
        }
        let mut conn = self.db.write_connection()?;
        let tx = conn
            .transaction()
            .map_err(|e| StorageError::sqlite(format!("Failed to create transaction: {}", e)))?;

        for record in records {
            CheckpointRepository::insert_work_unit(&tx, record)?;
        }

        tx.commit()
            .map_err(|e| StorageError::sqlite(format!("Failed to commit transaction: {}", e)))?;

        Ok(())
    }

    /// Update work unit status
    pub async fn update_work_unit_status(
        &self,
        operation_id: &str,
        stage: &str,
        work_unit_hash: &str,
        status: WorkUnitStatus,
    ) -> Result<(), StorageError> {
        let mut conn = self.db.write_connection()?;
        let tx = conn
            .transaction()
            .map_err(|e| StorageError::sqlite(format!("Failed to create transaction: {}", e)))?;

        CheckpointRepository::update_work_unit_status(
            &tx,
            self.project_id,
            operation_id,
            stage,
            work_unit_hash,
            status,
        )?;

        tx.commit()
            .map_err(|e| StorageError::sqlite(format!("Failed to commit transaction: {}", e)))?;

        Ok(())
    }

    /// Mark a batch of work units as committed in a single transaction.
    ///
    /// The caller accumulates one hash per successfully persisted microbatch
    /// and flushes once at the end of the stage instead of updating per
    /// microbatch.
    pub async fn update_work_unit_statuses(
        &self,
        operation_id: &str,
        stage: &str,
        work_unit_hashes: &[String],
        status: WorkUnitStatus,
    ) -> Result<(), StorageError> {
        if work_unit_hashes.is_empty() {
            return Ok(());
        }
        let mut conn = self.db.write_connection()?;
        let tx = conn
            .transaction()
            .map_err(|e| StorageError::sqlite(format!("Failed to create transaction: {}", e)))?;

        for hash in work_unit_hashes {
            CheckpointRepository::update_work_unit_status(
                &tx,
                self.project_id,
                operation_id,
                stage,
                hash,
                status,
            )?;
        }

        tx.commit()
            .map_err(|e| StorageError::sqlite(format!("Failed to commit transaction: {}", e)))?;

        Ok(())
    }

    /// Get all work units for an operation and stage
    pub async fn get_work_units(
        &self,
        operation_id: &str,
        stage: &str,
    ) -> Result<Vec<WorkUnitCheckpointRecord>, StorageError> {
        let conn = self.db.read_connection()?;
        CheckpointRepository::get_work_units(&conn, self.project_id, operation_id, stage)
    }

    /// Get a specific work unit by hash
    pub async fn get_work_unit_by_hash(
        &self,
        operation_id: &str,
        stage: &str,
        work_unit_hash: &str,
    ) -> Result<Option<WorkUnitCheckpointRecord>, StorageError> {
        let conn = self.db.read_connection()?;
        CheckpointRepository::get_work_unit_by_hash(
            &conn,
            self.project_id,
            operation_id,
            stage,
            work_unit_hash,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_operation_checkpoint(
        operation_id: &str,
        operation_type: &str,
        root_dir: &str,
    ) -> CheckpointRecord {
        CheckpointRecord {
            id: None,
            project_id: 1,
            operation_id: operation_id.to_string(),
            operation_type: operation_type.to_string(),
            root_dir: root_dir.to_string(),
            total_files: 1,
            batch_size: 1,
            current_batch_index: 0,
            current_phase: "Scanning".to_string(),
            file_list_hash: None,
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
            last_error: None,
            failure_count: 0,
            status: CheckpointStatus::InProgress,
            active_flag: true,
            priority: 1,
            last_heartbeat: None,
            failed_at: None,
        }
    }

    /// Attaching a summary to an existing parse checkpoint must preserve the
    /// parse blob and the record metadata (COALESCE upsert), and both stored
    /// payloads must decode back with their variants intact.
    #[tokio::test]
    async fn summary_attach_preserves_parse_checkpoint() {
        use crate::operation::checkpoint::{
            ParsedCheckpointPayload, decode_parsed_checkpoint, encode_parsed_checkpoint,
        };

        let db = Arc::new(SqliteClient::in_memory().expect("in-memory database"));
        let manager = CheckpointManager::new_for_project(1, db);
        let operation_id = "op-summary-attach";

        manager
            .create_checkpoint(CreateCheckpointParams {
                operation_id,
                operation_type: OperationKind::HotUpdate,
                root_dir: "/project",
                total_files: 1,
                batch_size: 1,
                file_list_hash: "",
            })
            .await
            .expect("checkpoint created");
        manager
            .create_batch_checkpoint(operation_id, 0, "src/lib.rs", "src/lib.rs", 1)
            .await
            .expect("batch checkpoint created");

        // Phase 1: parse checkpoint write (INSERT branch).
        let mut record = manager
            .create_file_checkpoint(operation_id, 0, "src/lib.rs")
            .await
            .expect("file checkpoint created");
        let payload = ParsedCheckpointPayload::Parsed(Box::new(ParsedCheckpointEnvelope::new(
            crate::hot_update::FileChangeType::Modified,
            cce_types::ParsedFile::default(),
        )));
        record.content_hash = Some("hash-1".to_string());
        record.parsed_data = Some(encode_parsed_checkpoint(&payload).expect("encode parse"));
        manager
            .save_file_checkpoint(&record)
            .await
            .expect("parse checkpoint saved");

        // Phase 2: summary attach (COALESCE UPDATE branch).
        let entries = [(
            "src/lib.rs".to_string(),
            cce_parser::summary::FileSummary::new("src/lib.rs").with_summary("entry"),
            None,
        )];
        persist_summaries_to_checkpoints(&manager, operation_id, 0, &entries, Some("fp-1".into()))
            .await
            .expect("summaries attached");

        let restored = manager
            .get_file_checkpoint(operation_id, "src/lib.rs")
            .await
            .expect("checkpoint read")
            .expect("checkpoint exists");
        assert_eq!(
            restored.content_hash.as_deref(),
            Some("hash-1"),
            "the parse-phase metadata must survive the summary attach"
        );

        let parsed_back =
            decode_parsed_checkpoint(restored.parsed_data.as_deref().expect("parsed data kept"))
                .expect("parse payload decodes");
        assert!(matches!(parsed_back, ParsedCheckpointPayload::Parsed(_)));

        let summary_back =
            decode_summary_checkpoint(restored.summary_data.as_deref().expect("summary attached"))
                .expect("summary payload decodes");
        assert_eq!(summary_back.file_summary.summary_text, "entry");
        assert_eq!(
            summary_back.summary_config_fingerprint.as_deref(),
            Some("fp-1")
        );
    }

    /// Clearing module progress on an operation's file checkpoints makes a
    /// later `read_module_progress` return an empty map.
    #[tokio::test]
    async fn clear_module_progress_empties_reads() {
        let db = Arc::new(SqliteClient::in_memory().expect("in-memory database"));
        let manager = CheckpointManager::new_for_project(1, db);
        let operation_id = "op-with-progress";

        manager
            .create_checkpoint(CreateCheckpointParams {
                operation_id,
                operation_type: OperationKind::HotUpdate,
                root_dir: "/project",
                total_files: 1,
                batch_size: 1,
                file_list_hash: "",
            })
            .await
            .expect("checkpoint created");
        manager
            .create_batch_checkpoint(operation_id, 0, "src/lib.rs", "src/lib.rs", 1)
            .await
            .expect("batch checkpoint created");

        let mut record = manager
            .create_file_checkpoint(operation_id, 0, "src/lib.rs")
            .await
            .expect("file checkpoint created");
        record.module_progress = Some(crate::hot_update::progress::write_module_progress(
            &std::collections::HashMap::from([
                ("embedding".to_string(), "fp-1".to_string()),
                ("bm25".to_string(), "fp-2".to_string()),
            ]),
        ));
        record.parsed_data = Some(b"{}".to_vec());
        manager
            .save_file_checkpoint(&record)
            .await
            .expect("file checkpoint saved");

        let restored = manager
            .get_file_checkpoint(operation_id, "src/lib.rs")
            .await
            .expect("checkpoint read")
            .expect("checkpoint exists");
        let progress =
            crate::hot_update::progress::read_module_progress(restored.module_progress.as_deref());
        assert_eq!(progress.len(), 2, "markers must be present before clearing");

        let cleared = manager
            .clear_module_progress(operation_id)
            .await
            .expect("module progress cleared");
        assert_eq!(cleared, 1, "exactly one file checkpoint must be cleared");

        let after = manager
            .get_file_checkpoint(operation_id, "src/lib.rs")
            .await
            .expect("checkpoint read")
            .expect("checkpoint exists");
        let progress =
            crate::hot_update::progress::read_module_progress(after.module_progress.as_deref());
        assert!(
            progress.is_empty(),
            "cleared operation must not expose stale module progress"
        );
    }

    /// Clearing module progress leaves unrelated operations untouched.
    #[tokio::test]
    async fn clear_module_progress_is_operation_scoped() {
        let db = Arc::new(SqliteClient::in_memory().expect("in-memory database"));
        let manager = CheckpointManager::new_for_project(1, db.clone());

        for (operation_id, marker) in [("op-a", "fp-a"), ("op-b", "fp-b")] {
            manager
                .create_checkpoint(CreateCheckpointParams {
                    operation_id,
                    operation_type: OperationKind::HotUpdate,
                    root_dir: "/project",
                    total_files: 1,
                    batch_size: 1,
                    file_list_hash: "",
                })
                .await
                .expect("checkpoint created");
            manager
                .create_batch_checkpoint(operation_id, 0, "src/lib.rs", "src/lib.rs", 1)
                .await
                .expect("batch checkpoint created");
            let mut record = manager
                .create_file_checkpoint(operation_id, 0, "src/lib.rs")
                .await
                .expect("file checkpoint created");
            record.module_progress = Some(format!("{{\"bm25\":\"{marker}\"}}"));
            record.parsed_data = Some(b"{}".to_vec());
            manager
                .save_file_checkpoint(&record)
                .await
                .expect("file checkpoint saved");
        }

        let cleared = manager
            .clear_module_progress("op-a")
            .await
            .expect("module progress cleared");
        assert_eq!(cleared, 1);

        let op_b = manager
            .get_file_checkpoint("op-b", "src/lib.rs")
            .await
            .expect("checkpoint read")
            .expect("checkpoint exists");
        assert!(
            op_b.module_progress
                .as_deref()
                .is_some_and(|json| json.contains("fp-b")),
            "the other operation's markers must be preserved"
        );
    }

    /// Recovery of an in_progress checkpoint must filter by both operation
    /// type and root directory: a hot-update resume must never adopt the
    /// unfinished checkpoint of a full-index operation or of another root.
    #[tokio::test]
    async fn validate_and_recover_checkpoint_filters_type_and_root() {
        let db = Arc::new(SqliteClient::in_memory().expect("in-memory database"));
        let manager = CheckpointManager::new_for_project(1, db.clone());

        {
            let mut conn = db.write_connection().expect("write connection");
            let tx = conn
                .transaction()
                .expect("start transaction for checkpoint inserts");
            for checkpoint in [
                make_operation_checkpoint("hot-root-a", "hot_update", "/root/a"),
                make_operation_checkpoint("full-root-a", "full_index", "/root/a"),
                make_operation_checkpoint("hot-root-b", "hot_update", "/root/b"),
                make_operation_checkpoint("done-hot", "hot_update", "/root/a"),
            ] {
                CheckpointRepository::create_checkpoint(&tx, 1, &checkpoint)
                    .expect("checkpoint inserted");
            }
            tx.commit().expect("checkpoint inserts committed");
        }

        let mut done = manager
            .get_checkpoint("done-hot")
            .await
            .expect("checkpoint read")
            .expect("checkpoint exists");
        done.status = CheckpointStatus::Completed;
        manager
            .update_checkpoint_status(&done.operation_id, CheckpointStatus::Completed)
            .await
            .expect("completed checkpoint marked");

        // Only the hot_update checkpoint of the same root is recovered.
        let recovered = manager
            .validate_and_recover_checkpoint("new-op", OperationKind::HotUpdate, "/root/a")
            .await
            .expect("recovery query succeeds");
        let recovered = recovered.expect("a matching checkpoint must be found");
        assert_eq!(recovered.operation_id, "hot-root-a");

        // A different root must not adopt the full-index checkpoint.
        let other_root = manager
            .validate_and_recover_checkpoint("new-op", OperationKind::HotUpdate, "/root/b")
            .await
            .expect("recovery query succeeds");
        assert_eq!(
            other_root
                .expect("root-b checkpoint must be found")
                .operation_id,
            "hot-root-b"
        );

        // Full-index recovery is separate from hot-update recovery.
        let full = manager
            .validate_and_recover_checkpoint("new-op", OperationKind::FullIndex, "/root/a")
            .await
            .expect("recovery query succeeds");
        assert_eq!(
            full.expect("full-index checkpoint must be found")
                .operation_id,
            "full-root-a"
        );
    }
}
