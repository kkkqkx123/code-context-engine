//! BM25 full-text write path.

use cce_storage_common::FulltextStorage;

use cce_parser::ast_to_nl::chunker::{ChunkPath, ChunkedResult};
use cce_storage_common::FulltextDocument;
use cce_storage_common::metadb::{ChunkRecord, EntityDetailMapping};
use cce_storage_common::metadb::{WorkUnitCheckpointRecord, WorkUnitStatus};

use crate::error::OrchestratorError;

use super::StorageCoordinator;
use super::mapping::{build_bm25_documents, build_chunk_record, compute_work_unit_hash};

/// Documents and metadata collected before the next fulltext commit.
///
/// The full-index loop merges several orchestration batches into one staged
/// write so Tantivy commits once per aggregation window instead of once per
/// source batch. Work units stay `Running` until the staged write commits.
#[derive(Default)]
pub(crate) struct StagedBm25Write {
    pub documents: usize,
    pub entity_mappings: Vec<EntityDetailMapping>,
    pub chunk_records: Vec<ChunkRecord>,
    pub hashes: Vec<String>,
}

impl StagedBm25Write {
    pub fn is_empty(&self) -> bool {
        self.documents == 0
            && self.entity_mappings.is_empty()
            && self.chunk_records.is_empty()
            && self.hashes.is_empty()
    }

    pub fn merge(&mut self, other: StagedBm25Write) {
        self.documents += other.documents;
        self.entity_mappings.extend(other.entity_mappings);
        self.chunk_records.extend(other.chunk_records);
        self.hashes.extend(other.hashes);
    }
}

impl StorageCoordinator {
    /// Build BM25 payloads for one call and stage them in the local writer.
    ///
    /// The caller owns the commit boundary: nothing is marked committed here.
    pub(crate) async fn stage_bm25(
        &self,
        chunks: &[ChunkedResult],
        batch_size: usize,
        source_entity_ids: Option<&std::collections::HashMap<(String, i64), i64>>,
    ) -> Result<StagedBm25Write, OrchestratorError> {
        let bm25_chunks: Vec<&ChunkedResult> = chunks
            .iter()
            .filter(|c| c.path == ChunkPath::Bm25)
            .collect();
        if bm25_chunks.is_empty() {
            return Ok(StagedBm25Write::default());
        }

        let Some(bm25) = self.fulltext.as_ref() else {
            tracing::warn!(
                chunks = bm25_chunks.len(),
                "BM25 client is not configured; skipping chunk fulltext indexing"
            );
            return Ok(StagedBm25Write::default());
        };

        let wk_cm = self.checkpoint_manager.clone();
        let wk_op = self.operation_id.clone();
        let mut running = Vec::new();
        let mut staged = StagedBm25Write::default();
        let loaded_ids;
        let source_entity_ids = match source_entity_ids {
            Some(ids) => ids,
            None => {
                loaded_ids = self.load_source_entity_ids().await?;
                &loaded_ids
            }
        };
        for batch in bm25_chunks.chunks(batch_size) {
            let hash = if wk_cm.is_some() && wk_op.is_some() {
                Some(compute_work_unit_hash(batch))
            } else {
                None
            };
            if let (Some(hash), Some(cm), Some(op_id)) =
                (hash.as_ref(), wk_cm.as_ref(), wk_op.as_ref())
                && let Ok(Some(record)) = cm.get_work_unit_by_hash(op_id, "bm25_commit", hash).await
                && record.status == WorkUnitStatus::Committed
            {
                continue;
            }

            let documents = build_bm25_documents(batch, self.project_id, self.epoch());
            let entity_mappings =
                self.build_bm25_entity_mappings(batch, &documents, &source_entity_ids)?;
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
                staged.hashes.push(hash.clone());
            }
            staged.documents += bm25
                .stage_documents(&bm25.configured_index_name(), &documents)
                .await?;
            staged.entity_mappings.extend(entity_mappings);
            staged.chunk_records.extend(chunk_records?);
        }
        if let (Some(cm), _) = (wk_cm.as_ref(), wk_op.as_ref())
            && !running.is_empty()
            && let Err(e) = cm.insert_work_units(&running).await
        {
            tracing::warn!(error = %e, "Failed to insert BM25 work unit checkpoints");
        }
        Ok(staged)
    }

    /// Publish one aggregated BM25 write and advance its work-unit markers.
    pub(crate) async fn commit_staged_bm25(
        &self,
        staged: &StagedBm25Write,
    ) -> Result<(), OrchestratorError> {
        if staged.is_empty() {
            return Ok(());
        }
        let Some(bm25) = self.fulltext.as_ref() else {
            return Ok(());
        };
        bm25.commit_staged(&bm25.configured_index_name()).await?;

        if !staged.chunk_records.is_empty() || !staged.entity_mappings.is_empty() {
            self.store_chunks_and_mappings(&staged.chunk_records, &staged.entity_mappings)
                .await?;
        }

        if let (Some(cm), Some(op_id)) =
            (self.checkpoint_manager.as_ref(), self.operation_id.as_ref())
            && !staged.hashes.is_empty()
            && let Err(e) = cm
                .update_work_unit_statuses(
                    op_id,
                    "bm25_commit",
                    &staged.hashes,
                    WorkUnitStatus::Committed,
                )
                .await
        {
            tracing::warn!(error = %e, "Failed to update BM25 work unit checkpoints to committed");
        }
        Ok(())
    }

    /// Store documents in BM25 from chunked results with batch processing
    pub async fn store_bm25_batched(
        &self,
        chunks: &[ChunkedResult],
        batch_size: usize,
    ) -> Result<(), OrchestratorError> {
        let staged = self.stage_bm25(chunks, batch_size, None).await?;
        self.commit_staged_bm25(&staged).await
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

        // Use a map to aggregate multiple BM25 docs per entity. Links are
        // accumulated in place: re-serializing the whole mapping on every hit
        // costs O(docs per entity) per link, so an entity referenced by many
        // chunks becomes quadratic.
        let mut entity_detail_map: std::collections::HashMap<i64, PendingEntityMapping> =
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

                let pending = entity_detail_map.entry(entity_id_i64).or_insert_with(|| {
                    PendingEntityMapping {
                        mapping: EntityDetailMapping::new(entity_id_i64)
                            .with_project_id(self.project_id)
                            .with_epoch(self.epoch()),
                        doc_ids: Vec::new(),
                        seen: std::collections::HashSet::new(),
                    }
                });
                if pending.seen.insert(doc.document_id.clone()) {
                    pending.doc_ids.push(doc.document_id.clone());
                }
            }
        }

        Ok(entity_detail_map
            .into_values()
            .map(|pending| pending.mapping.with_bm25_doc_ids(&pending.doc_ids))
            .collect())
    }
}

/// Accumulator for one entity's BM25 document links, collected in a single
/// pass so the mapping is serialized once instead of per link.
struct PendingEntityMapping {
    mapping: EntityDetailMapping,
    doc_ids: Vec<String>,
    seen: std::collections::HashSet<String>,
}
