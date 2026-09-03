//! Cross-backend per-file removal and hot-update orchestration.
//!
//! The write-before-delete ordering that keeps previously queryable data
//! intact lives here.

use cce_parser::ast_to_nl::chunker::ChunkedResult;
use cce_storage_sqlite::{ChunkRepository, EntityDetailMappingRepository, FileSummaryRepository};
use cce_types::path::normalize_project_path;

use crate::error::OrchestratorError;

use super::StorageCoordinator;

impl StorageCoordinator {
    /// Delete a file from all storage backends
    pub async fn remove_file(&self, file_path: &std::path::Path) -> Result<(), OrchestratorError> {
        let file_id = normalize_project_path(&file_path.to_string_lossy());

        // Remove from Qdrant (project-scoped)
        if let Some(ref qdrant) = self.qdrant {
            self.ensure_project_group_id()?;
            qdrant
                .delete_by_file_path_scoped(&file_id, &self.project_group_id, None)
                .await?;
        }

        // Remove from BM25 (scoped to project)
        if let Some(ref bm25) = self.bm25 {
            let mut client = bm25.lock().await;
            client
                .delete_by_file_path_scoped("default", &file_id, self.project_id)
                .await?;
        }

        // Remove entity detail mappings, file summaries, and file records via FK cascade.
        // DELETE FROM files cascades to entities → entity_detail_mappings and file_summaries.
        if let Some(client) = self.metadata_store.as_deref() {
            let result = client.with_transaction(|tx| {
                use rusqlite::params;
                tx.execute(
                    "DELETE FROM files WHERE path = ?1 AND project_id = ?2",
                    params![&file_id, self.project_id],
                )
                .ok();
                Ok(())
            });

            result.map_err(OrchestratorError::Storage)?;
        }

        // Remove chunk records from SQLite
        if let Some(client) = self.metadata_store.as_deref() {
            let result = client.with_transaction(|tx| {
                ChunkRepository::delete_by_file_path(tx, &file_id, self.project_id)
            });

            result.map_err(OrchestratorError::Storage)?;
        }

        Ok(())
    }

    /// Hot update: store new data first, then remove old data for a file.
    ///
    /// This write-then-delete order ensures that if the write fails, the
    /// previously queryable generation remains intact (fix for
    /// delete-then-write consistency issue).
    pub async fn hot_update_file(
        &self,
        file_path: &std::path::Path,
        chunks: &[ChunkedResult],
    ) -> Result<(), OrchestratorError> {
        let file_path_str = normalize_project_path(&file_path.to_string_lossy());

        // Step 1: Store new data first (write-before-delete)
        if !chunks.is_empty() {
            self.store_vectors_batched(chunks, 32, 0).await?;
            self.store_bm25(chunks).await?;
        }

        // Step 2: Remove old data after successful write
        if let Some(ref qdrant) = self.qdrant {
            self.ensure_project_group_id()?;
            qdrant
                .delete_by_file_path_scoped(&file_path_str, &self.project_group_id, None)
                .await?;
        }

        if let Some(ref bm25) = self.bm25 {
            let mut client = bm25.lock().await;
            client
                .delete_by_file_path_scoped("default", &file_path_str, self.project_id)
                .await?;
        }

        // Step 3: Remove old entity detail mappings (scope to current epoch)
        if let Some(client) = self.metadata_store.as_deref() {
            let file_id_opt = client
                .with_transaction(|tx| {
                    use rusqlite::{OptionalExtension, params};
                    tx.query_row(
                        "SELECT id FROM files WHERE path = ?1 AND project_id = ?2 AND epoch = ?3",
                        params![&file_path_str, self.project_id, self.epoch()],
                        |row| row.get::<_, i64>(0),
                    )
                    .optional()
                    .map_err(|e| cce_types::StorageError::Sqlite(e.to_string()))
                })
                .map_err(OrchestratorError::Storage)?;

            if let Some(file_id_num) = file_id_opt {
                client
                    .with_transaction(|tx| {
                        EntityDetailMappingRepository::delete_by_file_id_at_epoch(
                            tx,
                            file_id_num,
                            self.epoch(),
                        )?;
                        Ok(())
                    })
                    .map_err(OrchestratorError::Storage)?;
            }
        }
        Ok(())
    }

