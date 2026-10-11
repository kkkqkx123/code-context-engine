use cce_storage_common::FulltextStorage;
use cce_storage_common::VectorStorage;
use cce_storage_metadb_sqlite::{GenerationOverrideRepository, OverrideDisposition};
use cce_types::path::normalize_project_path;

use crate::error::OrchestratorError;

use super::super::StorageCoordinator;

impl StorageCoordinator {
    /// Prepare one changed file inside the candidate generation.
    ///
    /// Zero-copy write path: the candidate inherits unchanged files from its
    /// parent, so this only clears the candidate's own rows for the file and
    /// registers a `replaced` override that hides the parent-generation rows.
    /// External stores are cleaned strictly inside the candidate epoch: chunk
    /// IDs are not stable across re-parses, so stale own-generation documents
    /// must be removed before the processor writes the new ones. Parent data
    /// is never touched here — its physical reclamation happens in GC.
    pub async fn prepare_hot_update_file(
        &self,
        file_path: &std::path::Path,
    ) -> Result<(), OrchestratorError> {
        let Some(_operation) = self
            .candidate_operation
            .lock()
            .map_err(|_| {
                OrchestratorError::index("hot_update_candidate", "candidate state lock poisoned")
            })?
            .clone()
        else {
            return Ok(());
        };
        let path = normalize_project_path(&file_path.to_string_lossy());
        {
            let prepared = self.prepared_files.lock().map_err(|_| {
                OrchestratorError::index(
                    "hot_update_candidate",
                    "prepared file state lock poisoned",
                )
            })?;
            if prepared.contains(&path) {
                return Ok(());
            }
            drop(prepared);
        }
        self.delete_candidate_epoch_external_data(&path).await?;
        self.clear_candidate_file_rows(&path)?;
        self.register_override(&path, OverrideDisposition::Replaced)?;
        self.prepared_files
            .lock()
            .map_err(|_| {
                OrchestratorError::index(
                    "hot_update_candidate",
                    "prepared file state lock poisoned",
                )
            })?
            .insert(path.clone());
        Ok(())
    }

    /// Delete one file's vector points and BM25 documents **scoped to the
    /// candidate epoch only**. The published/parent generations are never
    /// addressed, so an abort leaves them intact (invariant 3).
    async fn delete_candidate_epoch_external_data(
        &self,
        path: &str,
    ) -> Result<(), OrchestratorError> {
        let epoch = self.epoch();
        if let Some(vector) = &self.vector {
            self.ensure_project_group_id()?;
            vector
                .delete_by_file_path_scoped_epoch(path, &self.project_group_id, epoch)
                .await?;
        }
        if let Some(bm25) = &self.fulltext {
            bm25.delete_by_file_path_scoped_epoch(
                &bm25.configured_index_name(),
                path,
                self.project_id,
                epoch,
            )
            .await?;
        }
        Ok(())
    }

    /// Remove one file's BM25 data only from the candidate generation before
    /// re-indexing it.
    ///
    /// Module-scoped: unlike [`Self::prepare_hot_update_file`] it clears only
    /// the BM25 chunk rows of the candidate plus its candidate-epoch documents;
    /// the `replaced` override hides the parent documents. Recovery may re-run
    /// this for the same file after the file-level preparation already
    /// registered the override (idempotent).
    pub async fn prepare_hot_update_bm25(
        &self,
        file_path: &std::path::Path,
    ) -> Result<(), OrchestratorError> {
        let path = normalize_project_path(&file_path.to_string_lossy());
        let epoch = self.epoch();
        if let Some(bm25) = &self.fulltext {
            bm25.delete_by_file_path_scoped_epoch(
                &bm25.configured_index_name(),
                &path,
                self.project_id,
                epoch,
            )
            .await?;
        }
        if let Some(client) = self.metadata_store.as_ref().map(|store| store.as_ref()) {
            client
                .with_transaction(|tx| {
                    tx.execute(
                        "DELETE FROM chunks WHERE project_id = ?1 AND epoch = ?2 AND file_path = ?3 AND path = 'nl'",
                        rusqlite::params![self.project_id, epoch, path],
                    )
                    .map_err(|error| cce_types::StorageError::delete("chunks", error.to_string()))?;
                    GenerationOverrideRepository::upsert(
                        tx,
                        self.project_id,
                        epoch,
                        &path,
                        OverrideDisposition::Replaced,
                    )
                })
                .map_err(OrchestratorError::Storage)?;
        }
        Ok(())
    }

