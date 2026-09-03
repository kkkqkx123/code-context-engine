//! File summary write path (Qdrant vectors, BM25 documents, SQLite rows).
//!
//! This module provides a clean separation between summary storage operations
//! and business logic. The `SummaryStorage` struct handles all persistence
//! concerns, while `SummaryManager` coordinates the generation and storage workflow.

use cce_parser::summary::FileSummary;
use cce_storage_bm25::Bm25Document;
use cce_storage_common::Payload;
use cce_storage_qdrant::VectorPoint;
use cce_storage_sqlite::FileSummaryRepository;
use cce_types::{FileCategory, PointKind};

use crate::error::OrchestratorError;

use super::StorageCoordinator;

/// Summary storage operations handler
///
/// Handles persistence of summaries to multiple backends:
/// - Qdrant for vector storage
/// - BM25 for full-text search
/// - SQLite for metadata storage
pub struct SummaryStorage<'a> {
    coordinator: &'a StorageCoordinator,
}

impl<'a> SummaryStorage<'a> {
    /// Create a new summary storage handler
    pub fn new(coordinator: &'a StorageCoordinator) -> Self {
        Self { coordinator }
    }

    /// Store file summaries to all backends
    pub async fn store(&self, summaries: &[FileSummary]) -> Result<usize, OrchestratorError> {
        if summaries.is_empty() {
            return Ok(0);
        }

        // Store to Qdrant vectors
        self.store_vectors(summaries).await?;

        // Store to SQLite metadata
        self.store_metadata(summaries).await?;

        // Store to BM25 full-text index
        self.store_bm25(summaries).await?;

        Ok(summaries.len())
    }

    /// Store summary vectors to Qdrant
    async fn store_vectors(&self, summaries: &[FileSummary]) -> Result<(), OrchestratorError> {
        let (Some(qdrant), Some(embedder)) = (&self.coordinator.qdrant, &self.coordinator.embedder)
        else {
            tracing::trace!("Qdrant summary embedding unavailable; skipping vector storage");
            return Ok(());
        };

        self.coordinator.ensure_project_group_id()?;
        let texts: Vec<String> = summaries.iter().map(|s| s.to_embedding_text()).collect();
        let text_refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
        let embeddings = embedder.embed(&text_refs).await?;

        let epoch = self.coordinator.epoch();
        let batch_id = self.coordinator.batch_id();
        let mut points = Vec::new();

        for (summary, vector) in summaries.iter().zip(embeddings.embeddings.iter()) {
            let category = summary.category.unwrap_or(FileCategory::Code);
            let payload = Payload::new(summary.file_path.clone())
                .with_type(PointKind::Summary)
                .with_source_id(format!("summary::{}", summary.file_path))
                .with_group_id(self.coordinator.project_group_id.clone())
                .with_category(category)
                .with_epoch(epoch)
                .with_batch_id(batch_id)
                .with_test(summary.test_info.is_test())
                .with_test_source(summary.test_info.source);

            let point_id = format!(
                "{}::{}::summary::{}",
                self.coordinator.project_group_id, epoch, summary.file_path
            );
            points.push(VectorPoint::new(point_id, vector.clone(), payload));
        }

        if !points.is_empty() {
            qdrant.upsert_points(&points).await?;
        }

        Ok(())
    }

