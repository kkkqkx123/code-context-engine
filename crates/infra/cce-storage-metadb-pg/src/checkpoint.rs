use cce_types::StorageError;

use cce_storage_common::metadb::{
    BatchCheckpointRecord, CheckpointRecord, CheckpointStatus, FileCheckpointRecord,
    WorkUnitCheckpointRecord, WorkUnitStatus,
};

use crate::error::classify_pg;
use crate::rows::*;

use super::PostgresClient;

const CHECKPOINT_COLUMNS: &str = "id, project_id, operation_id, operation_type, root_dir, \
     total_files, batch_size, current_batch_index, current_phase, file_list_hash, created_at, \
     updated_at, last_error, failure_count, status, active_flag, priority, last_heartbeat, \
     failed_at";

const FILE_CHECKPOINT_COLUMNS: &str = "id, operation_id, batch_index, file_path, file_id, \
     language, file_size, content_hash, parsed_data, parse_error, summary_data, \
     embedding_count, bm25_doc_id, export_path, render_fingerprint, module_progress, \
     created_at, updated_at";

impl PostgresClient {
    pub(crate) async fn checkpoint_create(
        &self,
        project_id: i64,
        checkpoint: &CheckpointRecord,
    ) -> Result<i64, StorageError> {
        let checkpoint = checkpoint.clone();
        self.run(async move {
            let client = self.pooled().await?;
            let id: i64 = client
                .query_one(
                    "INSERT INTO checkpoint (project_id, operation_id, operation_type, root_dir, \
                     total_files, batch_size, current_batch_index, current_phase, file_list_hash, \
                     created_at, updated_at, status, active_flag, priority, last_heartbeat, \
                     failed_at) \
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16) \
                     RETURNING id",
                    &[
                        &project_id,
                        &checkpoint.operation_id,
                        &checkpoint.operation_type,
                        &checkpoint.root_dir,
                        &(checkpoint.total_files as i64),
                        &(checkpoint.batch_size as i64),
                        &(checkpoint.current_batch_index as i64),
                        &checkpoint.current_phase,
                        &checkpoint.file_list_hash,
                        &checkpoint.created_at,
                        &checkpoint.updated_at,
                        &checkpoint.status.as_str(),
                        &(checkpoint.active_flag as i64),
                        &(checkpoint.priority as i64),
                        &checkpoint.last_heartbeat,
                        &checkpoint.failed_at,
                    ],
                )
                .await
                .map_err(classify_pg)?
                .get(0);
            Ok(id)
        })
        .await
    }