    /// Remove one file's embedding data only from the candidate generation
    /// before re-embedding it.
    ///
    /// Module-scoped: unlike [`Self::prepare_hot_update_file`] it clears only
    /// the embedding chunk rows of the candidate plus its candidate-epoch
    /// vectors; the `replaced` override hides the parent vectors.
    pub async fn prepare_hot_update_embedding(
        &self,
        file_path: &std::path::Path,
    ) -> Result<(), OrchestratorError> {
        let path = normalize_project_path(&file_path.to_string_lossy());
        let epoch = self.epoch();
        if let Some(vector) = &self.vector {
            self.ensure_project_group_id()?;
            vector
                .delete_by_file_path_scoped_epoch(&path, &self.project_group_id, epoch)
                .await?;
        }
        if let Some(client) = self.metadata_store.as_ref().map(|store| store.as_ref()) {
            client
                .with_transaction(|tx| {
                    tx.execute(
                        "DELETE FROM chunks WHERE project_id = ?1 AND epoch = ?2 AND file_path = ?3 AND path = 'emb'",
                        rusqlite::params![self.project_id, epoch, path],
                    )
                    .map_err(|error| cce_types::StorageError::delete("chunks", error.to_string()))?;
                    GenerationOverrideRepository::upsert(
                        tx,
                        self.project_id,
                        epoch,
                        &path,
                        OverrideDisposition::Replaced,
                    )
                })
                .map_err(OrchestratorError::Storage)?;
        }
        Ok(())
    }

    /// Remove one file's summary only from the candidate generation before
    /// re-summarizing it.
    ///
    /// Module-scoped: unlike [`Self::prepare_hot_update_file`] it leaves other
    /// modules' candidate data intact. The `replaced` override is already
    /// registered by the file-level preparation that necessarily preceded any
    /// summary rewrite, so no duplicate registration happens here.
    pub async fn prepare_hot_update_summary(
        &self,
        file_path: &std::path::Path,
    ) -> Result<(), OrchestratorError> {
        let path = normalize_project_path(&file_path.to_string_lossy());
        let epoch = self.epoch();
        if let Some(client) = self.metadata_store.as_ref().map(|store| store.as_ref()) {
            client
                .with_transaction(|tx| {
                    tx.execute(
                        "DELETE FROM file_summaries WHERE epoch = ?1 AND file_id IN
                         (SELECT id FROM files WHERE path = ?2 AND project_id = ?3 AND epoch = ?1)",
                        rusqlite::params![epoch, path, self.project_id],
                    )
                    .map_err(|error| {
                        cce_types::StorageError::delete("file_summaries", error.to_string())
                    })?;
                    Ok(())
                })
                .map_err(OrchestratorError::Storage)?;
        }
        Ok(())
    }

    /// Clear every candidate-generation row of one file (summaries, chunks,
    /// entities). Used by both replace and delete preparations.
    fn clear_candidate_file_rows(&self, path: &str) -> Result<(), OrchestratorError> {
        let epoch = self.epoch();
        if let Some(client) = self.metadata_store.as_ref().map(|store| store.as_ref()) {
            client
                .with_transaction(|tx| {
                    tx.execute(
                        "DELETE FROM file_summaries WHERE epoch = ?1 AND file_id IN
                         (SELECT id FROM files WHERE path = ?2 AND project_id = ?3 AND epoch = ?1)",
                        rusqlite::params![epoch, path, self.project_id],
                    )
                    .map_err(|error| cce_types::StorageError::delete("file_summaries", error.to_string()))?;
                    tx.execute(
                        "DELETE FROM chunks WHERE project_id = ?1 AND epoch = ?2 AND file_path = ?3",
                        rusqlite::params![self.project_id, epoch, path],
                    )
                    .map_err(|error| cce_types::StorageError::delete("chunks", error.to_string()))?;
                    tx.execute(
                        "DELETE FROM entities WHERE project_id = ?1 AND epoch = ?2 AND file_id IN
                         (SELECT id FROM files WHERE path = ?3 AND project_id = ?1 AND epoch = ?2)",
                        rusqlite::params![self.project_id, epoch, path],
                    )
                    .map_err(|error| cce_types::StorageError::delete("entities", error.to_string()))?;
                    Ok(())
                })
                .map_err(OrchestratorError::Storage)?;
        }
        Ok(())
    }

    /// Register a file-level exception against the parent generation.
    ///
    /// A later call for the same file upgrades the disposition (e.g. a file
    /// prepared as replaced and deleted afterwards ends up `deleted`).
    fn register_override(
        &self,
        path: &str,
        disposition: OverrideDisposition,
    ) -> Result<(), OrchestratorError> {
        let epoch = self.epoch();
        if let Some(client) = self.metadata_store.as_ref().map(|store| store.as_ref()) {
            client
                .with_transaction(|tx| {
                    GenerationOverrideRepository::upsert(
                        tx,
                        self.project_id,
                        epoch,
                        path,
                        disposition,
                    )
                })
                .map_err(OrchestratorError::Storage)?;
        }
        Ok(())
    }

    /// Register a deleted file inside the candidate generation.
    ///
    /// Zero-copy deletion path: the `deleted` override hides the
    /// parent-generation rows in every read path (their physical reclamation
    /// happens in GC), and any candidate-own rows — from an earlier replace
    /// within the same operation, or seeded by an interrupted run — are
    /// cleared so neither version surfaces. External data is removed strictly
    /// inside the candidate epoch for the same reason.
    pub async fn register_deleted_file(
        &self,
        file_path: &std::path::Path,
    ) -> Result<(), OrchestratorError> {
        let path = normalize_project_path(&file_path.to_string_lossy());
        self.delete_candidate_epoch_external_data(&path).await?;
        self.clear_candidate_file_rows(&path)?;
        self.register_override(&path, OverrideDisposition::Deleted)
    }
}
