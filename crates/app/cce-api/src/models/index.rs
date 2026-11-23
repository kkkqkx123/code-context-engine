//! Index management models

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Index request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct IndexRequest {
    pub project_id: i64,
    pub path: String,
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub exclude_dirs: Vec<String>,
    #[serde(default = "default_true")]
    pub respect_gitignore: bool,
    /// Additional ignore patterns (gitignore-style)
    #[serde(default)]
    pub ignore_patterns: Vec<String>,
    /// Path to custom gitignore file
    #[serde(default)]
    pub custom_gitignore: Option<String>,
}

/// Wall-clock accounting for one full-index run.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Default)]
pub struct IndexStageTimingsDto {
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

/// Index response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct IndexResponse {
    pub success: bool,
    pub files_scanned: usize,
    pub files_indexed: usize,
    pub failed_files: usize,
    /// Files that completed "successfully" but produced degraded content
    /// (lossy decode, zero entities/chunks, or a transient summary fallback).
    #[serde(default)]
    pub degraded_files: usize,
    /// Files with deterministic failures skipped without retry.
    #[serde(default)]
    pub skipped_permanent: usize,
    /// Whether a backend circuit breaker (vector store or LLM service)
    /// opened during this run.
    #[serde(default)]
    pub circuit_open: bool,
    pub total_entities: usize,
    pub total_relations: usize,
    pub total_vectors: usize,
    pub elapsed_ms: u64,
    /// Per-stage accounting for the run.
    #[serde(default)]
    pub stages: IndexStageTimingsDto,
    pub message: String,
    #[serde(default)]
    pub errors: Vec<String>,
}

/// Incremental index request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct IncrementalIndexRequest {
    pub project_id: i64,
    #[serde(default)]
    pub files_to_index: Vec<String>,
    #[serde(default)]
    pub files_to_remove: Vec<String>,
    #[serde(default)]
    pub force_reindex: bool,
}

/// Incremental index response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct IncrementalIndexResponse {
    pub success: bool,
    pub files_indexed: usize,
    pub files_removed: usize,
    pub total_entities: usize,
    pub total_vectors: usize,
    pub elapsed_ms: u64,
    #[serde(default)]
    pub errors: Vec<String>,
}

/// Parse request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ParseRequest {
    pub file_path: String,
}

/// Parse response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ParseResponse {
    pub success: bool,
    pub file_path: String,
    pub language: String,
    pub encoding: String,
    pub entities: Vec<EntityInfo>,
    pub relations: Vec<RelationInfo>,
    pub elapsed_ms: u64,
}

/// Entity info from parse
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct EntityInfo {
    pub id: u64,
    pub kind: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    pub start_line: u32,
    pub end_line: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc_comment: Option<String>,
}

/// Relation info from parse
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RelationInfo {
    pub caller_id: u64,
    pub callee_id: u64,
    pub relation_type: String,
    pub line: u32,
}

/// Index statistics response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct IndexStatsResponse {
    pub success: bool,
    pub statistics: IndexStatistics,
    pub elapsed_ms: u64,
}

/// Index statistics
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct IndexStatistics {
    pub total_entities: usize,
    pub total_relations: usize,
    pub total_vectors: usize,
    pub total_bm25_documents: usize,
    pub total_files: usize,
}

/// Clear index request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ClearIndexRequest {
    pub project_id: i64,
    #[serde(default)]
    pub vectors: bool,
    #[serde(default)]
    pub bm25: bool,
    #[serde(default)]
    pub relations: bool,
    #[serde(default)]
    pub cache: bool,
}

/// Clear index response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ClearIndexResponse {
    pub success: bool,
    pub project_id: i64,
    pub backends: Vec<BackendResultInfo>,
    pub elapsed_ms: u64,
    pub message: String,
}

/// Backend result info
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BackendResultInfo {
    pub backend: String,
    pub ok: bool,
    pub detail: String,
}

/// Delete file response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DeleteFileResponse {
    pub success: bool,
    pub message: String,
    pub file_path: String,
    pub vectors_deleted: usize,
    pub bm25_documents_deleted: usize,
    pub relations_deleted: usize,
    pub elapsed_ms: u64,
}

