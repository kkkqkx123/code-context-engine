//! Pipeline-level metrics for data processing stages
//!
//! This module provides metrics for individual processing steps in the indexing pipeline.
//! These metrics track the performance and quality of each stage:
//! - Embedding generation
//! - Code parsing
//! - Relation extraction
//! - Summary generation
//!
//! # Design Principles
//!
//! - **Granular Tracking**: Each pipeline stage has its own metrics
//! - **Provider/Context Labels**: Support labeling by provider or language
//! - **Type Safety**: Strong typing prevents metric naming inconsistencies

use std::sync::Arc;

use dashmap::DashMap;

use crate::{
    EmbeddingErrorType, LabeledCounter, LabeledFloatGauge, LabeledHistogram, MetricsRegistry,
    PipelineStage,
};

/// Embedding provider monitoring metrics
///
/// Tracks performance and quality metrics for embedding operations.
/// This structure encapsulates the raw metric primitives with embedding-specific semantics.
#[derive(Debug)]
pub struct EmbeddingMetrics {
    /// Total number of embedding requests
    pub requests_total: LabeledCounter,
    /// Request latency distribution (in milliseconds)
    pub latency_ms: LabeledHistogram,
    /// Total number of tokens processed
    pub tokens_total: LabeledCounter,
    /// Total number of errors (aggregate)
    pub errors_total: LabeledCounter,
    /// Batch size distribution (number of chunks per request)
    pub batch_size: LabeledHistogram,
    /// Distribution of tokens per batch
    pub tokens_per_batch: LabeledHistogram,
    /// Retry count for embedding requests
    pub retries_total: LabeledCounter,
    /// Per-error-type counters classified by `error_type` label
    pub errors_by_type: Arc<DashMap<String, LabeledCounter>>,
    /// Provider label for counter labels
    provider_label: String,
    /// Registry for lazy counter creation
    registry: MetricsRegistry,
}

impl EmbeddingMetrics {
    /// Create new embedding metrics with the given registry and provider label
    ///
    /// # Arguments
    ///
    /// * `registry` - The global metrics registry
    /// * `provider_label` - Label to identify the embedding provider (e.g., "openai", "llamacpp")
    ///
    /// # Example
    ///
    /// ```rust
    /// use cce_core::metrics::{MetricsRegistry, EmbeddingMetrics};
    ///
    /// let registry = MetricsRegistry::new();
    /// let metrics = EmbeddingMetrics::new(&registry, "openai");
    /// ```
    pub fn new(registry: &MetricsRegistry, provider_label: &str) -> Arc<Self> {
        let latency_buckets = crate::EMBEDDING_BUCKETS.to_vec();
        let prov = provider_label.to_string();
        Arc::new(Self {
            requests_total: registry
                .counter("embedding_requests_total", &[("provider", provider_label)]),
            latency_ms: registry.histogram(
                "embedding_latency_ms",
                latency_buckets,
                &[("provider", provider_label)],
            ),
            tokens_total: registry
                .counter("embedding_tokens_total", &[("provider", provider_label)]),
            errors_total: registry
                .counter("embedding_errors_total", &[("provider", provider_label)]),
            batch_size: registry.histogram(
                "embedding_batch_size",
                vec![1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0, 128.0, 256.0],
                &[("provider", provider_label)],
            ),
            tokens_per_batch: registry.histogram(
                "embedding_tokens_per_batch",
                vec![
                    100.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0, 32000.0, 64000.0,
                ],
                &[("provider", provider_label)],
            ),
            retries_total: registry
                .counter("embedding_retries_total", &[("provider", provider_label)]),
            errors_by_type: Arc::new(DashMap::new()),
            provider_label: prov,
            registry: registry.clone(),
        })
    }

    /// Record a completed embedding request
    ///
    /// This is a convenience method that updates all relevant metrics in one call.
    ///
    /// # Arguments
    ///
    /// * `latency_ms` - Request duration in milliseconds
    /// * `token_count` - Number of tokens processed
    /// * `success` - Whether the request succeeded
    pub fn record_request(&self, latency_ms: f64, token_count: usize, success: bool) {
        self.record_request_with_batch(latency_ms, token_count, 1, success)
    }

    /// Record a completed embedding request with batch size tracking
    ///
    /// # Arguments
    ///
    /// * `latency_ms` - Request duration in milliseconds
    /// * `token_count` - Number of tokens processed
    /// * `batch_size` - Number of chunks in this batch
    /// * `success` - Whether the request succeeded
    pub fn record_request_with_batch(
        &self,
        latency_ms: f64,
        token_count: usize,
        batch_size: usize,
        success: bool,
    ) {
        self.requests_total.increment();
        self.latency_ms.observe(latency_ms);
        self.tokens_total.add(token_count as u64);
        self.batch_size.observe(batch_size as f64);
        self.tokens_per_batch.observe(token_count as f64);

        if !success {
            self.errors_total.increment();
        }
    }

