//! Embedding/vector write path.
//!
//! Produces Qdrant points, SQLite chunk records and entity detail mappings
//! from embedding-path chunks, with per-microbatch checkpointing.

use std::sync::Arc;

use crate::CheckpointManager;
use cce_llm::LlmError;
use cce_parser::ast_to_nl::chunker::{ChunkPath, ChunkedResult};
use cce_storage_common::Payload;
use cce_storage_qdrant::VectorPoint;
use cce_storage_sqlite::types::{WorkUnitCheckpointRecord, WorkUnitStatus};
use cce_storage_sqlite::{
    ChunkRecord, ChunkRepository, EntityDetailMapping, EntityDetailMappingRepository,
};
use cce_types::PointKind;
use cce_types::TestSource;
use cce_types::ast_to_nl::FileCategory;
use cce_types::error::common::ErrorClassify;

use crate::error::OrchestratorError;

use super::StorageCoordinator;
use super::mapping::{
    build_chunk_record, chunk_segment_id, compute_work_unit_hash, project_chunk_point_id,
};

/// Whether an embedding failure should be retried rather than aborting the
/// whole batch pass: rate limits (429) and transient server errors (5xx) are
/// deferred, everything else is a hard failure.
fn is_retryable_llm_error(error: &LlmError) -> bool {
    error.is_retryable()
}

