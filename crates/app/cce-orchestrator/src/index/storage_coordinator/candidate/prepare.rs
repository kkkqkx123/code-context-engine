use cce_storage_common::FulltextStorage;
use cce_storage_common::VectorStorage;
use cce_storage_metadb_sqlite::{GenerationOverrideRepository, OverrideDisposition};
use cce_types::path::normalize_project_path;

use crate::error::OrchestratorError;

use super::super::StorageCoordinator;

impl StorageCoordinator {
    /// Delete one file's vector points and BM25 documents **scoped to the
    /// candidate epoch only**. The published/parent generations are never
    /// addressed, so an abort leaves them intact (invariant 3).
    async fn delete_candidate_epoch_external_data(
        &self,
        path: &str,
    ) -> Result<(), OrchestratorError> {
        self.delete_candidate_epoch_external_data_batch(std::slice::from_ref(&path))
            .await
    }

    /// Batch form of [`Self::delete_candidate_epoch_external_data`]: both
    /// branches issue one batch delete covering every path, so an
    /// operation-wide deletion costs a single round-trip (vector) or a
    /// single index search + commit (fulltext) instead of one per file.
    async fn delete_candidate_epoch_external_data_batch(
        &self,
        paths: &[&str],
    ) -> Result<(), OrchestratorError> {
        if paths.is_empty() {
            return Ok(());
        }
        let epoch = self.epoch();
        if let Some(vector) = &self.vector {
            self.ensure_project_group_id()?;
            vector
                .delete_by_file_paths_scoped_epoch(paths, &self.project_group_id, epoch)
                .await?;
        }
        if let Some(bm25) = &self.fulltext {
            bm25.delete_by_file_paths_scoped_epoch(
                &bm25.configured_index_name(),
                paths,
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
    /// Module-scoped: it clears only the BM25 chunk rows of the candidate
    /// plus its candidate-epoch documents; the `replaced` override hides the
    /// parent documents. Recovery may re-run this for the same file (the
    /// epoch-scoped delete and the override upsert are idempotent).
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
    /// Module-scoped: it clears only the embedding chunk rows of the
    /// candidate plus its candidate-epoch vectors; the `replaced` override
    /// hides the parent vectors.
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
    /// Module-scoped: it leaves other modules' candidate data intact. The
    /// `replaced` override is already registered by the module preparation
    /// that necessarily preceded any summary rewrite, so no duplicate
    /// registration happens here.
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
        self.clear_candidate_file_rows_batch(std::slice::from_ref(&path))
    }

    /// Batch form of [`Self::clear_candidate_file_rows`]: every file's rows
    /// are removed inside one transaction instead of one per file.
    fn clear_candidate_file_rows_batch(&self, paths: &[&str]) -> Result<(), OrchestratorError> {
        if paths.is_empty() {
            return Ok(());
        }
        let epoch = self.epoch();
        if let Some(client) = self.metadata_store.as_ref().map(|store| store.as_ref()) {
            client
                .with_transaction(|tx| {
                    for path in paths {
                        Self::clear_candidate_file_rows_tx(tx, self.project_id, epoch, path)?;
                    }
                    Ok(())
                })
                .map_err(OrchestratorError::Storage)
        } else {
            Ok(())
        }
    }

    /// Remove one file's candidate-generation rows inside an existing
    /// transaction.
    fn clear_candidate_file_rows_tx(
        tx: &rusqlite::Transaction<'_>,
        project_id: i64,
        epoch: i64,
        path: &str,
    ) -> Result<(), cce_types::StorageError> {
        tx.execute(
            "DELETE FROM file_summaries WHERE epoch = ?1 AND file_id IN
             (SELECT id FROM files WHERE path = ?2 AND project_id = ?3 AND epoch = ?1)",
            rusqlite::params![epoch, path, project_id],
        )
        .map_err(|error| cce_types::StorageError::delete("file_summaries", error.to_string()))?;
        tx.execute(
            "DELETE FROM chunks WHERE project_id = ?1 AND epoch = ?2 AND file_path = ?3",
            rusqlite::params![project_id, epoch, path],
        )
        .map_err(|error| cce_types::StorageError::delete("chunks", error.to_string()))?;
        tx.execute(
            "DELETE FROM entities WHERE project_id = ?1 AND epoch = ?2 AND file_id IN
             (SELECT id FROM files WHERE path = ?3 AND project_id = ?1 AND epoch = ?2)",
            rusqlite::params![project_id, epoch, path],
        )
        .map_err(|error| cce_types::StorageError::delete("entities", error.to_string()))?;
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
        self.register_overrides(std::slice::from_ref(&path), disposition)
    }

    /// Batch form of [`Self::register_override`]: every disposition is written
    /// inside one transaction.
    fn register_overrides(
        &self,
        paths: &[&str],
        disposition: OverrideDisposition,
    ) -> Result<(), OrchestratorError> {
        if paths.is_empty() {
            return Ok(());
        }
        let epoch = self.epoch();
        if let Some(client) = self.metadata_store.as_ref().map(|store| store.as_ref()) {
            client
                .with_transaction(|tx| {
                    for path in paths {
                        GenerationOverrideRepository::upsert(
                            tx,
                            self.project_id,
                            epoch,
                            path,
                            disposition,
                        )?;
                    }
                    Ok(())
                })
                .map_err(OrchestratorError::Storage)
        } else {
            Ok(())
        }
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

    /// Register a set of deleted files inside the candidate generation in one
    /// pass: a single vector filter delete covering all paths, plus one
    /// transaction each for the row cleanup and the `deleted` override
    /// registrations, so an operation-wide deletion costs a fixed number of
    /// external round-trips instead of one set per file.
    ///
    /// Requires a prepared candidate: without one there is no candidate
    /// epoch to scope the external delete to.
    pub async fn register_deleted_files(
        &self,
        file_paths: &[&std::path::Path],
    ) -> Result<(), OrchestratorError> {
        if file_paths.is_empty() {
            return Ok(());
        }
        if !self.has_hot_update_candidate()? {
            return Err(OrchestratorError::index(
                "hot_update_candidate",
                "batch deletion registration requires a prepared hot-update candidate",
            ));
        }
        let paths: Vec<String> = file_paths
            .iter()
            .map(|path| normalize_project_path(&path.to_string_lossy()))
            .collect();
        let path_refs: Vec<&str> = paths.iter().map(String::as_str).collect();
        self.delete_candidate_epoch_external_data_batch(&path_refs)
            .await?;
        self.clear_candidate_file_rows_batch(&path_refs)?;
        self.register_overrides(&path_refs, OverrideDisposition::Deleted)
    }
}