    /// Record an embedding error with detailed error type classification.
    pub fn record_error(&self, error_type: EmbeddingErrorType) {
        self.errors_total.increment();
        let type_str = error_type.to_string();
        let registry = self.registry.clone();
        let provider = self.provider_label.clone();
        let counter = self
            .errors_by_type
            .entry(type_str.clone())
            .or_insert_with(|| {
                registry.counter(
                    "embedding_errors_total",
                    &[("provider", &provider), ("error_type", &type_str)],
                )
            });
        counter.increment();
    }

    /// Record a retry attempt for an embedding request.
    pub fn record_retry(&self) {
        self.retries_total.increment();
    }
}

/// Parser monitoring metrics
///
/// Tracks performance and quality metrics for code parsing operations.
#[derive(Debug)]
pub struct ParserMetrics {
    /// Total number of parse attempts
    pub parse_attempts_total: LabeledCounter,
    /// Parse latency distribution (in milliseconds)
    pub parse_latency_ms: LabeledHistogram,
    /// Total number of parse errors
    pub parse_errors_total: LabeledCounter,
}

impl ParserMetrics {
    /// Create new parser metrics with the given registry
    pub fn new(registry: &MetricsRegistry, project_id: i64) -> Arc<Self> {
        let proj_val = project_id.to_string();
        Arc::new(Self {
            parse_attempts_total: registry
                .counter("parse_attempts_total", &[("project_id", &proj_val)]),
            parse_latency_ms: registry.histogram(
                "parse_latency_ms",
                crate::LATENCY_BUCKETS.to_vec(),
                &[("project_id", &proj_val)],
            ),
            parse_errors_total: registry
                .counter("parse_errors_total", &[("project_id", &proj_val)]),
        })
    }

    /// Record a completed parse operation
    ///
    /// # Arguments
    ///
    /// * `latency_ms` - Parse duration in milliseconds
    /// * `success` - Whether parsing succeeded
    pub fn record_parse(&self, latency_ms: f64, success: bool) {
        self.parse_attempts_total.increment();
        self.parse_latency_ms.observe(latency_ms);

        if !success {
            self.parse_errors_total.increment();
        }
    }
}

