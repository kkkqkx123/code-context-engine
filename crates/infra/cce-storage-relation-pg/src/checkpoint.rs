use cce_types::StorageError;

use cce_storage_common::relation::{
    CheckpointRecord, CheckpointStatus, FileCheckpointRecord, WorkUnitCheckpointRecord,
    WorkUnitStatus,
};

use crate::error::classify_pg;
use crate::rows::*;

use super::PostgresClient;

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