impl StorageCoordinator {
    /// Store vectors from chunked results with batch processing
    ///
    /// This method processes chunks in batches to:
    /// 1. Control memory usage during embedding
    /// 2. Avoid API rate limits by adding delays between batches
    ///
    /// # Arguments
    ///
    /// * `chunks` - Chunked results to store
    /// * `batch_size` - Number of chunks per embedding API call
    /// * `batch_delay_ms` - Milliseconds to sleep between batches
    pub async fn store_vectors_batched(
        &self,
        chunks: &[ChunkedResult],
        batch_size: usize,
        batch_delay_ms: u64,
    ) -> Result<usize, OrchestratorError> {
        // When embedder is missing, store chunk records only (no vector embedding).
        // This supports benchmark data generation without requiring an embedder.
        let embedder = match &self.embedder {
            Some(e) => e,
            None => {
                tracing::trace!("Embedder not configured; storing chunk records only");
                self.store_chunk_records_only(chunks)?;
                return Ok(0);
            }
        };

        let qdrant = match &self.qdrant {
            Some(q) => q,
            None => {
                tracing::trace!("Qdrant not configured; storing chunk records only");
                self.store_chunk_records_only(chunks)?;
                return Ok(0);
            }
        };
        self.ensure_project_group_id()?;

        // Filter to only Embedding-path chunks
        let embedding_chunks: Vec<&ChunkedResult> = chunks
            .iter()
            .filter(|c| c.path == ChunkPath::Embedding)
            .collect();

        if embedding_chunks.is_empty() {
            return Ok(0);
        }

        let mut total_stored = 0;
        let total_chunks = embedding_chunks.len();
        let num_batches = total_chunks.div_ceil(batch_size);

        let wk_cm = self.checkpoint_manager.clone();
        let wk_op = self.operation_id.clone();

        // Batches rejected by a rate limit (429) or a transient server error
        // (5xx) are retried once after the whole pass, mirroring the summary
        // generator's deferred-retry semantics: retries do not add load while
        // other batches are still embedding, and the inter-batch delay acts as
        // natural backoff.
        type DeferredBatch<'a> = (
            &'a [&'a ChunkedResult],
            Option<(Arc<CheckpointManager>, String, String)>,
        );
        let mut deferred_retries: Vec<DeferredBatch<'_>> = Vec::new();
        let mut deferred_retry_after_ms: u64 = 0;
        let mut deferred_reason: Option<String> = None;

        for (batch_idx, batch) in embedding_chunks.chunks(batch_size).enumerate() {
            // Work unit checkpoint: skip if this microbatch is already committed
            let wu_state: Option<(Arc<CheckpointManager>, String, String)> =
                if let (Some(cm), Some(op_id)) = (&wk_cm, &wk_op) {
                    let hash = compute_work_unit_hash(batch);
                    match cm
                        .get_work_unit_by_hash(op_id, "embedding_generation", &hash)
                        .await
                    {
                        Ok(Some(record)) if record.status == WorkUnitStatus::Committed => {
                            total_stored += batch.len();
                            continue;
                        }
                        _ => {
                            let record = WorkUnitCheckpointRecord {
                                id: None,
                                project_id: self.project_id,
                                operation_id: op_id.clone(),
                                stage: "embedding_generation".to_string(),
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
                                    "Failed to insert embedding work unit checkpoint"
                                );
                            }
                            Some((cm.clone(), op_id.clone(), hash))
                        }
                    }
                } else {
                    None
                };

            // Prepare texts for this batch
            let texts: Vec<&str> = batch.iter().map(|c| c.text.as_str()).collect();

            if texts.is_empty() {
                tracing::warn!("No embedding text found in batch {}", batch_idx);
                continue;
            }

            // Generate embeddings for this batch; rate-limited or transiently
            // failed (5xx) batches are deferred to the retry pass at the end
            // of the loop.
            let embeddings = match embedder.embed(&texts).await {
                Ok(embeddings) => embeddings,
                Err(err) if is_retryable_llm_error(&err) => {
                    if let cce_llm::LlmError::RateLimitExceeded(retry_after) = &err {
                        deferred_retry_after_ms = deferred_retry_after_ms.max(*retry_after);
                    }
                    deferred_reason = Some(err.to_string());
                    tracing::warn!(
                        batch = batch_idx,
                        error = %err,
                        "Embedding batch failed transiently, deferring retry to batch end"
                    );
                    deferred_retries.push((batch, wu_state));
                    continue;
                }
                Err(e) => return Err(e.into()),
            };

            total_stored += self
                .persist_embedding_batch(batch, &embeddings, &wu_state, qdrant)
                .await?;

            // Sleep between batches to avoid rate limits (skip last batch)
            if batch_delay_ms > 0 && batch_idx < num_batches - 1 {
                tokio::time::sleep(tokio::time::Duration::from_millis(batch_delay_ms)).await;
            }
        }

        // Retry pass for deferred (rate-limited / transient 5xx) batches: honor the
        // longest retry-after window seen so the upstream has time to recover.
        if !deferred_retries.is_empty() {
            tracing::warn!(
                count = deferred_retries.len(),
                error = deferred_reason.as_deref().unwrap_or(""),
                "Embedding batches failed transiently, retrying deferred batches"
            );
            let backoff_ms = deferred_retry_after_ms.max(batch_delay_ms);
            if backoff_ms > 0 {
                tokio::time::sleep(tokio::time::Duration::from_millis(backoff_ms)).await;
            }
            for (batch, wu_state) in deferred_retries {
                let texts: Vec<&str> = batch.iter().map(|c| c.text.as_str()).collect();
                match embedder.embed(&texts).await {
                    Ok(embeddings) => {
                        total_stored += self
                            .persist_embedding_batch(batch, &embeddings, &wu_state, qdrant)
                            .await?;
                    }
                    Err(err) if is_retryable_llm_error(&err) => {
                        // Leave the work unit Running (uncommitted): the
                        // checkpoint/resume mechanism reruns only this unit
                        // instead of the whole operation.
                        let work_unit_hash = wu_state
                            .as_ref()
                            .map(|(_, _, hash)| hash.as_str())
                            .unwrap_or("<none>");
                        tracing::warn!(
                            work_unit_hash,
                            error = %err,
                            "Embedding batch still failing after retry; work unit left uncommitted for resume"
                        );
                    }
                    Err(e) => return Err(e.into()),
                }
            }
        }

        Ok(total_stored)
    }

    /// Persist already-generated embeddings for one batch: builds Qdrant
    /// points and SQLite records, stores them, and commits the work unit
    /// checkpoint.
    async fn persist_embedding_batch(
        &self,
        batch: &[&ChunkedResult],
        embeddings: &cce_llm::EmbeddingResult,
        wu_state: &Option<(Arc<CheckpointManager>, String, String)>,
        qdrant: &Arc<cce_storage_qdrant::QdrantClient>,
    ) -> Result<usize, OrchestratorError> {
        // Build vector points and chunk records for this batch
        let (points, chunk_records, entity_mappings) = self
            .build_storage_data(batch, &embeddings.embeddings)
            .await?;

        let mut stored = 0;

        // Store to Qdrant
        if !points.is_empty() {
            qdrant.upsert_points(&points).await?;
            stored = points.len();
        }

        // Store chunk records
        if !chunk_records.is_empty() {
            self.store_chunk_records(&chunk_records)?;
        }

        // Store entity mappings
        if !entity_mappings.is_empty() {
            self.store_entity_mappings(&entity_mappings)?;
        }

        // Mark work unit as committed after successful processing
        if let Some((cm, op_id, hash)) = wu_state {
            if let Err(e) = cm
                .update_work_unit_status(
                    op_id,
                    "embedding_generation",
                    hash,
                    WorkUnitStatus::Committed,
                )
                .await
            {
                tracing::warn!(
                    error = %e,
                    "Failed to update embedding work unit checkpoint to committed"
                );
            }
        }

        Ok(stored)
    }

    /// Build storage data from chunks and embeddings
    async fn build_storage_data(
        &self,
        chunks: &[&ChunkedResult],
        dense_embeddings: &[Vec<f32>],
    ) -> Result<(Vec<VectorPoint>, Vec<ChunkRecord>, Vec<EntityDetailMapping>), OrchestratorError>
    {
        let mut points = Vec::new();
        let mut chunk_records = Vec::new();
        // Use a map to aggregate multiple chunks per entity
        let mut entity_detail_map: std::collections::HashMap<i64, EntityDetailMapping> =
            std::collections::HashMap::new();
        let group_id = self.project_group_id.clone();
        let epoch = self.epoch();
        let batch_id = self.batch_id();
        let source_entity_ids = self.load_source_entity_ids()?;

        for (chunk, dense_vec) in chunks.iter().zip(dense_embeddings.iter()) {
            let content_entity_ids: Vec<i64> = chunk
                .metadata
                .content_entity_ids()
                .iter()
                .map(|id| id.0 as i64)
                .collect();
            let payload = Payload::new(chunk.metadata.file_path.clone())
                .with_type(PointKind::Chunk)
                .with_source_id(chunk.chunk_id.clone())
                .with_group_id(group_id.clone())
                .with_category(chunk.metadata.file_category)
                .with_epoch(epoch)
                .with_batch_id(batch_id)
                .with_entity_ids(content_entity_ids)
                .with_segment_id(chunk_segment_id(chunk))
                .with_test(chunk.metadata.test_info.is_test())
                .with_test_source(chunk.metadata.test_info.source);

            // Create vector point with project-scoped ID
            let point_id = project_chunk_point_id(&group_id, epoch, &chunk.chunk_id);
            let point = VectorPoint::new(point_id.clone(), dense_vec.clone(), payload);
            points.push(point);

            // Chunk record for SQLite enrichment (raw source code + line numbers)
            let chunk_record = build_chunk_record(chunk, self.project_id, epoch, batch_id)?;

            chunk_records.push(chunk_record);

            // Aggregate entity detail mappings - one mapping per entity with multiple chunks
            for entity_id in chunk.metadata.content_entity_ids() {
                let entity_id_i64 = if self.metadata_store.is_some() {
                    let Some(db_id) = source_entity_ids
                        .get(&(chunk.metadata.file_path.clone(), entity_id.0 as i64))
                        .copied()
                    else {
                        tracing::trace!(
                            file = %chunk.metadata.file_path,
                            source_entity_id = entity_id.0,
                            "Skipping entity mapping without an epoch-scoped entity record"
                        );
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

                // Add Qdrant point ID
                let mut point_ids = entry.get_qdrant_point_ids();
                if !point_ids.contains(&point_id) {
                    point_ids.push(point_id.clone());
                }
                *entry = entry.clone().with_qdrant_point_ids(&point_ids);
            }
        }

        let entity_mappings = entity_detail_map.into_values().collect();
        Ok((points, chunk_records, entity_mappings))
    }

    pub(crate) fn load_source_entity_ids(
        &self,
    ) -> Result<std::collections::HashMap<(String, i64), i64>, OrchestratorError> {
        let mut result = std::collections::HashMap::new();
        let Some(client) = self.metadata_store.as_ref().map(|store| store.as_ref()) else {
            return Ok(result);
        };
        let conn = client
            .write_connection()
            .map_err(OrchestratorError::Storage)?;
        let mut statement = conn
            .prepare(
                "SELECT f.path, e.metadata, e.id
                 FROM entities e JOIN files f ON f.id = e.file_id
                 WHERE e.project_id = ?1 AND e.epoch = ?2",
            )
            .map_err(|error| {
                OrchestratorError::Storage(cce_types::StorageError::query(error.to_string()))
            })?;
        let rows = statement
            .query_map(rusqlite::params![self.project_id, self.epoch()], |row| {
                let path: String = row.get(0)?;
                let metadata: Option<String> = row.get(1)?;
                let db_id: i64 = row.get(2)?;
                Ok((path, metadata, db_id))
            })
            .map_err(|error| {
                OrchestratorError::Storage(cce_types::StorageError::query(error.to_string()))
            })?;
        for row in rows {
            let (path, metadata, db_id) = row.map_err(|error| {
                OrchestratorError::Storage(cce_types::StorageError::query(error.to_string()))
            })?;
            let Some(metadata) = metadata else {
                continue;
            };
            let Ok(metadata) =
                serde_json::from_str::<std::collections::HashMap<String, String>>(&metadata)
            else {
                continue;
            };
            let Some(source_id) = metadata
                .get("__source_entity_id")
                .and_then(|value| value.parse::<i64>().ok())
            else {
                continue;
            };
            result.insert((path, source_id), db_id);
        }
        Ok(result)
    }

    /// Store entity detail mappings in SQLite
    pub(crate) fn store_entity_mappings(
        &self,
        mappings: &[EntityDetailMapping],
    ) -> Result<(), OrchestratorError> {
        if let Some(client) = self.metadata_store.as_deref() {
            let result = client.with_transaction(|tx| {
                for mapping in mappings {
                    EntityDetailMappingRepository::upsert(tx, mapping)?;
                }
                Ok(())
            });

            result.map_err(OrchestratorError::Storage)?;
        }
        Ok(())
    }

    /// Store chunk records in SQLite
    pub(crate) fn store_chunk_records(
        &self,
        chunks: &[ChunkRecord],
    ) -> Result<(), OrchestratorError> {
        if let Some(client) = self.metadata_store.as_deref() {
            let result = client.with_transaction(|tx| ChunkRepository::insert_batch(tx, chunks));

            result.map_err(OrchestratorError::Storage)?;
        }
        Ok(())
    }

    /// Store only embedding-path chunk records to SQLite without requiring
    /// an embedder or Qdrant. Used by benchmark data generation.
    pub fn store_chunk_records_only(
        &self,
        chunks: &[ChunkedResult],
    ) -> Result<(), OrchestratorError> {
        let embedding_chunks: Vec<&ChunkedResult> = chunks
            .iter()
            .filter(|c| c.path == ChunkPath::Embedding)
            .collect();

        if embedding_chunks.is_empty() {
            return Ok(());
        }

        let chunk_records: Result<Vec<ChunkRecord>, OrchestratorError> = embedding_chunks
            .iter()
            .map(|chunk| build_chunk_record(chunk, self.project_id, self.epoch(), self.batch_id()))
            .collect();

        let chunk_records = chunk_records?;
        self.store_chunk_records(&chunk_records)?;
        Ok(())
    }

    /// Regenerate vectors for stored embedding-path chunk records in place.
    ///
    /// Each record is paired with its file-level category (resolved from the
    /// `files` table by the caller's query join). The chunk texts come
    /// straight from the SQLite records, so switching the embedder model
    /// never re-parses or re-chunks the source files. Chunk IDs (and
    /// therefore point IDs and entity mappings) are unchanged — only the
    /// vector values are refreshed via upsert.
    pub async fn reembed_vectors_from_records(
        &self,
        records: &[(ChunkRecord, u8)],
        batch_size: usize,
    ) -> Result<usize, OrchestratorError> {
        if records.is_empty() {
            return Ok(0);
        }
        let embedder = match &self.embedder {
            Some(e) => e,
            None => return Ok(0),
        };
        let qdrant = match &self.qdrant {
            Some(q) => q,
            None => return Ok(0),
        };
        self.ensure_project_group_id()?;

        let mut stored = 0;
        for batch in records.chunks(batch_size.max(1)) {
            let texts: Vec<&str> = batch.iter().map(|(r, _)| r.content.as_str()).collect();
            let embeddings = embedder.embed(&texts).await?;
            if embeddings.embeddings.len() != batch.len() {
                return Err(OrchestratorError::index(
                    "reembed_vectors",
                    format!(
                        "embedder returned {} vectors for {} chunks",
                        embeddings.embeddings.len(),
                        batch.len()
                    ),
                ));
            }
            let vectors: Vec<Vec<f32>> = embeddings.embeddings.clone();
            let points =
                build_reembed_points(batch.iter().zip(vectors.iter()), &self.project_group_id);
            stored += points.len();
            qdrant.upsert_points(&points).await?;
        }
        Ok(stored)
    }
}

/// Build Qdrant points for already-stored chunk records with fresh embeddings.
///
/// Payload fields are reconstructed from the persisted record columns plus the
/// file-level category resolved through the query join, so the regenerated
/// points keep their filtering semantics (category, test markers, segment
/// alignment).
fn build_reembed_points<'a>(
    records: impl Iterator<Item = (&'a (ChunkRecord, u8), &'a Vec<f32>)>,
    group_id: &str,
) -> Vec<VectorPoint> {
    records
        .map(|((record, category), vector)| {
            let segment_id = if record.segment_id.is_empty() {
                record.chunk_id.clone()
            } else {
                record.segment_id.clone()
            };
            let payload = Payload::new(record.file_path.clone())
                .with_type(PointKind::Chunk)
                .with_source_id(record.chunk_id.clone())
                .with_group_id(group_id)
                .with_category(FileCategory::from_u8(*category).unwrap_or_default())
                .with_epoch(record.epoch)
                .with_batch_id(record.batch_id)
                .with_entity_ids(record.get_entity_ids())
                .with_segment_id(segment_id)
                .with_test(record.test_status == 1)
                .with_test_source(TestSource::from_u8(record.test_source));

            VectorPoint::new(
                project_chunk_point_id(group_id, record.epoch, &record.chunk_id),
                vector.clone(),
                payload,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::StorageCoordinator;
    use cce_config::modules::{DistanceMetric, QdrantConfig};
    use cce_llm::{Embedder, EmbeddingResult, LlmError};
    use cce_parser::ast_to_nl::chunker::{
        ChunkMetadata, ChunkPath, ChunkedResult, CodeSpecificMetadata,
    };
    use cce_storage_qdrant::QdrantClient;
    use cce_storage_sqlite::ChunkRecord;
    use cce_types::ast_to_nl::FileCategory;
    use cce_types::entity::{EntityId, EntityKind};
    use cce_types::{Language, Span};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// Embedder stub: rate-limits the first `rate_limit_calls` invocations and
    /// succeeds afterwards with fixed-dimension vectors.
    struct StubEmbedder {
        calls: AtomicU32,
        rate_limit_calls: u32,
    }

    #[async_trait::async_trait]
    impl Embedder for StubEmbedder {
        async fn embed(&self, texts: &[&str]) -> Result<EmbeddingResult, LlmError> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            if call < self.rate_limit_calls {
                return Err(LlmError::rate_limit_exceeded(5));
            }
            Ok(EmbeddingResult {
                embeddings: texts.iter().map(|_| vec![0.5_f32, 0.5_f32]).collect(),
                prompt_tokens: 0,
                total_tokens: 0,
            })
        }

        async fn embed_one(&self, text: &str) -> Result<Vec<f32>, LlmError> {
            self.embed(&[text])
                .await
                .map(|r| r.embeddings.first().cloned().unwrap_or_default())
        }

        async fn embed_vectors(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, LlmError> {
            self.embed(texts).await.map(|r| r.embeddings)
        }

        fn dimension(&self) -> usize {
            2
        }

        fn model_name(&self) -> &str {
            "stub-embedder"
        }

        fn is_healthy(&self) -> bool {
            true
        }
    }

    /// Minimal in-process Qdrant stand-in: answers every request with a
    /// successful upsert response.
    async fn spawn_mock_qdrant_url() -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock qdrant port");
        let addr = listener.local_addr().expect("local addr");
        tokio::spawn(async move {
            loop {
                let (mut socket, _) = match listener.accept().await {
                    Ok(accepted) => accepted,
                    Err(_) => break,
                };
                tokio::spawn(async move {
                    let mut buf = [0u8; 4096];
                    let _ = socket.read(&mut buf).await;
                    let response = "HTTP/1.1 200 OK\r\ncontent-length: 2\r\ncontent-type: application/json\r\n\r\n{}";
                    let _ = socket.write_all(response.as_bytes()).await;
                });
            }
        });
        format!("http://{addr}")
    }

    fn embedding_chunk(id: &str, text: &str) -> ChunkedResult {
        let mut chunk = ChunkedResult::new(
            id.to_string(),
            format!("{id}-source"),
            ChunkPath::Embedding,
            0,
            1,
        );
        chunk.text = text.to_string();
        chunk.metadata = ChunkMetadata::for_code(
            format!("{id}.rs"),
            Span::default(),
            Language::Rust,
            CodeSpecificMetadata {
                content_entity_ids: vec![EntityId(7)],
                entity_kind: EntityKind::Function,
                ..Default::default()
            },
        );
        chunk
    }

    fn storage_with(qdrant_url: &str, embedder: Arc<dyn Embedder>) -> StorageCoordinator {
        let qdrant_config = QdrantConfig {
            url: qdrant_url.to_string(),
            vector_size: 2,
            distance_metric: DistanceMetric::Cosine,
            timeout_ms: 5000,
            max_retries: 0,
            retry_delay_ms: 10,
            enabled: true,
            ..Default::default()
        };
        let qdrant =
            Arc::new(QdrantClient::new(qdrant_config, ".").expect("qdrant client must build"));
        StorageCoordinator::new(7)
            .expect("valid project ID")
            .with_project_group_id("project-7-root")
            .with_qdrant(qdrant)
            .with_embedder(embedder)
    }

    #[tokio::test]
    async fn rate_limited_batch_is_deferred_and_retried_after_other_batches() {
        let qdrant_url = spawn_mock_qdrant_url().await;
        let stub = Arc::new(StubEmbedder {
            calls: AtomicU32::new(0),
            rate_limit_calls: 1,
        });
        let embedder: Arc<dyn Embedder> = stub.clone();
        let storage = storage_with(&qdrant_url, embedder.clone());

        let chunks = [
            embedding_chunk("a", "first"),
            embedding_chunk("b", "second"),
        ];
        let stored = storage
            .store_vectors_batched(&chunks, 1, 0)
            .await
            .expect("store must succeed");

        // Batch "a" was rate limited on its first attempt, deferred, and
        // retried after batch "b"; both batches end up stored.
        assert_eq!(stored, 2);
        assert_eq!(
            stub.calls.load(Ordering::SeqCst),
            3,
            "expected: batch a (429) + batch b + batch a retry"
        );
    }

    #[tokio::test]
    async fn rate_limited_retry_failure_leaves_batch_uncommitted_but_continues() {
        let qdrant_url = spawn_mock_qdrant_url().await;
        // Both the initial attempt and the deferred retry are rate limited.
        let stub = Arc::new(StubEmbedder {
            calls: AtomicU32::new(0),
            rate_limit_calls: 3,
        });
        let embedder: Arc<dyn Embedder> = stub.clone();
        let storage = storage_with(&qdrant_url, embedder.clone());

        let chunks = [
            embedding_chunk("a", "first"),
            embedding_chunk("b", "second"),
        ];
        let stored = storage
            .store_vectors_batched(&chunks, 1, 0)
            .await
            .expect("store must not abort on rate-limited retry");

        // Batch "b" commits; batch "a" stays uncommitted (recoverable via the
        // checkpoint/resume mechanism on a later operation).
        assert_eq!(stored, 1);
        assert_eq!(stub.calls.load(Ordering::SeqCst), 4);
    }

    #[tokio::test]
    async fn storage_mapping_uses_the_actual_scoped_qdrant_point_id() {
        let mut chunk = ChunkedResult::new(
            "shared-chunk".to_string(),
            "shared-source".to_string(),
            ChunkPath::Embedding,
            0,
            1,
        );
        chunk.text = "embedded content".to_string();
        chunk.metadata = ChunkMetadata::for_code(
            "src/lib.rs".to_string(),
            Span::default(),
            Language::Rust,
            CodeSpecificMetadata {
                content_entity_ids: vec![EntityId(42)],
                entity_kind: EntityKind::Function,
                ..Default::default()
            },
        );
        let storage = StorageCoordinator::new(7)
            .expect("valid project ID")
            .with_project_group_id("project-7-root");
        let chunks = [&chunk];
        let embeddings = [vec![0.25_f32, 0.75_f32]];

        let (points, _, mappings) = storage
            .build_storage_data(&chunks, &embeddings)
            .await
            .expect("build storage data");

        assert_eq!(points.len(), 1);
        assert_eq!(points[0].id, "project-7-root::0::shared-chunk");
        assert_eq!(mappings.len(), 1);
        assert_eq!(
            mappings[0].get_qdrant_point_ids(),
            vec!["project-7-root::0::shared-chunk"]
        );
    }

    #[tokio::test]
    async fn build_storage_data_stores_entity_id_and_segment_id_in_payload() {
        let mut chunk = ChunkedResult::new(
            "shared-chunk".to_string(),
            "shared-source".to_string(),
            ChunkPath::Embedding,
            0,
            1,
        );
        chunk.text = "embedded content".to_string();
        chunk.metadata = ChunkMetadata::for_code(
            "src/lib.rs".to_string(),
            Span::default(),
            Language::Rust,
            CodeSpecificMetadata {
                content_entity_ids: vec![EntityId(42)],
                entity_kind: EntityKind::Function,
                ..Default::default()
            },
        );
        chunk.metadata.segment_id = "shared-source".to_string();

        let storage = StorageCoordinator::new(7)
            .expect("valid project ID")
            .with_project_group_id("project-7-root");
        let chunks = [&chunk];
        let embeddings = [vec![0.25_f32, 0.75_f32]];

        let (points, _, _) = storage
            .build_storage_data(&chunks, &embeddings)
            .await
            .expect("build storage data");

        assert_eq!(points.len(), 1);
        let payload = &points[0].payload;

        assert_eq!(payload.entity_ids, Some(vec![42]));
        assert_eq!(payload.segment_id, Some("shared-source".to_string()));
    }

    #[tokio::test]
    async fn build_storage_data_document_chunk_segment_id_no_entity() {
        let mut chunk = ChunkedResult::new(
            "doc-chunk".to_string(),
            "doc-source".to_string(),
            ChunkPath::Embedding,
            0,
            1,
        );
        chunk.text = "document content".to_string();
        chunk.metadata.segment_id = "doc-source".to_string();

        let storage = StorageCoordinator::new(7)
            .expect("valid project ID")
            .with_project_group_id("project-7-root");
        let chunks = [&chunk];
        let embeddings = [vec![0.3_f32, 0.7_f32]];

        let (points, _, _) = storage
            .build_storage_data(&chunks, &embeddings)
            .await
            .expect("build storage data");

        assert_eq!(points.len(), 1);
        let payload = &points[0].payload;

        assert_eq!(payload.entity_ids, None);
        assert_eq!(payload.segment_id, Some("doc-source".to_string()));
    }

    #[tokio::test]
    async fn build_storage_data_fills_empty_segment_id_with_chunk_id() {
        let mut chunk = ChunkedResult::new(
            "external_emb_0".to_string(),
            "external".to_string(),
            ChunkPath::Embedding,
            0,
            1,
        );
        chunk.text = "plugin content".to_string();
        chunk.metadata.segment_id = String::new();

        let storage = StorageCoordinator::new(7)
            .expect("valid project ID")
            .with_project_group_id("project-7-root");
        let chunks = [&chunk];
        let embeddings = [vec![0.3_f32, 0.7_f32]];

        let (points, _, _) = storage
            .build_storage_data(&chunks, &embeddings)
            .await
            .expect("build storage data");

        assert_eq!(points.len(), 1);
        assert_eq!(
            points[0].payload.segment_id,
            Some("external_emb_0".to_string()),
            "empty segment_id must fall back to the raw chunk id"
        );
    }

    fn stored_record(id: &str) -> (ChunkRecord, u8) {
        (
            ChunkRecord::new(
                id.to_string(),
                "src/lib.rs".to_string(),
                format!("content of {id}"),
                0,
                9,
            )
            .with_project_id(7)
            .with_epoch(3)
            .with_batch_id(11)
            .with_entity_ids_json("[42]".to_string())
            .with_segment_id(format!("group_{id}"))
            .with_test_status(1)
            .with_test_source(2),
            FileCategory::Code.as_u8(),
        )
    }

    #[test]
    fn build_reembed_points_reconstructs_payload_from_records() {
        let record = stored_record("g1_emb_0");
        let records = [record];
        let points = super::build_reembed_points(
            records.iter().zip([&vec![0.1_f32, 0.9_f32]]),
            "project-7-root",
        );

        assert_eq!(points.len(), 1);
        let point = &points[0];
        assert_eq!(point.id, "project-7-root::3::g1_emb_0");
        assert_eq!(point.payload.segment_id.as_deref(), Some("group_g1_emb_0"));
        assert_eq!(point.payload.category, Some(FileCategory::Code));
        assert_eq!(point.payload.epoch, Some(3));
        assert_eq!(point.payload.batch_id, Some(11));
        assert_eq!(point.payload.entity_ids, Some(vec![42]));
        assert_eq!(point.payload.test, Some(true));
        assert_eq!(point.payload.source_id, "g1_emb_0");
    }

    #[test]
    fn build_reembed_points_falls_back_to_chunk_id_segment() {
        let (mut record, category) = stored_record("g1_emb_1");
        record.segment_id = String::new();
        let records = [(record, category)];
        let points = super::build_reembed_points(records.iter().zip([&vec![0.5_f32; 2]]), "g");

        assert_eq!(points[0].payload.segment_id, Some("g1_emb_1".to_string()));
    }

    #[tokio::test]
    async fn reembed_vectors_from_records_upserts_every_chunk() {
        let qdrant_url = spawn_mock_qdrant_url().await;
        let stub = Arc::new(StubEmbedder {
            calls: AtomicU32::new(0),
            rate_limit_calls: 0,
        });
        let embedder: Arc<dyn Embedder> = stub.clone();
        let storage = storage_with(&qdrant_url, embedder);

        let records = [stored_record("a"), stored_record("b")];
        let stored = storage
            .reembed_vectors_from_records(&records, 1)
            .await
            .expect("re-embed must succeed");

        assert_eq!(stored, 2);
        assert_eq!(
            stub.calls.load(Ordering::SeqCst),
            2,
            "one call per microbatch"
        );
    }

    #[tokio::test]
    async fn reembed_vectors_is_a_noop_without_embedder() {
        let storage = StorageCoordinator::new(7)
            .expect("valid project ID")
            .with_project_group_id("project-7-root");
        let records = [stored_record("a")];
        let stored = storage
            .reembed_vectors_from_records(&records, 8)
            .await
            .expect("no embedder must be a no-op");
        assert_eq!(stored, 0);
    }
}