/// Delete entity response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DeleteEntityResponse {
    pub success: bool,
    pub message: String,
    pub entity_id: u64,
    pub vectors_deleted: usize,
    pub bm25_documents_deleted: usize,
    pub relations_deleted: usize,
    pub elapsed_ms: u64,
}

/// Batch delete request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BatchDeleteRequest {
    #[serde(default)]
    pub file_paths: Vec<String>,
    #[serde(default)]
    pub entity_ids: Vec<u64>,
}

/// Batch delete response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BatchDeleteResponse {
    pub success: bool,
    pub files_deleted: usize,
    pub entities_deleted: usize,
    #[serde(default)]
    pub errors: Vec<String>,
    pub elapsed_ms: u64,
}

/// Summary request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SummaryRequest {
    #[serde(default)]
    pub file_paths: Vec<String>,
    #[serde(default)]
    pub directory_paths: Vec<String>,
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub exclude_dirs: Vec<String>,
    #[serde(default = "default_true")]
    pub respect_gitignore: bool,
    #[serde(default)]
    pub ignore_patterns: Vec<String>,
    #[serde(default = "default_true")]
    pub recursive: bool,
    #[serde(default = "default_max_files")]
    pub max_files: usize,
}

/// Summary response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SummaryResponse {
    pub success: bool,
    pub total_files: usize,
    pub success_count: usize,
    pub failed_count: usize,
    pub summaries: Vec<FileSummaryItem>,
    pub elapsed_ms: u64,
    #[serde(default)]
    pub warnings: Vec<String>,
}

/// File summary item
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct FileSummaryItem {
    pub file_path: String,
    pub language: String,
    pub summary: String,
    pub main_entities: Vec<String>,
    pub imports: Vec<String>,
    pub exports: Vec<String>,
    pub entity_count: u32,
    pub line_count: u32,
    pub tags: Vec<String>,
    pub importance_level: String,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn default_true() -> bool {
    true
}

/// Per-module failure summary of a dead-lettered file.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DeadLetterModuleEntry {
    /// Module name (relation / summary / embedding / bm25 / export).
    pub module: String,
    /// Retry attempts already made.
    pub retry_count: u32,
    /// Stable error code of the last failure, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    /// Human-readable failure message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    /// Whether the module already went through a lossy truncate-retry.
    pub truncated: bool,
    /// Whether an operator acknowledged this dead letter (it no longer
    /// participates in retry passes).
    pub acknowledged: bool,
}

/// One dead-lettered file in the project dead-letter list.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DeadLetterFileEntry {
    /// Recorded file path.
    pub file_path: String,
    /// Index version the dead letter belongs to.
    pub version: u64,
    /// Modules currently in dead-letter state.
    pub modules: Vec<DeadLetterModuleEntry>,
    /// Last update timestamp (RFC3339).
    pub updated_at: String,
}

/// Response of `GET /api/project/{project_id}/dead-letters`.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DeadLetterListResponse {
    pub project_id: i64,
    pub files: Vec<DeadLetterFileEntry>,
}

/// Response of the dead-letter retry / acknowledge actions.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DeadLetterActionResponse {
    pub success: bool,
    /// Files affected by the action (retained entries after acknowledge).
    pub affected: usize,
    pub message: String,
}

/// Request body of `POST /api/project/{project_id}/dead-letters/retry`.
#[derive(Debug, Default, Serialize, Deserialize, ToSchema)]
pub struct DeadLetterRetryRequest {
    /// Restrict the retry pass to these file paths; empty means all candidates.
    #[serde(default)]
    pub files: Vec<String>,
}

/// Request body of `POST /api/project/{project_id}/dead-letters/acknowledge`.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DeadLetterAcknowledgeRequest {
    /// File path to acknowledge.
    pub file_path: String,
    /// Restrict acknowledgement to one module; omitted means all modules.
    #[serde(default)]
    pub module: Option<String>,
}

fn default_max_files() -> usize {
    100
}
