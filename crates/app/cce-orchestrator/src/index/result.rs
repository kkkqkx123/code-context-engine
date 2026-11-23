//! Indexing result types
//!
//! This module provides a summary of indexing operations for reporting purposes.

use cce_storage_common::{FulltextWriteMetrics, VectorWriteMetrics};

/// Execution outcome for a full index operation.
///
/// Replaces the implicit "errors list means partial failure" convention
/// with an explicit enum that callers must match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexExecutionOutcome {
    /// All required stages completed successfully.
    Success,
    /// One or more required stages failed; the result is partial.
    /// Contains non-blocking warnings only.
    Incomplete { errors: Vec<String> },
}

/// Cumulative vector-stage work recorded inside the storage coordinator.
///
/// Durations are converted to milliseconds when folded into
/// [`IndexStageTimings`]; the counters stay in nanoseconds until that point.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct VectorStageMetrics {
    pub embed_nanos: u64,
    pub point_build_nanos: u64,
    pub metadata_nanos: u64,
    pub work_unit_nanos: u64,
    pub flush_nanos: u64,
    pub stage_push_nanos: u64,
    pub entity_lookup_nanos: u64,
    pub delay_nanos: u64,
    pub work_unit_query_nanos: u64,
    pub work_unit_queries: u64,
    pub microbatches: u64,
    pub chunk_rows: u64,
    pub entity_mappings: u64,
    pub staged_records_max: u64,
}

/// Wall-clock accounting for one full-index run.
///
/// Per-batch stages accumulate across the run. `batch_overhead_ms` is the
/// remainder of the batch loop after all explicitly timed stages, covering
/// state tracking, checkpoints, and bookkeeping.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IndexStageTimings {
    pub scan_ms: u64,
    pub batch_process_ms: u64,
    pub batch_setup_ms: u64,
    pub entity_store_ms: u64,
    pub vector_store_ms: u64,
    pub bm25_store_ms: u64,
    pub summary_ms: u64,
    pub relation_ms: u64,
    pub export_ms: u64,
    pub relation_spool_ms: u64,
    pub relation_spool_write_ms: u64,
    pub relation_symbols_ms: u64,
    pub symbol_modules_ms: u64,
    pub symbol_type_index_ms: u64,
    pub symbol_overload_ms: u64,
    pub symbol_flat_ms: u64,
    pub symbol_cross_file_ms: u64,
    pub export_spool_ms: u64,
    pub manifest_ms: u64,
    pub batch_overhead_ms: u64,
    pub state_projection_ms: u64,
    pub checkpoint_ms: u64,
    pub progress_ms: u64,
    pub batch_dispatch_ms: u64,
    pub write_commit_ms: u64,
    pub batches: usize,
    pub vector_store_calls: usize,
    pub entity_store_calls: usize,
    pub bm25_store_calls: usize,
    pub vector_txns_applied: u64,
    pub vector_points_written: u64,
    pub vector_wal_flushes: u64,
    pub vector_wal_txns: u64,
    pub vector_commit_wait_ms: u64,
    pub vector_hnsw_builds: u64,
    pub vector_hnsw_build_ms: u64,
    pub vector_checkpoints: u64,
    pub vector_checkpoint_ms: u64,
    pub bm25_commits: u64,
    pub bm25_documents: u64,
    pub bm25_document_ms: u64,
    pub bm25_commit_ms: u64,
    pub bm25_refresh_ms: u64,
    pub bm25_merge_ms: u64,
    pub bm25_staged_documents: u64,
    pub rss_after_scan_kb: u64,
    pub rss_after_batches_kb: u64,
    pub rss_after_relation_kb: u64,
    pub rss_final_kb: u64,
    pub rss_peak_batch_kb: u64,
    pub vector_pending_points_max: u64,
    pub bm25_pending_documents_max: u64,
    pub vector_embed_ms: u64,
    pub vector_point_build_ms: u64,
    pub vector_metadata_ms: u64,
    pub vector_work_unit_ms: u64,
    pub vector_flush_ms: u64,
    pub vector_stage_push_ms: u64,
    pub vector_entity_lookup_ms: u64,
    pub vector_delay_ms: u64,
    pub vector_work_unit_queries: u64,
    pub vector_work_unit_query_ms: u64,
    pub vector_microbatches: u64,
    pub vector_chunk_rows: u64,
    pub vector_entity_mappings: u64,
    pub vector_staged_records_max: u64,

    pub relation_spool_entries: u64,
    pub relation_spool_bytes: u64,
    pub symbol_table_entries: u64,
    pub symbol_table_estimated_bytes: u64,
    pub relation_nodes: u64,
    pub relation_edges: u64,
    pub relation_estimated_bytes: u64,
    pub relation_snapshot_entries: u64,
    pub relation_snapshot_estimated_bytes: u64,
    pub export_spool_entries: u64,
    pub export_spool_bytes: u64,
    pub entity_id_map_entries: u64,
    pub entity_id_map_estimated_bytes: u64,
}

