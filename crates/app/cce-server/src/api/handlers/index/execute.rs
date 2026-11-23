//! Execute index handler
//!
//! This module provides handlers for full index execution.

use axum::{Json, extract::State};
use std::path::PathBuf;

use cce_api::models::{ErrorResponse, IndexRequest, IndexResponse, IndexStageTimingsDto};
use cce_orchestrator::IndexResult;
use cce_types::error_codes;

use crate::api::response::ApiResult;

fn index_stages_dto(result: &IndexResult) -> IndexStageTimingsDto {
    let stages = &result.stages;
    IndexStageTimingsDto {
        scan_ms: stages.scan_ms,
        batch_process_ms: stages.batch_process_ms,
        batch_setup_ms: stages.batch_setup_ms,
        entity_store_ms: stages.entity_store_ms,
        vector_store_ms: stages.vector_store_ms,
        bm25_store_ms: stages.bm25_store_ms,
        summary_ms: stages.summary_ms,
        relation_ms: stages.relation_ms,
        export_ms: stages.export_ms,
        relation_spool_ms: stages.relation_spool_ms,
        relation_spool_write_ms: stages.relation_spool_write_ms,
        relation_symbols_ms: stages.relation_symbols_ms,
        symbol_modules_ms: stages.symbol_modules_ms,
        symbol_type_index_ms: stages.symbol_type_index_ms,
        symbol_overload_ms: stages.symbol_overload_ms,
        symbol_flat_ms: stages.symbol_flat_ms,
        symbol_cross_file_ms: stages.symbol_cross_file_ms,
        export_spool_ms: stages.export_spool_ms,
        manifest_ms: stages.manifest_ms,
        batch_overhead_ms: stages.batch_overhead_ms,
        state_projection_ms: stages.state_projection_ms,
        checkpoint_ms: stages.checkpoint_ms,
        progress_ms: stages.progress_ms,
        batch_dispatch_ms: stages.batch_dispatch_ms,
        write_commit_ms: stages.write_commit_ms,
        batches: stages.batches,
        vector_store_calls: stages.vector_store_calls,
        entity_store_calls: stages.entity_store_calls,
        bm25_store_calls: stages.bm25_store_calls,
        vector_txns_applied: stages.vector_txns_applied,
        vector_points_written: stages.vector_points_written,
        vector_wal_flushes: stages.vector_wal_flushes,
        vector_wal_txns: stages.vector_wal_txns,
        vector_commit_wait_ms: stages.vector_commit_wait_ms,
        vector_hnsw_builds: stages.vector_hnsw_builds,
        vector_hnsw_build_ms: stages.vector_hnsw_build_ms,
        vector_checkpoints: stages.vector_checkpoints,
        vector_checkpoint_ms: stages.vector_checkpoint_ms,
        bm25_commits: stages.bm25_commits,
        bm25_documents: stages.bm25_documents,
        bm25_document_ms: stages.bm25_document_ms,
        bm25_commit_ms: stages.bm25_commit_ms,
        bm25_refresh_ms: stages.bm25_refresh_ms,
        bm25_merge_ms: stages.bm25_merge_ms,
        bm25_staged_documents: stages.bm25_staged_documents,
        rss_after_scan_kb: stages.rss_after_scan_kb,
        rss_after_batches_kb: stages.rss_after_batches_kb,
        rss_after_relation_kb: stages.rss_after_relation_kb,
        rss_final_kb: stages.rss_final_kb,
        rss_peak_batch_kb: stages.rss_peak_batch_kb,
        vector_pending_points_max: stages.vector_pending_points_max,
        bm25_pending_documents_max: stages.bm25_pending_documents_max,
        vector_embed_ms: stages.vector_embed_ms,
        vector_point_build_ms: stages.vector_point_build_ms,
        vector_metadata_ms: stages.vector_metadata_ms,
        vector_work_unit_ms: stages.vector_work_unit_ms,
        vector_flush_ms: stages.vector_flush_ms,
        vector_stage_push_ms: stages.vector_stage_push_ms,
        vector_entity_lookup_ms: stages.vector_entity_lookup_ms,
        vector_delay_ms: stages.vector_delay_ms,
        vector_work_unit_queries: stages.vector_work_unit_queries,
        vector_work_unit_query_ms: stages.vector_work_unit_query_ms,
        vector_microbatches: stages.vector_microbatches,
        vector_chunk_rows: stages.vector_chunk_rows,
        vector_entity_mappings: stages.vector_entity_mappings,
        vector_staged_records_max: stages.vector_staged_records_max,
        relation_spool_entries: stages.relation_spool_entries,
        relation_spool_bytes: stages.relation_spool_bytes,
        symbol_table_entries: stages.symbol_table_entries,
        symbol_table_estimated_bytes: stages.symbol_table_estimated_bytes,
        relation_nodes: stages.relation_nodes,
        relation_edges: stages.relation_edges,
        relation_estimated_bytes: stages.relation_estimated_bytes,
        relation_snapshot_entries: stages.relation_snapshot_entries,
        relation_snapshot_estimated_bytes: stages.relation_snapshot_estimated_bytes,
        export_spool_entries: stages.export_spool_entries,
        export_spool_bytes: stages.export_spool_bytes,
        entity_id_map_entries: stages.entity_id_map_entries,
        entity_id_map_estimated_bytes: stages.entity_id_map_estimated_bytes,
    }
}

