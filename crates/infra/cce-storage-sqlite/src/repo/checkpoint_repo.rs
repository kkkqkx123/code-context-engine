//! Checkpoint repository for unified checkpoint system
//!
//! This module manages the checkpoint system with unified table structure
//! that supports file-level tracking and module-level state management.

use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::types::{
    BatchCheckpointRecord, CheckpointRecord, CheckpointStatus, FileCheckpointRecord,
    WorkUnitCheckpointRecord, WorkUnitStatus,
};
use cce_types::StorageError;

/// Checkpoint repository for CRUD operations
pub struct CheckpointRepository;

impl CheckpointRepository {
    /// Create a new checkpoint record
    pub fn create_checkpoint(
        tx: &Transaction,
        project_id: i64,
        checkpoint: &CheckpointRecord,
    ) -> Result<i64, StorageError> {
        tx.execute(
            "INSERT INTO checkpoint
              (project_id, operation_id, operation_type, root_dir, total_files, batch_size,
               current_batch_index, current_phase, file_list_hash,
               created_at, updated_at, status,
               active_flag, priority, last_heartbeat,
               failed_at)
              VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                project_id,
                &checkpoint.operation_id,
                &checkpoint.operation_type,
                &checkpoint.root_dir,
                checkpoint.total_files,
                checkpoint.batch_size,
                checkpoint.current_batch_index,
                &checkpoint.current_phase,
                &checkpoint.file_list_hash,
                &checkpoint.created_at,
                &checkpoint.updated_at,
                checkpoint.status.as_str(),
                checkpoint.active_flag as i32,
                checkpoint.priority,
                &checkpoint.last_heartbeat,
                &checkpoint.failed_at,
            ],
        )
        .map_err(|e| StorageError::insert(format!("Failed to create checkpoint: {}", e)))?;