/// Relation builder monitoring metrics
///
/// Tracks performance and quality metrics for relation extraction and building operations.
#[derive(Debug)]
pub struct RelationMetrics {
    /// Total number of relations extracted
    pub relations_extracted_total: LabeledCounter,
    /// Relation build latency distribution (in milliseconds)
    pub build_latency_ms: LabeledHistogram,
    /// Total number of files processed for relations
    pub files_processed_total: LabeledCounter,
    /// Total number of relations dropped because the per-file budget was exceeded
    pub truncated_relations: LabeledCounter,
    /// Total number of unresolved relations dropped because their callee looked like a standard library name
    pub stdlib_filtered: LabeledCounter,
    /// Total number of unresolved standard-library-like relations preserved as external calls
    pub stdlib_preserved_external: LabeledCounter,
    /// Total number of stable symbol key collisions (same key registered to a different entity)
    pub symbol_key_conflicts: LabeledCounter,
    /// Total number of module path collisions (same module path claimed by multiple files)
    pub module_path_conflicts: LabeledCounter,
    /// Total number of local call resolutions with multiple viable candidate
    /// targets at the same precedence tier: the resolver keeps the
    /// deterministic first candidate but the count signals overload/shadowing
    /// that warrants a language-specific disambiguation rule
    pub relation_ambiguous_targets_total: LabeledCounter,
    /// Total number of files that fell back to a second AST parse for import
    /// extraction (the coordinator normally fills `import_table` for built-in
    /// languages, so this path only fires for fixtures or legacy inputs)
    pub import_fallback_total: LabeledCounter,
    /// Total number of exports whose stable symbol key could not be resolved
    /// during delta application (the export is skipped instead of being
    /// attached to a dangling entity ID)
    pub delta_export_unresolved_total: LabeledCounter,
    /// Total number of entities exported with a derived symbol key during
    /// snapshot building (snapshot degradation; the entity remains addressable
    /// through its derived identity)
    pub entity_derived_key_total: LabeledCounter,
    /// Total number of relation callers/targets exported with a derived
    /// symbol key during snapshot building (snapshot degradation)
    pub relation_derived_key_total: LabeledCounter,
    /// Total number of relation edges dropped because their caller file was
    /// outside the affected scope of a hot update: the edge is lost
    /// without a re-parse of the caller and can never be re-derived by a
    /// later update. Non-zero growth signals under-propagated hot updates.
    pub relation_edges_dropped_unbounded_total: LabeledCounter,
    /// Total number of full package export rebuilds (legacy path)
    pub symbol_table_rebuild_total: LabeledCounter,
    /// Total number of incremental package export updates
    pub symbol_table_incremental_total: LabeledCounter,
    /// Total number of build configuration scan failures
    pub config_scan_failures_total: LabeledCounter,
    /// Total number of times fine-grained config narrowing fell back to the
    /// extension-based set (e.g. import mapping failure or empty narrowed set)
    pub config_fine_grained_fallback_total: LabeledCounter,
    /// Total number of files identified as affected by a config change
    /// (extension-based scope)
    pub config_affected_files_total: LabeledCounter,
    /// Total number of files narrowed by package import intersection
    pub config_narrowed_files_total: LabeledCounter,
    /// Total bytes replayed from the relation spool (rkyv+zstd decompressed)
    pub relation_spool_replay_bytes_total: LabeledCounter,
    /// Number of spool replay passes executed (2 in steady state, 4 in fallback)
    pub relation_spool_replay_passes_total: LabeledCounter,
    /// Decompression time per spool replay pass in milliseconds
    pub relation_spool_decompress_ms: LabeledHistogram,
    /// Number of times max_entity_id fell back to a full scan self-heal
    pub relation_max_entity_id_scan_fallback_total: LabeledCounter,
    /// Total number of relations left unresolved, bucketed by `reason` label
    pub relation_unresolved_total: Arc<DashMap<String, LabeledCounter>>,
    /// Ratio of unresolved relations over extracted relations (0..=1),
    /// refreshed when a build completes
    pub relation_unresolved_ratio: LabeledFloatGauge,
    /// Total symbol-table resolution calls
    pub resolve_calls_total: LabeledCounter,
    /// Total symbol-table resolution lookups (each resolution attempt)
    pub resolve_lookups_total: LabeledCounter,
    /// Average lookups per resolution call, refreshed when a build completes
    pub resolve_avg_lookups: LabeledFloatGauge,
    /// Type-member lookups total
    pub type_member_lookup_total: LabeledCounter,
    /// Type-member hits
    pub type_member_hit_total: LabeledCounter,
    /// Type-member misses
    pub type_member_miss_total: LabeledCounter,
    /// Duplicate member definitions
    pub type_member_duplicate_total: LabeledCounter,
    /// Resolution cache hits (positive cache)
    pub resolution_cache_hit_total: LabeledCounter,
    /// Resolution cache misses (negative cache hits or cache misses)
    pub resolution_cache_miss_total: LabeledCounter,
    /// Total number of relation edges dropped due to self-loop suppression
    pub relation_self_loop_filtered_total: LabeledCounter,
    /// Registry for lazy per-reason counter creation
    registry: MetricsRegistry,
    /// Project id label value shared by all counters
    project_id: String,
}

