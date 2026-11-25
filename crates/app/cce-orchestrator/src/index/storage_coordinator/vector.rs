//! Embedding/vector write path.
//!
//! Produces Qdrant points, SQLite chunk records and entity detail mappings
//! from embedding-path chunks, with checkpoints flushed in batches.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use cce_llm::LlmError;
use cce_parser::ast_to_nl::chunker::{ChunkPath, ChunkedResult};
use cce_storage_common::metadb::{
    ChunkRecord, EntityDetailMapping, WorkUnitCheckpointRecord, WorkUnitStatus,
};
use cce_storage_common::{MetadataStorage, Payload, VectorPoint};
use cce_types::PointKind;
use cce_types::StorageError;
use cce_types::TestSource;
use cce_types::ast_to_nl::FileCategory;
use cce_types::error::common::ErrorClassify;

use crate::error::OrchestratorError;
use crate::index::result::VectorStageMetrics;
use crate::index::vector_store::{VectorBatchWriter, VectorStore};

use super::mapping::{
    build_chunk_record, chunk_segment_id, compute_work_unit_hash, project_chunk_point_id,
};
use super::{StorageCoordinator, WriteFailurePoint};

/// Identifiable error code for an embedding stage that exceeded its
/// wall-clock deadline (kept uncommitted for resume).
/// The canonical value lives with the error type so failure projections
/// preserve it; re-exported here for sibling and test use.
pub(crate) use crate::error::EMBEDDING_STAGE_TIMEOUT_CODE;

/// Read the parse-time entity id persisted in an entity row's metadata.
fn source_id_from_metadata(metadata: Option<&str>) -> Option<i64> {
    metadata
        .and_then(|raw| serde_json::from_str::<std::collections::HashMap<String, String>>(raw).ok())
        .and_then(|map| {
            map.get("__source_entity_id")
                .and_then(|value| value.parse::<i64>().ok())
        })
}

/// Per-microbatch wall-clock budget used to derive the embedding stage
/// deadline when none is configured: the single-request HTTP timeout (30s)
/// multiplied by a factor covering the client's retry/backoff budget for
/// both the first pass and the deferred-retry pass.
const EMBEDDING_MICROBATCH_BUDGET_SECS: u64 = 600;

/// Whether an embedding failure should be retried rather than aborting the
/// whole batch pass: rate limits (429) and transient server errors (5xx) are
/// deferred, everything else is a hard failure.
fn is_retryable_llm_error(error: &LlmError) -> bool {
    error.is_retryable()
}

/// Cumulative counters for one vector write pass.
///
/// The full-index loop resets them before the batch loop and snapshots them
/// once afterwards, so the stage record can attribute time without passing
/// mutable timing state through every storage call.
#[derive(Default)]
pub(crate) struct VectorStageCounters {
    embed_nanos: AtomicU64,
    point_build_nanos: AtomicU64,
    metadata_nanos: AtomicU64,
    work_unit_nanos: AtomicU64,
    flush_nanos: AtomicU64,
    stage_push_nanos: AtomicU64,
    entity_lookup_nanos: AtomicU64,
    delay_nanos: AtomicU64,
    work_unit_query_nanos: AtomicU64,
    work_unit_queries: AtomicU64,
    microbatches: AtomicU64,
    chunk_rows: AtomicU64,
    entity_mappings: AtomicU64,
    staged_records_max: AtomicU64,
}

impl VectorStageCounters {
    pub fn add_embed(&self, elapsed: Duration) {
        self.embed_nanos
            .fetch_add(elapsed.as_nanos() as u64, Ordering::Relaxed);
    }

    pub fn add_microbatch(&self) {
        self.microbatches.fetch_add(1, Ordering::Relaxed);
    }

    pub fn add_point_build(&self, elapsed: Duration) {
        self.point_build_nanos
            .fetch_add(elapsed.as_nanos() as u64, Ordering::Relaxed);
    }

    pub fn add_metadata(&self, elapsed: Duration) {
        self.metadata_nanos
            .fetch_add(elapsed.as_nanos() as u64, Ordering::Relaxed);
    }

    pub fn add_work_unit(&self, elapsed: Duration) {
        self.work_unit_nanos
            .fetch_add(elapsed.as_nanos() as u64, Ordering::Relaxed);
    }

    pub fn add_flush(&self, elapsed: Duration) {
        self.flush_nanos
            .fetch_add(elapsed.as_nanos() as u64, Ordering::Relaxed);
    }

    pub fn add_stage_push(&self, elapsed: Duration) {
        self.stage_push_nanos
            .fetch_add(elapsed.as_nanos() as u64, Ordering::Relaxed);
    }

    pub fn add_entity_lookup(&self, elapsed: Duration) {
        self.entity_lookup_nanos
            .fetch_add(elapsed.as_nanos() as u64, Ordering::Relaxed);
    }

    pub fn add_delay(&self, elapsed: Duration) {
        self.delay_nanos
            .fetch_add(elapsed.as_nanos() as u64, Ordering::Relaxed);
    }

    pub fn record_work_unit_query(&self, elapsed: Duration) {
        self.work_unit_query_nanos
            .fetch_add(elapsed.as_nanos() as u64, Ordering::Relaxed);
        self.work_unit_queries.fetch_add(1, Ordering::Relaxed);
    }

    pub fn add_staged_rows(&self, chunk_records: usize, entity_mappings: usize) {
        self.chunk_rows
            .fetch_add(chunk_records as u64, Ordering::Relaxed);
        self.entity_mappings
            .fetch_add(entity_mappings as u64, Ordering::Relaxed);
    }

    pub fn record_staged_window(&self, chunk_records: usize, entity_mappings: usize) {
        self.staged_records_max
            .fetch_max((chunk_records + entity_mappings) as u64, Ordering::Relaxed);
    }