    /// Store summary metadata to SQLite
    async fn store_metadata(&self, summaries: &[FileSummary]) -> Result<(), OrchestratorError> {
        let Some(ref db) = self.coordinator.metadata_store else {
            return Ok(());
        };

        match db.write_connection() {
            Ok(conn) => {
                match conn.unchecked_transaction() {
                    Ok(tx) => {
                        for summary in summaries {
                            let file_path_str = &summary.file_path;
                            let project_id = self.coordinator.project_id;

                            match cce_storage_sqlite::FileRepository::get_by_path_and_project_at_epoch(
                                &tx,
                                file_path_str,
                                project_id,
                                self.coordinator.epoch(),
                            ) {
                                Ok(Some(file_record)) => {
                                    let file_id = file_record.id;
                                    match serde_json::to_string(summary) {
                                        Ok(summary_json) => {
                                            if let Err(e) = cce_storage_sqlite::FileSummaryRepository::upsert_with_epoch(
                                                &tx,
                                                file_id,
                                                self.coordinator.epoch(),
                                                &summary_json,
                                            ) {
                                                tracing::warn!(
                                                    file = %file_path_str,
                                                    error = %e,
                                                    "Failed to persist summary to SQLite"
                                                );
                                            }
                                        }
                                        Err(e) => {
                                            tracing::warn!(
                                                file = %file_path_str,
                                                error = %e,
                                                "Failed to serialize summary for SQLite persistence"
                                            );
                                        }
                                    }
                                }
                                Ok(None) => {
                                    // File not found in database, skip
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        file = %file_path_str,
                                        error = %e,
                                        "Failed to lookup file_id for summary persistence"
                                    );
                                }
                            }
                        }

                        if let Err(e) = tx.commit() {
                            tracing::warn!(error = %e, "Failed to commit summary persistence");
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to start transaction for summary persistence");
                    }
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to get database connection for summary persistence");
            }
        }

        Ok(())
    }

    /// Store summary documents to BM25 index
    async fn store_bm25(&self, summaries: &[FileSummary]) -> Result<(), OrchestratorError> {
        let Some(ref bm25) = self.coordinator.bm25 else {
            return Ok(());
        };

        let project_id_str = self.coordinator.project_id.to_string();
        let epoch_str = self.coordinator.epoch().to_string();
        let batch_id_str = self.coordinator.batch_id().to_string();

        let bm25_documents: Vec<Bm25Document> = summaries
            .iter()
            .map(|s| {
                let document_id = format!(
                    "{}::{}::summary::{}",
                    self.coordinator.project_id,
                    self.coordinator.epoch(),
                    s.file_path
                );
                let keywords = if s.tags.is_empty() {
                    String::new()
                } else {
                    s.tags.join(" ")
                };
                Bm25Document::new(&document_id)
                    .with_field("content", s.to_bm25_text())
                    .with_field("title", format!("{} summary", s.file_path))
                    .with_field("keywords", &keywords)
                    .with_field("file_path", &s.file_path)
                    .with_field("project_id", &project_id_str)
                    .with_field("epoch", &epoch_str)
                    .with_field("batch_id", &batch_id_str)
            })
            .collect();

        if !bm25_documents.is_empty() {
            match bm25
                .lock()
                .await
                .batch_index("default", &bm25_documents)
                .await
            {
                Ok(_count) => {
                    // Update bm25_doc_id in SQLite
                    self.update_bm25_doc_ids(summaries, &bm25_documents).await;
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to index summaries in BM25, continuing");
                }
            }
        }

        Ok(())
    }

    /// Update BM25 document IDs in SQLite
    async fn update_bm25_doc_ids(
        &self,
        summaries: &[FileSummary],
        bm25_documents: &[Bm25Document],
    ) {
        let Some(ref db) = self.coordinator.metadata_store else {
            return;
        };

        if let Ok(conn) = db.write_connection() {
            let _ = conn.unchecked_transaction().map(|tx| {
                for (summary, doc) in summaries.iter().zip(bm25_documents.iter()) {
                    let file_path_str = &summary.file_path;
                    let project_id = self.coordinator.project_id;
                    if let Ok(Some(file_record)) =
                        cce_storage_sqlite::FileRepository::get_by_path_and_project_at_epoch(
                            &tx,
                            file_path_str,
                            project_id,
                            self.coordinator.epoch(),
                        )
                    {
                        let _ =
                            cce_storage_sqlite::FileSummaryRepository::update_bm25_doc_id_at_epoch(
                                &tx,
                                file_record.id,
                                self.coordinator.epoch(),
                                Some(doc.document_id.clone()),
                            );
                    }
                }
                let _ = tx.commit();
            });
        }
    }
}

impl StorageCoordinator {
    /// Store file summaries to summary index
    ///
    /// This method delegates to `SummaryStorage` for persistence operations.
    pub async fn store_summaries(
        &self,
        summaries: &[FileSummary],
    ) -> Result<usize, OrchestratorError> {
        SummaryStorage::new(self).store(summaries).await
    }