impl RelationMetrics {
    /// Create new relation metrics with the given registry
    pub fn new(registry: &MetricsRegistry, project_id: i64) -> Arc<Self> {
        let proj_val = project_id.to_string();
        Arc::new(Self {
            relations_extracted_total: registry
                .counter("relations_extracted_total", &[("project_id", &proj_val)]),
            build_latency_ms: registry.histogram(
                "relation_build_latency_ms",
                crate::LATENCY_BUCKETS.to_vec(),
                &[("project_id", &proj_val)],
            ),
            files_processed_total: registry.counter(
                "files_processed_for_relations_total",
                &[("project_id", &proj_val)],
            ),
            truncated_relations: registry
                .counter("relation_truncated_total", &[("project_id", &proj_val)]),
            stdlib_filtered: registry.counter(
                "relation_stdlib_filtered_total",
                &[("project_id", &proj_val)],
            ),
            stdlib_preserved_external: registry.counter(
                "relation_stdlib_preserved_external_total",
                &[("project_id", &proj_val)],
            ),
            symbol_key_conflicts: registry.counter(
                "relation_symbol_key_conflicts_total",
                &[("project_id", &proj_val)],
            ),
            module_path_conflicts: registry.counter(
                "relation_module_path_conflicts_total",
                &[("project_id", &proj_val)],
            ),
            relation_ambiguous_targets_total: registry.counter(
                "relation_ambiguous_targets_total",
                &[("project_id", &proj_val)],
            ),
            import_fallback_total: registry.counter(
                "relation_import_fallback_total",
                &[("project_id", &proj_val)],
            ),
            delta_export_unresolved_total: registry.counter(
                "relation_delta_export_unresolved_total",
                &[("project_id", &proj_val)],
            ),
            entity_derived_key_total: registry.counter(
                "relation_entity_derived_key_total",
                &[("project_id", &proj_val)],
            ),
            relation_derived_key_total: registry.counter(
                "relation_relation_derived_key_total",
                &[("project_id", &proj_val)],
            ),
            relation_edges_dropped_unbounded_total: registry.counter(
                "relation_edges_dropped_unbounded_total",
                &[("project_id", &proj_val)],
            ),
            symbol_table_rebuild_total: registry.counter(
                "relation_symbol_table_rebuild_total",
                &[("project_id", &proj_val)],
            ),
            symbol_table_incremental_total: registry.counter(
                "relation_symbol_table_incremental_total",
                &[("project_id", &proj_val)],
            ),
            config_scan_failures_total: registry.counter(
                "relation_config_scan_failures_total",
                &[("project_id", &proj_val)],
            ),
            config_fine_grained_fallback_total: registry.counter(
                "relation_config_fine_grained_fallback_total",
                &[("project_id", &proj_val)],
            ),
            config_affected_files_total: registry.counter(
                "relation_config_affected_files_total",
                &[("project_id", &proj_val)],
            ),
            config_narrowed_files_total: registry.counter(
                "relation_config_narrowed_files_total",
                &[("project_id", &proj_val)],
            ),
            relation_spool_replay_bytes_total: registry.counter(
                "relation_spool_replay_bytes_total",
                &[("project_id", &proj_val)],
            ),
            relation_spool_replay_passes_total: registry.counter(
                "relation_spool_replay_passes_total",
                &[("project_id", &proj_val)],
            ),
            relation_spool_decompress_ms: registry.histogram(
                "relation_spool_decompress_ms",
                crate::LATENCY_BUCKETS.to_vec(),
                &[("project_id", &proj_val)],
            ),
            relation_max_entity_id_scan_fallback_total: registry.counter(
                "relation_max_entity_id_scan_fallback_total",
                &[("project_id", &proj_val)],
            ),
            relation_unresolved_total: Arc::new(DashMap::new()),
            relation_unresolved_ratio: registry
                .float_gauge("relation_unresolved_ratio", &[("project_id", &proj_val)]),
            resolve_calls_total: registry
                .counter("relation_resolve_calls_total", &[("project_id", &proj_val)]),
            resolve_lookups_total: registry.counter(
                "relation_resolve_lookups_total",
                &[("project_id", &proj_val)],
            ),
            resolve_avg_lookups: registry
                .float_gauge("relation_resolve_avg_lookups", &[("project_id", &proj_val)]),
            type_member_lookup_total: registry
                .counter("type_member_lookup_total", &[("project_id", &proj_val)]),
            type_member_hit_total: registry
                .counter("type_member_hit_total", &[("project_id", &proj_val)]),
            type_member_miss_total: registry
                .counter("type_member_miss_total", &[("project_id", &proj_val)]),
            type_member_duplicate_total: registry
                .counter("type_member_duplicate_total", &[("project_id", &proj_val)]),
            resolution_cache_hit_total: registry.counter(
                "relation_resolution_cache_hit_total",
                &[("project_id", &proj_val)],
            ),
            resolution_cache_miss_total: registry.counter(
                "relation_resolution_cache_miss_total",
                &[("project_id", &proj_val)],
            ),
            relation_self_loop_filtered_total: registry.counter(
                "relation_self_loop_filtered_total",
                &[("project_id", &proj_val)],
            ),
            registry: registry.clone(),
            project_id: proj_val,
        })
    }

    /// Record completion of relation building for a batch of files
    pub fn record_build(&self, latency_ms: f64, extracted_count: usize, file_count: usize) {
        self.build_latency_ms.observe(latency_ms);
        self.relations_extracted_total.add(extracted_count as u64);
        self.files_processed_total.add(file_count as u64);

        let unresolved: u64 = self
            .relation_unresolved_total
            .iter()
            .map(|entry| entry.value().get())
            .sum();
        if extracted_count > 0 {
            self.relation_unresolved_ratio
                .set(unresolved as f64 / extracted_count as f64);
        }

        let calls = self.resolve_calls_total.get();
        if calls > 0 {
            let lookups = self.resolve_lookups_total.get();
            self.resolve_avg_lookups.set(lookups as f64 / calls as f64);
        }
    }