impl IndexStageTimings {
    /// Fold a before/after snapshot of the embedded vector engine into the
    /// stage record. Remote backends have no counters and stay at zero.
    pub fn record_vector_write_delta(
        &mut self,
        before: Option<VectorWriteMetrics>,
        after: Option<VectorWriteMetrics>,
    ) {
        let (Some(before), Some(after)) = (before, after) else {
            return;
        };
        let delta = |current: u64, previous: u64| current.saturating_sub(previous);
        self.vector_txns_applied = delta(after.txns_applied, before.txns_applied);
        self.vector_points_written = delta(after.points_upserted, before.points_upserted);
        self.vector_wal_flushes = delta(after.wal_flushes, before.wal_flushes);
        self.vector_wal_txns = delta(after.wal_txns_flushed, before.wal_txns_flushed);
        self.vector_commit_wait_ms =
            delta(after.apply_wait_nanos, before.apply_wait_nanos) / 1_000_000;
        self.vector_hnsw_builds = delta(after.hnsw_builds, before.hnsw_builds);
        self.vector_hnsw_build_ms =
            delta(after.hnsw_build_total_nanos, before.hnsw_build_total_nanos) / 1_000_000;
        self.vector_checkpoints = delta(after.checkpoints, before.checkpoints);
        self.vector_checkpoint_ms =
            delta(after.checkpoint_total_nanos, before.checkpoint_total_nanos) / 1_000_000;
    }

    /// Fold a before/after snapshot of the embedded fulltext writer into the
    /// stage record. Remote backends have no counters and stay at zero.
    pub fn record_fulltext_write_delta(
        &mut self,
        before: Option<FulltextWriteMetrics>,
        after: Option<FulltextWriteMetrics>,
    ) {
        let (Some(before), Some(after)) = (before, after) else {
            return;
        };
        let delta = |current: u64, previous: u64| current.saturating_sub(previous);
        self.bm25_commits = delta(after.commits, before.commits);
        self.bm25_documents = delta(after.documents, before.documents);
        self.bm25_document_ms = delta(after.document_nanos, before.document_nanos) / 1_000_000;
        self.bm25_commit_ms = delta(after.commit_nanos, before.commit_nanos) / 1_000_000;
        self.bm25_refresh_ms = delta(after.refresh_nanos, before.refresh_nanos) / 1_000_000;
        self.bm25_merge_ms = delta(after.merge_nanos, before.merge_nanos) / 1_000_000;
        self.bm25_staged_documents = delta(after.staged_documents, before.staged_documents);
    }

    /// Fold the storage coordinator's vector-stage counters into the stage
    /// record. The counters cover one full-index run and are reset at its
    /// start.
    pub fn record_vector_stage_metrics(&mut self, metrics: VectorStageMetrics) {
        let millis = |nanos: u64| nanos / 1_000_000;
        self.vector_embed_ms = millis(metrics.embed_nanos);
        self.vector_point_build_ms = millis(metrics.point_build_nanos);
        self.vector_metadata_ms = millis(metrics.metadata_nanos);
        self.vector_work_unit_ms = millis(metrics.work_unit_nanos);
        self.vector_flush_ms = millis(metrics.flush_nanos);
        self.vector_stage_push_ms = millis(metrics.stage_push_nanos);
        self.vector_entity_lookup_ms = millis(metrics.entity_lookup_nanos);
        self.vector_delay_ms = millis(metrics.delay_nanos);
        self.vector_work_unit_queries = metrics.work_unit_queries;
        self.vector_work_unit_query_ms = millis(metrics.work_unit_query_nanos);
        self.vector_microbatches = metrics.microbatches;
        self.vector_chunk_rows = metrics.chunk_rows;
        self.vector_entity_mappings = metrics.entity_mappings;
        self.vector_staged_records_max = metrics.staged_records_max;
    }
}

