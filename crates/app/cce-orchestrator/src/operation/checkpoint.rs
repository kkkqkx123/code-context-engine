//! Checkpoint manager for unified checkpoint operations
//!
//! This module provides a high-level API for managing checkpoints:
//! - Creating and updating checkpoints
//! - Recording file-level progress
//! - Querying checkpoint states
//! - Recovery support

use crate::hot_update::FileChangeType;
use cce_storage_sqlite::CheckpointRepository;
use cce_storage_sqlite::SqliteClient;
use cce_storage_sqlite::types::{
    BatchCheckpointRecord, CheckpointRecord, CheckpointStatus, FileCheckpointRecord,
    WorkUnitCheckpointRecord, WorkUnitStatus,
};
use cce_types::{OperationKind, ParsedFile, StorageError};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, info, trace};

use super::file_diff::FileDiffManager;

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

/// Schema version for parsed checkpoint payloads.
///
/// The project is in the development stage and treats checkpoint schema as an
/// internal iteration concern. Version 3 introduced the explicit
/// [`cce_types::INDEX_FORMAT_VERSION`] record on the envelope; payloads
/// written before the change fail to decode (missing required field) or the
/// compatibility check, and recovery falls back to re-parsing the source file.
pub const PARSED_CHECKPOINT_SCHEMA_VERSION: u32 = 3;

/// Schema version for summary checkpoint payloads.
pub const SUMMARY_CHECKPOINT_SCHEMA_VERSION: u32 = 2;

/// Tagged payload stored in the `checkpoint_file.parsed_data` blob.
///
/// The two record kinds were previously multiplexed onto one struct via an
/// optional parse result; the tombstone is now explicit so a deleted entry
/// can never carry parse data, and every consumer of
/// [`ParsedCheckpointEnvelope`] can rely on its parse result being present.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParsedCheckpointPayload {
    /// A successfully parsed file. Boxed because the tombstone variant makes
    /// the payload size otherwise dominated by the envelope.
    Parsed(Box<ParsedCheckpointEnvelope>),
    /// Deletion marker for a removed file. The path identity lives on the
    /// checkpoint row (`file_path`), so the variant carries no data.
    Deleted,
}

impl ParsedCheckpointPayload {
    /// Whether this payload can be adopted by recovery. Tombstones carry no
    /// versioned data; envelopes delegate to their own compatibility check.
    pub fn is_compatible(&self) -> bool {
        match self {
            Self::Parsed(envelope) => envelope.is_compatible(),
            Self::Deleted => true,
        }
    }
}

/// Envelope for parsed file data stored in checkpoint
///
/// Wraps `ParsedFile` with schema/parser version metadata to enable
/// compatibility checks during recovery. Used by both full-index and
/// hot-update paths. The hot-update event kind (`change_type`) drives
/// resume-side Added/Modified handling; it is never `Deleted` — deletions use
/// [`ParsedCheckpointPayload::Deleted`]. Generated summaries are stored next
/// to this envelope in their own record column (see
/// [`SummaryCheckpointPayload`]), not inside it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedCheckpointEnvelope {
    pub schema_version: u32,
    pub parser_version: u32,
    pub path_normalization_version: u32,
    /// Index format version the payload was written with. Entries whose
    /// recorded version differs from [`cce_types::INDEX_FORMAT_VERSION`]
    /// (classification encoding changed) are invalid and re-parsed.
    pub index_format_version: u32,
    /// Fingerprint of the plugin-language registry — recorded only when the
    /// parsed file references a [`cce_types::Language::Custom`] index.
    /// `None` entries stay valid regardless of plugin registration changes;
    /// `Some(fp)` entries are invalid as soon as the current fingerprint
    /// differs (plugin added/removed/reordered), because the persisted custom
    /// index may dangle.
    pub plugin_language_fingerprint: Option<String>,
    pub change_type: FileChangeType,
    pub parsed_file: ParsedFile,
}

