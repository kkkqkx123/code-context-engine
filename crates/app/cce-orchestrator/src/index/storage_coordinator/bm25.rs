//! BM25 full-text write path.

use cce_storage_common::FulltextStorage;

use cce_parser::ast_to_nl::chunker::{ChunkPath, ChunkedResult};
use cce_storage_common::FulltextDocument;
use cce_storage_metadb_sqlite::types::{WorkUnitCheckpointRecord, WorkUnitStatus};
use cce_storage_metadb_sqlite::{ChunkRecord, EntityDetailMapping};

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
        // Filter to only BM25-path chunks first: empty work succeeds
        // regardless of backend configuration, while a missing client with
        // pending work surfaces as an error instead of silent success.
        let bm25_chunks: Vec<&ChunkedResult> = chunks
            .iter()
            .filter(|c| c.path == ChunkPath::Bm25)
            .collect();

        if bm25_chunks.is_empty() {
            return Ok(());
        }

        let bm25 = match &self.fulltext {
            Some(b) => b,
            None => {
                return Err(OrchestratorError::index(
                    "bm25_store",
                    "BM25 client is not configured but BM25 chunks are pending",
                ));
            }
        };

        let wk_cm = self.checkpoint_manager.clone();
        let wk_op = self.operation_id.clone();

        // Build one payload per microbatch, skipping already-committed ones.
        // Running markers are staged for a single batched insert; resume
        // replays every microbatch without a Committed marker.
        struct PendingBatch {
            documents: Vec<FulltextDocument>,
            entity_mappings: Vec<EntityDetailMapping>,
            chunk_records: Vec<ChunkRecord>,
            hash: Option<String>,
        }
        let tracking = wk_cm.is_some() && wk_op.is_some();
        let mut running = Vec::new();
        let mut pending: Vec<PendingBatch> = Vec::new();
        // Entity ID map is epoch-scoped and immutable for this pass: load it
        // once instead of once per microbatch.
        let source_entity_ids = self.load_source_entity_ids()?;
        for batch in bm25_chunks.chunks(batch_size) {
            let hash = if tracking {
                Some(compute_work_unit_hash(batch))
            } else {
                None
            };
            if let (Some(hash), Some(cm), Some(op_id)) =
                (hash.as_ref(), wk_cm.as_ref(), wk_op.as_ref())
            {
                match cm.get_work_unit_by_hash(op_id, "bm25_commit", hash).await {
                    Ok(Some(record)) if record.status == WorkUnitStatus::Committed => {
                        continue;
                    }
                    _ => {}
                }
            }

            let documents = build_bm25_documents(batch, self.project_id, self.epoch());
            let entity_mappings =
                self.build_bm25_entity_mappings(batch, &documents, &source_entity_ids)?;

            // Persist BM25 chunk records so SQLite enrichment can resolve raw
            // code, line numbers and entity IDs for BM25-only hits.
            let chunk_records: Result<Vec<ChunkRecord>, OrchestratorError> = batch
                .iter()
                .filter(|chunk| !chunk.text.is_empty())
                .map(|chunk| {
                    build_chunk_record(chunk, self.project_id, self.epoch(), self.batch_id())
                })
                .collect();
            if let (Some(hash), Some(op_id)) = (hash.as_ref(), wk_op.as_ref()) {
                running.push(WorkUnitCheckpointRecord {
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
                });
            }
            pending.push(PendingBatch {
                documents,
                entity_mappings,
                chunk_records: chunk_records?,
                hash,
            });
        }
        if tracking {
            if let (Some(cm), _) = (wk_cm.as_ref(), wk_op.as_ref()) {
                if let Err(e) = cm.insert_work_units(&running).await {
                    tracing::warn!(
                        error = %e,
                        "Failed to insert BM25 work unit checkpoints"
                    );
                }
            }
        }

        // Single commit for the whole call instead of one commit per
        // microbatch: visibility advances only after every payload is built,
        // and Committed markers flush only after the commit is durable, so a
        // crash anywhere replays the uncommitted microbatches.
        let mut all_documents: Vec<FulltextDocument> = Vec::new();
        let mut all_mappings: Vec<EntityDetailMapping> = Vec::new();
        let mut all_records: Vec<ChunkRecord> = Vec::new();
        let mut committed_hashes: Vec<String> = Vec::new();
        for item in pending {
            all_documents.extend(item.documents);
            all_mappings.extend(item.entity_mappings);
            all_records.extend(item.chunk_records);
            if let Some(hash) = item.hash {
                committed_hashes.push(hash);
            }
        }

        if !all_documents.is_empty() {
            bm25.batch_index(&bm25.configured_index_name(), &all_documents)
                .await?;
        }

        if !all_records.is_empty() || !all_mappings.is_empty() {
            self.store_chunks_and_mappings(&all_records, &all_mappings)?;
        }

        if let (Some(cm), Some(op_id)) = (&wk_cm, &wk_op)
            && !committed_hashes.is_empty()
        {
            if let Err(e) = cm
                .update_work_unit_statuses(
                    op_id,
                    "bm25_commit",
                    &committed_hashes,
                    WorkUnitStatus::Committed,
                )
                .await
            {
                tracing::warn!(
                    error = %e,
                    "Failed to update BM25 work unit checkpoints to committed"
                );
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
        documents: &[FulltextDocument],
        source_entity_ids: &std::collections::HashMap<(String, i64), i64>,
    ) -> Result<Vec<EntityDetailMapping>, OrchestratorError> {
        // Build a map from chunk_id to document_id to avoid zip-order alignment issues.
        // This is necessary because build_bm25_documents may filter out chunks with
        // empty text, causing document count to differ from chunk count.
        let doc_by_chunk_id: std::collections::HashMap<&str, &FulltextDocument> = documents
            .iter()
            .map(|d| (d.fields.get("chunk_id").map_or("", |s| s.as_str()), d))
            .collect();

        // Use a map to aggregate multiple BM25 docs per entity
        let mut entity_detail_map: std::collections::HashMap<i64, EntityDetailMapping> =
            std::collections::HashMap::new();

        for chunk in chunks {
            let Some(doc) = doc_by_chunk_id.get(chunk.chunk_id.as_str()) else {
                continue;
            };

            for entity_id in chunk.metadata.content_entity_ids() {
                // File-level pseudo-entities carry no SQLite row by design.
                if entity_id.0 == u64::MAX || entity_id.0 == u64::MAX - 1 {
                    continue;
                }
                let entity_id_i64 = if self.metadata_store.is_some() {
                    let Some(db_id) = source_entity_ids
                        .get(&(chunk.metadata.file_path.clone(), entity_id.0 as i64))
                        .copied()
                    else {
                        // Every stored entity carries `__source_entity_id`, so a
                        // miss means the entity record is absent from this epoch:
                        // silently dropping the mapping would permanently sever
                        // the entity -> BM25 document link.
                        if let Some(metrics) = &self.quality_metrics {
                            metrics.record_entity_mapping_miss();
                        }
                        return Err(OrchestratorError::index(
                            "entity_mapping",
                            format!(
                                "no epoch-scoped entity record for {} (source entity {})",
                                chunk.metadata.file_path, entity_id.0
                            ),
                        ));
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

        Ok(entity_detail_map.into_values().collect())
    }
}
