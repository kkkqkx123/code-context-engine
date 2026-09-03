//! Health monitoring and retry queue management handlers
//!
//! Provides endpoints for:
//! - Unified health check across all external services
//! - Per-service detailed diagnostics
//! - Retry queue inspection and manual processing

use axum::{Json, extract::State, http::StatusCode};

use crate::api::state::AppState;
use cce_api::models::{
    Bm25HealthResponse, EmbeddingHealthResponse, HealthStatus, QdrantDiagnostic,
    QdrantHealthResponse, RetryQueueStatusResponse, ServiceStatus,
};

// --- Handlers ---

/// GET /api/health — Aggregate health of all external services
pub async fn handle_health(
    State(state): State<AppState>,
) -> Result<Json<HealthStatus>, (StatusCode, Json<serde_json::Value>)> {
    let qdrant_health = check_qdrant(&state).await;
    let bm25_health = check_bm25(&state).await;
    let embedding_health = check_embedding(&state);

    let all_healthy =
        qdrant_health.reachable && bm25_health.reachable && embedding_health.reachable;

    Ok(Json(HealthStatus {
        healthy: all_healthy,
        qdrant: qdrant_health,
        bm25: bm25_health,
        embedding: embedding_health,
    }))
}

/// GET /api/health/qdrant — Qdrant detailed diagnostic
pub async fn handle_qdrant_health(
    State(state): State<AppState>,
) -> Result<Json<QdrantHealthResponse>, (StatusCode, Json<serde_json::Value>)> {
    let circuit_breaker = state
        .qdrant
        .as_ref()
        .map(|q| q.circuit_breaker_state())
        .unwrap_or_else(|| "no client".to_string());

    let diagnostic = if let Some(qdrant) = &state.qdrant {
        match qdrant.diagnose().await {
            Ok(diag) => QdrantDiagnostic {
                reachable: diag.reachable,
                version: diag.version,
                collection_exists: diag.collection_exists,
                points_count: diag.points_count,
                error: diag.error,
            },
            Err(e) => QdrantDiagnostic {
                reachable: false,
                version: None,
                collection_exists: false,
                points_count: 0,
                error: Some(format!("Diagnostic failed: {}", e)),
            },
        }
    } else {
        QdrantDiagnostic {
            reachable: false,
            version: None,
            collection_exists: false,
            points_count: 0,
            error: Some("Qdrant client not configured".to_string()),
        }
    };

    let healthy = diagnostic.reachable;

    Ok(Json(QdrantHealthResponse {
        healthy,
        circuit_breaker,
        diagnostic,
    }))
}

/// GET /api/health/embedding — Embedding service health
pub async fn handle_embedding_health(
    State(state): State<AppState>,
) -> Json<EmbeddingHealthResponse> {
    let (healthy, model_name, message) = if let Some(embedder) = &state.embedder {
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
    } else {
        (false, None, "Embedding provider not configured".to_string())
    };

    Json(EmbeddingHealthResponse {
        healthy,
        model_name,
        message,
    })
}

/// GET /api/health/bm25 — BM25 index health
pub async fn handle_bm25_health(State(state): State<AppState>) -> Json<Bm25HealthResponse> {
    let (enabled, connected, index_path) = if let Some(bm25) = &state.bm25 {
        let bm25 = bm25.lock().await;
        (
            bm25.is_enabled(),
            bm25.is_connected(),
            bm25.config().index_path.clone(),
        )
    } else {
        (false, false, None)
    };

    Json(Bm25HealthResponse {
        enabled,
        connected,
        index_path,
    })
}

/// GET /api/retry-queue — View retry queue status (aggregated across all projects)
pub async fn handle_retry_queue_status(
    State(state): State<AppState>,
) -> Json<RetryQueueStatusResponse> {
    let pending_count = state.engine.retry_queue_total_len().await;
    Json(RetryQueueStatusResponse {
        pending_count,
        is_empty: pending_count == 0,
    })
}

/// POST /api/retry-queue/process — Manually trigger retry queue processing
///
/// Drains all queries that are ready for retry (cooldown expired)
/// and re-executes them. Returns the number of queries re-attempted.
pub async fn handle_retry_queue_process(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let count = state.engine.process_retry_queue(1).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("Failed to process retry queue: {}", e)
            })),
        )
    })?;
    Ok(Json(serde_json::json!({
        "processed": count,
        "message": format!("Retry queue processing complete, {} queries re-attempted", count)
    })))
}

/// DELETE /api/retry-queue — Clear all retry queues across all projects
pub async fn handle_retry_queue_clear(State(state): State<AppState>) -> Json<serde_json::Value> {
    let pending = state.engine.retry_queue_total_len().await;
    state.engine.clear_all_retry_queues().await;
    Json(serde_json::json!({
        "cleared": pending,
        "message": format!("Retry queue cleared, {} queries discarded", pending)
    }))
}

// --- Internal helpers ---

async fn check_qdrant(state: &AppState) -> ServiceStatus {
    match &state.qdrant {
        Some(qdrant) => match qdrant.health().await {
            Ok(true) => ServiceStatus {
                reachable: true,
                message: "Qdrant is reachable and healthy".to_string(),
            },
            Ok(false) => ServiceStatus {
                reachable: false,
                message: "Qdrant returned non-success health status".to_string(),
            },
            Err(e) => ServiceStatus {
                reachable: false,
                message: format!("Qdrant health check failed: {}", e),
            },
        },
        None => ServiceStatus {
            reachable: false,
            message: "Qdrant client not configured".to_string(),
        },
    }
}

async fn check_bm25(state: &AppState) -> ServiceStatus {
    match &state.bm25 {
        Some(bm25) => {
            let bm25 = bm25.lock().await;
            if bm25.is_enabled() && bm25.is_connected() {
                ServiceStatus {
                    reachable: true,
                    message: "BM25 is enabled and connected".to_string(),
                }
            } else if bm25.is_connected() {
                ServiceStatus {
                    reachable: true,
                    message: "BM25 is connected but disabled in config".to_string(),
                }
            } else if bm25.config().enabled {
                ServiceStatus {
                    reachable: false,
                    message: "BM25 is enabled but not connected".to_string(),
                }
            } else {
                ServiceStatus {
                    reachable: false,
                    message: "BM25 is disabled".to_string(),
                }
            }
        }
        None => ServiceStatus {
            reachable: false,
            message: "BM25 client not configured".to_string(),
        },
    }
}

fn check_embedding(state: &AppState) -> ServiceStatus {
    let (healthy, model_name) = if let Some(embedder) = &state.embedder {
        (
            embedder.is_healthy(),
            Some(embedder.model_name().to_string()),
        )
    } else {
        (false, None)
    };

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