        Ok(tx.last_insert_rowid())
    }

    /// Get a checkpoint by operation ID
    pub fn get_checkpoint(
        conn: &Connection,
        project_id: i64,
        operation_id: &str,
    ) -> Result<Option<CheckpointRecord>, StorageError> {
        let mut stmt = conn
            .prepare(
                "SELECT id, project_id, operation_id, operation_type, root_dir, total_files, batch_size,
                         current_batch_index, current_phase, file_list_hash,
                         created_at, updated_at, last_error, failure_count, status,
                         active_flag, priority, last_heartbeat,
                         failed_at
                  FROM checkpoint
                  WHERE project_id = ? AND operation_id = ?
                  LIMIT 1",
            )
            .map_err(|e| StorageError::query(e.to_string()))?;

        let checkpoint = stmt
            .query_row(params![project_id, operation_id], |row| {
                Ok(CheckpointRecord {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    operation_id: row.get(2)?,
                    operation_type: row.get(3)?,
                    root_dir: row.get(4)?,
                    total_files: row.get(5)?,
                    batch_size: row.get(6)?,
                    current_batch_index: row.get(7)?,
                    current_phase: row.get(8)?,
                    file_list_hash: row.get(9)?,
                    created_at: row.get(10)?,
                    updated_at: row.get(11)?,
                    last_error: row.get(12)?,
                    failure_count: row.get(13)?,
                    status: row.get(14)?,
                    active_flag: row.get::<_, i32>(15)? != 0,
                    priority: row.get(16)?,
                    last_heartbeat: row.get(17)?,
                    failed_at: row.get(18)?,
                })
            })
            .optional()
            .map_err(|e| StorageError::query(e.to_string()))?;

        Ok(checkpoint)
    }

    /// Update checkpoint status (also clears active_flag for terminal states)
    pub fn update_checkpoint_status(
        tx: &Transaction,
        project_id: i64,
        operation_id: &str,
        status: CheckpointStatus,
    ) -> Result<(), StorageError> {
        let now = Utc::now().to_rfc3339();
        let active_flag: i32 = match status {
            CheckpointStatus::Completed | CheckpointStatus::Failed => 0,
            CheckpointStatus::InProgress => 1,
        };
        tx.execute(
            "UPDATE checkpoint SET status = ?, active_flag = ?, updated_at = ? WHERE project_id = ? AND operation_id = ?",
            params![status.as_str(), active_flag, &now, project_id, operation_id],
        )
        .map_err(|e| StorageError::update(format!("Failed to update checkpoint status: {}", e)))?;

        Ok(())
    }

    /// Update current batch index
    pub fn update_current_batch_index(
        tx: &Transaction,
        project_id: i64,
        operation_id: &str,
        batch_index: u32,
    ) -> Result<(), StorageError> {
        tx.execute(
            "UPDATE checkpoint SET current_batch_index = ?, updated_at = ? WHERE project_id = ? AND operation_id = ?",
            params![batch_index, Utc::now().to_rfc3339(), project_id, operation_id],
        )
        .map_err(|e| {
            StorageError::update(format!("Failed to update current batch index: {}", e))
        })?;

        Ok(())
    }

    /// Update entire checkpoint record
    pub fn update_checkpoint(
        tx: &Transaction,
        project_id: i64,
        checkpoint: &CheckpointRecord,
    ) -> Result<(), StorageError> {
        tx.execute(
            "UPDATE checkpoint SET
              operation_type = ?, root_dir = ?, total_files = ?, batch_size = ?,
              current_batch_index = ?, current_phase = ?, file_list_hash = ?,
              updated_at = ?, last_error = ?, failure_count = ?, status = ?,
              active_flag = ?, priority = ?, last_heartbeat = ?,
              failed_at = ?
              WHERE project_id = ? AND operation_id = ?",
            params![
                &checkpoint.operation_type,
                &checkpoint.root_dir,
                checkpoint.total_files,
                checkpoint.batch_size,
                checkpoint.current_batch_index,
                &checkpoint.current_phase,
                &checkpoint.file_list_hash,
                Utc::now().to_rfc3339(),
                &checkpoint.last_error,
                checkpoint.failure_count,
                checkpoint.status.as_str(),
                checkpoint.active_flag as i32,
                checkpoint.priority,
                &checkpoint.last_heartbeat,
                &checkpoint.failed_at,
                project_id,
                &checkpoint.operation_id,
            ],
        )
        .map_err(|e| StorageError::update(format!("Failed to update checkpoint: {}", e)))?;

        Ok(())
    }

    /// Insert a batch checkpoint
    pub fn insert_batch_checkpoint(
        tx: &Transaction,
        project_id: i64,
        batch: &BatchCheckpointRecord,
    ) -> Result<i64, StorageError> {
        tx.execute(
            "INSERT INTO checkpoint_batch
             (project_id, operation_id, batch_index, first_file, last_file, file_count,
              processed_files, failed_files,
              entities_extracted, relations_found, chunks_generated, vectors_stored,
              start_time, end_time, duration_ms, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                project_id,
                &batch.operation_id,
                batch.batch_index,
                &batch.first_file,
                &batch.last_file,
                batch.file_count,
                batch.processed_files,
                batch.failed_files,
                batch.entities_extracted,
                batch.relations_found,
                batch.chunks_generated,
                batch.vectors_stored,
                &batch.start_time,
                &batch.end_time,
                batch.duration_ms,
                &batch.created_at,
                &batch.updated_at,
            ],
        )
        .map_err(|e| StorageError::insert(format!("Failed to insert batch checkpoint: {}", e)))?;

        Ok(tx.last_insert_rowid())
    }

    /// Insert or update a file checkpoint
    ///
    /// The UPDATE branch uses `COALESCE(?, col)` so a `NULL` input retains the
    /// existing column value. This is intentional for incremental writes:
    /// callers that only touch `module_progress` must not clear
    /// `render_fingerprint` and vice versa. Future columns that add a new
    /// field to `checkpoint_file` must follow the same contract:
    /// 1. `create_file_checkpoint` must supply a deterministic default for the
    ///    new column so an INSERT never leaves it unintentionally NULL.
    /// 2. The UPDATE branch must use `COALESCE(?, new_col)` and the intent must
    ///    be documented in the SQL comment.
    /// 3. A direct-write clear path (like `clear_module_progress` bypassing
    ///    COALESCE) must be added if the new column needs explicit NULLing.
    pub fn upsert_file_checkpoint(
        tx: &Transaction,
        project_id: i64,
        file: &FileCheckpointRecord,
    ) -> Result<(), StorageError> {
        let exists: bool = tx
            .query_row(
                "SELECT 1 FROM checkpoint_file WHERE project_id = ? AND operation_id = ? AND file_path = ?",
                params![project_id, &file.operation_id, &file.file_path],
                |_| Ok(true),
            )
            .optional()
            .map_err(|e| StorageError::query(e.to_string()))?
            .unwrap_or(false);

        if exists {
            // UPDATE retains existing values when the caller passes NULL via
            // COALESCE. See method docs for future-column contract.
            tx.execute(
                "UPDATE checkpoint_file SET
                 batch_index = ?, language = COALESCE(?, language),
                 file_size = COALESCE(?, file_size),
                 content_hash = COALESCE(?, content_hash),
                 parsed_data = COALESCE(?, parsed_data),
                 parse_error = COALESCE(?, parse_error),
                 summary_data = COALESCE(?, summary_data),
                 embedding_count = COALESCE(?, embedding_count),
                 bm25_doc_id = COALESCE(?, bm25_doc_id),
                 export_path = COALESCE(?, export_path),
                 render_fingerprint = COALESCE(?, render_fingerprint),
                 module_progress = COALESCE(?, module_progress),
                 updated_at = ?
                 WHERE project_id = ? AND operation_id = ? AND file_path = ?",
                params![
                    file.batch_index,
                    &file.language,
                    file.file_size,
                    &file.content_hash,
                    &file.parsed_data,
                    &file.parse_error,
                    &file.summary_data,
                    file.embedding_count,
                    &file.bm25_doc_id,
                    &file.export_path,
                    &file.render_fingerprint,
                    &file.module_progress,
                    Utc::now().to_rfc3339(),
                    project_id,
                    &file.operation_id,
                    &file.file_path,
                ],
            )
        } else {
            // INSERT
            tx.execute(
                "INSERT INTO checkpoint_file
                 (project_id, operation_id, batch_index, file_path, file_id,
                  language, file_size, content_hash,
                  parsed_data, parse_error, summary_data,
                  embedding_count, bm25_doc_id, export_path,
                  render_fingerprint, module_progress,
                  created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    project_id,
                    &file.operation_id,
                    file.batch_index,
                    &file.file_path,
                    file.file_id,
                    &file.language,
                    file.file_size,
                    &file.content_hash,
                    &file.parsed_data,
                    &file.parse_error,
                    &file.summary_data,
                    file.embedding_count,
                    &file.bm25_doc_id,
                    &file.export_path,
                    &file.render_fingerprint,
                    &file.module_progress,
                    Utc::now().to_rfc3339(),
                    Utc::now().to_rfc3339(),
                ],
            )
        }
        .map_err(|e| StorageError::insert(format!("Failed to upsert file checkpoint: {}", e)))?;

        Ok(())
    }

    /// Get file checkpoint by operation ID and file path
    pub fn get_file_checkpoint(
        conn: &Connection,
        project_id: i64,
        operation_id: &str,
        file_path: &str,
    ) -> Result<Option<FileCheckpointRecord>, StorageError> {
        let mut stmt = conn
            .prepare(
                "SELECT id, operation_id, batch_index, file_path, file_id,
                        language, file_size, content_hash,
                        parsed_data, parse_error,
                        embedding_count, bm25_doc_id, export_path,
                        render_fingerprint, module_progress,
                        created_at, updated_at, summary_data
                 FROM checkpoint_file
                 WHERE project_id = ? AND operation_id = ? AND file_path = ?
                 LIMIT 1",
            )
            .map_err(|e| StorageError::query(e.to_string()))?;

        let file = stmt
            .query_row(params![project_id, operation_id, file_path], |row| {
                Ok(FileCheckpointRecord {
                    id: row.get(0)?,
                    operation_id: row.get(1)?,
                    batch_index: row.get(2)?,
                    file_path: row.get(3)?,
                    file_id: row.get(4)?,
                    language: row.get(5)?,
                    file_size: row.get(6)?,
                    content_hash: row.get(7)?,
                    parsed_data: row.get(8)?,
                    parse_error: row.get(9)?,
                    embedding_count: row.get(10)?,
                    bm25_doc_id: row.get(11)?,
                    export_path: row.get(12)?,
                    render_fingerprint: row.get(13)?,
                    module_progress: row.get(14)?,
                    created_at: row.get(15)?,
                    updated_at: row.get(16)?,
                    summary_data: row.get(17)?,
                })
            })
            .optional()
            .map_err(|e| StorageError::query(e.to_string()))?;

        Ok(file)
    }

    /// Get all files in a batch
    pub fn get_batch_files(
        conn: &Connection,
        project_id: i64,
        operation_id: &str,
        batch_index: u32,
    ) -> Result<Vec<FileCheckpointRecord>, StorageError> {
        let mut stmt = conn
            .prepare(
                "SELECT id, operation_id, batch_index, file_path, file_id,
                        language, file_size, content_hash,
                        parsed_data, parse_error,
                        embedding_count, bm25_doc_id, export_path,
                        render_fingerprint, module_progress,
                        created_at, updated_at, summary_data
                 FROM checkpoint_file
                 WHERE project_id = ? AND operation_id = ? AND batch_index = ?
                 ORDER BY file_path",
            )
            .map_err(|e| StorageError::query(e.to_string()))?;

        let files = stmt
            .query_map(params![project_id, operation_id, batch_index], |row| {
                Ok(FileCheckpointRecord {
                    id: row.get(0)?,
                    operation_id: row.get(1)?,
                    batch_index: row.get(2)?,
                    file_path: row.get(3)?,
                    file_id: row.get(4)?,
                    language: row.get(5)?,
                    file_size: row.get(6)?,
                    content_hash: row.get(7)?,
                    parsed_data: row.get(8)?,
                    parse_error: row.get(9)?,
                    embedding_count: row.get(10)?,
                    bm25_doc_id: row.get(11)?,
                    export_path: row.get(12)?,
                    render_fingerprint: row.get(13)?,
                    module_progress: row.get(14)?,
                    created_at: row.get(15)?,
                    updated_at: row.get(16)?,
                    summary_data: row.get(17)?,
                })
            })
            .map_err(|e| StorageError::query(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| StorageError::query(e.to_string()))?;

        Ok(files)
    }

    /// Get a batch checkpoint by operation ID and batch index
    pub fn get_batch_checkpoint(
        conn: &Connection,
        project_id: i64,
        operation_id: &str,
        batch_index: u32,
    ) -> Result<Option<BatchCheckpointRecord>, StorageError> {
        let mut stmt = conn
            .prepare(
                "SELECT id, operation_id, batch_index, first_file, last_file, file_count,
                        processed_files, failed_files,
                        entities_extracted, relations_found, chunks_generated, vectors_stored,
                        start_time, end_time, duration_ms, created_at, updated_at
                 FROM checkpoint_batch
                 WHERE project_id = ? AND operation_id = ? AND batch_index = ?
                 LIMIT 1",
            )
            .map_err(|e| StorageError::query(e.to_string()))?;

        let batch = stmt
            .query_row(params![project_id, operation_id, batch_index], |row| {
                Ok(BatchCheckpointRecord {
                    id: row.get(0)?,
                    operation_id: row.get(1)?,
                    batch_index: row.get(2)?,
                    first_file: row.get(3)?,
                    last_file: row.get(4)?,
                    file_count: row.get(5)?,
                    processed_files: row.get(6)?,
                    failed_files: row.get(7)?,
                    entities_extracted: row.get(8)?,
                    relations_found: row.get(9)?,
                    chunks_generated: row.get(10)?,
                    vectors_stored: row.get(11)?,
                    start_time: row.get(12)?,
                    end_time: row.get(13)?,
                    duration_ms: row.get(14)?,
                    created_at: row.get(15)?,
                    updated_at: row.get(16)?,
                })
            })
            .optional()
            .map_err(|e| StorageError::query(e.to_string()))?;

        Ok(batch)
    }

    /// Get failed files in an operation
    pub fn get_failed_files(
        conn: &Connection,
        project_id: i64,
        operation_id: &str,
    ) -> Result<Vec<(String, String)>, StorageError> {
        let mut stmt = conn
            .prepare(
                "SELECT file_path, COALESCE(parse_error, 'Unknown error')
                 FROM checkpoint_file
                 WHERE project_id = ? AND operation_id = ? AND parse_error IS NOT NULL
                 ORDER BY file_path",
            )
            .map_err(|e| StorageError::query(e.to_string()))?;

        let files = stmt
            .query_map(params![project_id, operation_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| StorageError::query(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| StorageError::query(e.to_string()))?;

        Ok(files)
    }

    /// Get the latest incomplete checkpoint filtered by operation_type and root_dir
    pub fn get_latest_incomplete_by_type(
        conn: &Connection,
        project_id: i64,
        operation_type: &str,
        root_dir: &str,
    ) -> Result<Option<CheckpointRecord>, StorageError> {
        let mut stmt = conn
            .prepare(
                "SELECT id, project_id, operation_id, operation_type, root_dir, total_files, batch_size,
                        current_batch_index, current_phase, file_list_hash,
                        created_at, updated_at, last_error, failure_count, status,
                        active_flag, priority, last_heartbeat
                 FROM checkpoint
                 WHERE project_id = ? AND status = 'in_progress' AND operation_type = ? AND root_dir = ?
                 ORDER BY created_at DESC
                 LIMIT 1",
            )
            .map_err(|e| StorageError::query(e.to_string()))?;

        let checkpoint = stmt
            .query_row(params![project_id, operation_type, root_dir], |row| {
                Ok(CheckpointRecord {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    operation_id: row.get(2)?,
                    operation_type: row.get(3)?,
                    root_dir: row.get(4)?,
                    total_files: row.get(5)?,
                    batch_size: row.get(6)?,
                    current_batch_index: row.get(7)?,
                    current_phase: row.get(8)?,
                    file_list_hash: row.get(9)?,
                    created_at: row.get(10)?,
                    updated_at: row.get(11)?,
                    last_error: row.get(12)?,
                    failure_count: row.get(13)?,
                    status: row.get(14)?,
                    active_flag: row.get::<_, i32>(15)? != 0,
                    priority: row.get(16)?,
                    last_heartbeat: row.get(17)?,
                    failed_at: None,
                })
            })
            .optional()
            .map_err(|e| StorageError::query(e.to_string()))?;

        Ok(checkpoint)
    }

    /// Validate checkpoint record for corruption
    pub fn validate_checkpoint(checkpoint: &CheckpointRecord) -> Result<(), StorageError> {
        // Validate operation_id not empty
        if checkpoint.operation_id.is_empty() {
            return Err(StorageError::validation(
                "checkpoint operation_id is empty".to_string(),
            ));
        }

        // Validate operation_type
        match checkpoint.operation_type.as_str() {
            "full_index" | "hot_update" | "incremental" => {}
            _ => {
                return Err(StorageError::validation(format!(
                    "Invalid operation_type: {}",
                    checkpoint.operation_type
                )));
            }
        }

        // status is already validated as CheckpointStatus enum
        // current_phase not empty
        if checkpoint.current_phase.is_empty() {
            return Err(StorageError::validation(
                "checkpoint current_phase is empty".to_string(),
            ));
        }

        // Validate timestamps are valid RFC3339
        chrono::DateTime::parse_from_rfc3339(&checkpoint.created_at).map_err(|e| {
            StorageError::validation(format!("Invalid created_at timestamp: {}", e))
        })?;

        chrono::DateTime::parse_from_rfc3339(&checkpoint.updated_at).map_err(|e| {
            StorageError::validation(format!("Invalid updated_at timestamp: {}", e))
        })?;

        Ok(())
    }

    /// Get all unfinished operations (status = in_progress)
    pub fn get_unfinished_operations(
        conn: &Connection,
        project_id: i64,
    ) -> Result<Vec<CheckpointRecord>, StorageError> {
        let mut stmt = conn
            .prepare(
                "SELECT id, project_id, operation_id, operation_type, root_dir, total_files, batch_size,
                        current_batch_index, current_phase, file_list_hash,
                        created_at, updated_at, last_error, failure_count, status,
                        active_flag, priority, last_heartbeat
                 FROM checkpoint
                 WHERE project_id = ? AND status = 'in_progress'
                 ORDER BY created_at ASC",
            )
            .map_err(|e| StorageError::query(e.to_string()))?;

        let records = stmt
            .query_map(params![project_id], |row| {
                Ok(CheckpointRecord {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    operation_id: row.get(2)?,
                    operation_type: row.get(3)?,
                    root_dir: row.get(4)?,
                    total_files: row.get(5)?,
                    batch_size: row.get(6)?,
                    current_batch_index: row.get(7)?,
                    current_phase: row.get(8)?,
                    file_list_hash: row.get(9)?,
                    created_at: row.get(10)?,
                    updated_at: row.get(11)?,
                    last_error: row.get(12)?,
                    failure_count: row.get(13)?,
                    status: row.get(14)?,
                    active_flag: row.get::<_, i32>(15)? != 0,
                    priority: row.get(16)?,
                    last_heartbeat: row.get(17)?,
                    failed_at: None,
                })
            })
            .map_err(|e| StorageError::query(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| StorageError::query(e.to_string()))?;

        Ok(records)
    }

    // ---------------------------------------------------------------------------
    // Work unit checkpoint
    // ---------------------------------------------------------------------------

    /// Insert a work unit checkpoint record
    pub fn insert_work_unit(
        tx: &Transaction,
        record: &WorkUnitCheckpointRecord,
    ) -> Result<i64, StorageError> {
        tx.execute(
            "INSERT INTO work_unit_checkpoint
             (project_id, operation_id, stage, target_epoch, work_unit_hash, status, item_count,
              created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                record.project_id,
                &record.operation_id,
                &record.stage,
                record.target_epoch,
                &record.work_unit_hash,
                record.status.as_str(),
                record.item_count,
                &record.created_at,
                &record.updated_at,
            ],
        )
        .map_err(|e| {
            StorageError::insert(format!("Failed to insert work unit checkpoint: {}", e))
        })?;

        Ok(tx.last_insert_rowid())
    }

    /// Update work unit checkpoint status
    pub fn update_work_unit_status(
        tx: &Transaction,
        project_id: i64,
        operation_id: &str,
        stage: &str,
        work_unit_hash: &str,
        status: WorkUnitStatus,
    ) -> Result<(), StorageError> {
        tx.execute(
            "UPDATE work_unit_checkpoint SET
             status = ?, updated_at = ?
             WHERE project_id = ? AND operation_id = ? AND stage = ? AND work_unit_hash = ?",
            params![
                status.as_str(),
                Utc::now().to_rfc3339(),
                project_id,
                operation_id,
                stage,
                work_unit_hash,
            ],
        )
        .map_err(|e| StorageError::update(format!("Failed to update work unit status: {}", e)))?;

        Ok(())
    }

    /// Get all work units for an operation and stage
    pub fn get_work_units(
        conn: &Connection,
        project_id: i64,
        operation_id: &str,
        stage: &str,
    ) -> Result<Vec<WorkUnitCheckpointRecord>, StorageError> {
        let mut stmt = conn
            .prepare(
                "SELECT id, project_id, operation_id, stage, target_epoch, work_unit_hash,
                        status, item_count, created_at, updated_at
                 FROM work_unit_checkpoint
                 WHERE project_id = ? AND operation_id = ? AND stage = ?
                 ORDER BY id ASC",
            )
            .map_err(|e| StorageError::query(e.to_string()))?;

        let records = stmt
            .query_map(params![project_id, operation_id, stage], |row| {
                Ok(WorkUnitCheckpointRecord {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    operation_id: row.get(2)?,
                    stage: row.get(3)?,
                    target_epoch: row.get(4)?,
                    work_unit_hash: row.get(5)?,
                    status: row.get(6)?,
                    item_count: row.get(7)?,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                })
            })
            .map_err(|e| StorageError::query(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| StorageError::query(e.to_string()))?;

        Ok(records)
    }

    /// Get a specific work unit by hash
    pub fn get_work_unit_by_hash(
        conn: &Connection,
        project_id: i64,
        operation_id: &str,
        stage: &str,
        work_unit_hash: &str,
    ) -> Result<Option<WorkUnitCheckpointRecord>, StorageError> {
        let mut stmt = conn
            .prepare(
                "SELECT id, project_id, operation_id, stage, target_epoch, work_unit_hash,
                        status, item_count, created_at, updated_at
                 FROM work_unit_checkpoint
                 WHERE project_id = ? AND operation_id = ? AND stage = ? AND work_unit_hash = ?
                 LIMIT 1",
            )
            .map_err(|e| StorageError::query(e.to_string()))?;

        let record = stmt
            .query_row(
                params![project_id, operation_id, stage, work_unit_hash],
                |row| {
                    Ok(WorkUnitCheckpointRecord {
                        id: row.get(0)?,
                        project_id: row.get(1)?,
                        operation_id: row.get(2)?,
                        stage: row.get(3)?,
                        target_epoch: row.get(4)?,
                        work_unit_hash: row.get(5)?,
                        status: row.get(6)?,
                        item_count: row.get(7)?,
                        created_at: row.get(8)?,
                        updated_at: row.get(9)?,
                    })
                },
            )
            .optional()
            .map_err(|e| StorageError::query(e.to_string()))?;

        Ok(record)
    }

    // ---------------------------------------------------------------------------
    // Active operation gate methods (merged from active_operations table)
    // ---------------------------------------------------------------------------

    /// Get the currently active checkpoint for a project (active_flag = 1)
    ///
    /// Durable backing store for `OperationQueue::active`. Callers outside
    /// the operation queue should treat this as read-only; mutations must go
    /// through the queue so the in-memory and durable states stay consistent.
    pub fn get_active_checkpoint(
        conn: &Connection,
        project_id: i64,
    ) -> Result<Option<CheckpointRecord>, StorageError> {
        let mut stmt = conn
            .prepare(
                "SELECT id, project_id, operation_id, operation_type, root_dir, total_files, batch_size,
                        current_batch_index, current_phase, file_list_hash,
                        created_at, updated_at, last_error, failure_count, status,
                        active_flag, priority, last_heartbeat
                 FROM checkpoint
                 WHERE project_id = ? AND active_flag = 1
                 LIMIT 1",
            )
            .map_err(|e| StorageError::query(e.to_string()))?;

        let checkpoint = stmt
            .query_row(params![project_id], |row| {
                Ok(CheckpointRecord {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    operation_id: row.get(2)?,
                    operation_type: row.get(3)?,
                    root_dir: row.get(4)?,
                    total_files: row.get(5)?,
                    batch_size: row.get(6)?,
                    current_batch_index: row.get(7)?,
                    current_phase: row.get(8)?,
                    file_list_hash: row.get(9)?,
                    created_at: row.get(10)?,
                    updated_at: row.get(11)?,
                    last_error: row.get(12)?,
                    failure_count: row.get(13)?,
                    status: row.get(14)?,
                    active_flag: row.get::<_, i32>(15)? != 0,
                    priority: row.get(16)?,
                    last_heartbeat: row.get(17)?,
                    failed_at: None,
                })
            })
            .optional()
            .map_err(|e| StorageError::query(e.to_string()))?;

        Ok(checkpoint)
    }

    /// Set active_flag = 1 and record heartbeat (called when operation starts)
    ///
    /// Internal to `OperationQueue::dequeue`; external callers must use the
    /// queue instead of mutating the flag directly.
    pub fn set_active_flag(
        tx: &Transaction,
        project_id: i64,
        operation_id: &str,
        priority: i32,
    ) -> Result<(), StorageError> {
        let now = Utc::now().to_rfc3339();
        tx.execute(
            "UPDATE checkpoint SET
             active_flag = 1, priority = ?, last_heartbeat = ?, updated_at = ?
             WHERE project_id = ? AND operation_id = ?",
            params![priority, &now, &now, project_id, operation_id],
        )
        .map_err(|e| StorageError::update(format!("Failed to set active flag: {}", e)))?;

        Ok(())
    }

    /// Clear active_flag = 0 (called when operation completes or is aborted)
    ///
    /// Internal to `OperationQueue::complete_active` and
    /// `clear_active_by_operation`; external callers must use the queue.
    pub fn clear_active_flag(
        tx: &Transaction,
        project_id: i64,
        operation_id: &str,
    ) -> Result<(), StorageError> {
        tx.execute(
            "UPDATE checkpoint SET
             active_flag = 0, updated_at = ?
             WHERE project_id = ? AND operation_id = ?",
            params![Utc::now().to_rfc3339(), project_id, operation_id],
        )
        .map_err(|e| StorageError::update(format!("Failed to clear active flag: {}", e)))?;

        Ok(())
    }

    /// Update heartbeat for the active operation
    pub fn update_heartbeat(
        conn: &Connection,
        project_id: i64,
        operation_id: &str,
    ) -> Result<(), StorageError> {
        let now = Utc::now().to_rfc3339();
        let rows_affected = conn
            .execute(
                "UPDATE checkpoint SET
                 last_heartbeat = ?, updated_at = ?
                 WHERE project_id = ? AND operation_id = ? AND active_flag = 1",
                params![&now, &now, project_id, operation_id],
            )
            .map_err(|e| StorageError::update(format!("Failed to update heartbeat: {}", e)))?;

        if rows_affected == 0 {
            return Err(cce_types::StorageError::from(
                cce_types::error::common::NotFoundError::new(format!(
                    "Active checkpoint not found: {}",
                    operation_id
                )),
            ));
        }

        Ok(())
    }

    // ---------------------------------------------------------------------------
    // Cleanup methods for operation lifecycle management
    // ---------------------------------------------------------------------------

    /// Delete all checkpoint files for an operation
    pub fn delete_checkpoint_files_by_operation_id(
        conn: &Connection,
        project_id: i64,
        operation_id: &str,
    ) -> Result<usize, StorageError> {
        let deleted = conn
            .execute(
                "DELETE FROM checkpoint_file WHERE project_id = ? AND operation_id = ?",
                params![project_id, operation_id],
            )
            .map_err(|e| {
                StorageError::delete(format!("Failed to delete checkpoint files: {}", e))
            })?;

        Ok(deleted)
    }

    /// Delete all checkpoint batches for an operation
    pub fn delete_checkpoint_batches_by_operation_id(
        conn: &Connection,
        project_id: i64,
        operation_id: &str,
    ) -> Result<usize, StorageError> {
        let deleted = conn
            .execute(
                "DELETE FROM checkpoint_batch WHERE project_id = ? AND operation_id = ?",
                params![project_id, operation_id],
            )
            .map_err(|e| {
                StorageError::delete(format!("Failed to delete checkpoint batches: {}", e))
            })?;

        Ok(deleted)
    }

    /// Delete all work unit checkpoints for an operation
    pub fn delete_work_units_by_operation_id(
        conn: &Connection,
        project_id: i64,
        operation_id: &str,
    ) -> Result<usize, StorageError> {
        let deleted = conn
            .execute(
                "DELETE FROM work_unit_checkpoint WHERE project_id = ? AND operation_id = ?",
                params![project_id, operation_id],
            )
            .map_err(|e| StorageError::delete(format!("Failed to delete work units: {}", e)))?;

        Ok(deleted)
    }

    /// Clear the module progress markers for every file checkpoint of an
    /// operation.
    ///
    /// Module progress markers are only valid while the candidate generation
    /// they were written against survives. When a resumed operation's
    /// candidate can no longer be adopted (e.g. after an abort voided it),
    /// the markers must be cleared so every module re-executes against the
    /// fresh clone. Returns the number of file checkpoints updated.
    pub fn clear_module_progress(
        conn: &Connection,
        project_id: i64,
        operation_id: &str,
    ) -> Result<usize, StorageError> {
        let cleared = conn
            .execute(
                "UPDATE checkpoint_file SET module_progress = NULL, updated_at = ?
                 WHERE project_id = ? AND operation_id = ? AND module_progress IS NOT NULL",
                params![Utc::now().to_rfc3339(), project_id, operation_id],
            )
            .map_err(|e| StorageError::update(format!("Failed to clear module progress: {}", e)))?;

        Ok(cleared)
    }

    /// Delete expired checkpoints (completed/failed + older than TTL)
    ///
    /// Returns the number of checkpoints deleted.
    /// Related records (checkpoint_file, checkpoint_batch, work_unit_checkpoint)
    /// are cascade-deleted via foreign key constraints.
    pub fn delete_expired_checkpoints(
        conn: &Connection,
        project_id: i64,
        ttl_seconds: u64,
    ) -> Result<usize, StorageError> {
        let cutoff = (Utc::now() - chrono::Duration::seconds(ttl_seconds as i64)).to_rfc3339();

        let deleted = conn
            .execute(
                "DELETE FROM checkpoint
                 WHERE project_id = ?
                   AND status IN ('completed', 'failed')
                   AND updated_at < ?
                   AND active_flag = 0",
                params![project_id, &cutoff],
            )
            .map_err(|e| {
                StorageError::delete(format!("Failed to delete expired checkpoints: {}", e))
            })?;

        Ok(deleted)
    }

    /// Clean up stale active operations (heartbeat too old) by clearing their active_flag
    pub fn cleanup_stale_active(
        conn: &Connection,
        project_id: i64,
        stale_threshold_secs: i64,
    ) -> Result<usize, StorageError> {
        let cutoff_time =
            (Utc::now() - chrono::Duration::seconds(stale_threshold_secs)).to_rfc3339();

        let rows_affected = conn
            .execute(
                "UPDATE checkpoint SET
                 active_flag = 0, updated_at = ?
                 WHERE project_id = ? AND active_flag = 1 AND (last_heartbeat IS NULL OR last_heartbeat < ?)",
                params![Utc::now().to_rfc3339(), project_id, &cutoff_time],
            )
            .map_err(|e| {
                StorageError::delete(format!("Failed to cleanup stale active checkpoints: {}", e))
            })?;

        Ok(rows_affected)
    }
}
