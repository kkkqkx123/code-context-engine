//! BM25 full-text write path.

use cce_storage_common::FulltextStorage;

use cce_parser::ast_to_nl::chunker::{ChunkPath, ChunkedResult};
use cce_storage_common::FulltextDocument;
use cce_storage_common::metadb::{ChunkRecord, EntityDetailMapping};
use cce_storage_common::metadb::{WorkUnitCheckpointRecord, WorkUnitStatus};

use crate::error::OrchestratorError;

use super::mapping::{build_bm25_documents, build_chunk_record, compute_work_unit_hash};
use super::{StorageCoordinator, WriteFailurePoint};

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
        let committed: std::collections::HashMap<String, WorkUnitStatus> = if let (
            Some(cm),
            Some(op_id),
        ) =
            (wk_cm.as_ref(), wk_op.as_ref())
        {
            match cm.get_work_units(op_id, "bm25_commit").await {
                Ok(records) => records
                    .into_iter()
                    .map(|record| (record.work_unit_hash, record.status))
                    .collect(),
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to read BM25 work units; replaying all microbatches");
                    std::collections::HashMap::new()
                }
            }
        } else {
            std::collections::HashMap::new()
        };
        for batch in bm25_chunks.chunks(batch_size) {
            let hash = if wk_cm.is_some() && wk_op.is_some() {
                Some(compute_work_unit_hash(batch))
            } else {
                None
            };
            if let Some(hash) = hash.as_ref()
                && committed.get(hash) == Some(&WorkUnitStatus::Committed)
            {
                continue;
            }

            let documents = build_bm25_documents(batch, self.project_id, self.epoch());
            let entity_mappings =
                self.build_bm25_entity_mappings(batch, &documents, source_entity_ids)?;
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
        if self.injected_failure(WriteFailurePoint::Bm25BeforeCommit) {
            return Err(OrchestratorError::index(
                "test_write_failure",
                "BM25 commit failed before publishing documents",
            ));
        }
        bm25.commit_staged(&bm25.configured_index_name()).await?;

        if self.injected_failure(WriteFailurePoint::Bm25BeforeDerivedMetadata) {
            return Err(OrchestratorError::index(
                "test_write_failure",
                "BM25 derived metadata failed after commit",
            ));
        }
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{StagedBm25Write, StorageCoordinator, WriteFailurePoint};
    use crate::CheckpointManager;
    use crate::index::{MetadataStore, WriteFailureInjector};
    use crate::operation::checkpoint::CreateCheckpointParams;
    use cce_config::modules::Bm25Config;
    use cce_storage_bm25::Bm25Client;
    use cce_storage_common::FulltextDocument;
    use cce_storage_common::FulltextStorage;
    use cce_storage_common::MetadataStorage;
    use cce_storage_common::metadb::{EntityRecord, WorkUnitCheckpointRecord, WorkUnitStatus};
    use cce_types::OperationKind;

    async fn bm25_storage(
        injector: Arc<WriteFailureInjector>,
        op: &str,
    ) -> (
        tempfile::TempDir,
        StorageCoordinator,
        CheckpointManager,
        StagedBm25Write,
        MetadataStore,
        i64,
    ) {
        let index_dir = tempfile::tempdir().expect("bm25 index temp dir");
        let config = Bm25Config::default()
            .enabled()
            .with_index_name("default")
            .with_index_path(index_dir.path().to_string_lossy().as_ref());
        let mut client = Bm25Client::new(config);
        client.connect().await.expect("connect bm25 client");
        let bm25 = Arc::new(client);
        let document = FulltextDocument::new("doc-post-commit").with_field("content", "sample");
        FulltextStorage::stage_documents(bm25.as_ref(), "default", &[document])
            .await
            .expect("stage bm25 document");

        let metadata = crate::index::in_memory_metadata_store().expect("metadata store");
        metadata
            .ensure_project(7, "/root")
            .await
            .expect("register project");
        metadata
            .files_record_hashes(7, 3, &[("src/lib.rs".to_string(), "hash".to_string())])
            .await
            .expect("seed file row");
        let file = metadata
            .file_by_path(7, "src/lib.rs")
            .await
            .expect("read seeded file")
            .expect("seeded file exists");
        let entity_ids = metadata
            .entities_insert_returning_ids(&[EntityRecord {
                id: 0,
                name: "seed".to_string(),
                kind: "function".to_string(),
                file_id: file.id,
                signature: None,
                span_start_row: None,
                span_end_row: None,
                span_start_column: None,
                span_end_column: None,
                span_start_byte: None,
                span_end_byte: None,
                scoped_name: None,
                overload_discriminator: String::new(),
                depth: None,
                parent_id: None,
                metadata: None,
                parameters_json: None,
                return_type: None,
                doc_comment: None,
                modifiers_json: None,
                project_id: 7,
                epoch: 3,
                batch_id: 1,
                rank: 0.0,
            }])
            .await
            .expect("seed entity");
        let entity_id = entity_ids.first().copied().expect("entity id returned");

        let cm = CheckpointManager::new_for_project(7, metadata.clone());
        cm.create_checkpoint(CreateCheckpointParams {
            operation_id: op,
            operation_type: OperationKind::FullIndex,
            root_dir: "/root",
            total_files: 1,
            batch_size: 1,
            file_list_hash: "hash",
        })
        .await
        .expect("create checkpoint");
        cm.insert_work_units(&[WorkUnitCheckpointRecord {
            id: None,
            project_id: 7,
            operation_id: op.to_string(),
            stage: "bm25_commit".to_string(),
            target_epoch: 3,
            work_unit_hash: "hash-fail".to_string(),
            status: WorkUnitStatus::Running,
            item_count: 1,
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        }])
        .await
        .expect("insert running work unit");

        let test_cm = CheckpointManager::new_for_project(7, metadata.clone());
        let storage = StorageCoordinator::new(7)
            .expect("valid project ID")
            .with_fulltext_store(crate::index::FulltextStore::local(bm25))
            .with_metadata_store(metadata.clone())
            .with_checkpoint_manager(Arc::new(cm), op.to_string())
            .with_test_write_failure(injector);
        let staged = StagedBm25Write {
            documents: 1,
            entity_mappings: vec![
                cce_storage_common::metadb::EntityDetailMapping::new(entity_id)
                    .with_project_id(7)
                    .with_epoch(4)
                    .with_bm25_doc_ids(&["doc-post-commit".to_string()]),
            ],
            chunk_records: Vec::new(),
            hashes: vec!["hash-fail".to_string()],
        };
        (index_dir, storage, test_cm, staged, metadata, entity_id)
    }

    #[tokio::test]
    async fn bm25_failure_before_commit_keeps_work_unit_running() {
        let injector = Arc::new(WriteFailureInjector::default());
        injector.arm(WriteFailurePoint::Bm25BeforeCommit, 1);
        let (_dir, storage, cm, staged, _, _) =
            bm25_storage(injector.clone(), "bm25-pre-commit").await;

        storage
            .commit_staged_bm25(&staged)
            .await
            .expect_err("pre-commit failure must propagate");
        assert_eq!(storage.fulltext_write_metrics().map(|m| m.commits), Some(0));
        assert_eq!(
            cm.get_work_unit_by_hash("bm25-pre-commit", "bm25_commit", "hash-fail")
                .await
                .expect("read work unit")
                .expect("work unit exists")
                .status,
            WorkUnitStatus::Running
        );

        injector.arm(WriteFailurePoint::Bm25BeforeCommit, 0);
        storage
            .commit_staged_bm25(&staged)
            .await
            .expect("retry commits");
        assert_eq!(
            cm.get_work_unit_by_hash("bm25-pre-commit", "bm25_commit", "hash-fail")
                .await
                .expect("read work unit")
                .expect("work unit exists")
                .status,
            WorkUnitStatus::Committed
        );
    }

    #[tokio::test]
    async fn bm25_failure_after_commit_keeps_metadata_unmarked() {
        let injector = Arc::new(WriteFailureInjector::default());
        injector.arm(WriteFailurePoint::Bm25BeforeDerivedMetadata, 1);
        let (_dir, storage, cm, staged, metadata, entity_id) =
            bm25_storage(injector.clone(), "bm25-post-commit").await;

        storage
            .commit_staged_bm25(&staged)
            .await
            .expect_err("post-commit failure must propagate");
        assert!(
            storage
                .fulltext_write_metrics()
                .is_some_and(|m| m.commits >= 1),
            "fulltext commit must have happened before this failure"
        );
        assert_eq!(
            cm.get_work_unit_by_hash("bm25-post-commit", "bm25_commit", "hash-fail")
                .await
                .expect("read work unit")
                .expect("work unit exists")
                .status,
            WorkUnitStatus::Running
        );

        injector.arm(WriteFailurePoint::Bm25BeforeDerivedMetadata, 0);
        storage
            .commit_staged_bm25(&staged)
            .await
            .expect("retry publishes derived metadata");
        assert_eq!(
            cm.get_work_unit_by_hash("bm25-post-commit", "bm25_commit", "hash-fail")
                .await
                .expect("read work unit")
                .expect("work unit exists")
                .status,
            WorkUnitStatus::Committed
        );
        assert!(
            metadata
                .mappings_by_entity(7, entity_id, 4)
                .await
                .expect("read mapping")
                .is_some(),
            "retry must restore derived mapping"
        );
    }
}