    /// Hot update: delete old vectors and store new vectors for a file
    pub async fn hot_update_vectors_file(
        &self,
        file_path: &std::path::Path,
        chunks: &[ChunkedResult],
    ) -> Result<(), OrchestratorError> {
        let file_path_str = normalize_project_path(&file_path.to_string_lossy());

        // Step 1: Remove old vectors from Qdrant (project-scoped)
        if let Some(ref qdrant) = self.qdrant {
            self.ensure_project_group_id()?;
            qdrant
                .delete_by_file_path_scoped(&file_path_str, &self.project_group_id, None)
                .await?;
        }

        // Step 2: Clear old Qdrant references (scope to current epoch)
        if let Some(client) = self.metadata_store.as_deref() {
            let file_id_opt = client
                .with_transaction(|tx| {
                    use rusqlite::{OptionalExtension, params};
                    tx.query_row(
                        "SELECT id FROM files WHERE path = ?1 AND project_id = ?2 AND epoch = ?3",
                        params![&file_path_str, self.project_id, self.epoch()],
                        |row| row.get::<_, i64>(0),
                    )
                    .optional()
                    .map_err(|e| cce_types::StorageError::Sqlite(e.to_string()))
                })
                .map_err(OrchestratorError::Storage)?;

            if let Some(file_id_num) = file_id_opt {
                client
                    .with_transaction(|tx| {
                        FileSummaryRepository::clear_qdrant_point_id_at_epoch(
                            tx,
                            file_id_num,
                            self.epoch(),
                        )?;
                        Ok(())
                    })
                    .map_err(OrchestratorError::Storage)?;
            }
        }

        // Step 3: Store new vectors (skip if empty)
        if chunks.is_empty() {
            return Ok(());
        }

        // Store new vectors and entity mappings (use batch processing with default settings)
        self.store_vectors_batched(chunks, 32, 0).await?;

        Ok(())
    }

    /// Hot update: delete old BM25 documents and store new ones for a file
    pub async fn hot_update_bm25_file(
        &self,
        _file_path: &std::path::Path,
        chunks: &[ChunkedResult],
    ) -> Result<(), OrchestratorError> {
        // Store before any destructive cleanup. Batch indexing replaces equal
        // logical document IDs in one Tantivy commit, so a failed write leaves
        // the previously queryable generation intact.
        if chunks.is_empty() {
            return Ok(());
        }

        // Store new BM25 documents and entity mappings.
        self.store_bm25(chunks).await?;

        Ok(())
    }

    /// Remove file from vector index only (Qdrant + entity mappings)
    pub async fn remove_file_from_vectors(
        &self,
        file_path: &std::path::Path,
    ) -> Result<(), OrchestratorError> {
        let file_id = normalize_project_path(&file_path.to_string_lossy());

        // Inside a hot operation the deletion is registered against the
        // candidate generation (override + own-row cleanup); the published
        // generation remains untouched until manifest activation and the
        // parent data is reclaimed by generation GC.
        if self
            .candidate_operation
            .lock()
            .ok()
            .and_then(|state| state.clone())
            .is_some()
        {
            return self.register_deleted_file(file_path).await;
        }

        // Remove from Qdrant (project-scoped)
        if let Some(ref qdrant) = self.qdrant {
            self.ensure_project_group_id()?;
            qdrant
                .delete_by_file_path_scoped(&file_id, &self.project_group_id, None)
                .await?;
        }

        // Clear Qdrant references in file summary mappings (all epochs)
        if let Some(client) = self.metadata_store.as_deref() {
            let result = client.with_transaction(|tx| {
                    use rusqlite::params;
                    let now = chrono::Utc::now().to_rfc3339();
                    tx.execute(
                        "UPDATE file_summaries SET qdrant_point_id = NULL, updated_at = ?1 \
                         WHERE file_id IN (SELECT id FROM files WHERE path = ?2 AND project_id = ?3)",
                        params![now, &file_id, self.project_id],
                    )
                    .map_err(|e| {
                        cce_types::StorageError::Sqlite(e.to_string())
                    })?;
                    Ok(())
                });
            if let Err(e) = result {
                tracing::warn!(file = %file_id, error = %e, "Failed to clear Qdrant references");
            }
        }

        Ok(())
    }

    /// Remove file from BM25 index only
    pub async fn remove_file_from_bm25(
        &self,
        file_path: &std::path::Path,
    ) -> Result<(), OrchestratorError> {
        let file_id = normalize_project_path(&file_path.to_string_lossy());

        if self
            .candidate_operation
            .lock()
            .ok()
            .and_then(|state| state.clone())
            .is_some()
        {
            return self.register_deleted_file(file_path).await;
        }

        // Remove from BM25 (scoped to project)
        if let Some(ref bm25) = self.bm25 {
            let mut client = bm25.lock().await;
            client
                .delete_by_file_path_scoped("default", &file_id, self.project_id)
                .await?;
        }

        // Clear BM25 references in file summary mappings (all epochs)
        if let Some(client) = self.metadata_store.as_deref() {
            let result = client.with_transaction(|tx| {
                    use rusqlite::params;
                    let now = chrono::Utc::now().to_rfc3339();
                    tx.execute(
                        "UPDATE file_summaries SET bm25_doc_id = NULL, updated_at = ?1 \
                         WHERE file_id IN (SELECT id FROM files WHERE path = ?2 AND project_id = ?3)",
                        params![now, &file_id, self.project_id],
                    )
                    .map_err(|e| {
                        cce_types::StorageError::Sqlite(e.to_string())
                    })?;
                    Ok(())
                });
            if let Err(e) = result {
                tracing::warn!(file = %file_id, error = %e, "Failed to clear BM25 references");
            }
        }

        Ok(())
    }
}