    pub(crate) async fn checkpoint_get(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<Option<CheckpointRecord>, StorageError> {
        let operation_id = operation_id.to_string();
        self.run(async move {
            let client = self.pooled().await?;
            let row = client
                .query_opt(
                    "SELECT id, project_id, operation_id, operation_type, root_dir, total_files, \
                     batch_size, current_batch_index, current_phase, file_list_hash, created_at, \
                     updated_at, last_error, failure_count, status, active_flag, priority, \
                     last_heartbeat, failed_at FROM checkpoint \
                     WHERE project_id = $1 AND operation_id = $2 LIMIT 1",
                    &[&project_id, &operation_id],
                )
                .await
                .map_err(classify_pg)?;
            row.map(|row| checkpoint_from_row(&row)).transpose()
        })
        .await
    }

    pub(crate) async fn checkpoint_set_status(
        &self,
        project_id: i64,
        operation_id: &str,
        status: CheckpointStatus,
    ) -> Result<(), StorageError> {
        let operation_id = operation_id.to_string();
        self.run(async move {
            let client = self.pooled().await?;
            let active_flag: i64 = match status {
                CheckpointStatus::Completed | CheckpointStatus::Failed => 0,
                CheckpointStatus::InProgress => 1,
            };
            let now = chrono::Utc::now().to_rfc3339();
            client
                .execute(
                    "UPDATE checkpoint SET status = $1, active_flag = $2, updated_at = $3 \
                     WHERE project_id = $4 AND operation_id = $5",
                    &[
                        &status.as_str(),
                        &active_flag,
                        &now,
                        &project_id,
                        &operation_id,
                    ],
                )
                .await
                .map_err(classify_pg)?;
            Ok(())
        })
        .await
    }

    pub(crate) async fn checkpoint_update(
        &self,
        project_id: i64,
        checkpoint: &CheckpointRecord,
    ) -> Result<(), StorageError> {
        let checkpoint = checkpoint.clone();
        self.run(async move {
            let client = self.pooled().await?;
            let now = chrono::Utc::now().to_rfc3339();
            client
                .execute(
                    "UPDATE checkpoint SET operation_type = $1, root_dir = $2, total_files = $3, \
                     batch_size = $4, current_batch_index = $5, current_phase = $6, \
                     file_list_hash = $7, updated_at = $8, last_error = $9, failure_count = $10, \
                     status = $11, active_flag = $12, priority = $13, last_heartbeat = $14, \
                     failed_at = $15 WHERE project_id = $16 AND operation_id = $17",
                    &[
                        &checkpoint.operation_type,
                        &checkpoint.root_dir,
                        &(checkpoint.total_files as i64),
                        &(checkpoint.batch_size as i64),
                        &(checkpoint.current_batch_index as i64),
                        &checkpoint.current_phase,
                        &checkpoint.file_list_hash,
                        &now,
                        &checkpoint.last_error,
                        &(checkpoint.failure_count as i64),
                        &checkpoint.status.as_str(),
                        &(checkpoint.active_flag as i64),
                        &(checkpoint.priority as i64),
                        &checkpoint.last_heartbeat,
                        &checkpoint.failed_at,
                        &project_id,
                        &checkpoint.operation_id,
                    ],
                )
                .await
                .map_err(classify_pg)?;
            Ok(())
        })
        .await
    }

    pub(crate) async fn file_checkpoint_upsert(
        &self,
        project_id: i64,
        file: &FileCheckpointRecord,
    ) -> Result<(), StorageError> {
        let file = file.clone();
        self.run(async move {
            let client = self.pooled().await?;
            let now = chrono::Utc::now().to_rfc3339();
            let updated = client
                .execute(
                    "UPDATE checkpoint_file SET batch_index = $1, \
                     language = COALESCE($2, language), file_size = COALESCE($3, file_size), \
                     content_hash = COALESCE($4, content_hash), \
                     parsed_data = COALESCE($5, parsed_data), \
                     parse_error = COALESCE($6, parse_error), \
                     summary_data = COALESCE($7, summary_data), \
                     embedding_count = COALESCE($8, embedding_count), \
                     bm25_doc_id = COALESCE($9, bm25_doc_id), \
                     export_path = COALESCE($10, export_path), \
                     render_fingerprint = COALESCE($11, render_fingerprint), \
                     module_progress = COALESCE($12, module_progress), updated_at = $13 \
                     WHERE project_id = $14 AND operation_id = $15 AND file_path = $16",
                    &[
                        &(file.batch_index as i64),
                        &file.language,
                        &file.file_size,
                        &file.content_hash,
                        &file.parsed_data,
                        &file.parse_error,
                        &file.summary_data,
                        &Some(file.embedding_count as i64),
                        &file.bm25_doc_id,
                        &file.export_path,
                        &file.render_fingerprint,
                        &file.module_progress,
                        &now,
                        &project_id,
                        &file.operation_id,
                        &file.file_path,
                    ],
                )
                .await
                .map_err(classify_pg)?;
            if updated == 0 {
                client
                    .execute(
                        "INSERT INTO checkpoint_file (project_id, operation_id, batch_index, \
                         file_path, file_id, language, file_size, content_hash, parsed_data, \
                         parse_error, summary_data, embedding_count, bm25_doc_id, export_path, \
                         render_fingerprint, module_progress, created_at, updated_at) \
                         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, \
                         $15, $16, $17, $17)",
                        &[
                            &project_id,
                            &file.operation_id,
                            &(file.batch_index as i64),
                            &file.file_path,
                            &file.file_id,
                            &file.language,
                            &file.file_size,
                            &file.content_hash,
                            &file.parsed_data,
                            &file.parse_error,
                            &file.summary_data,
                            &(file.embedding_count as i64),
                            &file.bm25_doc_id,
                            &file.export_path,
                            &file.render_fingerprint,
                            &file.module_progress,
                            &now,
                        ],
                    )
                    .await
                    .map_err(classify_pg)?;
            }
            Ok(())
        })
        .await
    }

    pub(crate) async fn file_checkpoint_get(
        &self,
        project_id: i64,
        operation_id: &str,
        file_path: &str,
    ) -> Result<Option<FileCheckpointRecord>, StorageError> {
        let operation_id = operation_id.to_string();
        let file_path = file_path.to_string();
        self.run(async move {
            let client = self.pooled().await?;
            let row = client
                .query_opt(
                    "SELECT id, operation_id, batch_index, file_path, file_id, language, \
                     file_size, content_hash, parsed_data, parse_error, summary_data, \
                     embedding_count, bm25_doc_id, export_path, render_fingerprint, \
                     module_progress, created_at, updated_at FROM checkpoint_file \
                     WHERE project_id = $1 AND operation_id = $2 AND file_path = $3 LIMIT 1",
                    &[&project_id, &operation_id, &file_path],
                )
                .await
                .map_err(classify_pg)?;
            row.map(|row| file_checkpoint_from_row(project_id, &row))
                .transpose()
        })
        .await
    }

    pub(crate) async fn checkpoint_files_delete_by_operation(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<usize, StorageError> {
        let operation_id = operation_id.to_string();
        self.run(async move {
            let client = self.pooled().await?;
            let removed = client
                .execute(
                    "DELETE FROM checkpoint_file WHERE project_id = $1 AND operation_id = $2",
                    &[&project_id, &operation_id],
                )
                .await
                .map_err(classify_pg)?;
            Ok(removed as usize)
        })
        .await
    }

    pub(crate) async fn work_unit_insert(
        &self,
        record: &WorkUnitCheckpointRecord,
    ) -> Result<i64, StorageError> {
        let record = record.clone();
        self.run(async move {
            let client = self.pooled().await?;
            let id: i64 = client
                .query_one(
                    "INSERT INTO work_unit_checkpoint (project_id, operation_id, stage, \
                     target_epoch, work_unit_hash, status, item_count, created_at, updated_at) \
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING id",
                    &[
                        &record.project_id,
                        &record.operation_id,
                        &record.stage,
                        &record.target_epoch,
                        &record.work_unit_hash,
                        &record.status.as_str(),
                        &(record.item_count as i64),
                        &record.created_at,
                        &record.updated_at,
                    ],
                )
                .await
                .map_err(classify_pg)?
                .get(0);
            Ok(id)
        })
        .await
    }

    pub(crate) async fn work_unit_set_status(
        &self,
        project_id: i64,
        operation_id: &str,
        stage: &str,
        work_unit_hash: &str,
        status: WorkUnitStatus,
    ) -> Result<(), StorageError> {
        let operation_id = operation_id.to_string();
        let stage = stage.to_string();
        let work_unit_hash = work_unit_hash.to_string();
        self.run(async move {
            let client = self.pooled().await?;
            let now = chrono::Utc::now().to_rfc3339();
            client
                .execute(
                    "UPDATE work_unit_checkpoint SET status = $1, updated_at = $2 \
                     WHERE project_id = $3 AND operation_id = $4 AND stage = $5 \
                     AND work_unit_hash = $6",
                    &[
                        &status.as_str(),
                        &now,
                        &project_id,
                        &operation_id,
                        &stage,
                        &work_unit_hash,
                    ],
                )
                .await
                .map_err(classify_pg)?;
            Ok(())
        })
        .await
    }

    pub(crate) async fn work_units_list(
        &self,
        project_id: i64,
        operation_id: &str,
        stage: &str,
    ) -> Result<Vec<WorkUnitCheckpointRecord>, StorageError> {
        let operation_id = operation_id.to_string();
        let stage = stage.to_string();
        self.run(async move {
            let client = self.pooled().await?;
            let rows = client
                .query(
                    "SELECT id, project_id, operation_id, stage, target_epoch, work_unit_hash, \
                     status, item_count, created_at, updated_at FROM work_unit_checkpoint \
                     WHERE project_id = $1 AND operation_id = $2 AND stage = $3 ORDER BY id ASC",
                    &[&project_id, &operation_id, &stage],
                )
                .await
                .map_err(classify_pg)?;
            rows.iter().map(work_unit_from_row).collect()
        })
        .await
    }

    pub(crate) async fn work_unit_by_hash(
        &self,
        project_id: i64,
        operation_id: &str,
        stage: &str,
        work_unit_hash: &str,
    ) -> Result<Option<WorkUnitCheckpointRecord>, StorageError> {
        let operation_id = operation_id.to_string();
        let stage = stage.to_string();
        let work_unit_hash = work_unit_hash.to_string();
        self.run(async move {
            let client = self.pooled().await?;
            let row = client
                .query_opt(
                    "SELECT id, project_id, operation_id, stage, target_epoch, work_unit_hash, \
                     status, item_count, created_at, updated_at FROM work_unit_checkpoint \
                     WHERE project_id = $1 AND operation_id = $2 AND stage = $3 \
                     AND work_unit_hash = $4 LIMIT 1",
                    &[&project_id, &operation_id, &stage, &work_unit_hash],
                )
                .await
                .map_err(classify_pg)?;
            row.map(|row| work_unit_from_row(&row)).transpose()
        })
        .await
    }
}

impl PostgresClient {
    pub(crate) async fn checkpoint_active(
        &self,
        project_id: i64,
    ) -> Result<Option<CheckpointRecord>, StorageError> {
        self.run(async move {
            let client = self.pooled().await?;
            let row = client
                .query_opt(
                    &format!(
                        "SELECT {CHECKPOINT_COLUMNS} FROM checkpoint \
                         WHERE project_id = $1 AND active_flag = 1 \
                         ORDER BY id LIMIT 1"
                    ),
                    &[&project_id],
                )
                .await
                .map_err(classify_pg)?;
            row.map(|row| checkpoint_from_row(&row)).transpose()
        })
        .await
    }

    pub(crate) async fn checkpoint_set_active(
        &self,
        project_id: i64,
        operation_id: &str,
        priority: i32,
    ) -> Result<(), StorageError> {
        let operation_id = operation_id.to_string();
        let priority = priority as i64;
        self.run(async move {
            let client = self.pooled().await?;
            let now = chrono::Utc::now().to_rfc3339();
            client
                .execute(
                    "UPDATE checkpoint SET active_flag = 1, priority = $1, \
                     last_heartbeat = $2, updated_at = $2 \
                     WHERE project_id = $3 AND operation_id = $4",
                    &[&priority, &now, &project_id, &operation_id],
                )
                .await
                .map_err(classify_pg)?;
            Ok(())
        })
        .await
    }

    pub(crate) async fn checkpoint_clear_active(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<usize, StorageError> {
        let operation_id = operation_id.to_string();
        self.run(async move {
            let client = self.pooled().await?;
            let now = chrono::Utc::now().to_rfc3339();
            let changed = client
                .execute(
                    "UPDATE checkpoint SET active_flag = 0, updated_at = $1 \
                     WHERE project_id = $2 AND operation_id = $3 AND active_flag = 1",
                    &[&now, &project_id, &operation_id],
                )
                .await
                .map_err(classify_pg)?;
            Ok(changed as usize)
        })
        .await
    }

    pub(crate) async fn checkpoint_heartbeat(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<(), StorageError> {
        let operation_id = operation_id.to_string();
        self.run(async move {
            let client = self.pooled().await?;
            let now = chrono::Utc::now().to_rfc3339();
            let changed = client
                .execute(
                    "UPDATE checkpoint SET last_heartbeat = $1, updated_at = $1 \
                     WHERE project_id = $2 AND operation_id = $3 AND active_flag = 1",
                    &[&now, &project_id, &operation_id],
                )
                .await
                .map_err(classify_pg)?;
            if changed == 0 {
                return Err(StorageError::not_found(format!(
                    "Active checkpoint not found: {operation_id}"
                )));
            }
            Ok(())
        })
        .await
    }

    pub(crate) async fn checkpoints_cleanup_stale(
        &self,
        project_id: i64,
        stale_threshold_secs: i64,
    ) -> Result<usize, StorageError> {
        self.run(async move {
            let client = self.pooled().await?;
            let now = chrono::Utc::now().to_rfc3339();
            let cutoff =
                (chrono::Utc::now() - chrono::Duration::seconds(stale_threshold_secs)).to_rfc3339();
            let changed = client
                .execute(
                    "UPDATE checkpoint SET active_flag = 0, updated_at = $1 \
                     WHERE project_id = $2 AND active_flag = 1 \
                       AND (last_heartbeat IS NULL OR last_heartbeat < $3)",
                    &[&now, &project_id, &cutoff],
                )
                .await
                .map_err(classify_pg)?;
            Ok(changed as usize)
        })
        .await
    }

    pub(crate) async fn checkpoints_delete_expired(
        &self,
        project_id: i64,
        ttl_seconds: u64,
    ) -> Result<usize, StorageError> {
        self.run(async move {
            let client = self.pooled().await?;
            let cutoff = (chrono::Utc::now()
                - chrono::Duration::seconds(ttl_seconds.min(i64::MAX as u64) as i64))
            .to_rfc3339();
            let changed = client
                .execute(
                    "DELETE FROM checkpoint WHERE project_id = $1 \
                     AND status IN ('completed', 'failed') \
                     AND updated_at < $2 AND active_flag = 0",
                    &[&project_id, &cutoff],
                )
                .await
                .map_err(classify_pg)?;
            Ok(changed as usize)
        })
        .await
    }

    pub(crate) async fn checkpoints_unfinished(
        &self,
        project_id: i64,
    ) -> Result<Vec<CheckpointRecord>, StorageError> {
        self.run(async move {
            let client = self.pooled().await?;
            let rows = client
                .query(
                    &format!(
                        "SELECT {CHECKPOINT_COLUMNS} FROM checkpoint \
                         WHERE project_id = $1 AND status = 'in_progress' ORDER BY created_at"
                    ),
                    &[&project_id],
                )
                .await
                .map_err(classify_pg)?;
            rows.iter().map(checkpoint_from_row).collect()
        })
        .await
    }

    pub(crate) async fn checkpoint_latest_incomplete(
        &self,
        project_id: i64,
        operation_type: &str,
        root_dir: &str,
    ) -> Result<Option<CheckpointRecord>, StorageError> {
        let operation_type = operation_type.to_string();
        let root_dir = root_dir.to_string();
        self.run(async move {
            let client = self.pooled().await?;
            let row = client
                .query_opt(
                    &format!(
                        "SELECT {CHECKPOINT_COLUMNS} FROM checkpoint \
                         WHERE project_id = $1 AND status = 'in_progress' \
                           AND operation_type = $2 AND root_dir = $3 \
                         ORDER BY created_at DESC LIMIT 1"
                    ),
                    &[&project_id, &operation_type, &root_dir],
                )
                .await
                .map_err(classify_pg)?;
            row.map(|row| checkpoint_from_row(&row)).transpose()
        })
        .await
    }

    pub(crate) async fn checkpoint_set_batch_index(
        &self,
        project_id: i64,
        operation_id: &str,
        batch_index: u32,
    ) -> Result<(), StorageError> {
        let operation_id = operation_id.to_string();
        self.run(async move {
            let client = self.pooled().await?;
            let now = chrono::Utc::now().to_rfc3339();
            client
                .execute(
                    "UPDATE checkpoint SET current_batch_index = $1, updated_at = $2 \
                     WHERE project_id = $3 AND operation_id = $4",
                    &[&(batch_index as i64), &now, &project_id, &operation_id],
                )
                .await
                .map_err(classify_pg)?;
            Ok(())
        })
        .await
    }

    pub(crate) async fn checkpoint_batch_create(
        &self,
        project_id: i64,
        batch: &BatchCheckpointRecord,
    ) -> Result<(), StorageError> {
        let batch = batch.clone();
        self.run(async move {
            let client = self.pooled().await?;
            client
                .execute(
                    "INSERT INTO checkpoint_batch (project_id, operation_id, batch_index, \
                     first_file, last_file, file_count, processed_files, failed_files, \
                     entities_extracted, relations_found, chunks_generated, vectors_stored, \
                     start_time, end_time, duration_ms, created_at, updated_at) \
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, \
                     $16, $17) \
                     ON CONFLICT (project_id, operation_id, batch_index) DO UPDATE SET \
                        first_file = EXCLUDED.first_file, last_file = EXCLUDED.last_file, \
                        file_count = EXCLUDED.file_count, \
                        processed_files = EXCLUDED.processed_files, \
                        failed_files = EXCLUDED.failed_files, \
                        entities_extracted = EXCLUDED.entities_extracted, \
                        relations_found = EXCLUDED.relations_found, \
                        chunks_generated = EXCLUDED.chunks_generated, \
                        vectors_stored = EXCLUDED.vectors_stored, \
                        end_time = EXCLUDED.end_time, duration_ms = EXCLUDED.duration_ms, \
                        updated_at = EXCLUDED.updated_at",
                    &[
                        &project_id,
                        &batch.operation_id,
                        &(batch.batch_index as i64),
                        &batch.first_file,
                        &batch.last_file,
                        &(batch.file_count as i64),
                        &(batch.processed_files as i64),
                        &(batch.failed_files as i64),
                        &(batch.entities_extracted as i64),
                        &(batch.relations_found as i64),
                        &(batch.chunks_generated as i64),
                        &(batch.vectors_stored as i64),
                        &batch.start_time,
                        &batch.end_time,
                        &batch.duration_ms,
                        &batch.created_at,
                        &batch.updated_at,
                    ],
                )
                .await
                .map_err(classify_pg)?;
            Ok(())
        })
        .await
    }

    pub(crate) async fn checkpoint_batch_get(
        &self,
        project_id: i64,
        operation_id: &str,
        batch_index: u32,
    ) -> Result<Option<BatchCheckpointRecord>, StorageError> {
        let operation_id = operation_id.to_string();
        self.run(async move {
            let client = self.pooled().await?;
            let row = client
                .query_opt(
                    &format!(
                        "SELECT {BATCH_CHECKPOINT_COLUMNS} FROM checkpoint_batch \
                         WHERE project_id = $1 AND operation_id = $2 AND batch_index = $3 LIMIT 1"
                    ),
                    &[&project_id, &operation_id, &(batch_index as i64)],
                )
                .await
                .map_err(classify_pg)?;
            row.map(|row| batch_checkpoint_from_row(&row)).transpose()
        })
        .await
    }

    pub(crate) async fn checkpoint_batch_files(
        &self,
        project_id: i64,
        operation_id: &str,
        batch_index: u32,
    ) -> Result<Vec<FileCheckpointRecord>, StorageError> {
        let operation_id = operation_id.to_string();
        self.run(async move {
            let client = self.pooled().await?;
            let rows = client
                .query(
                    &format!(
                        "SELECT {FILE_CHECKPOINT_COLUMNS} FROM checkpoint_file \
                         WHERE project_id = $1 AND operation_id = $2 AND batch_index = $3 \
                         ORDER BY file_path"
                    ),
                    &[&project_id, &operation_id, &(batch_index as i64)],
                )
                .await
                .map_err(classify_pg)?;
            rows.iter()
                .map(|row| file_checkpoint_from_row(project_id, row))
                .collect()
        })
        .await
    }

    pub(crate) async fn checkpoint_failed_files(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<Vec<(String, String)>, StorageError> {
        let operation_id = operation_id.to_string();
        self.run(async move {
            let client = self.pooled().await?;
            let rows = client
                .query(
                    "SELECT file_path, COALESCE(parse_error, 'Unknown error') \
                     FROM checkpoint_file \
                     WHERE project_id = $1 AND operation_id = $2 AND parse_error IS NOT NULL \
                     ORDER BY file_path",
                    &[&project_id, &operation_id],
                )
                .await
                .map_err(classify_pg)?;
            Ok(rows
                .into_iter()
                .map(|row| (row.get::<_, String>(0), row.get::<_, String>(1)))
                .collect())
        })
        .await
    }

    pub(crate) async fn checkpoint_batches_delete_by_operation(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<usize, StorageError> {
        let operation_id = operation_id.to_string();
        self.run(async move {
            let client = self.pooled().await?;
            let changed = client
                .execute(
                    "DELETE FROM checkpoint_batch \
                     WHERE project_id = $1 AND operation_id = $2",
                    &[&project_id, &operation_id],
                )
                .await
                .map_err(classify_pg)?;
            Ok(changed as usize)
        })
        .await
    }

    pub(crate) async fn checkpoint_file_set_parsed_data(
        &self,
        project_id: i64,
        operation_id: &str,
        file_path: &str,
        data: &[u8],
    ) -> Result<bool, StorageError> {
        let operation_id = operation_id.to_string();
        let file_path = file_path.to_string();
        let data = data.to_vec();
        self.run(async move {
            let client = self.pooled().await?;
            let changed = client
                .execute(
                    "UPDATE checkpoint_file SET parsed_data = $1 \
                     WHERE project_id = $2 AND operation_id = $3 AND file_path = $4",
                    &[&data, &project_id, &operation_id, &file_path],
                )
                .await
                .map_err(classify_pg)?;
            Ok(changed > 0)
        })
        .await
    }

    pub(crate) async fn file_checkpoints_clear_module_progress(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<usize, StorageError> {
        let operation_id = operation_id.to_string();
        self.run(async move {
            let client = self.pooled().await?;
            let now = chrono::Utc::now().to_rfc3339();
            let changed = client
                .execute(
                    "UPDATE checkpoint_file SET module_progress = NULL, updated_at = $1 \
                     WHERE project_id = $2 AND operation_id = $3 AND module_progress IS NOT NULL",
                    &[&now, &project_id, &operation_id],
                )
                .await
                .map_err(classify_pg)?;
            Ok(changed as usize)
        })
        .await
    }

    pub(crate) async fn work_units_insert_batch(
        &self,
        records: &[WorkUnitCheckpointRecord],
    ) -> Result<Vec<i64>, StorageError> {
        let records = records.to_vec();
        if records.is_empty() {
            return Ok(Vec::new());
        }
        self.run(async move {
            let mut client = self.pooled().await?;
            let tx = client.transaction().await.map_err(classify_pg)?;
            let mut ids = Vec::with_capacity(records.len());
            for record in &records {
                let id: i64 = tx
                    .query_one(
                        "INSERT INTO work_unit_checkpoint (project_id, operation_id, stage, \
                         target_epoch, work_unit_hash, status, item_count, created_at, updated_at) \
                         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING id",
                        &[
                            &record.project_id,
                            &record.operation_id,
                            &record.stage,
                            &record.target_epoch,
                            &record.work_unit_hash,
                            &record.status.as_str(),
                            &(record.item_count as i64),
                            &record.created_at,
                            &record.updated_at,
                        ],
                    )
                    .await
                    .map_err(classify_pg)?
                    .get(0);
                ids.push(id);
            }
            tx.commit().await.map_err(classify_pg)?;
            Ok(ids)
        })
        .await
    }

    pub(crate) async fn work_units_delete_by_operation(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<usize, StorageError> {
        let operation_id = operation_id.to_string();
        self.run(async move {
            let client = self.pooled().await?;
            let changed = client
                .execute(
                    "DELETE FROM work_unit_checkpoint \
                     WHERE project_id = $1 AND operation_id = $2",
                    &[&project_id, &operation_id],
                )
                .await
                .map_err(classify_pg)?;
            Ok(changed as usize)
        })
        .await
    }
}
