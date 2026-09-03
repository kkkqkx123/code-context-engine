//! Centralized metric descriptions
//!
//! Single source of truth for Prometheus `# HELP` text. Every metric name in
//! the workspace must be listed here; the exporter looks descriptions up from
//! this table instead of maintaining its own hard-coded match block.

/// Look up the HELP description for a metric name.
pub fn metric_description(name: &str) -> Option<&'static str> {
    DESCRIPTIONS
        .binary_search_by(|(n, _)| (*n).cmp(name))
        .ok()
        .map(|idx| DESCRIPTIONS[idx].1)
}

/// (name, description) pairs, kept sorted by name for binary search.
/// (name, description) pairs, kept sorted by name for binary search.
static DESCRIPTIONS: &[(&str, &str)] = &[
    (
        "bg_aggregated_records_total",
        "Total aggregated metric records stored",
    ),
    (
        "bg_aggregation_cycle_latency_ms",
        "Background aggregation cycle latency in milliseconds",
    ),
    (
        "bg_aggregation_cycles_total",
        "Total background aggregation cycles",
    ),
    (
        "bg_aggregation_errors_total",
        "Total background aggregation errors",
    ),
    (
        "bg_last_aggregation_timestamp",
        "Last successful aggregation timestamp (Unix epoch seconds)",
    ),
    (
        "bg_last_cleanup_timestamp",
        "Last successful cleanup timestamp (Unix epoch seconds)",
    ),
    (
        "bm25_delete_latency_ms",
        "BM25 deletion latency in milliseconds",
    ),
    (
        "bm25_documents_deleted_total",
        "Total BM25 documents deleted",
    ),
    (
        "bm25_documents_indexed_total",
        "Total BM25 documents indexed",
    ),
    ("bm25_errors_total", "Total BM25 errors"),
    ("bm25_index_disk_bytes", "BM25 index disk usage in bytes"),
    (
        "bm25_index_latency_ms",
        "BM25 indexing latency in milliseconds",
    ),
    (
        "candidate_reuse_adopted_total",
        "Resumed operations that adopted an existing ready candidate generation",
    ),
    (
        "candidate_reuse_rejected_total",
        "Resumed operations whose candidate was not reusable (fresh clone forced)",
    ),
    ("embedding_batch_size", "Embedding batch size"),
    ("embedding_errors_total", "Total embedding request errors"),
    (
        "embedding_latency_ms",
        "Embedding request latency in milliseconds",
    ),
    ("embedding_requests_total", "Total embedding API requests"),
    ("embedding_retries_total", "Total embedding request retries"),
    ("embedding_tokens_per_batch", "Embedding tokens per batch"),
    (
        "embedding_tokens_total",
        "Total tokens processed by embedding provider",
    ),
    ("entity_changes_total", "Total entity changes detected"),
    (
        "file_processing_total_latency_ms",
        "File processing end-to-end latency in milliseconds",
    ),
    ("files_changed_total", "Total files changed"),
    (
        "files_failed_in_hot_update_total",
        "Total files failed in hot update",
    ),
    ("files_failed_total", "Total files that failed processing"),
    (
        "files_permanently_failed_total",
        "Total files permanently failed after retries",
    ),
    (
        "files_processed_for_relations_total",
        "Total files processed for relation extraction",
    ),
    (
        "files_processed_in_hot_update_total",
        "Total files processed in hot update",
    ),
    ("files_processed_total", "Total files processed"),
    (
        "full_rescan_fallback_total",
        "Total full rescan fallbacks after watch overflow",
    ),
    ("hot_update_cycles_total", "Total hot update cycles"),
    (
        "hot_update_full_rescan_fallback_total",
        "Total hot update full rescan fallbacks",
    ),
    (
        "hot_update_latency_ms",
        "Hot update cycle latency in milliseconds",
    ),
    (
        "hot_update_module_retry_total",
        "Total module failures a later hot-update pass must retry",
    ),
    (
        "hot_update_watch_overflow_total",
        "Total hot update watch queue overflows",
    ),
    (
        "http_active_connections",
        "Current number of active HTTP connections",
    ),
    ("http_errors_total", "Total number of HTTP 5xx errors"),
    (
        "http_request_body_size_bytes",
        "HTTP request body size distribution in bytes",
    ),
    (
        "http_request_duration_ms",
        "HTTP request latency distribution in milliseconds",
    ),
    (
        "http_requests_in_flight",
        "Current number of HTTP requests in flight",
    ),
    ("http_requests_total", "Total number of HTTP requests"),
    (
        "llm_circuit_breaker_rejections_total",
        "Total requests rejected by an open LLM circuit breaker",
    ),
    (
        "llm_circuit_breaker_state",
        "LLM circuit breaker state (0=closed, 0.5=half-open, 1=open)",
    ),
    (
        "llm_circuit_breaker_transitions_total",
        "Total LLM circuit breaker state transitions",
    ),
    (
        "llm_retry_exhausted_total",
        "Total LLM retries exhausted, by error class",
    ),
    (
        "llm_retry_failures_total",
        "Total final LLM failures after retries, by error class",
    ),
    (
        "llm_retry_total",
        "Total LLM retry attempts, by error class",
    ),
    (
        "llm_retry_wait_ms_total",
        "Accumulated LLM retry waiting time in milliseconds",
    ),
    ("operation_queue_depth", "Current operation queue depth"),
    ("parse_attempts_total", "Total code parse attempts"),
    ("parse_errors_total", "Total code parse errors"),
    ("parse_latency_ms", "Code parse latency in milliseconds"),
    (
        "pending_watch_changes",
        "Current pending watch changes depth",
    ),
    (
        "pipeline_chunk_size_bytes",
        "Pipeline chunk size distribution in bytes",
    ),
    ("pipeline_stage_errors_total", "Total pipeline stage errors"),
    (
        "pipeline_stage_latency_ms",
        "Pipeline stage processing latency in milliseconds",
    ),
    (
        "pipeline_stage_output_items_total",
        "Total output items from pipeline stage",
    ),
    (
        "pipeline_stage_processed_total",
        "Total items processed by pipeline stage",
    ),
    (
        "plugin_capability_errors_total",
        "Total plugin capability execution errors",
    ),
    (
        "plugin_capability_executions_total",
        "Total plugin capability executions",
    ),
    (
        "plugin_execution_errors_total",
        "Total plugin execution errors",
    ),
    (
        "plugin_execution_latency_ms",
        "Plugin execution latency in milliseconds",
    ),
    ("plugin_executions_total", "Total plugin executions"),
    ("plugin_load_failures_total", "Total plugin load failures"),
    ("plugin_loads_total", "Total plugin loads"),
    ("plugin_unloads_total", "Total plugin unloads"),
    (
        "project_registry_cache_hits_total",
        "Total project registry cache hits",
    ),
    (
        "project_registry_cache_invalidations_total",
        "Total project registry cache invalidations",
    ),
    (
        "project_registry_cache_misses_total",
        "Total project registry cache misses",
    ),
    (
        "project_registry_cache_size",
        "Current project registry cache size",
    ),
    (
        "project_registry_load_latency_ms",
        "Project load latency in milliseconds",
    ),
    (
        "qdrant_circuit_breaker_state",
        "Qdrant circuit breaker state (0=closed, 1=open, 2=half-open, 3=manual-open)",
    ),
    (
        "qdrant_circuit_breaker_transitions_total",
        "Total Qdrant circuit breaker state transitions",
    ),
    ("qdrant_collection_size", "Qdrant collection vector count"),
    (
        "qdrant_delete_latency_ms",
        "Qdrant deletion latency in milliseconds",
    ),
    ("qdrant_errors_total", "Total Qdrant errors"),
    (
        "qdrant_search_latency_ms",
        "Qdrant search latency in milliseconds",
    ),
    ("qdrant_search_queries_total", "Total Qdrant search queries"),
    (
        "qdrant_upsert_latency_ms",
        "Qdrant upsert latency in milliseconds",
    ),
    (
        "qdrant_vectors_deleted_total",
        "Total Qdrant vectors deleted",
    ),
    (
        "qdrant_vectors_upserted_total",
        "Total Qdrant vectors upserted",
    ),
    ("queries_executed_total", "Total queries executed"),
    ("query_cache_hit_rate", "Query cache hit rate (percentage)"),
    ("query_cache_hits_total", "Total query cache hits"),
    ("query_cache_misses_total", "Total query cache misses"),
    (
        "query_execution_latency_ms",
        "Query execution latency in milliseconds",
    ),
    (
        "query_results_returned_total",
        "Total query results returned",
    ),
    (
        "rechunk_rebuilt_total",
        "Files re-chunked during chunking-drift sweeps",
    ),
    (
        "rechunk_skipped_total",
        "Files skipped by chunking-drift sweeps because on-disk content drifted",
    ),
    (
        "relation_ambiguous_targets_total",
        "Total local call resolutions with multiple viable candidate targets at the same precedence tier",
    ),
    (
        "relation_build_latency_ms",
        "Relation extraction latency in milliseconds",
    ),
    (
        "relation_config_affected_files_total",
        "Total files identified as affected by a config change (extension-based scope)",
    ),
    (
        "relation_config_fine_grained_fallback_total",
        "Total times fine-grained config narrowing fell back to the extension-based set",
    ),
    (
        "relation_config_narrowed_files_total",
        "Total files narrowed by package import intersection",
    ),
    (
        "relation_config_scan_failures_total",
        "Total build configuration scan failures",
    ),
    (
        "relation_delta_export_unresolved_total",
        "Total exports whose stable symbol key could not be resolved during delta application",
    ),
    (
        "relation_edges_dropped_unbounded_total",
        "Total relation edges dropped because their caller file was outside the affected scope of a hot update",
    ),
    (
        "relation_entity_derived_key_total",
        "Total entities exported with a derived symbol key during snapshot building",
    ),
    (
        "relation_import_fallback_total",
        "Total files that fell back to a second AST parse for import extraction",
    ),
    (
        "relation_max_entity_id_scan_fallback_total",
        "Total fallbacks to full scan for max entity ID when the atomic counter is stale",
    ),
    (
        "relation_module_path_conflicts_total",
        "Total module path collisions (same module path claimed by multiple files)",
    ),
    (
        "relation_relation_derived_key_total",
        "Total relation callers/targets exported with a derived symbol key during snapshot building",
    ),
    (
        "relation_resolution_cache_hit_total",
        "Total symbol-table resolution cache hits (positive cache)",
    ),
    (
        "relation_resolution_cache_miss_total",
        "Total symbol-table resolution cache misses (negative cache hits or cache misses)",
    ),
    (
        "relation_resolve_avg_lookups",
        "Average symbol-table resolution lookups per call",
    ),
    (
        "relation_resolve_calls_total",
        "Total symbol-table resolution calls",
    ),
    (
        "relation_resolve_lookups_total",
        "Total symbol-table resolution lookups across all resolution attempts",
    ),
    (
        "relation_self_loop_filtered_total",
        "Total relation edges dropped due to self-loop suppression",
    ),
    (
        "relation_spool_decompress_ms",
        "Relation spool decompression latency in milliseconds",
    ),
    (
        "relation_spool_replay_bytes_total",
        "Total bytes replayed from the relation spool",
    ),
    (
        "relation_spool_replay_passes_total",
        "Total number of spool replay passes",
    ),
    (
        "relation_stdlib_filtered_total",
        "Total unresolved relations dropped because their callee looked like a standard library name",
    ),
    (
        "relation_stdlib_preserved_external_total",
        "Total unresolved standard-library-like relations preserved as external calls",
    ),
    (
        "relation_symbol_key_conflicts_total",
        "Total stable symbol key collisions (same key registered to a different entity)",
    ),
    (
        "relation_symbol_table_incremental_total",
        "Total incremental symbol table updates",
    ),
    (
        "relation_symbol_table_rebuild_total",
        "Total symbol table rebuilds",
    ),
    (
        "relation_truncated_total",
        "Total relations dropped because the per-file budget was exceeded",
    ),
    (
        "relation_unresolved_ratio",
        "Ratio of unresolved relations over extracted relations (0..=1)",
    ),
    (
        "relation_unresolved_total",
        "Total relations left unresolved, bucketed by reason",
    ),
    (
        "relations_extracted_total",
        "Total relations extracted from code",
    ),
    (
        "rerank_candidates_total",
        "Total rerank candidates processed",
    ),
    ("rerank_errors_total", "Total rerank request errors"),
    (
        "rerank_latency_ms",
        "Rerank request latency in milliseconds",
    ),
    ("rerank_requests_total", "Total rerank requests"),
    ("rerank_retries_total", "Total rerank request retries"),
    ("retry_queue_depth", "Current retry queue depth"),
    (
        "retry_queue_failed_total",
        "Total retry queue entries failed",
    ),
    (
        "retry_queue_processed_total",
        "Total retry queue entries processed",
    ),
    (
        "scanner_files_filtered_total",
        "Total files filtered by scanner",
    ),
    (
        "scanner_files_hash_reused_total",
        "Total files whose content hash was reused from a previous scan (unchanged size and mtime)",
    ),
    ("scanner_files_scanned_total", "Total files scanned"),
    (
        "scanner_files_skipped_total",
        "Total files skipped by scanner",
    ),
    (
        "scanner_languages_detected_total",
        "Total language detections by scanner",
    ),
    (
        "scanner_scan_latency_ms",
        "Scanner scan latency in milliseconds",
    ),
    ("search_documents_indexed_total", "Total documents indexed"),
    (
        "search_hybrid_alignment_match_ratio",
        "Hybrid search alignment match ratio",
    ),
    ("search_index_operations_total", "Total indexing operations"),
    ("search_index_size", "Current index size in documents"),
    ("search_queries_total", "Total search queries"),
    (
        "search_query_latency_ms",
        "Search query latency in milliseconds",
    ),
    ("sqlite_errors_total", "Total SQLite errors"),
    (
        "sqlite_read_transactions_total",
        "Total SQLite read transactions",
    ),
    (
        "sqlite_transaction_latency_ms",
        "SQLite transaction latency in milliseconds",
    ),
    (
        "sqlite_write_transactions_total",
        "Total SQLite write transactions",
    ),
    ("summaries_generated_total", "Total summaries generated"),
    ("summary_avg_length", "Average summary length in characters"),
    (
        "summary_generation_latency_ms",
        "Summary generation latency in milliseconds",
    ),
    (
        "summary_model_enhancement_errors_total",
        "Total model enhancement errors",
    ),
    (
        "summary_model_enhancement_latency_ms",
        "Model enhancement latency in milliseconds",
    ),
    (
        "summary_model_enhancement_requests_total",
        "Total model enhancement requests",
    ),
    ("system_cpu_usage_percent", "System CPU usage percentage"),
    ("system_disk_read_bytes", "Total bytes read from disk"),
    ("system_disk_write_bytes", "Total bytes written to disk"),
    ("system_memory_total_bytes", "Total system memory in bytes"),
    (
        "system_memory_usage_percent",
        "System memory usage percentage",
    ),
    ("system_memory_used_bytes", "System memory used in bytes"),
    ("system_net_recv_bytes", "Total bytes received over network"),
    ("system_net_sent_bytes", "Total bytes sent over network"),
    ("system_swap_total_bytes", "Total system swap in bytes"),
    ("system_swap_used_bytes", "System swap used in bytes"),
    ("tokio_active_tasks", "Number of active Tokio tasks"),
    ("tokio_global_queue_depth", "Tokio global queue depth"),
    (
        "tokio_worker_busy_duration_ms",
        "Tokio worker busy duration in milliseconds",
    ),
    ("tokio_workers_total", "Number of Tokio worker threads"),
    (
        "type_member_duplicate_total",
        "Total type-member entries that conflicted with an existing registration",
    ),
    (
        "type_member_hit_total",
        "Total type-member lookups that found a matching member",
    ),
    (
        "type_member_lookup_total",
        "Total type-member lookups performed during relation resolution",
    ),
    (
        "type_member_miss_total",
        "Total type-member lookups that found no matching member",
    ),
    ("watch_active", "Whether the file watcher is active"),
    ("watch_config_events_total", "Total config watch events"),
    ("watch_dir_events_total", "Total directory watch events"),
    ("watch_events_total", "Total file watch events received"),
    (
        "watch_failed_events_total",
        "Total failed watch event forwardings",
    ),
    ("watch_file_events_total", "Total file watch events"),
    ("watch_filtered_events_total", "Total filtered watch events"),
    (
        "watch_forwarded_events_total",
        "Total forwarded watch events",
    ),
    ("watch_overflow_total", "Total watch queue overflows"),
    ("watch_status_code", "Current watcher status code"),
    ("watch_watched_paths", "Number of watched paths"),
    (
        "work_unit_committed_total",
        "Work units committed after successful processing",
    ),
    (
        "work_unit_skip_committed_total",
        "Work units skipped on resume because they were already committed",
    ),
    (
        "work_unit_uncommitted_total",
        "Work units left uncommitted after retryable failures",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_descriptions_sorted_for_binary_search() {
        for pair in DESCRIPTIONS.windows(2) {
            assert!(
                pair[0].0 < pair[1].0,
                "descriptions must be sorted: {} after {}",
                pair[1].0,
                pair[0].0
            );
        }
    }

    #[test]
    fn test_metric_description_lookup() {
        assert_eq!(
            metric_description("http_requests_total"),
            Some("Total number of HTTP requests")
        );
        assert_eq!(
            metric_description("embedding_latency_ms"),
            Some("Embedding request latency in milliseconds")
        );
        assert_eq!(metric_description("unknown_metric"), None);
    }
}