    /// Record one unresolved relation, bucketed by reason
    pub fn record_unresolved(&self, reason: &str) {
        let reason_str = reason.to_string();
        let proj = self.project_id.clone();
        let counter = self
            .relation_unresolved_total
            .entry(reason_str.clone())
            .or_insert_with(|| {
                self.registry.counter(
                    "relation_unresolved_total",
                    &[("project_id", &proj), ("reason", &reason_str)],
                )
            });
        counter.increment();
    }
}

/// Summary generator monitoring metrics
///
/// Tracks performance and quality metrics for summary generation operations.
#[derive(Debug)]
pub struct SummaryMetrics {
    /// Total number of summaries generated
    pub summaries_generated_total: LabeledCounter,
    /// Summary generation latency distribution (in milliseconds)
    pub generation_latency_ms: LabeledHistogram,
    /// Average summary length (float gauge for precision)
    pub avg_summary_length: LabeledFloatGauge,
    /// Total number of model-enhanced summary requests
    pub model_enhancement_requests_total: LabeledCounter,
    /// Model-enhanced summary latency distribution (in milliseconds)
    pub model_enhancement_latency_ms: LabeledHistogram,
    /// Total number of model-enhanced summary errors
    pub model_enhancement_errors_total: LabeledCounter,
}

impl SummaryMetrics {
    /// Create new summary metrics with the given registry
    pub fn new(registry: &MetricsRegistry, project_id: i64) -> Arc<Self> {
        let proj_val = project_id.to_string();
        Arc::new(Self {
            summaries_generated_total: registry
                .counter("summaries_generated_total", &[("project_id", &proj_val)]),
            generation_latency_ms: registry.histogram_default(
                "summary_generation_latency_ms",
                &[("project_id", &proj_val)],
            ),
            avg_summary_length: registry
                .float_gauge("summary_avg_length", &[("project_id", &proj_val)]),
            model_enhancement_requests_total: registry.counter(
                "summary_model_enhancement_requests_total",
                &[("project_id", &proj_val)],
            ),
            model_enhancement_latency_ms: registry.histogram_default(
                "summary_model_enhancement_latency_ms",
                &[("project_id", &proj_val)],
            ),
            model_enhancement_errors_total: registry.counter(
                "summary_model_enhancement_errors_total",
                &[("project_id", &proj_val)],
            ),
        })
    }

    /// Record a completed summary generation
    ///
    /// # Arguments
    ///
    /// * `latency_ms` - Generation duration in milliseconds
    /// * `summary_length` - Length of the generated summary (characters)
    pub fn record_generation(&self, latency_ms: f64, summary_length: usize) {
        self.summaries_generated_total.increment();
        self.generation_latency_ms.observe(latency_ms);

        // Update average summary length using precise floating-point calculation
        let current_avg = self.avg_summary_length.get();
        let count = self.summaries_generated_total.get() as f64;

        // Calculate new average: ((old_avg * (count-1)) + new_value) / count
        let new_avg = if count > 1.0 {
            ((current_avg * (count - 1.0)) + summary_length as f64) / count
        } else {
            // First summary, average is just the first value
            summary_length as f64
        };

        self.avg_summary_length.set(new_avg);
    }

    /// Record a model-enhanced summary request
    pub fn record_model_enhancement(&self, latency_ms: f64, success: bool) {
        self.model_enhancement_requests_total.increment();
        self.model_enhancement_latency_ms.observe(latency_ms);

        if !success {
            self.model_enhancement_errors_total.increment();
        }
    }
}

/// Pipeline stage metrics for grouper, converter, and chunker
///
/// Tracks latency, item counts, and chunk size distribution for the intermediate
/// stages of the indexing pipeline.
#[derive(Debug)]
pub struct PipelineStageMetrics {
    /// Stage processing latency distribution (in milliseconds)
    pub stage_latency_ms: LabeledHistogram,
    /// Total number of items processed by stage
    pub stage_processed_total: LabeledCounter,
    /// Total number of errors by stage
    pub stage_errors_total: LabeledCounter,
    /// Total number of output items produced by stage
    pub stage_output_items_total: LabeledCounter,
    /// Chunk size distribution in bytes (only populated for the chunker stage)
    pub chunk_size_bytes: LabeledHistogram,
}

