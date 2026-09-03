//! Compression tool handler
//!
//! Provides semantic compression for code files, converting AST to natural language
//! for large monolithic files. This is an on-demand operation without side effects.

use axum::{Json, extract::State};
use serde::Serialize;

use cce_api::models::{BatchCompressRequest, CompressRequest};
use cce_orchestrator::{BatchCompressionRequest, CompressionRequest, CompressionResponse};

use crate::api::AppState;

/// Single file compression response
#[derive(Debug, Serialize)]
pub struct CompressApiResponse {
    /// Whether the operation succeeded
    pub success: bool,
    /// Compression result (if successful)
    #[serde(flatten)]
    pub data: Option<CompressionResponse>,
    /// Error message (if failed)
    pub error: Option<String>,
}

/// Batch compression response
#[derive(Debug, Serialize)]
pub struct CompressBatchApiResponse {
    /// Successful compressions
    pub successes: Vec<(String, CompressionResponse)>,
    /// Failed compressions with error messages
    pub failures: Vec<(String, String)>,
}

/// Handle single file compression
///
/// # Endpoint
///
/// `POST /api/tools/compress`
pub async fn handle_compress(
    State(state): State<AppState>,
    Json(request): Json<CompressRequest>,
) -> Json<CompressApiResponse> {
    let retrieval = match &state.compression_retrieval {
        Some(r) => r,
        None => {
            return Json(CompressApiResponse {
                success: false,
                data: None,
                error: Some("Compression tool not initialized".to_string()),
            });
        }
    };

    let req = CompressionRequest {
        file_path: request.file_path,
        include_entities: request.include_entities,
        include_groups: request.include_groups,
    };

    match retrieval.compress(req).await {
        Ok(response) => Json(CompressApiResponse {
            success: true,
            data: Some(response),
            error: None,
        }),
        Err(e) => Json(CompressApiResponse {
            success: false,
            data: None,
            error: Some(e.to_string()),
        }),
    }
}

/// Handle batch file compression
///
/// # Endpoint
///
/// `POST /api/tools/compress/batch`
pub async fn handle_compress_batch(
    State(state): State<AppState>,
    Json(request): Json<BatchCompressRequest>,
) -> Json<CompressBatchApiResponse> {
    let retrieval = match &state.compression_retrieval {
        Some(r) => r,
        None => {
            return Json(CompressBatchApiResponse {
                successes: Vec::new(),
                failures: request
                    .file_paths
                    .into_iter()
                    .map(|p| (p, "Compression tool not initialized".to_string()))
                    .collect(),
            });
        }
    };

    let req = BatchCompressionRequest {
        file_paths: request.file_paths,
        include_entities: request.include_entities.unwrap_or(false),
        include_groups: request.include_groups.unwrap_or(false),
        max_concurrency: request.max_concurrency,
    };

    let result = retrieval.compress_batch(req).await;

    Json(CompressBatchApiResponse {
        successes: result.successes,
        failures: result
            .failures
            .into_iter()
            .map(|(path, err)| (path, err.to_string()))
            .collect(),
    })
}