    pub fn reset(&self) {
        self.embed_nanos.store(0, Ordering::Relaxed);
        self.point_build_nanos.store(0, Ordering::Relaxed);
        self.metadata_nanos.store(0, Ordering::Relaxed);
        self.work_unit_nanos.store(0, Ordering::Relaxed);
        self.flush_nanos.store(0, Ordering::Relaxed);
        self.stage_push_nanos.store(0, Ordering::Relaxed);
        self.entity_lookup_nanos.store(0, Ordering::Relaxed);
        self.delay_nanos.store(0, Ordering::Relaxed);
        self.work_unit_query_nanos.store(0, Ordering::Relaxed);
        self.work_unit_queries.store(0, Ordering::Relaxed);
        self.microbatches.store(0, Ordering::Relaxed);
        self.chunk_rows.store(0, Ordering::Relaxed);
        self.entity_mappings.store(0, Ordering::Relaxed);
        self.staged_records_max.store(0, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> VectorStageMetrics {
        VectorStageMetrics {
            embed_nanos: self.embed_nanos.load(Ordering::Relaxed),
            point_build_nanos: self.point_build_nanos.load(Ordering::Relaxed),
            metadata_nanos: self.metadata_nanos.load(Ordering::Relaxed),
            work_unit_nanos: self.work_unit_nanos.load(Ordering::Relaxed),
            flush_nanos: self.flush_nanos.load(Ordering::Relaxed),
            stage_push_nanos: self.stage_push_nanos.load(Ordering::Relaxed),
            entity_lookup_nanos: self.entity_lookup_nanos.load(Ordering::Relaxed),
            delay_nanos: self.delay_nanos.load(Ordering::Relaxed),
            work_unit_query_nanos: self.work_unit_query_nanos.load(Ordering::Relaxed),
            work_unit_queries: self.work_unit_queries.load(Ordering::Relaxed),
            microbatches: self.microbatches.load(Ordering::Relaxed),
            chunk_rows: self.chunk_rows.load(Ordering::Relaxed),
            entity_mappings: self.entity_mappings.load(Ordering::Relaxed),
            staged_records_max: self.staged_records_max.load(Ordering::Relaxed),
        }
    }
}

/// Vector points and their derived metadata waiting for one commit boundary.
///
/// Points, chunk records, entity mappings and work-unit hashes advance
/// together: only [`StorageCoordinator::commit_staged_vectors`] publishes them.
pub(crate) struct StagedVectorWrite {
    writer: VectorBatchWriter,
    chunk_records: Vec<ChunkRecord>,
    entity_mappings: Vec<EntityDetailMapping>,
    hashes: Vec<String>,
}

impl StagedVectorWrite {
    pub fn new(store: VectorStore) -> Self {
        Self {
            writer: VectorBatchWriter::new(store),
            chunk_records: Vec::new(),
            entity_mappings: Vec::new(),
            hashes: Vec::new(),
        }
    }

    pub fn pending_points(&self) -> usize {
        self.writer.pending_len()
    }

    pub fn is_full(&self) -> bool {
        self.writer.is_full()
    }

    pub fn is_empty(&self) -> bool {
        self.pending_points() == 0
            && self.chunk_records.is_empty()
            && self.entity_mappings.is_empty()
            && self.hashes.is_empty()
    }

    pub fn push(
        &mut self,
        points: Vec<VectorPoint>,
        chunk_records: Vec<ChunkRecord>,
        entity_mappings: Vec<EntityDetailMapping>,
        hash: Option<String>,
        counters: &VectorStageCounters,
    ) {
        let chunk_rows = chunk_records.len();
        let mapping_rows = entity_mappings.len();
        self.writer.extend_pending(points);
        self.chunk_records.extend(chunk_records);
        for mapping in entity_mappings {
            merge_mapping(&mut self.entity_mappings, mapping);
        }
        if let Some(hash) = hash {
            self.hashes.push(hash);
        }
        counters.add_staged_rows(chunk_rows, mapping_rows);
        counters.record_staged_window(self.chunk_records.len(), self.entity_mappings.len());
    }

    async fn flush_points(&mut self) -> Result<(), StorageError> {
        self.writer.flush().await
    }

    fn take_hashes(&mut self) -> Vec<String> {
        std::mem::take(&mut self.hashes)
    }
}

/// Merge mapping rows for the same entity so one window writes one row per
/// entity even when several microbatches reference it.
fn merge_mapping(target: &mut Vec<EntityDetailMapping>, mapping: EntityDetailMapping) {
    let Some(existing) = target
        .iter_mut()
        .find(|entry| entry.entity_id == mapping.entity_id)
    else {
        target.push(mapping);
        return;
    };
    let mut point_ids = existing.get_qdrant_point_ids();
    for point_id in mapping.get_qdrant_point_ids() {
        if !point_ids.contains(&point_id) {
            point_ids.push(point_id);
        }
    }
    let doc_ids = existing.get_bm25_doc_ids();
    let mut merged = existing.clone().with_qdrant_point_ids(&point_ids);
    if !doc_ids.is_empty() {
        merged = merged.with_bm25_doc_ids(&doc_ids);
    }
    *existing = merged;
}

impl StorageCoordinator {
    /// Store vectors from chunked results with batch processing
    ///
    /// This method processes chunks in batches to:
    /// 1. Control memory usage during embedding
    /// 2. Avoid API rate limits by adding delays between batches
    ///
    /// When an embedding stage deadline is configured, the whole pass is
    /// bounded by it: a wedged provider that keeps tripping retries can no
    /// longer hang the index run indefinitely. The deadline surfaces as a
    /// batch-level error so the checkpoint boundary does not advance and a
    /// resume replays the uncommitted work units.
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
        let mut staged = self.vector.clone().map(StagedVectorWrite::new);
        let stored = self
            .stage_vectors_batched(chunks, batch_size, batch_delay_ms, staged.as_mut(), None)
            .await?;
        if let Some(staged) = staged.as_mut() {
            self.commit_staged_vectors(staged).await?;
        }
        Ok(stored)
    }

    /// Generate and buffer vectors without publishing a checkpoint boundary.
    ///
    /// The full-index loop shares one writer across orchestration batches so
    /// the embedded backend performs one WAL commit per aggregation window.
    pub(crate) async fn stage_vectors_batched(
        &self,
        chunks: &[ChunkedResult],
        batch_size: usize,
        batch_delay_ms: u64,
        staged: Option<&mut StagedVectorWrite>,
        source_entity_ids: Option<&std::collections::HashMap<(String, i64), i64>>,
    ) -> Result<usize, OrchestratorError> {
        let embedding_count = chunks
            .iter()
            .filter(|c| c.path == ChunkPath::Embedding)
            .count();
        if embedding_count == 0 {
            return self
                .stage_vectors_batched_impl(
                    chunks,
                    batch_size,
                    batch_delay_ms,
                    staged,
                    source_entity_ids,
                )
                .await;
        }
        // 0 means auto-derive: microbatch count x per-microbatch budget. A
        // configured value overrides it. Either way the pass is bounded so a
        // wedged provider cannot hang the run indefinitely.
        let num_microbatches = embedding_count.div_ceil(batch_size.max(1)) as u64;
        let timeout_secs = if self.embedding_stage_timeout_secs > 0 {
            self.embedding_stage_timeout_secs
        } else {
            num_microbatches.saturating_mul(EMBEDDING_MICROBATCH_BUDGET_SECS)
        };
        let deadline = std::time::Duration::from_secs(timeout_secs);
        match tokio::time::timeout(
            deadline,
            self.stage_vectors_batched_impl(
                chunks,
                batch_size,
                batch_delay_ms,
                staged,
                source_entity_ids,
            ),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(OrchestratorError::index(
                EMBEDDING_STAGE_TIMEOUT_CODE,
                format!(
                    "embedding stage exceeded its {}s deadline; work units left uncommitted for resume",
                    timeout_secs
                ),
            )),
        }
    }

    /// Publish one vector aggregation window.
    ///
    /// Points are flushed first, then their chunk/entity metadata is written
    /// in one transaction, and only then are the window's work units marked
    /// committed. A crash before any of those steps leaves the work units
    /// uncommitted so replay overwrites the same deterministic point IDs.
    pub(crate) async fn commit_staged_vectors(
        &self,
        staged: &mut StagedVectorWrite,
    ) -> Result<(), OrchestratorError> {
        if self.injected_failure(WriteFailurePoint::VectorBeforeFlush) {
            return Err(OrchestratorError::index(
                "test_write_failure",
                "vector flush failed before publishing points",
            ));
        }
        let flush_started = Instant::now();
        staged.flush_points().await?;
        self.vector_stage.add_flush(flush_started.elapsed());

        if !staged.chunk_records.is_empty() || !staged.entity_mappings.is_empty() {
            let started = Instant::now();
            self.store_chunks_and_mappings(&staged.chunk_records, &staged.entity_mappings)
                .await?;
            self.vector_stage.add_metadata(started.elapsed());
            staged.chunk_records.clear();
            staged.entity_mappings.clear();
        }

        if self.injected_failure(WriteFailurePoint::VectorBeforeWorkUnits) {
            return Err(OrchestratorError::index(
                "test_write_failure",
                "vector work units failed after points were published",
            ));
        }
        let started = Instant::now();
        let hashes = staged.take_hashes();
        if let (Some(cm), Some(op_id)) =
            (self.checkpoint_manager.as_ref(), self.operation_id.as_ref())
            && !hashes.is_empty()
            && let Err(e) = cm
                .update_work_unit_statuses(
                    op_id,
                    "embedding_generation",
                    &hashes,
                    WorkUnitStatus::Committed,
                )
                .await
        {
            tracing::warn!(error = %e, "Failed to update embedding work unit checkpoints to committed");
        }
        self.vector_stage.add_work_unit(started.elapsed());
        Ok(())
    }

    async fn stage_vectors_batched_impl(
        &self,
        chunks: &[ChunkedResult],
        batch_size: usize,
        batch_delay_ms: u64,
        staged: Option<&mut StagedVectorWrite>,
        source_entity_ids: Option<&std::collections::HashMap<(String, i64), i64>>,
    ) -> Result<usize, OrchestratorError> {
        // Filter to only Embedding-path chunks first: empty work succeeds
        // regardless of backend configuration. Runs without an embedder or
        // vector backend (local indexing, tests, CLI debug runs) skip the
        // vector stage entirely instead of failing the run; production
        // assemblies always wire both backends.
        let embedding_chunks: Vec<&ChunkedResult> = chunks
            .iter()
            .filter(|c| c.path == ChunkPath::Embedding)
            .collect();

        if embedding_chunks.is_empty() {
            return Ok(0);
        }

        let embedder = match &self.embedder {
            Some(e) => e,
            None => {
                // Runs without an embedder (tests, CLI debug runs) still
                // persist chunk rows and entity mappings so hot-update
                // identity and incremental flows keep working; only the
                // vector upsert is skipped. Production always wires both.
                tracing::warn!(
                    chunks = embedding_chunks.len(),
                    "Embedder is not configured; persisting chunk metadata without vectors"
                );
                return self.persist_chunks_without_vectors(&embedding_chunks).await;
            }
        };

        if self.vector.is_none() {
            tracing::warn!(
                chunks = embedding_chunks.len(),
                "Vector store is not configured; persisting chunk metadata without vectors"
            );
            return self.persist_chunks_without_vectors(&embedding_chunks).await;
        }
        self.ensure_project_group_id()?;

        // Points and their derived records accumulate in the caller-owned
        // staging window across microbatches and orchestration batches.
        let staged = staged.ok_or_else(|| {
            OrchestratorError::index(
                "vector_writer",
                "vector staging window is required when a vector backend is configured",
            )
        })?;

        let mut total_stored = 0;

        let wk_cm = self.checkpoint_manager.clone();
        let wk_op = self.operation_id.clone();

        // Batches rejected by a rate limit (429) or a transient server error
        // (5xx) are retried once after the whole pass: retries do not add
        // load while other batches are still embedding, and the inter-batch
        // delay acts as natural backoff.
        type DeferredBatch<'a> = (&'a [&'a ChunkedResult], Option<String>);
        let mut deferred_retries: Vec<DeferredBatch<'_>> = Vec::new();
        let mut deferred_retry_after_ms: u64 = 0;
        let mut deferred_reason: Option<String> = None;
        let mut deferred_failures: usize = 0;

        // Checkpoint pre-pass: skip already-committed microbatches and stage
        // the pending Running records for a single batched insert instead of
        // one transaction per microbatch. A missing record replays exactly
        // like a Running one, so the deferred insert never skips work.
        let work_unit_started = Instant::now();
        let mut pending_batches: Vec<(&[&ChunkedResult], Option<String>)> = Vec::new();
        if let (Some(cm), Some(op_id)) = (wk_cm.clone(), wk_op.clone()) {
            let query_started = Instant::now();
            let committed: std::collections::HashMap<String, WorkUnitStatus> = match cm
                .get_work_units(&op_id, "embedding_generation")
                .await
            {
                Ok(records) => records
                    .into_iter()
                    .map(|record| (record.work_unit_hash, record.status))
                    .collect(),
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to read embedding work unit checkpoints; replaying all microbatches");
                    std::collections::HashMap::new()
                }
            };
            self.vector_stage
                .record_work_unit_query(query_started.elapsed());
            let mut running = Vec::new();
            for batch in embedding_chunks.chunks(batch_size) {
                let hash = compute_work_unit_hash(batch);
                if committed.get(&hash) == Some(&WorkUnitStatus::Committed) {
                    total_stored += batch.len();
                } else {
                    running.push(WorkUnitCheckpointRecord {
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
                    });
                    pending_batches.push((batch, Some(hash)));
                }
            }
            if let Err(e) = cm.insert_work_units(&running).await {
                tracing::warn!(
                    error = %e,
                    "Failed to insert embedding work unit checkpoints"
                );
            }
        } else {
            for batch in embedding_chunks.chunks(batch_size) {
                pending_batches.push((batch, None));
            }
        }
        self.vector_stage.add_work_unit(work_unit_started.elapsed());
        let pending_count = pending_batches.len();
        // Work-unit hashes live inside the staging window and are marked
        // Committed only after its points and metadata have been published.

        // The full-index loop owns an incremental map; a single-call path
        // (hot update, dead-letter retry) loads the epoch's rows on demand.
        let loaded_ids;
        let source_entity_ids = match source_entity_ids {
            Some(ids) => ids,
            None => {
                let lookup_started = Instant::now();
                loaded_ids = self.load_source_entity_ids().await?;
                self.vector_stage
                    .add_entity_lookup(lookup_started.elapsed());
                &loaded_ids
            }
        };

        for (batch_idx, (batch, hash)) in pending_batches.iter().enumerate() {
            // Prepare texts for this batch
            let texts: Vec<&str> = batch.iter().map(|c| c.text.as_str()).collect();

            if texts.is_empty() {
                tracing::warn!("No embedding text found in batch {}", batch_idx);
                continue;
            }

            // Generate embeddings for this batch; rate-limited or transiently
            // failed (5xx) batches are deferred to the retry pass at the end
            // of the loop.
            let embed_started = Instant::now();
            let embed_result = embedder.embed(&texts).await;
            self.vector_stage.add_embed(embed_started.elapsed());
            self.vector_stage.add_microbatch();
            let embeddings = match embed_result {
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
                    deferred_retries.push((*batch, hash.clone()));
                    continue;
                }
                Err(e) => return Err(e.into()),
            };

            total_stored += self
                .persist_embedding_batch(
                    batch,
                    &embeddings,
                    staged,
                    source_entity_ids,
                    hash.clone(),
                )
                .await?;

            // Sleep between batches to avoid rate limits (skip last batch)
            if batch_delay_ms > 0 && batch_idx + 1 < pending_count {
                let delay_started = Instant::now();
                tokio::time::sleep(tokio::time::Duration::from_millis(batch_delay_ms)).await;
                self.vector_stage.add_delay(delay_started.elapsed());
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
                let delay_started = Instant::now();
                tokio::time::sleep(tokio::time::Duration::from_millis(backoff_ms)).await;
                self.vector_stage.add_delay(delay_started.elapsed());
            }
            for (batch, hash) in deferred_retries {
                let texts: Vec<&str> = batch.iter().map(|c| c.text.as_str()).collect();
                let embed_started = Instant::now();
                let embed_result = embedder.embed(&texts).await;
                self.vector_stage.add_embed(embed_started.elapsed());
                self.vector_stage.add_microbatch();
                match embed_result {
                    Ok(embeddings) => {
                        total_stored += self
                            .persist_embedding_batch(
                                batch,
                                &embeddings,
                                staged,
                                source_entity_ids,
                                hash,
                            )
                            .await?;
                    }
                    Err(err) if is_retryable_llm_error(&err) => {
                        // Record the failure: the work unit stays Running
                        // (uncommitted) so a resume replays exactly this unit,
                        // but the caller must not treat the pass as complete.
                        let work_unit_hash = hash.as_deref().unwrap_or("<none>");
                        tracing::warn!(
                            work_unit_hash,
                            error = %err,
                            "Embedding batch still failing after retry; work unit left uncommitted"
                        );
                        deferred_failures += 1;
                    }
                    Err(e) => return Err(e.into()),
                }
            }
        }

        // A deferred batch that is still failing means vectors were not
        // produced for this pass. Reporting Ok here would let the batch loop
        // advance the checkpoint boundary over silently-missing vectors, so
        // the failure must surface as a batch-level error.
        if deferred_failures > 0 {
            if let Some(metrics) = &self.quality_metrics {
                metrics.record_deferred_uncommitted(deferred_failures);
            }
            return Err(OrchestratorError::index(
                "embedding_deferred_retry",
                format!(
                    "{deferred_failures} embedding batch(es) still failing after the deferred retry; work units left uncommitted for resume"
                ),
            ));
        }

        Ok(total_stored)
    }