impl PipelineStageMetrics {
    /// Create new pipeline stage metrics
    pub fn new(registry: &MetricsRegistry, stage: PipelineStage, project_id: i64) -> Arc<Self> {
        let proj_val = project_id.to_string();
        let stage_str = stage.to_string();
        let stage_label = [("stage", stage_str.as_str()), ("project_id", &proj_val)];
        Arc::new(Self {
            stage_latency_ms: registry.histogram(
                "pipeline_stage_latency_ms",
                crate::LATENCY_BUCKETS.to_vec(),
                &stage_label,
            ),
            stage_processed_total: registry.counter("pipeline_stage_processed_total", &stage_label),
            stage_errors_total: registry.counter("pipeline_stage_errors_total", &stage_label),
            stage_output_items_total: registry
                .counter("pipeline_stage_output_items_total", &stage_label),
            chunk_size_bytes: registry.histogram(
                "pipeline_chunk_size_bytes",
                vec![
                    256.0, 512.0, 1024.0, 2048.0, 4096.0, 8192.0, 16384.0, 32768.0, 65536.0,
                    131072.0, 262144.0,
                ],
                &stage_label,
            ),
        })
    }

    /// Record a completed stage operation
    pub fn record(&self, input_count: usize, output_count: usize, latency_ms: f64, error: bool) {
        self.stage_processed_total.add(input_count as u64);
        self.stage_output_items_total.add(output_count as u64);
        self.stage_latency_ms.observe(latency_ms);
        if error {
            self.stage_errors_total.increment();
        }
    }

    /// Record the size of a produced chunk in bytes
    pub fn record_chunk_size(&self, size_bytes: usize) {
        self.chunk_size_bytes.observe(size_bytes as f64);
    }
}

/// File-level end-to-end processing metrics
///
/// Tracks the total time from parse through chunking for a single file,
/// providing visibility into per-file processing cost.
#[derive(Debug)]
pub struct FileProcessingMetrics {
    /// End-to-end file processing latency distribution (in milliseconds)
    pub file_total_latency_ms: LabeledHistogram,
    /// Total number of files processed
    pub files_processed_total: LabeledCounter,
    /// Total number of files that failed processing
    pub files_failed_total: LabeledCounter,
}

impl FileProcessingMetrics {
    /// Create new file processing metrics
    pub fn new(registry: &MetricsRegistry, project_id: i64) -> Arc<Self> {
        let proj_val = project_id.to_string();
        Arc::new(Self {
            file_total_latency_ms: registry.histogram(
                "file_processing_total_latency_ms",
                crate::LATENCY_BUCKETS.to_vec(),
                &[("project_id", &proj_val)],
            ),
            files_processed_total: registry
                .counter("files_processed_total", &[("project_id", &proj_val)]),
            files_failed_total: registry
                .counter("files_failed_total", &[("project_id", &proj_val)]),
        })
    }

