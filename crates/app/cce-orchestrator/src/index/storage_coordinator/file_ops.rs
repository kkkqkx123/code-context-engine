//! Cross-backend per-file removal and hot-update orchestration.
//!
//! The write-before-delete ordering that keeps previously queryable data
//! intact lives here.

use cce_parser::ast_to_nl::chunker::ChunkedResult;
use cce_storage_common::FulltextStorage;
use cce_storage_common::VectorStorage;
use cce_storage_metadb_sqlite::ChunkRepository;
use cce_types::path::normalize_project_path;

use crate::error::OrchestratorError;

use super::StorageCoordinator;

impl StorageCoordinator {
    /// Delete a file from all storage backends
    pub async fn remove_file(&self, file_path: &std::path::Path) -> Result<(), OrchestratorError> {
        // Inside a hot operation the deletion is registered against the
        // candidate generation (override + own-row cleanup); the published
        // generation remains untouched until manifest activation and the
        // parent data is reclaimed by generation GC.
        if self.has_hot_update_candidate()? {
            return self.register_deleted_file(file_path).await;
        }
        let file_id = normalize_project_path(&file_path.to_string_lossy());
        let mut backend_errors: Vec<String> = Vec::new();

        // Remove from vector store (project-scoped)
        if let Some(ref vector) = self.vector {
            if let Err(error) = self.ensure_project_group_id() {
                tracing::warn!(path = %file_id, error = %error, "Failed to resolve project group for vector removal");
                backend_errors.push(format!("vector group: {error}"));
            } else if let Err(error) = vector
                .delete_by_file_path_scoped(&file_id, &self.project_group_id, None)
                .await
            {
                tracing::warn!(path = %file_id, error = %error, "Failed to remove file from vector store");
                backend_errors.push(format!("vector: {error}"));
            }
        }

        // Remove from BM25 (scoped to project)
        if let Some(ref bm25) = self.fulltext {
            if let Err(error) = bm25
                .delete_by_file_path_scoped(
                    &bm25.configured_index_name(),
                    &file_id,
                    self.project_id,
                )
                .await
            {
                tracing::warn!(path = %file_id, error = %error, "Failed to remove file from BM25");
                backend_errors.push(format!("bm25: {error}"));
            }
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
                .map_err(|e| cce_types::StorageError::Sqlite(e.to_string()))?;
                Ok(())
            });

            if let Err(error) = result.map_err(OrchestratorError::Storage) {
                tracing::warn!(path = %file_id, error = %error, "Failed to remove file records");
                backend_errors.push(format!("sqlite files: {error}"));
            }
        }

        // Remove chunk records from SQLite
        if let Some(client) = self.metadata_store.as_deref() {
            let result = client.with_transaction(|tx| {
                ChunkRepository::delete_by_file_path(tx, &file_id, self.project_id)
            });

            if let Err(error) = result.map_err(OrchestratorError::Storage) {
                tracing::warn!(path = %file_id, error = %error, "Failed to remove chunk records");
                backend_errors.push(format!("sqlite chunks: {error}"));
            }
        }

        if backend_errors.is_empty() {
            Ok(())
        } else {
            Err(OrchestratorError::index(
                "remove_file",
                format!(
                    "Partial failure removing {}: {}",
                    file_id,
                    backend_errors.join("; ")
                ),
            ))
        }
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

    /// Batch form of [`Self::hot_update_bm25_file`] for changed files.
    ///
    /// Registers the candidate-epoch deletion for every file in one pass,
    /// persists every file's parsed entities, then commits all files' BM25
    /// documents in a single Tantivy commit. Without this a multi-file hot
    /// update pays one commit per changed file.
    ///
    /// Entities must be stored before the commit: the BM25 commit resolves
    /// source entity ids against the epoch's stored entities. A failure
    /// anywhere leaves the previously queryable generation intact, because
    /// the candidate-epoch cleanup and the commit are both scoped to the
    /// candidate.
    pub async fn hot_update_bm25_files(
        &self,
        parsed_files: &[&cce_types::ParsedFile],
        chunks: &[ChunkedResult],
    ) -> Result<(), OrchestratorError> {
        if parsed_files.is_empty() {
            return Ok(());
        }

        let paths: Vec<String> = parsed_files
            .iter()
            .map(|parsed| parsed.path.clone())
            .collect();
        self.prepare_hot_update_bm25(&paths).await?;
        for parsed in parsed_files {
            self.store_parsed_files(std::slice::from_ref(*parsed))?;
        }

        self.store_bm25(chunks).await
    }

    /// Batch BM25 rewrite for files whose parsed entities are already stored
    /// (chunking-drift rebuilds).
    ///
    /// Registers the candidate-epoch deletion for every path in one pass, then
    /// commits all chunks in a single Tantivy commit.
    pub async fn hot_update_bm25_paths(
        &self,
        paths: &[std::path::PathBuf],
        chunks: &[ChunkedResult],
    ) -> Result<(), OrchestratorError> {
        if paths.is_empty() {
            return Ok(());
        }

        let raw: Vec<String> = paths
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect();
        self.prepare_hot_update_bm25(&raw).await?;

        self.store_bm25(chunks).await
    }

    /// Remove file from vector index only (vector store + entity mappings)
    pub async fn remove_file_from_vectors(
        &self,
        file_path: &std::path::Path,
    ) -> Result<(), OrchestratorError> {
        let file_id = normalize_project_path(&file_path.to_string_lossy());

        // Inside a hot operation the deletion is registered against the
        // candidate generation (override + own-row cleanup); the published
        // generation remains untouched until manifest activation and the
        // parent data is reclaimed by generation GC.
        if self.has_hot_update_candidate()? {
            return self.register_deleted_file(file_path).await;
        }

        // Remove from vector store (project-scoped)
        if let Some(ref vector) = self.vector {
            self.ensure_project_group_id()?;
            vector
                .delete_by_file_path_scoped(&file_id, &self.project_group_id, None)
                .await?;
        }

        // Clear vector references in file summary mappings (all epochs)
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
            result.map_err(OrchestratorError::Storage)?;
        }

        Ok(())
    }

    /// Remove file from BM25 index only
    pub async fn remove_file_from_bm25(
        &self,
        file_path: &std::path::Path,
    ) -> Result<(), OrchestratorError> {
        let file_id = normalize_project_path(&file_path.to_string_lossy());

        if self.has_hot_update_candidate()? {
            return self.register_deleted_file(file_path).await;
        }

        // Remove from BM25 (scoped to project)
        if let Some(ref bm25) = self.fulltext {
            bm25.delete_by_file_path_scoped(
                &bm25.configured_index_name(),
                &file_id,
                self.project_id,
            )
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
            result.map_err(OrchestratorError::Storage)?;
        }

        Ok(())
    }
}