/// Indexing result summary
#[derive(Debug, Clone)]
pub struct IndexResult {
    /// Total files processed
    pub total_files: usize,
    /// Files successfully indexed
    pub indexed_files: usize,
    /// Files that failed
    pub failed_files: usize,
    /// Files that completed "successfully" but produced degraded content:
    /// lossy decoding (U+FFFD) or zero entities and zero chunks.
    pub degraded_files: usize,
    /// Files with deterministic failures that were skipped without retry
    /// (unsupported language, drifted or undecodable content). Separate from
    /// `failed_files`, which only counts transient failures kept for resume.
    pub skipped_permanent: usize,
    /// Total entities extracted
    pub total_entities: usize,
    /// Total relations extracted
    pub total_relations: usize,
    /// Total vectors stored
    pub total_vectors: usize,
    /// Total tokens used for embedding
    pub total_tokens: u64,
    /// Whether a backend circuit breaker (vector store or LLM service)
    /// opened during this run, meaning writes or summaries were rejected by
    /// an outage rather than by per-file content failures.
    pub circuit_open: bool,
    /// Execution outcome (Success or Incomplete)
    pub outcome: IndexExecutionOutcome,
    /// Elapsed time in milliseconds
    pub elapsed_ms: u64,
    /// Per-stage wall-clock accounting for the run.
    pub stages: IndexStageTimings,
}

impl Default for IndexResult {
    fn default() -> Self {
        Self {
            total_files: 0,
            indexed_files: 0,
            failed_files: 0,
            degraded_files: 0,
            skipped_permanent: 0,
            total_entities: 0,
            total_relations: 0,
            total_vectors: 0,
            total_tokens: 0,
            circuit_open: false,
            outcome: IndexExecutionOutcome::Success,
            elapsed_ms: 0,
            stages: IndexStageTimings::default(),
        }
    }
}

impl IndexResult {
    /// Create a new empty result
    pub fn new() -> Self {
        Self::default()
    }

    /// Get success rate as percentage
    pub fn success_rate(&self) -> f32 {
        if self.total_files == 0 {
            return 100.0;
        }
        (self.indexed_files as f32 / self.total_files as f32) * 100.0
    }

    /// Check if indexing was completely successful
    pub fn is_success(&self) -> bool {
        self.outcome == IndexExecutionOutcome::Success
    }

    /// Get error messages (only populated when outcome is Incomplete)
    pub fn errors(&self) -> &[String] {
        match &self.outcome {
            IndexExecutionOutcome::Success => &[],
            IndexExecutionOutcome::Incomplete { errors } => errors,
        }
    }

    /// Format summary for logging
    pub fn format_summary(&self) -> String {
        let mut summary = format!(
            "Indexed {}/{} files ({} degraded, {} permanently skipped), {} entities, {} relations, {} vectors in {}ms",
            self.indexed_files,
            self.total_files,
            self.degraded_files,
            self.skipped_permanent,
            self.total_entities,
            self.total_relations,
            self.total_vectors,
            self.elapsed_ms
        );
        if self.circuit_open {
            summary.push_str(" (circuit breaker open)");
        }
        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_result_default() {
        let result = IndexResult::default();
        assert_eq!(result.total_files, 0);
        assert_eq!(result.indexed_files, 0);
        assert_eq!(result.failed_files, 0);
        assert_eq!(result.success_rate(), 100.0);
    }

    #[test]
    fn test_success_rate() {
        let result = IndexResult {
            total_files: 100,
            indexed_files: 90,
            failed_files: 10,
            ..Default::default()
        };
        assert_eq!(result.success_rate(), 90.0);
    }

    #[test]
    fn test_is_success() {
        let _result = IndexResult {
            total_files: 10,
            indexed_files: 10,
            failed_files: 0,
            ..Default::default()
        };
    }
}