    /// Persist already-generated embeddings for one batch: builds vector
    /// points and SQLite records and stores them. The caller batches the
    /// work unit checkpoint writes; a batch is marked Committed only after
    /// this returns Ok.
    ///
    /// The vector write and the SQLite write are ordered but not atomic: when
    /// a later write fails, earlier points may already exist without SQLite
    /// links. Such orphans are invisible to entity-linked queries and converge
    /// on resume: point IDs are deterministic, so replaying the uncommitted
    /// work unit overwrites them and completes the missing links instead
    /// of duplicating data.
    async fn persist_embedding_batch(
        &self,
        batch: &[&ChunkedResult],
        embeddings: &cce_llm::EmbeddingResult,
        staged: &mut StagedVectorWrite,
        source_entity_ids: &std::collections::HashMap<(String, i64), i64>,
        work_unit_hash: Option<String>,
    ) -> Result<usize, OrchestratorError> {
        let build_started = Instant::now();
        let (points, chunk_records, entity_mappings) = self
            .build_storage_data(batch, &embeddings.embeddings, source_entity_ids)
            .await?;
        self.vector_stage.add_point_build(build_started.elapsed());

        let stored = points.len();
        let push_started = Instant::now();
        staged.push(
            points,
            chunk_records,
            entity_mappings,
            work_unit_hash,
            &self.vector_stage,
        );
        self.vector_stage.add_stage_push(push_started.elapsed());
        if stored > 0 && staged.is_full() {
            self.commit_staged_vectors(staged).await?;
        }
        Ok(stored)
    }