    /// Record a completed file processing operation
    pub fn record_file(&self, latency_ms: f64, success: bool) {
        self.files_processed_total.increment();
        self.file_total_latency_ms.observe(latency_ms);
        if !success {
            self.files_failed_total.increment();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_stage_metrics() {
        let registry = MetricsRegistry::new();
        let metrics = PipelineStageMetrics::new(&registry, PipelineStage::Grouper, 1);

        metrics.record(10, 8, 5.0, false);
        assert_eq!(metrics.stage_processed_total.get(), 10);
        assert_eq!(metrics.stage_output_items_total.get(), 8);
        assert_eq!(metrics.stage_latency_ms.get_count(), 1);
        assert_eq!(metrics.stage_errors_total.get(), 0);

        metrics.record(5, 0, 2.0, true);
        assert_eq!(metrics.stage_processed_total.get(), 15);
        assert_eq!(metrics.stage_errors_total.get(), 1);
    }

    #[test]
    fn test_pipeline_stage_metrics_chunk_size() {
        let registry = MetricsRegistry::new();
        let metrics = PipelineStageMetrics::new(&registry, PipelineStage::Chunker, 1);

        metrics.record_chunk_size(512);
        metrics.record_chunk_size(2048);
        metrics.record_chunk_size(4096);

        assert_eq!(metrics.chunk_size_bytes.get_count(), 3);
    }

    #[test]
    fn test_embedding_metrics_creation() {
        let registry = MetricsRegistry::new();
        let metrics = EmbeddingMetrics::new(&registry, "test_provider");

        assert_eq!(metrics.requests_total.get(), 0);
        assert_eq!(metrics.tokens_total.get(), 0);
        assert_eq!(metrics.errors_total.get(), 0);
    }

    #[test]
    fn test_embedding_metrics_record() {
        let registry = MetricsRegistry::new();
        let metrics = EmbeddingMetrics::new(&registry, "test_provider");

        // Record successful request
        metrics.record_request(100.0, 512, true);

        assert_eq!(metrics.requests_total.get(), 1);
        assert_eq!(metrics.tokens_total.get(), 512);
        assert_eq!(metrics.errors_total.get(), 0);
        assert_eq!(metrics.latency_ms.get_count(), 1);

        // Record failed request
        metrics.record_request(50.0, 256, false);

        assert_eq!(metrics.requests_total.get(), 2);
        assert_eq!(metrics.tokens_total.get(), 768);
        assert_eq!(metrics.errors_total.get(), 1);
    }

    #[test]
    fn test_relation_metrics_creation() {
        let registry = MetricsRegistry::new();
        let metrics = RelationMetrics::new(&registry, 1);

        assert_eq!(metrics.relations_extracted_total.get(), 0);
        assert_eq!(metrics.files_processed_total.get(), 0);
    }

    #[test]
    fn test_relation_metrics_record_build() {
        let registry = MetricsRegistry::new();
        let metrics = RelationMetrics::new(&registry, 1);

        // Record relation build
        metrics.record_build(45.0, 100, 10);

        assert_eq!(metrics.relations_extracted_total.get(), 100);
        assert_eq!(metrics.files_processed_total.get(), 10);
        assert_eq!(metrics.build_latency_ms.get_count(), 1);
    }

    #[test]
    fn test_parser_metrics_creation() {
        let registry = MetricsRegistry::new();
        let metrics = ParserMetrics::new(&registry, 1);

        assert_eq!(metrics.parse_attempts_total.get(), 0);
        assert_eq!(metrics.parse_errors_total.get(), 0);
    }

    #[test]
    fn test_parser_metrics_record() {
        let registry = MetricsRegistry::new();
        let metrics = ParserMetrics::new(&registry, 1);

        // Record successful parse
        metrics.record_parse(25.0, true);
        assert_eq!(metrics.parse_attempts_total.get(), 1);
        assert_eq!(metrics.parse_errors_total.get(), 0);
        assert_eq!(metrics.parse_latency_ms.get_count(), 1);

        // Record failed parse
        metrics.record_parse(10.0, false);
        assert_eq!(metrics.parse_attempts_total.get(), 2);
        assert_eq!(metrics.parse_errors_total.get(), 1);
    }

    #[test]
    fn test_summary_metrics_creation() {
        let registry = MetricsRegistry::new();
        let metrics = SummaryMetrics::new(&registry, 1);

        assert_eq!(metrics.summaries_generated_total.get(), 0);
        assert!((metrics.avg_summary_length.get() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_summary_metrics_record() {
        let registry = MetricsRegistry::new();
        let metrics = SummaryMetrics::new(&registry, 1);

        // Record first summary generation
        metrics.record_generation(50.0, 200);
        assert_eq!(metrics.summaries_generated_total.get(), 1);
        assert!((metrics.avg_summary_length.get() - 200.0).abs() < f64::EPSILON);
        assert_eq!(metrics.generation_latency_ms.get_count(), 1);

        // Record second summary generation
        metrics.record_generation(30.0, 300);
        assert_eq!(metrics.summaries_generated_total.get(), 2);
        // Average should be (200 + 300) / 2 = 250
        assert!((metrics.avg_summary_length.get() - 250.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_summary_model_enhancement_metrics() {
        let registry = MetricsRegistry::new();
        let metrics = SummaryMetrics::new(&registry, 1);

        metrics.record_model_enhancement(42.0, true);
        assert_eq!(metrics.model_enhancement_requests_total.get(), 1);
        assert_eq!(metrics.model_enhancement_errors_total.get(), 0);
        assert_eq!(metrics.model_enhancement_latency_ms.get_count(), 1);

        metrics.record_model_enhancement(10.0, false);
        assert_eq!(metrics.model_enhancement_requests_total.get(), 2);
        assert_eq!(metrics.model_enhancement_errors_total.get(), 1);
    }

    #[test]
    fn test_embedding_metrics_arc_sharing() {
        let registry = MetricsRegistry::new();
        let metrics = EmbeddingMetrics::new(&registry, "test_provider");

        // Verify it's an Arc
        let metrics_clone = Arc::clone(&metrics);

        metrics.record_request(100.0, 512, true);

        assert_eq!(metrics_clone.requests_total.get(), 1);
        assert_eq!(metrics_clone.tokens_total.get(), 512);
    }

    #[test]
    fn test_embedding_metrics_batch_tracking() {
        let registry = MetricsRegistry::new();
        let metrics = EmbeddingMetrics::new(&registry, "test_provider");

        metrics.record_request_with_batch(100.0, 2000, 8, true);
        metrics.record_request_with_batch(150.0, 4000, 16, true);

        assert_eq!(metrics.requests_total.get(), 2);
        assert_eq!(metrics.tokens_total.get(), 6000);
        assert_eq!(metrics.batch_size.get_count(), 2);
        assert_eq!(metrics.tokens_per_batch.get_count(), 2);
    }

    #[test]
    fn test_relation_metrics_zero_total_relations() {
        let registry = MetricsRegistry::new();
        let metrics = RelationMetrics::new(&registry, 1);

        metrics.record_build(45.0, 100, 10);

        assert_eq!(metrics.relations_extracted_total.get(), 100);
        assert_eq!(metrics.files_processed_total.get(), 10);
    }

    #[test]
    fn test_summary_metrics_average_calculation() {
        let registry = MetricsRegistry::new();
        let metrics = SummaryMetrics::new(&registry, 1);

        // First summary: avg = 200
        metrics.record_generation(50.0, 200);
        assert!((metrics.avg_summary_length.get() - 200.0).abs() < f64::EPSILON);

        // Second summary: avg = (200 + 300) / 2 = 250
        metrics.record_generation(30.0, 300);
        assert!((metrics.avg_summary_length.get() - 250.0).abs() < f64::EPSILON);

        // Third summary: avg = (200 + 300 + 100) / 3 = 200
        metrics.record_generation(40.0, 100);
        assert!((metrics.avg_summary_length.get() - 200.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parser_metrics_success_rate() {
        let registry = MetricsRegistry::new();
        let metrics = ParserMetrics::new(&registry, 1);

        // 8 successful parses
        for _ in 0..8 {
            metrics.record_parse(25.0, true);
        }

        // 2 failed parses
        for _ in 0..2 {
            metrics.record_parse(10.0, false);
        }

        assert_eq!(metrics.parse_attempts_total.get(), 10);
        assert_eq!(metrics.parse_errors_total.get(), 2);
        assert_eq!(metrics.parse_latency_ms.get_count(), 10);
    }

    #[test]
    fn test_summary_metrics_precision_with_float_gauge() {
        let registry = MetricsRegistry::new();
        let metrics = SummaryMetrics::new(&registry, 1);

        // Test precision is maintained with float gauge
        metrics.record_generation(33.3, 100);
        metrics.record_generation(33.3, 200);
        metrics.record_generation(33.4, 300);

        // Average should be (100 + 200 + 300) / 3 = 200.0
        let avg = metrics.avg_summary_length.get();
        assert!((avg - 200.0).abs() < 0.01, "Expected ~200.0, got {}", avg);
    }

    #[test]
    fn test_concurrent_metric_updates() {
        use std::sync::Arc;
        use std::thread;

        let registry = Arc::new(MetricsRegistry::new());
        let metrics = EmbeddingMetrics::new(&registry, "test");

        let mut handles = vec![];

        // Spawn 10 threads, each recording 100 requests
        for _ in 0..10 {
            let m = Arc::clone(&metrics);
            let handle = thread::spawn(move || {
                for i in 0..100 {
                    m.record_request(10.0 + (i as f64), 100, i % 10 != 0);
                }
            });
            handles.push(handle);
        }

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        // Verify final counts
        assert_eq!(metrics.requests_total.get(), 1000);
        assert_eq!(metrics.tokens_total.get(), 100000);
        // 10% should fail (every 10th request)
        assert_eq!(metrics.errors_total.get(), 100);
    }

    #[test]
    fn test_boundary_values() {
        let registry = MetricsRegistry::new();

        // Test with zero values
        let summary_metrics = SummaryMetrics::new(&registry, 1);
        summary_metrics.record_generation(0.0, 0);
        assert_eq!(summary_metrics.summaries_generated_total.get(), 1);
        assert!((summary_metrics.avg_summary_length.get() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_file_processing_metrics() {
        let registry = MetricsRegistry::new();
        let metrics = FileProcessingMetrics::new(&registry, 1);

        metrics.record_file(120.0, true);
        assert_eq!(metrics.files_processed_total.get(), 1);
        assert_eq!(metrics.files_failed_total.get(), 0);
        assert_eq!(metrics.file_total_latency_ms.get_count(), 1);

        metrics.record_file(80.0, false);
        assert_eq!(metrics.files_processed_total.get(), 2);
        assert_eq!(metrics.files_failed_total.get(), 1);
    }
}
