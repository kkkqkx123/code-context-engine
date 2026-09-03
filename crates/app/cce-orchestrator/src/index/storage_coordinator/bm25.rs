//! BM25 full-text write path.

use std::sync::Arc;

use crate::CheckpointManager;
use cce_parser::ast_to_nl::chunker::{ChunkPath, ChunkedResult};
use cce_storage_bm25::Bm25Document;
use cce_storage_sqlite::types::{WorkUnitCheckpointRecord, WorkUnitStatus};
use cce_storage_sqlite::{ChunkRecord, EntityDetailMapping};

use crate::error::OrchestratorError;

use super::StorageCoordinator;
use super::mapping::{build_bm25_documents, build_chunk_record, compute_work_unit_hash};

impl StorageCoordinator {
    /// Store documents in BM25 from chunked results with batch processing
    pub async fn store_bm25_batched(
        &self,
        chunks: &[ChunkedResult],
        batch_size: usize,
    ) -> Result<(), OrchestratorError> {
        let bm25 = match &self.bm25 {
            Some(b) => b,
            None => {
                tracing::warn!("BM25 client not configured, skipping BM25 storage");
                return Ok(());
            }
        };

        // Filter to only BM25-path chunks
        let bm25_chunks: Vec<&ChunkedResult> = chunks
            .iter()
            .filter(|c| c.path == ChunkPath::Bm25)
            .collect();

        if bm25_chunks.is_empty() {
            return Ok(());
        }

        let wk_cm = self.checkpoint_manager.clone();
        let wk_op = self.operation_id.clone();

        for batch in bm25_chunks.chunks(batch_size) {
            // Work unit checkpoint: skip if this microbatch is already committed
            let wu_state: Option<(Arc<CheckpointManager>, String, String)> =
                if let (Some(cm), Some(op_id)) = (&wk_cm, &wk_op) {
                    let hash = compute_work_unit_hash(batch);
                    match cm.get_work_unit_by_hash(op_id, "bm25_commit", &hash).await {
                        Ok(Some(record)) if record.status == WorkUnitStatus::Committed => {
                            continue;
                        }
                        _ => {
                            let record = WorkUnitCheckpointRecord {
                                id: None,
                                project_id: self.project_id,
                                operation_id: op_id.clone(),
                                stage: "bm25_commit".to_string(),
                                target_epoch: self.epoch(),
                                work_unit_hash: hash.clone(),
                                status: WorkUnitStatus::Running,
                                item_count: batch.len() as u32,
                                created_at: chrono::Utc::now().to_rfc3339(),
                                updated_at: chrono::Utc::now().to_rfc3339(),
                            };
                            if let Err(e) = cm.insert_work_unit(&record).await {
                                tracing::warn!(
                                    error = %e,
                                    "Failed to insert BM25 work unit checkpoint"
                                );
                            }
                            Some((cm.clone(), op_id.clone(), hash))
                        }
                    }
                } else {
                    None
                };

            let documents = build_bm25_documents(batch, self.project_id, self.epoch());
            let entity_mappings = self.build_bm25_entity_mappings(batch, &documents);

            if !documents.is_empty() {
                let mut client = bm25.lock().await;
                client.batch_index("default", &documents).await?;
                drop(client);
            }

            if !entity_mappings.is_empty() {
                self.store_entity_mappings(&entity_mappings)?;
            }

            // Persist BM25 chunk records so SQLite enrichment can resolve raw
            // code, line numbers and entity IDs for BM25-only hits. Previously
            // BM25 chunks were only stored in the tantivy index, so the SQLite
            // enrichment lookup keyed by the BM25 chunk_id always missed and
            // BM25 results were returned with empty content and line 0.
            let bm25_records: Result<Vec<ChunkRecord>, OrchestratorError> = batch
                .iter()
                .filter(|chunk| !chunk.text.is_empty())
                .map(|chunk| {
                    build_chunk_record(chunk, self.project_id, self.epoch(), self.batch_id())
                })
                .collect();
            self.store_chunk_records(&bm25_records?)?;

            // Mark work unit as committed after successful processing
            if let Some((ref cm, ref op_id, ref hash)) = wu_state {
                if let Err(e) = cm
                    .update_work_unit_status(op_id, "bm25_commit", hash, WorkUnitStatus::Committed)
                    .await
                {
                    tracing::warn!(
                        error = %e,
                        "Failed to update BM25 work unit checkpoint to committed"
                    );
                }
            }
        }

        Ok(())
    }

    /// Store documents in BM25 from chunked results
    pub async fn store_bm25(&self, chunks: &[ChunkedResult]) -> Result<(), OrchestratorError> {
        self.store_bm25_batched(chunks, 100).await
    }

    fn build_bm25_entity_mappings(
        &self,
        chunks: &[&ChunkedResult],
        documents: &[Bm25Document],
    ) -> Vec<EntityDetailMapping> {
        // Build a map from chunk_id to document_id to avoid zip-order alignment issues.
        // This is necessary because build_bm25_documents may filter out chunks with
        // empty text, causing document count to differ from chunk count.
        let doc_by_chunk_id: std::collections::HashMap<&str, &Bm25Document> = documents
            .iter()
            .map(|d| (d.fields.get("chunk_id").map_or("", |s| s.as_str()), d))
            .collect();

        let source_entity_ids = self.load_source_entity_ids().unwrap_or_default();

        // Use a map to aggregate multiple BM25 docs per entity
        let mut entity_detail_map: std::collections::HashMap<i64, EntityDetailMapping> =
            std::collections::HashMap::new();

        for chunk in chunks {
            let Some(doc) = doc_by_chunk_id.get(chunk.chunk_id.as_str()) else {
                continue;
            };

            for entity_id in chunk.metadata.content_entity_ids() {
                let entity_id_i64 = if self.metadata_store.is_some() {
                    let Some(db_id) = source_entity_ids
                        .get(&(chunk.metadata.file_path.clone(), entity_id.0 as i64))
                        .copied()
                    else {
                        continue;
                    };
                    db_id
                } else {
                    entity_id.0 as i64
                };
                let entry = entity_detail_map.entry(entity_id_i64).or_insert_with(|| {
                    EntityDetailMapping::new(entity_id_i64)
                        .with_project_id(self.project_id)
                        .with_epoch(self.epoch())
                });

                // Add BM25 document ID
                let mut doc_ids = entry.get_bm25_doc_ids();
                if !doc_ids.contains(&doc.document_id) {
                    doc_ids.push(doc.document_id.clone());
                }
                *entry = entry.clone().with_bm25_doc_ids(&doc_ids);
            }
        }

        entity_detail_map.into_values().collect()
    }
}