    /// Metadata-only fallback when no embedder or vector backend is wired:
    /// chunk rows and entity mappings are persisted with zero-vector
    /// placeholders so chunk identity (content, spans, source entity links)
    /// stays durable; the vector upsert step is skipped entirely.
    pub(crate) async fn persist_chunks_without_vectors(
        &self,
        chunks: &[&ChunkedResult],
    ) -> Result<usize, OrchestratorError> {
        if chunks.is_empty() {
            return Ok(0);
        }
        let source_entity_ids = self.load_source_entity_ids().await?;
        let embeddings = cce_llm::EmbeddingResult {
            embeddings: vec![Vec::new(); chunks.len()],
            prompt_tokens: 0,
            total_tokens: 0,
        };
        let (points, chunk_records, entity_mappings) = self
            .build_storage_data(chunks, &embeddings.embeddings, &source_entity_ids)
            .await?;
        // No vector backend: points are dropped, never upserted.
        let _ = points;
        self.store_chunks_and_mappings(&chunk_records, &entity_mappings)
            .await?;
        Ok(chunk_records.len())
    }

    /// Build storage data from chunks and embeddings
    async fn build_storage_data(
        &self,
        chunks: &[&ChunkedResult],
        dense_embeddings: &[Vec<f32>],
        source_entity_ids: &std::collections::HashMap<(String, i64), i64>,
    ) -> Result<(Vec<VectorPoint>, Vec<ChunkRecord>, Vec<EntityDetailMapping>), OrchestratorError>
    {
        // A plugin or custom embedder may return a mismatched vector count;
        // zipping silently would drop the tail chunks from storage.
        if dense_embeddings.len() != chunks.len() {
            return Err(OrchestratorError::index(
                "embedding",
                format!(
                    "embedder returned {} vectors for {} chunks",
                    dense_embeddings.len(),
                    chunks.len()
                ),
            ));
        }
        let mut points = Vec::new();
        let mut chunk_records = Vec::new();
        // Use a map to aggregate multiple chunks per entity
        let mut entity_detail_map: std::collections::HashMap<i64, EntityDetailMapping> =
            std::collections::HashMap::new();
        let group_id = self.project_group_id.clone();
        let epoch = self.epoch();
        let batch_id = self.batch_id();

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
                .with_generated(cce_types::is_generated_path(&chunk.metadata.file_path))
                .with_dependency(cce_types::is_dependency_path(&chunk.metadata.file_path))
                .with_test_source(chunk.metadata.test_info.source)
                .with_truncated(chunk.truncated);

            // Create vector point with project-scoped ID
            let point_id = project_chunk_point_id(&group_id, epoch, &chunk.chunk_id);
            let point = VectorPoint::new(point_id.clone(), dense_vec.clone(), payload);
            points.push(point);

            // Chunk record for SQLite enrichment (raw source code + line numbers)
            let chunk_record = build_chunk_record(chunk, self.project_id, epoch, batch_id)?;

            chunk_records.push(chunk_record);

            // Aggregate entity detail mappings - one mapping per entity with multiple chunks
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
                        // the entity -> vector link.
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

    /// Resolve only the entity ids referenced by the current batch.
    ///
    /// The full-index loop calls this after persisting a batch's entities, so
    /// the map's lifetime and size follow the orchestration batch instead of
    /// the whole project. Cross-file references still resolve because the
    /// lookup is keyed by `(path, source entity id)` in durable storage.
    pub(crate) async fn resolve_source_entity_ids(
        &self,
        chunks: &[ChunkedResult],
    ) -> Result<std::collections::HashMap<(String, i64), i64>, OrchestratorError> {
        let Some(store) = self.metadata_store.as_ref() else {
            return Ok(std::collections::HashMap::new());
        };
        let wanted: std::collections::HashSet<(String, i64)> = chunks
            .iter()
            .flat_map(|chunk| {
                chunk
                    .metadata
                    .content_entity_ids()
                    .iter()
                    .copied()
                    .filter(|id| id.0 != u64::MAX && id.0 != u64::MAX - 1)
                    .map(|id| (chunk.metadata.file_path.clone(), id.0 as i64))
            })
            .collect();
        if wanted.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let mut paths: Vec<String> = wanted.iter().map(|(path, _)| path.clone()).collect();
        paths.sort();
        paths.dedup();
        let started = Instant::now();
        let rows = store
            .entities_with_files_in_paths(self.project_id, self.epoch(), &paths)
            .await
            .map_err(OrchestratorError::Storage)?;
        self.add_vector_entity_lookup(started.elapsed());
        let mut resolved = std::collections::HashMap::with_capacity(wanted.len());
        for entry in rows {
            let Some(source_id) = source_id_from_metadata(entry.entity.metadata.as_deref()) else {
                continue;
            };
            let key = (entry.file_path, source_id);
            if wanted.contains(&key) {
                resolved.insert(key, entry.entity.id);
            }
        }
        Ok(resolved)
    }

    pub(crate) async fn load_source_entity_ids(
        &self,
    ) -> Result<std::collections::HashMap<(String, i64), i64>, OrchestratorError> {
        let mut result = std::collections::HashMap::new();
        let mut max_id = 0;
        self.extend_source_entity_ids(&mut result, &mut max_id)
            .await?;
        Ok(result)
    }

    /// Extend an existing source-id map with entity rows inserted since the
    /// last call. The row-id cursor keeps a long full-index run from
    /// rescanning all entities after every orchestration batch.
    pub(crate) async fn extend_source_entity_ids(
        &self,
        result: &mut std::collections::HashMap<(String, i64), i64>,
        max_id: &mut i64,
    ) -> Result<(), OrchestratorError> {
        let Some(store) = self.metadata_store.as_ref() else {
            return Ok(());
        };
        let rows = store
            .entities_with_files_after(self.project_id, self.epoch(), *max_id)
            .await
            .map_err(OrchestratorError::Storage)?;
        let mut malformed = 0usize;
        for entry in rows {
            *max_id = (*max_id).max(entry.entity.id);
            let (path, metadata, db_id) = (entry.file_path, entry.entity.metadata, entry.entity.id);
            // Every stored entity carries `__source_entity_id` in its
            // metadata; a row without a parseable one is malformed and its
            // entity -> vector link will miss in the mapping lookup.
            let source_id = source_id_from_metadata(metadata.as_deref());
            match source_id {
                Some(source_id) => {
                    result.insert((path, source_id), db_id);
                }
                None => malformed += 1,
            }
        }
        if malformed > 0 {
            if let Some(metrics) = &self.quality_metrics {
                metrics.record_entity_metadata_malformed(malformed);
            }
            tracing::warn!(
                count = malformed,
                project_id = self.project_id,
                epoch = self.epoch(),
                "Skipped entity rows with malformed metadata while loading source entity IDs"
            );
        }
        Ok(())
    }

    /// Store chunk records in the relation store
    pub(crate) async fn store_chunk_records(
        &self,
        chunks: &[ChunkRecord],
    ) -> Result<(), OrchestratorError> {
        if let Some(store) = self.metadata_store.as_ref() {
            store
                .chunks_upsert(chunks)
                .await
                .map(|_| ())
                .map_err(OrchestratorError::Storage)?;
        }
        Ok(())
    }

    /// Store chunk records and entity mappings in a single transaction.
    ///
    /// The embedding and BM25 persist paths write both tables per
    /// microbatch; sharing one transaction halves the per-microbatch
    /// transaction count.
    pub(crate) async fn store_chunks_and_mappings(
        &self,
        chunks: &[ChunkRecord],
        mappings: &[EntityDetailMapping],
    ) -> Result<(), OrchestratorError> {
        if let Some(store) = self.metadata_store.as_ref() {
            store
                .chunks_and_mappings_upsert(chunks, mappings)
                .await
                .map(|_| ())
                .map_err(OrchestratorError::Storage)?;
        }
        Ok(())
    }

    /// Store only embedding-path chunk records to the relation store without
    /// requiring an embedder or Qdrant. Used by benchmark data generation.
    pub async fn store_chunk_records_only(
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
        self.store_chunk_records(&chunk_records).await?;
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
        let num_microbatches = records.len().div_ceil(batch_size.max(1)) as u64;
        let timeout_secs = if self.embedding_stage_timeout_secs > 0 {
            self.embedding_stage_timeout_secs
        } else {
            num_microbatches.saturating_mul(EMBEDDING_MICROBATCH_BUDGET_SECS)
        };
        match tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            self.reembed_vectors_from_records_impl(records, batch_size),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(OrchestratorError::index(
                EMBEDDING_STAGE_TIMEOUT_CODE,
                format!("reembed stage exceeded its {timeout_secs}s deadline; retry on resume"),
            )),
        }
    }