impl ParsedCheckpointEnvelope {
    pub fn new(change_type: FileChangeType, parsed_file: ParsedFile) -> Self {
        let plugin_language_fingerprint = match parsed_file.language {
            cce_types::Language::Custom(_) => {
                Some(cce_types::language::plugin_language_fingerprint())
            }
            _ => None,
        };
        Self {
            schema_version: PARSED_CHECKPOINT_SCHEMA_VERSION,
            parser_version: cce_types::RELATION_PARSER_VERSION,
            path_normalization_version: cce_types::RELATION_PATH_NORMALIZATION_VERSION,
            index_format_version: cce_types::INDEX_FORMAT_VERSION,
            plugin_language_fingerprint,
            change_type,
            parsed_file,
        }
    }

    pub fn is_compatible(&self) -> bool {
        self.schema_version == PARSED_CHECKPOINT_SCHEMA_VERSION
            && self.parser_version == cce_types::RELATION_PARSER_VERSION
            && self.path_normalization_version == cce_types::RELATION_PATH_NORMALIZATION_VERSION
            && self.index_format_version == cce_types::INDEX_FORMAT_VERSION
            && self
                .plugin_language_fingerprint
                .as_ref()
                .map(|fp| fp == &cce_types::language::plugin_language_fingerprint())
                .unwrap_or(true)
    }
}

/// Pre-generated summary persisted next to the parse checkpoint so recovery
/// can reuse it without invoking the LLM-based summary generator again.
///
/// Stored in its own record column (`summary_data`) instead of inside the
/// parsed envelope: summaries are produced after parsing, and a dedicated
/// small payload lets the summary pass avoid re-encoding (and re-compressing)
/// the source-bearing parse blob.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryCheckpointPayload {
    pub schema_version: u32,
    /// Index format version recorded at write time; see
    /// [`ParsedCheckpointEnvelope::index_format_version`].
    pub index_format_version: u32,
    /// Plugin-language fingerprint for files whose content references a
    /// custom language (see [`ParsedCheckpointEnvelope`]); `None` entries
    /// stay valid regardless of plugin registration changes.
    pub plugin_language_fingerprint: Option<String>,
    pub file_summary: cce_parser::summary::FileSummary,
    /// Fingerprint of the summary configuration that produced `file_summary`.
    /// Recovery regenerates the summary when this no longer matches the
    /// current configuration.
    pub summary_config_fingerprint: Option<String>,
}

impl SummaryCheckpointPayload {
    pub fn new(
        file_summary: cce_parser::summary::FileSummary,
        plugin_language_fingerprint: Option<String>,
        summary_config_fingerprint: Option<String>,
    ) -> Self {
        Self {
            schema_version: SUMMARY_CHECKPOINT_SCHEMA_VERSION,
            index_format_version: cce_types::INDEX_FORMAT_VERSION,
            plugin_language_fingerprint,
            file_summary,
            summary_config_fingerprint,
        }
    }

    pub fn is_compatible(&self) -> bool {
        self.schema_version == SUMMARY_CHECKPOINT_SCHEMA_VERSION
            && self.index_format_version == cce_types::INDEX_FORMAT_VERSION
            && self
                .plugin_language_fingerprint
                .as_ref()
                .map(|fp| fp == &cce_types::language::plugin_language_fingerprint())
                .unwrap_or(true)
    }
}

/// Plugin-language fingerprint to record for a parsed file.
///
/// `Some(current)` only when the file references a
/// [`cce_types::Language::Custom`] index — files without custom
/// languages are immune to plugin-registration changes and record `None`.
pub(crate) fn plugin_fingerprint_for(language: cce_types::Language) -> Option<String> {
    match language {
        cce_types::Language::Custom(_) => Some(cce_types::language::plugin_language_fingerprint()),
        _ => None,
    }
}