fn index_response_from_result(result: IndexResult) -> IndexResponse {
    let has_errors = !result.errors().is_empty();
    IndexResponse {
        success: result.is_success(),
        files_scanned: result.total_files,
        files_indexed: result.indexed_files,
        failed_files: result.failed_files,
        degraded_files: result.degraded_files,
        skipped_permanent: result.skipped_permanent,
        circuit_open: result.circuit_open,
        total_entities: result.total_entities,
        total_relations: result.total_relations,
        total_vectors: result.total_vectors,
        elapsed_ms: result.elapsed_ms,
        stages: index_stages_dto(&result),
        message: if !has_errors {
            format!(
                "Indexing completed, a total of {} files were processed, {} entities were extracted, and {} relationships were identified.",
                result.indexed_files, result.total_entities, result.total_relations
            )
        } else {
            format!(
                "The indexing process is complete. {} files were successful, and {} files failed.",
                result.indexed_files, result.failed_files
            )
        },
        errors: result.errors().to_vec(),
    }
}

/// Handle index request
#[axum::debug_handler]
#[utoipa::path(
    post, path = "/api/index", tag = "Index",
    request_body = IndexRequest,
    responses(
        (status = 200, body = IndexResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_index(
    State(state): State<cce_server_shared::context::SharedContext>,
    Json(query): Json<IndexRequest>,
) -> ApiResult<IndexResponse> {
    // Validate project_id
    if let Err(e) = crate::api::validation::validate_project_id(query.project_id) {
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            format!("Invalid project_id: {}", e),
        ));
    }

    // Validate root directory
    let root_dir = PathBuf::from(&query.path);
    if !root_dir.exists() {
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            format!("Directory does not exist: {}", query.path),
        ));
    }

    if !root_dir.is_dir() {
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            format!("The path is not a directory: {}", query.path),
        ));
    }

    // Build index options via the shared construction path.
    let options = cce_server_shared::services::index::build_index_options(&query);

    // Execute indexing with project_id using engine's index method
    let result = match state.engine.index(query.project_id, options).await {
        Ok(r) => r,
        Err(e) => {
            return ApiResult::Error(ErrorResponse::with_details(
                error_codes::INTERNAL_ERROR,
                "Index execution failed",
                e.to_string(),
            ));
        }
    };

    ApiResult::Success(index_response_from_result(result))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_response_from_result() {
        let result = IndexResult {
            total_files: 10,
            indexed_files: 9,
            failed_files: 1,
            total_entities: 100,
            total_relations: 50,
            total_vectors: 150,
            elapsed_ms: 1000,
            ..Default::default()
        };

        let response = index_response_from_result(result);
        assert!(response.success);
        assert_eq!(response.files_scanned, 10);
        assert_eq!(response.files_indexed, 9);
        assert_eq!(response.failed_files, 1);
        assert_eq!(response.degraded_files, 0);
        assert_eq!(response.skipped_permanent, 0);
        assert!(!response.circuit_open);
    }

    #[test]
    fn test_index_response_carries_degradation_signals() {
        let result = IndexResult {
            total_files: 10,
            indexed_files: 8,
            failed_files: 1,
            degraded_files: 3,
            skipped_permanent: 2,
            circuit_open: true,
            ..Default::default()
        };

        let response = index_response_from_result(result);
        assert_eq!(response.degraded_files, 3);
        assert_eq!(response.skipped_permanent, 2);
        assert!(response.circuit_open);
    }
}
