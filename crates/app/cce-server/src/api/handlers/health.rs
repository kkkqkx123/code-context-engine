//! Health monitoring and retry queue management handlers
//!
//! Provides endpoints for:
//! - Unified health check across all external services
//! - Per-service detailed diagnostics
//! - Retry queue inspection and manual processing

use axum::extract::{Query, State};

use crate::api::response::ApiResult;
use cce_api::models::{
    Bm25HealthResponse, EmbeddingHealthResponse, ErrorResponse, HealthStatus, QdrantDiagnostic,
    QdrantHealthResponse, RetryQueueClearResponse, RetryQueueDeadClearResponse,
    RetryQueueDeadEntry, RetryQueueDeadResponse, RetryQueueProcessResponse, RetryQueueQuery,
    RetryQueueStatusResponse, ServiceStatus, error_codes,
};
use cce_server_shared::context::SharedContext;

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
pub async fn handle_health(State(state): State<SharedContext>) -> ApiResult<HealthStatus> {
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
    State(state): State<SharedContext>,
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
    State(state): State<SharedContext>,
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
pub async fn handle_bm25_health(
    State(state): State<SharedContext>,
) -> ApiResult<Bm25HealthResponse> {
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
    params(RetryQueueQuery),
    responses(
        (status = 200, body = RetryQueueStatusResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_retry_queue_status(
    State(state): State<SharedContext>,
    Query(query): Query<RetryQueueQuery>,
) -> ApiResult<RetryQueueStatusResponse> {
    if let Some(project_id) = query.project_id {
        if let Err(e) = crate::api::validation::validate_project_id(project_id) {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::INVALID_REQUEST,
                format!("Invalid project_id: {}", e),
            ));
        }
        let pending_count = state.engine.retry_queue_len_for_project(project_id).await;
        let dead_count = state
            .engine
            .retry_queue_dead_len_for_project(project_id)
            .await;
        ApiResult::Success(RetryQueueStatusResponse {
            pending_count,
            is_empty: pending_count == 0,
            dead_count,
        })
    } else {
        let pending_count = state.engine.retry_queue_total_len().await;
        let dead_count = state.engine.retry_queue_total_dead_len().await;
        ApiResult::Success(RetryQueueStatusResponse {
            pending_count,
            is_empty: pending_count == 0,
            dead_count,
        })
    }
}

/// POST /api/retry-queue/process — Manually trigger retry queue processing
///
/// Drains all queries that are ready for retry (cooldown expired)
/// and re-executes them. Returns the number of queries re-attempted.
#[utoipa::path(
    post, path = "/api/retry-queue/process", tag = "Health",
    params(RetryQueueQuery),
    responses(
        (status = 200, body = RetryQueueProcessResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_retry_queue_process(
    State(state): State<SharedContext>,
    Query(query): Query<RetryQueueQuery>,
) -> ApiResult<RetryQueueProcessResponse> {
    let count = if let Some(project_id) = query.project_id {
        if let Err(e) = crate::api::validation::validate_project_id(project_id) {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::INVALID_REQUEST,
                format!("Invalid project_id: {}", e),
            ));
        }
        match state.engine.process_retry_queue(project_id).await {
            Ok(count) => count,
            Err(e) => {
                return ApiResult::Error(ErrorResponse::new(
                    error_codes::INTERNAL_ERROR,
                    format!("Failed to process retry queue: {}", e),
                ));
            }
        }
    } else {
        let project_ids: Vec<i64> = state
            .engine
            .metadata_store()
            .map(|store| {
                store
                    .as_ref()
                    .with_transaction(|tx| {
                        cce_storage_metadb_sqlite::ProjectRepository::get_all(tx)
                    })
                    .map(|records| records.into_iter().map(|r| r.id).collect())
                    .unwrap_or_default()
            })
            .unwrap_or_default();
        let mut total = 0usize;
        for project_id in project_ids {
            if let Ok(count) = state.engine.process_retry_queue(project_id).await {
                total += count;
            }
        }
        total
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
    params(RetryQueueQuery),
    responses(
        (status = 200, body = RetryQueueClearResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_retry_queue_clear(
    State(state): State<SharedContext>,
    Query(query): Query<RetryQueueQuery>,
) -> ApiResult<RetryQueueClearResponse> {
    if let Some(project_id) = query.project_id {
        if let Err(e) = crate::api::validation::validate_project_id(project_id) {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::INVALID_REQUEST,
                format!("Invalid project_id: {}", e),
            ));
        }
        let pending = state.engine.retry_queue_len_for_project(project_id).await;
        state.engine.clear_retry_queue_for_project(project_id).await;
        ApiResult::Success(RetryQueueClearResponse {
            cleared: pending,
            message: format!("Retry queue cleared, {} queries discarded", pending),
        })
    } else {
        let pending = state.engine.retry_queue_total_len().await;
        state.engine.clear_all_retry_queues().await;
        ApiResult::Success(RetryQueueClearResponse {
            cleared: pending,
            message: format!("Retry queue cleared, {} queries discarded", pending),
        })
    }
}

/// GET /api/retry-queue/dead — Snapshot of dead-lettered queries
#[utoipa::path(
    get, path = "/api/retry-queue/dead", tag = "Health",
    params(RetryQueueQuery),
    responses(
        (status = 200, body = RetryQueueDeadResponse, description = "Success"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_retry_queue_dead_list(
    State(state): State<SharedContext>,
    Query(query): Query<RetryQueueQuery>,
) -> ApiResult<RetryQueueDeadResponse> {
    let entries = if let Some(project_id) = query.project_id {
        if let Err(e) = crate::api::validation::validate_project_id(project_id) {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::INVALID_REQUEST,
                format!("Invalid project_id: {}", e),
            ));
        }
        state
            .engine
            .retry_queue_dead_snapshot_for_project(project_id)
            .await
    } else {
        state.engine.retry_queue_dead_snapshot().await
    };
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
    params(RetryQueueQuery),
    responses(
        (status = 200, body = RetryQueueDeadClearResponse, description = "Success"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_retry_queue_dead_clear(
    State(state): State<SharedContext>,
    Query(query): Query<RetryQueueQuery>,
) -> ApiResult<RetryQueueDeadClearResponse> {
    let cleared = if let Some(project_id) = query.project_id {
        if let Err(e) = crate::api::validation::validate_project_id(project_id) {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::INVALID_REQUEST,
                format!("Invalid project_id: {}", e),
            ));
        }
        state
            .engine
            .clear_retry_queue_dead_for_project(project_id)
            .await
    } else {
        state.engine.clear_all_retry_queue_dead().await
    };
    ApiResult::Success(RetryQueueDeadClearResponse {
        cleared,
        message: format!(
            "Retry queue dead list cleared, {} entries discarded",
            cleared
        ),
    })
}

// --- Internal helpers ---

async fn check_qdrant(state: &SharedContext) -> ServiceStatus {
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

async fn check_bm25(state: &SharedContext) -> ServiceStatus {
    let diag = state.engine.fulltext().diagnose_summary().await;
    if diag.reachable {
        ServiceStatus {
            reachable: true,
            message: "BM25 is enabled and connected".to_string(),
        }
    } else {
        let detail = diag
            .error
            .unwrap_or_else(|| "BM25 health check failed".to_string());
        ServiceStatus {
            reachable: false,
            message: detail,
        }
    }
}

fn check_embedding(state: &SharedContext) -> ServiceStatus {
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