/// Encode a parsed checkpoint payload for durable storage.
///
/// The JSON payload (which embeds the full file source) is zstd-compressed;
/// uncompressed checkpoints would dominate the `checkpoint_file` volume.
pub fn encode_parsed_checkpoint(
    payload: &ParsedCheckpointPayload,
) -> Result<Vec<u8>, StorageError> {
    let json = serde_json::to_vec(payload).map_err(|error| {
        StorageError::query(format!("Failed to serialize parsed checkpoint: {error}"))
    })?;
    zstd::encode_all(&*json, 3).map_err(|error| {
        StorageError::query(format!("Failed to compress parsed checkpoint: {error}"))
    })
}

/// Decode a stored parsed checkpoint payload.
///
/// Returns `None` when the payload cannot be decoded; callers treat this as
/// a missing checkpoint and fall back to re-parsing the source file.
pub fn decode_parsed_checkpoint(bytes: &[u8]) -> Option<ParsedCheckpointPayload> {
    let json = zstd::decode_all(bytes).ok()?;
    serde_json::from_slice(&json).ok()
}

/// Encode a summary checkpoint payload (same compression contract as
/// [`encode_parsed_checkpoint`]).
pub fn encode_summary_checkpoint(
    payload: &SummaryCheckpointPayload,
) -> Result<Vec<u8>, StorageError> {
    let json = serde_json::to_vec(payload).map_err(|error| {
        StorageError::query(format!("Failed to serialize summary checkpoint: {error}"))
    })?;
    zstd::encode_all(&*json, 3).map_err(|error| {
        StorageError::query(format!("Failed to compress summary checkpoint: {error}"))
    })
}

/// Decode a stored summary checkpoint payload.
///
/// Returns `None` when the payload cannot be decoded or was written by an
/// incompatible schema version; callers regenerate the summary in that case.
pub fn decode_summary_checkpoint(bytes: &[u8]) -> Option<SummaryCheckpointPayload> {
    let json = zstd::decode_all(bytes).ok()?;
    serde_json::from_slice(&json)
        .ok()
        .filter(SummaryCheckpointPayload::is_compatible)
}