    /// Regenerate summary vectors in place from their persisted JSON rows.
    ///
    /// Used when the embedder configuration changed: summaries are re-embedded
    /// from `file_summaries.summary_json` at the active generation so no file
    /// is re-parsed. Point IDs are unchanged (same formula and epoch), so only
    /// vector values are refreshed.
    pub async fn reembed_stored_summaries(
        &self,
        batch_size: usize,
    ) -> Result<usize, OrchestratorError> {
        let (Some(qdrant), Some(embedder)) = (&self.qdrant, &self.embedder) else {
            return Ok(0);
        };
        let Some(client) = self.metadata_store.as_ref().map(|store| store.as_ref()) else {
            return Ok(0);
        };

        // The sweep always targets the active generation: inherited summaries
        // live at their original epochs and keep those point IDs.
        let Some(active_epoch) = self.active_data_epoch()? else {
            return Ok(0);
        };

        let rows = {
            let conn = client
                .read_connection()
                .map_err(OrchestratorError::Storage)?;
            FileSummaryRepository::list_json_by_epoch(&conn, self.project_id, active_epoch)
                .map_err(OrchestratorError::Storage)?
        };
        if rows.is_empty() {
            return Ok(0);
        }
        self.ensure_project_group_id()?;

        let mut summaries = Vec::with_capacity(rows.len());
        for (path, json, epoch) in rows {
            match serde_json::from_str::<FileSummary>(&json) {
                Ok(mut summary) => {
                    if summary.file_path.is_empty() {
                        summary.file_path = path;
                    }
                    summaries.push((summary, epoch));
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Skipping unreadable persisted summary during re-embed");
                }
            }
        }

        let mut stored = 0;
        for batch in summaries.chunks(batch_size.max(1)) {
            let texts: Vec<String> = batch
                .iter()
                .map(|(summary, _)| summary.to_embedding_text())
                .collect();
            let text_refs: Vec<&str> = texts.iter().map(String::as_str).collect();
            let embeddings = embedder.embed(&text_refs).await?;
            let mut points = Vec::with_capacity(batch.len());
            for ((summary, epoch), vector) in batch.iter().zip(embeddings.embeddings.iter()) {
                let category = summary.category.unwrap_or(FileCategory::Code);
                let payload = Payload::new(summary.file_path.clone())
                    .with_type(PointKind::Summary)
                    .with_source_id(format!("summary::{}", summary.file_path))
                    .with_group_id(self.project_group_id.clone())
                    .with_category(category)
                    .with_epoch(*epoch)
                    .with_test(summary.test_info.is_test())
                    .with_test_source(summary.test_info.source);

                let point_id = format!(
                    "{}::{}::summary::{}",
                    self.project_group_id, epoch, summary.file_path
                );
                points.push(VectorPoint::new(point_id, vector.clone(), payload));
            }
            stored += points.len();
            qdrant.upsert_points(&points).await?;
        }
        Ok(stored)
    }

    /// Remove file from summary index
    pub async fn remove_file_from_summary(
        &self,
        file_path: &std::path::Path,
    ) -> Result<(), OrchestratorError> {
        let file_id = cce_types::path::normalize_project_path(&file_path.to_string_lossy());

        if self
            .candidate_operation
            .lock()
            .ok()
            .and_then(|state| state.clone())
            .is_some()
        {
            return self.register_deleted_file(file_path).await;
        }

        // Step 1: Remove summary vectors from Qdrant
        if let Some(qdrant) = &self.qdrant {
            self.ensure_project_group_id()?;
            qdrant
                .delete_by_file_path_scoped(
                    &file_id,
                    &self.project_group_id,
                    Some(PointKind::Summary),
                )
                .await?;
        }

        // Step 2: Remove summary from BM25 index
        if let Some(ref bm25) = self.bm25 {
            match bm25
                .lock()
                .await
                .delete_by_file_path_scoped("default", &file_id, self.project_id)
                .await
            {
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!(
                        file = %file_id,
                        error = %e,
                        "Failed to remove summary from BM25, continuing"
                    );
                }
            }
        }

        // Step 3: Remove summary records from SQLite (all epochs)
        if let Some(client) = self.metadata_store.as_deref() {
            let result = client.with_transaction(|tx| {
                use rusqlite::params;
                tx.execute(
                    "DELETE FROM file_summaries WHERE file_id IN \
                     (SELECT id FROM files WHERE path = ?1 AND project_id = ?2)",
                    params![&file_id, self.project_id],
                )
                .ok(); // Ignore errors if record doesn't exist
                Ok(())
            });

            if let Err(e) = result {
                tracing::warn!(file = %file_id, error = %e, "Failed to remove summary from SQLite");
                // Don't fail the whole operation if SQLite cleanup fails
                // (Qdrant is the primary index)
            }
        }

        Ok(())
    }
}
