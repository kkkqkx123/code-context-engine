//! Health monitoring and retry queue management handlers
//!
//! Provides endpoints for:
//! - Unified health check across all external services
//! - Per-service detailed diagnostics
//! - Retry queue inspection and manual processing

use axum::extract::State;

use crate::api::response::ApiResult;
use crate::api::state::AppState;
use cce_api::models::{
    Bm25HealthResponse, EmbeddingHealthResponse, ErrorResponse, HealthStatus, QdrantDiagnostic,
    QdrantHealthResponse, RetryQueueClearResponse, RetryQueueDeadClearResponse,
    RetryQueueDeadEntry, RetryQueueDeadResponse, RetryQueueProcessResponse,
    RetryQueueStatusResponse, ServiceStatus, error_codes,
};
use cce_storage_common::VectorStorage;

// --- Handlers ---

/// GET /api/health — Aggregate health of all external services
#[utoipa::path(
    get, path = "/api/health", tag = "Health",
    responses(
        (status = 200, body = HealthStatus, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 503, body = ErrorResponse, description = "Index unavailable"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_health(State(state): State<AppState>) -> ApiResult<HealthStatus> {
    let qdrant_health = check_qdrant(&state).await;
    let bm25_health = check_bm25(&state).await;
    let embedding_health = check_embedding(&state);

    let all_healthy =
        qdrant_health.reachable && bm25_health.reachable && embedding_health.reachable;

    ApiResult::Success(HealthStatus {
        healthy: all_healthy,
        qdrant: qdrant_health,
        bm25: bm25_health,
        embedding: embedding_health,
    })
}

/// GET /api/health/qdrant — Qdrant detailed diagnostic
#[utoipa::path(
    get, path = "/api/health/qdrant", tag = "Health",
    responses(
        (status = 200, body = QdrantHealthResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 503, body = ErrorResponse, description = "Index unavailable"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_qdrant_health(
    State(state): State<AppState>,
) -> ApiResult<QdrantHealthResponse> {
    let vector = state.engine.vector();
    let circuit_breaker = vector.circuit_breaker_summary();

    let diag = vector.diagnose_summary().await;
    let diagnostic = QdrantDiagnostic {
        reachable: diag.reachable,
        version: diag.version,
        collection_exists: diag.collection_exists,
        points_count: diag.points_count,
        error: diag.error,
    };

    let healthy = diagnostic.reachable;

    ApiResult::Success(QdrantHealthResponse {
        healthy,
        circuit_breaker,
        diagnostic,
    })
}

/// GET /api/health/embedding — Embedding service health
#[utoipa::path(
    get, path = "/api/health/embedding", tag = "Health",
    responses(
        (status = 200, body = EmbeddingHealthResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 503, body = ErrorResponse, description = "Index unavailable"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_embedding_health(
    State(state): State<AppState>,
) -> ApiResult<EmbeddingHealthResponse> {
    let (healthy, model_name, message) = {
        let embedder = state.engine.embedder();
        let healthy = embedder.is_healthy();
        let model_name = Some(embedder.model_name().to_string());
        let message = if healthy {
            format!("Embedding provider '{}' is healthy", embedder.model_name())
        } else {
            format!(
                "Embedding provider '{}' is unhealthy",
                embedder.model_name()
            )
        };
        (healthy, model_name, message)
    };

    ApiResult::Success(EmbeddingHealthResponse {
        healthy,
        model_name,
        message,
    })
}

/// GET /api/health/bm25 — BM25 index health
#[utoipa::path(
    get, path = "/api/health/bm25", tag = "Health",
    responses(
        (status = 200, body = Bm25HealthResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 503, body = ErrorResponse, description = "Index unavailable"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_bm25_health(State(state): State<AppState>) -> ApiResult<Bm25HealthResponse> {
    // Status stays branch-free: reachability comes from the diagnostics
    // snapshot, enablement from the contract, and the configured index name
    // from the active branch configuration.
    use cce_storage_common::FulltextStorage;
    let store = state.engine.fulltext().clone();
    let diag = store.diagnose_summary().await;
    let enabled = FulltextStorage::is_enabled(&store);
    let index_path = Some(store.configured_index_name());

    ApiResult::Success(Bm25HealthResponse {
        enabled,
        connected: diag.reachable,
        index_path,
    })
}

/// GET /api/retry-queue — View retry queue status (aggregated across all projects)
#[utoipa::path(
    get, path = "/api/retry-queue", tag = "Health",
    responses(
        (status = 200, body = RetryQueueStatusResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_retry_queue_status(
    State(state): State<AppState>,
) -> ApiResult<RetryQueueStatusResponse> {
    let pending_count = state.engine.retry_queue_total_len().await;
    let dead_count = state.engine.retry_queue_total_dead_len().await;
    ApiResult::Success(RetryQueueStatusResponse {
        pending_count,
        is_empty: pending_count == 0,
        dead_count,
    })
}

/// POST /api/retry-queue/process — Manually trigger retry queue processing
///
/// Drains all queries that are ready for retry (cooldown expired)
/// and re-executes them. Returns the number of queries re-attempted.
#[utoipa::path(
    post, path = "/api/retry-queue/process", tag = "Health",
    responses(
        (status = 200, body = RetryQueueProcessResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_retry_queue_process(
    State(state): State<AppState>,
) -> ApiResult<RetryQueueProcessResponse> {
    let count = match state.engine.process_retry_queue(1).await {
        Ok(count) => count,
        Err(e) => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                format!("Failed to process retry queue: {}", e),
            ));
        }
    };
    ApiResult::Success(RetryQueueProcessResponse {
        processed: count,
        message: format!(
            "Retry queue processing complete, {} queries re-attempted",
            count
        ),
    })
}

/// DELETE /api/retry-queue — Clear all retry queues across all projects
#[utoipa::path(
    delete, path = "/api/retry-queue", tag = "Health",
    responses(
        (status = 200, body = RetryQueueClearResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_retry_queue_clear(
    State(state): State<AppState>,
) -> ApiResult<RetryQueueClearResponse> {
    let pending = state.engine.retry_queue_total_len().await;
    state.engine.clear_all_retry_queues().await;
    ApiResult::Success(RetryQueueClearResponse {
        cleared: pending,
        message: format!("Retry queue cleared, {} queries discarded", pending),
    })
}

/// GET /api/retry-queue/dead — Snapshot of dead-lettered queries
#[utoipa::path(
    get, path = "/api/retry-queue/dead", tag = "Health",
    responses(
        (status = 200, body = RetryQueueDeadResponse, description = "Success"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_retry_queue_dead_list(
    State(state): State<AppState>,
) -> ApiResult<RetryQueueDeadResponse> {
    let entries = state.engine.retry_queue_dead_snapshot().await;
    let dead_count = entries.len();
    ApiResult::Success(RetryQueueDeadResponse {
        dead_count,
        entries: entries
            .into_iter()
            .map(|(query, retry_count)| RetryQueueDeadEntry { query, retry_count })
            .collect(),
    })
}

/// DELETE /api/retry-queue/dead — Discard the dead-letter lists
#[utoipa::path(
    delete, path = "/api/retry-queue/dead", tag = "Health",
    responses(
        (status = 200, body = RetryQueueDeadClearResponse, description = "Success"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_retry_queue_dead_clear(
    State(state): State<AppState>,
) -> ApiResult<RetryQueueDeadClearResponse> {
    let cleared = state.engine.clear_all_retry_queue_dead().await;
    ApiResult::Success(RetryQueueDeadClearResponse {
        cleared,
        message: format!(
            "Retry queue dead list cleared, {} entries discarded",
            cleared
        ),
    })
}

// --- Internal helpers ---

async fn check_qdrant(state: &AppState) -> ServiceStatus {
    let vector = state.engine.vector();
    let backend = vector.backend_name();
    let diag = vector.diagnose_summary().await;
    if diag.reachable {
        ServiceStatus {
            reachable: true,
            message: format!("Vector store ({backend}) is reachable and healthy"),
        }
    } else {
        let detail = diag
            .error
            .unwrap_or_else(|| "health check failed".to_string());
        ServiceStatus {
            reachable: false,
            message: format!("Vector store ({backend}) health check failed: {detail}"),
        }
    }
}

async fn check_bm25(state: &AppState) -> ServiceStatus {
    let diag = state.engine.fulltext().diagnose_summary().await;
    if diag.reachable {
        ServiceStatus {
            reachable: true,
            message: "BM25 is enabled and connected".to_string(),
        }
    } else {
        let detail = diag
            .error
            .unwrap_or_else(|| "BM25 remote branch is not supported".to_string());
        ServiceStatus {
            reachable: false,
            message: detail,
        }
    }
}

fn check_embedding(state: &AppState) -> ServiceStatus {
    let embedder = state.engine.embedder();
    let healthy = embedder.is_healthy();
    let model_name = Some(embedder.model_name().to_string());

    ServiceStatus {
        reachable: healthy,
        message: if healthy {
            format!(
                "Embedding provider '{}' is healthy",
                model_name.as_deref().unwrap_or("unknown")
            )
        } else if let Some(model_name) = model_name {
            format!("Embedding provider '{}' is unhealthy", model_name)
        } else {
            "Embedding provider not configured".to_string()
        },
    }
}