/// Persist generated file summaries into the batch's file checkpoints.
///
/// Summaries are generated after parsing, so they are written to the
/// dedicated `summary_data` column instead of rewriting the parse blob. The
/// upsert preserves the existing parse checkpoint (COALESCE semantics), so
/// this pass only ever attaches summary payloads. Shared by the full-index
/// and hot-update paths.
pub async fn persist_summaries_to_checkpoints(
    cm: &CheckpointManager,
    operation_id: &str,
    batch_index: u32,
    entries: &[(String, cce_parser::summary::FileSummary, Option<String>)],
    summary_config_fingerprint: Option<String>,
) -> Result<(), StorageError> {
    for (path, summary, plugin_fingerprint) in entries {
        let mut checkpoint = cm
            .create_file_checkpoint(operation_id, batch_index, path)
            .await?;
        let payload = SummaryCheckpointPayload::new(
            summary.clone(),
            plugin_fingerprint.clone(),
            summary_config_fingerprint.clone(),
        );
        checkpoint.summary_data = Some(encode_summary_checkpoint(&payload)?);
        checkpoint.updated_at = chrono::Utc::now().to_rfc3339();
        cm.save_file_checkpoint(&checkpoint).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsed_payload_roundtrip_preserves_envelope() {
        let parsed = cce_types::ParsedFile::default();
        let payload = ParsedCheckpointPayload::Parsed(Box::new(ParsedCheckpointEnvelope::new(
            FileChangeType::Modified,
            parsed,
        )));

        let encoded = encode_parsed_checkpoint(&payload).expect("payload encodes");
        let back = decode_parsed_checkpoint(&encoded).expect("payload decodes");

        assert!(back.is_compatible());
        match back {
            ParsedCheckpointPayload::Parsed(envelope) => {
                assert_eq!(envelope.change_type, FileChangeType::Modified);
                assert_eq!(envelope.schema_version, PARSED_CHECKPOINT_SCHEMA_VERSION);
            }
            ParsedCheckpointPayload::Deleted => panic!("variant must survive roundtrip"),
        }
    }

    #[test]
    fn deleted_payload_roundtrip() {
        let encoded =
            encode_parsed_checkpoint(&ParsedCheckpointPayload::Deleted).expect("payload encodes");
        let back = decode_parsed_checkpoint(&encoded).expect("payload decodes");

        assert!(matches!(back, ParsedCheckpointPayload::Deleted));
        assert!(back.is_compatible(), "tombstones carry no versioned data");
    }

    /// Files without custom languages record no plugin fingerprint and stay
    /// valid regardless of registry changes; custom-language files pin the
    /// current fingerprint and invalidate once it drifts.
    #[test]
    fn plugin_fingerprint_gates_custom_language_envelopes() {
        let plain = ParsedCheckpointEnvelope::new(
            FileChangeType::Modified,
            cce_types::ParsedFile {
                language: cce_types::Language::Rust,
                ..Default::default()
            },
        );
        assert!(plain.plugin_language_fingerprint.is_none());
        assert!(plain.is_compatible());

        let custom = ParsedCheckpointEnvelope::new(
            FileChangeType::Modified,
            cce_types::ParsedFile {
                language: cce_types::Language::Custom(0),
                ..Default::default()
            },
        );
        assert!(custom.plugin_language_fingerprint.is_some());
        assert!(custom.is_compatible());

        // Simulate a registry change between write and read.
        let mut stale = custom;
        stale.plugin_language_fingerprint = Some("drifted-fingerprint".to_string());
        assert!(!stale.is_compatible());
    }

    #[test]
    fn summary_payload_roundtrip_preserves_fingerprint() {
        let summary = cce_parser::summary::FileSummary::new("src/lib.rs")
            .with_summary("Main entry point")
            .with_entities(vec!["run".to_string()]);
        let payload = SummaryCheckpointPayload::new(summary, None, Some("fp-1".to_string()));

        let encoded = encode_summary_checkpoint(&payload).expect("summary encodes");
        let mut back = decode_summary_checkpoint(&encoded).expect("summary decodes");
        assert_eq!(back.file_summary.summary_text, "Main entry point");
        assert_eq!(back.file_summary.main_entities, vec!["run".to_string()]);
        assert_eq!(back.summary_config_fingerprint.as_deref(), Some("fp-1"));

        // An incompatible schema version must make recovery regenerate.
        back.schema_version = 999;
        let tampered = encode_summary_checkpoint(&back).expect("tampered payload encodes");
        assert!(
            decode_summary_checkpoint(&tampered).is_none(),
            "incompatible summary payloads must be rejected"
        );
    }

    #[test]
    fn parsed_payload_compresses_source_bearing_data() {
        let parsed = cce_types::ParsedFile {
            source: "fn example() {}".repeat(512).into(),
            ..Default::default()
        };
        let payload = ParsedCheckpointPayload::Parsed(Box::new(ParsedCheckpointEnvelope::new(
            FileChangeType::Modified,
            parsed,
        )));

        let json_len = serde_json::to_vec(&payload)
            .expect("payload serializes")
            .len();
        let encoded = encode_parsed_checkpoint(&payload).expect("payload encodes");
        assert!(
            encoded.len() < json_len / 2,
            "compressed payload ({}) must be far smaller than raw JSON ({})",
            encoded.len(),
            json_len
        );
    }

    #[test]
    fn decode_rejects_undecodable_payloads() {
        assert!(decode_parsed_checkpoint(b"not a zstd stream").is_none());
        assert!(decode_parsed_checkpoint(&[0xff, 0xff, 0xff]).is_none());
    }

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
            FileChangeType::Modified,
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