    async fn reembed_vectors_from_records_impl(
        &self,
        records: &[(ChunkRecord, u8)],
        batch_size: usize,
    ) -> Result<usize, OrchestratorError> {
        if records.is_empty() {
            return Ok(0);
        }
        let embedder = match &self.embedder {
            Some(e) => e,
            None => {
                return Err(OrchestratorError::index(
                    "reembed_vectors",
                    "embedder is not configured but records are pending",
                ));
            }
        };
        let vector = match &self.vector {
            Some(v) => v,
            None => {
                return Err(OrchestratorError::index(
                    "reembed_vectors",
                    "vector store is not configured but records are pending",
                ));
            }
        };
        self.ensure_project_group_id()?;

        // Batch the refreshed vectors across microbatches the same way the
        // main write path does, then flush before returning.
        let mut vector_writer = VectorBatchWriter::new(vector.clone());

        let mut stored = 0;
        let mut deferred: Vec<&[(ChunkRecord, u8)]> = Vec::new();
        for batch in records.chunks(batch_size.max(1)) {
            let texts: Vec<&str> = batch.iter().map(|(r, _)| r.content.as_str()).collect();
            let embeddings = match embedder.embed(&texts).await {
                Ok(embeddings) => embeddings,
                Err(error) if is_retryable_llm_error(&error) => {
                    tracing::warn!(
                        error = %error,
                        chunks = batch.len(),
                        "Reembed batch failed transiently; deferring one retry to the end"
                    );
                    deferred.push(batch);
                    continue;
                }
                Err(error) => return Err(error.into()),
            };
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
            vector_writer.push(points).await?;
        }
        for batch in deferred {
            let texts: Vec<&str> = batch.iter().map(|(r, _)| r.content.as_str()).collect();
            let embeddings = embedder.embed(&texts).await?;
            if embeddings.embeddings.len() != batch.len() {
                return Err(OrchestratorError::index(
                    "reembed_vectors",
                    format!(
                        "embedder returned {} vectors for {} chunks on deferred retry",
                        embeddings.embeddings.len(),
                        batch.len()
                    ),
                ));
            }
            let vectors: Vec<Vec<f32>> = embeddings.embeddings.clone();
            let points =
                build_reembed_points(batch.iter().zip(vectors.iter()), &self.project_group_id);
            stored += points.len();
            vector_writer.push(points).await?;
        }
        vector_writer.flush().await?;
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
                .with_generated(cce_types::is_generated_path(&record.file_path))
                .with_dependency(cce_types::is_dependency_path(&record.file_path))
                .with_test_source(TestSource::from_u8(record.test_source))
                .with_truncated(record.truncated != 0);

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
    use std::sync::Arc;

    use super::super::StorageCoordinator;
    use super::{StagedVectorWrite, VectorStageCounters};
    use crate::index::vector_store::VectorStore;
    use cce_config::modules::LocalVectorConfig;
    use cce_parser::ast_to_nl::chunker::{
        ChunkMetadata, ChunkPath, ChunkedResult, CodeSpecificMetadata,
    };
    use cce_storage_common::metadb::{
        ChunkRecord, EntityDetailMapping, EntityRecord, WorkUnitCheckpointRecord, WorkUnitStatus,
    };
    use cce_storage_common::{MetadataStorage, Payload, VectorPoint};
    use cce_storage_vector_local::LocalVectorStore;
    use cce_types::OperationKind;
    use cce_types::ast_to_nl::FileCategory;
    use cce_types::entity::{EntityId, EntityKind};
    use cce_types::{Language, Span};

    use super::super::{WriteFailureInjector, WriteFailurePoint};
    use crate::CheckpointManager;
    use crate::operation::checkpoint::CreateCheckpointParams;

    #[tokio::test]
    async fn build_storage_data_rejects_vector_count_mismatch() {
        let chunk = ChunkedResult::new(
            "chunk-a".to_string(),
            "source-a".to_string(),
            ChunkPath::Embedding,
            0,
            1,
        );
        let storage = StorageCoordinator::new(7)
            .expect("valid project ID")
            .with_project_group_id("project-7-root");
        let chunks = [&chunk];
        let embeddings: [Vec<f32>; 0] = [];

        let err = storage
            .build_storage_data(&chunks, &embeddings, &std::collections::HashMap::new())
            .await
            .expect_err("mismatched vector count must fail instead of truncating");
        assert!(
            err.to_string()
                .contains("embedder returned 0 vectors for 1 chunks"),
            "unexpected error: {err}"
        );
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
            .build_storage_data(&chunks, &embeddings, &std::collections::HashMap::new())
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
            .build_storage_data(&chunks, &embeddings, &std::collections::HashMap::new())
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
            .build_storage_data(&chunks, &embeddings, &std::collections::HashMap::new())
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
            .build_storage_data(&chunks, &embeddings, &std::collections::HashMap::new())
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
    async fn reembed_vectors_without_embedder_reports_error() {
        let storage = StorageCoordinator::new(7)
            .expect("valid project ID")
            .with_project_group_id("project-7-root");
        let records = [stored_record("a")];
        let error = storage
            .reembed_vectors_from_records(&records, 8)
            .await
            .expect_err("pending records without an embedder must surface an error");
        assert!(
            error.to_string().contains("embedder is not configured"),
            "unexpected error: {error}"
        );
    }

    fn staging_store() -> (tempfile::TempDir, Arc<LocalVectorStore>) {
        let dir = tempfile::TempDir::new().expect("vector temp dir");
        let config = LocalVectorConfig {
            vector_size: 2,
            ..Default::default()
        };
        let store = Arc::new(
            LocalVectorStore::open_at(dir.path(), &config).expect("open local vector store"),
        );
        (dir, store)
    }

    fn staging_point(id: &str) -> VectorPoint {
        VectorPoint::new(
            id.to_string(),
            vec![0.1, 0.2],
            Payload::new("src/lib.rs".to_string()),
        )
    }

    #[test]
    fn staged_write_merges_mappings_and_tracks_capacity() {
        let (_dir, store) = staging_store();
        let mut staged = StagedVectorWrite::new(VectorStore::local(store));
        let counters = VectorStageCounters::default();
        let (chunk_a, _) = stored_record("chunk-a");
        let (chunk_b, _) = stored_record("chunk-b");
        let mapping_a = EntityDetailMapping::new(42).with_qdrant_point_ids(&["a".to_string()]);
        let mapping_b = EntityDetailMapping::new(42).with_qdrant_point_ids(&["b".to_string()]);

        staged.push(
            vec![staging_point("a")],
            vec![chunk_a],
            vec![mapping_a],
            Some("hash-a".to_string()),
            &counters,
        );
        staged.push(
            vec![staging_point("b")],
            vec![chunk_b],
            vec![mapping_b],
            Some("hash-b".to_string()),
            &counters,
        );

        assert_eq!(staged.pending_points(), 2, "both points stay buffered");
        assert_eq!(staged.entity_mappings.len(), 1, "one row per entity");
        assert_eq!(
            staged.entity_mappings[0].get_qdrant_point_ids(),
            vec!["a".to_string(), "b".to_string()],
            "point IDs from both microbatches are merged in order"
        );
        assert_eq!(staged.take_hashes(), vec!["hash-a", "hash-b"]);
        let snapshot = counters.snapshot();
        assert_eq!(snapshot.chunk_rows, 2);
        assert_eq!(snapshot.entity_mappings, 2);
        assert_eq!(snapshot.staged_records_max, 3, "two chunks and one mapping");
    }

    #[tokio::test]
    async fn staged_write_commits_points_and_metadata_at_threshold() {
        let (_dir, vector_store) = staging_store();
        let metadata = crate::index::in_memory_metadata_store().expect("metadata store");
        metadata
            .ensure_project(7, "/root")
            .await
            .expect("register project before metadata writes");
        metadata
            .files_record_hashes(7, 3, &[("src/lib.rs".to_string(), "hash".to_string())])
            .await
            .expect("seed file row for mapping foreign key");
        let file = metadata
            .file_by_path(7, "src/lib.rs")
            .await
            .expect("read seeded file")
            .expect("seeded file exists");
        let entity = EntityRecord {
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
        };
        let entity_ids = metadata
            .entities_insert_returning_ids(&[entity])
            .await
            .expect("seed entity row for mapping foreign key");
        let entity_id = entity_ids.first().copied().expect("entity id returned");
        let storage = StorageCoordinator::new(7)
            .expect("valid project ID")
            .with_local(vector_store)
            .with_metadata_store(metadata.clone());
        let mut staged = StagedVectorWrite::new(
            storage
                .vector()
                .cloned()
                .expect("configured vector backend"),
        );
        let counters = VectorStageCounters::default();
        let (chunk, _) = stored_record("chunk-window");
        let mapping = EntityDetailMapping::new(entity_id)
            .with_project_id(7)
            .with_epoch(3)
            .with_qdrant_point_ids(&["point-window".to_string()]);
        let points: Vec<_> = (0..512)
            .map(|index| staging_point(&format!("point-{index}")))
            .collect();

        staged.push(points, vec![chunk], vec![mapping], None, &counters);
        assert!(staged.is_full(), "512 points fill one aggregation window");
        storage
            .commit_staged_vectors(&mut staged)
            .await
            .expect("commit staged vector window");

        assert!(staged.is_empty(), "commit clears the whole window");
        let stored = metadata
            .mappings_by_entity(7, entity_id, 3)
            .await
            .expect("read committed mapping")
            .expect("mapping row exists after commit");
        assert_eq!(
            stored.get_qdrant_point_ids(),
            vec!["point-window".to_string()]
        );
        assert!(
            storage.vector_stage_metrics().metadata_nanos > 0,
            "metadata commit is attributed to the vector stage"
        );
    }

    fn failure_work_unit(op: &str, hash: &str) -> WorkUnitCheckpointRecord {
        WorkUnitCheckpointRecord {
            id: None,
            project_id: 7,
            operation_id: op.to_string(),
            stage: "embedding_generation".to_string(),
            target_epoch: 3,
            work_unit_hash: hash.to_string(),
            status: WorkUnitStatus::Running,
            item_count: 1,
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    async fn failure_fixture(
        vector_store: Arc<LocalVectorStore>,
        metadata: crate::index::MetadataStore,
        injector: Arc<WriteFailureInjector>,
        op: &str,
    ) -> (StorageCoordinator, CheckpointManager) {
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
        .expect("create checkpoint for failure fixture");
        cm.insert_work_units(&[failure_work_unit(op, "hash-fail")])
            .await
            .expect("insert running work unit");
        let test_cm = CheckpointManager::new_for_project(7, metadata.clone());
        let storage = StorageCoordinator::new(7)
            .expect("valid project ID")
            .with_project_group_id("project-7-root")
            .with_local(vector_store)
            .with_metadata_store(metadata)
            .with_checkpoint_manager(Arc::new(cm), op.to_string())
            .with_test_write_failure(injector);
        (storage, test_cm)
    }

    #[tokio::test]
    async fn vector_failure_before_flush_keeps_work_unit_running() {
        let (_dir, vector_store) = staging_store();
        let metadata = crate::index::in_memory_metadata_store().expect("metadata store");
        metadata
            .ensure_project(7, "/root")
            .await
            .expect("register project");
        let injector = Arc::new(WriteFailureInjector::default());
        injector.arm(WriteFailurePoint::VectorBeforeFlush, 1);
        let (storage, cm) = failure_fixture(
            vector_store.clone(),
            metadata,
            injector.clone(),
            "vector-pre-flush",
        )
        .await;
        let mut staged = StagedVectorWrite::new(
            storage
                .vector()
                .cloned()
                .expect("configured vector backend"),
        );
        let counters = VectorStageCounters::default();
        staged.push(
            vec![staging_point("point-before-flush")],
            Vec::new(),
            Vec::new(),
            Some("hash-fail".to_string()),
            &counters,
        );

        storage
            .commit_staged_vectors(&mut staged)
            .await
            .expect_err("flush boundary failure must propagate");
        let unit = cm
            .get_work_unit_by_hash("vector-pre-flush", "embedding_generation", "hash-fail")
            .await
            .expect("read work unit")
            .expect("work unit exists");
        assert_eq!(
            unit.status,
            WorkUnitStatus::Running,
            "a failure before flush must not advance the checkpoint"
        );

        injector.arm(WriteFailurePoint::VectorBeforeFlush, 0);
        storage
            .commit_staged_vectors(&mut staged)
            .await
            .expect("retry publishes the buffered window");
        let unit = cm
            .get_work_unit_by_hash("vector-pre-flush", "embedding_generation", "hash-fail")
            .await
            .expect("read work unit")
            .expect("work unit exists");
        assert_eq!(unit.status, WorkUnitStatus::Committed, "retry commits");
    }

    #[tokio::test]
    async fn vector_failure_before_work_units_keeps_derived_data_unmarked() {
        let (_dir, vector_store) = staging_store();
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
        let injector = Arc::new(WriteFailureInjector::default());
        injector.arm(WriteFailurePoint::VectorBeforeWorkUnits, 1);
        let (storage, cm) = failure_fixture(
            vector_store.clone(),
            metadata.clone(),
            injector.clone(),
            "vector-post-flush",
        )
        .await;
        let mut staged = StagedVectorWrite::new(
            storage
                .vector()
                .cloned()
                .expect("configured vector backend"),
        );
        let counters = VectorStageCounters::default();
        let (chunk, _) = stored_record("chunk-post-flush");
        let mapping = EntityDetailMapping::new(entity_id)
            .with_project_id(7)
            .with_epoch(4)
            .with_qdrant_point_ids(&["point-post-flush".to_string()]);
        staged.push(
            vec![staging_point("point-post-flush")],
            vec![chunk],
            vec![mapping],
            Some("hash-fail".to_string()),
            &counters,
        );

        storage
            .commit_staged_vectors(&mut staged)
            .await
            .expect_err("post-flush boundary failure must propagate");
        let unit = cm
            .get_work_unit_by_hash("vector-post-flush", "embedding_generation", "hash-fail")
            .await
            .expect("read work unit")
            .expect("work unit exists");
        assert_eq!(unit.status, WorkUnitStatus::Running);
        assert!(
            metadata
                .mappings_by_entity(7, entity_id, 4)
                .await
                .expect("read mapping")
                .is_some(),
            "derived metadata is durable before the work-unit marker"
        );

        injector.arm(WriteFailurePoint::VectorBeforeWorkUnits, 0);
        storage
            .commit_staged_vectors(&mut staged)
            .await
            .expect("retry publishes derived metadata");
        let unit = cm
            .get_work_unit_by_hash("vector-post-flush", "embedding_generation", "hash-fail")
            .await
            .expect("read work unit")
            .expect("work unit exists");
        assert_eq!(unit.status, WorkUnitStatus::Committed);
        assert!(
            metadata
                .mappings_by_entity(7, entity_id, 4)
                .await
                .expect("read mapping")
                .is_some(),
            "retry must restore the mapping"
        );
    }
}
