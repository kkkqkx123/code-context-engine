//! Compression tool handler
//!
//! Provides semantic compression for code files, converting AST to natural language
//! for large monolithic files. This is an on-demand operation without side effects.

use axum::Json;

use cce_api::models::{
    BatchCompressFailure, BatchCompressRequest, BatchCompressResponse, BatchCompressSuccess,
    CompressApiResponse, CompressRequest, CompressResult,
};
use cce_server_shared::services::tools::{self, BatchCompressSpec, CompressSpec};

fn to_compress_result(response: cce_orchestrator::CompressionResponse) -> CompressResult {
    CompressResult {
        file_path: response.file_path,
        language: response.language,
        file_hash: response.file_hash,
        from_cache: response.from_cache,
        entities: response.entities.and_then(|v| serde_json::to_value(v).ok()),
        groups: response.groups.and_then(|v| serde_json::to_value(v).ok()),
        semantic_text: response.semantic_text,
    }
}

/// Handle single file compression
///
/// # Endpoint
///
/// `POST /api/tools/compress`
#[utoipa::path(
    post, path = "/api/tools/compress", tag = "Tools",
    request_body = CompressRequest,
    responses(
        (status = 200, body = CompressApiResponse, description = "Compression result, errors reported in-band")
    )
)]
pub async fn handle_compress(Json(request): Json<CompressRequest>) -> Json<CompressApiResponse> {
    let spec = CompressSpec {
        file_path: request.file_path,
        include_entities: request.include_entities,
        include_groups: request.include_groups,
    };
    match tools::compress(spec).await {
        Ok(response) => Json(CompressApiResponse {
            success: true,
            result: Some(to_compress_result(response)),
            error: None,
        }),
        Err(error) => Json(CompressApiResponse {
            success: false,
            result: None,
            error: Some(error.to_string()),
        }),
    }
}

/// Handle batch file compression
///
/// # Endpoint
///
/// `POST /api/tools/compress/batch`
#[utoipa::path(
    post, path = "/api/tools/compress/batch", tag = "Tools",
    request_body = BatchCompressRequest,
    responses(
        (status = 200, body = BatchCompressResponse, description = "Batch compression result, errors reported in-band")
    )
)]
pub async fn handle_compress_batch(
    Json(request): Json<BatchCompressRequest>,
) -> Json<BatchCompressResponse> {
    let result = tools::compress_batch(BatchCompressSpec {
        file_paths: request.file_paths,
        include_entities: request.include_entities.unwrap_or(false),
        include_groups: request.include_groups.unwrap_or(false),
        max_concurrency: request.max_concurrency,
    })
    .await;

    Json(BatchCompressResponse {
        successes: result
            .successes
            .into_iter()
            .map(|(path, response)| BatchCompressSuccess {
                path,
                result: to_compress_result(response),
            })
            .collect(),
        failures: result
            .failures
            .into_iter()
            .map(|(path, err)| BatchCompressFailure {
                path,
                error: err.to_string(),
            })
            .collect(),
    })
}
