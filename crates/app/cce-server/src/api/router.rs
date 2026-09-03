//! HTTP router definition
//!
//! Defines all API routes and maps them to handler functions.

use axum::{
    Router, middleware,
    routing::{delete, get, post, put},
};

use super::handlers;
use super::middleware::metrics_middleware;
use super::state::AppState;
use cce_metrics_infra::HttpMetrics;

/// Create API router with all routes
pub fn create_router(state: AppState) -> Router {
    // Create HTTP metrics wrapper for middleware
    let http_metrics = HttpMetrics::new(state.engine.metrics_registry());

    Router::new()
        // Index operations
        .route("/api/index", post(handlers::index::handle_index))
        .route(
            "/api/index/incremental",
            post(handlers::index::handle_incremental),
        )
        .route("/api/parse", post(handlers::index::handle_parse))
        // Summary generation (temporary, no storage)
        .route("/api/summary", post(handlers::summary::handle_summary))
        // Storage management
        .route("/api/index", delete(handlers::storage::handle_clear_index))
        .route(
            "/api/index/file/{path}",
            delete(handlers::storage::handle_delete_file),
        )
        .route(
            "/api/index/entity/{id}",
            delete(handlers::storage::handle_delete_entity),
        )
        .route(
            "/api/index/batch",
            delete(handlers::storage::handle_batch_delete),
        )
        .route(
            "/api/index/stats",
            get(handlers::storage::handle_index_stats),
        )
        .route(
            "/api/storage/status",
            get(handlers::storage::handle_storage_status),
        )
        // Project management
        .route(
            "/api/project",
            post(handlers::project::handle_create_project),
        )
        .route("/api/project", get(handlers::project::handle_list_projects))
        .route(
            "/api/project/{id}",
            get(handlers::project::handle_get_project),
        )
        .route(
            "/api/project/{id}",
            put(handlers::project::handle_update_project),
        )
        .route(
            "/api/project/{id}",
            delete(handlers::project::handle_delete_project),
        )
        .route(
            "/api/project/{id}/index",
            post(handlers::project::handle_project_index),
        )
        .route(
            "/api/project/{id}/reload",
            post(handlers::project::handle_reload_project_config),
        )
        .route(
            "/api/project/{id}/config",
            put(handlers::project::handle_update_project_config),
        )
        // Project-scoped entity queries
        .route(
            "/api/project/{project_id}/function/{id}",
            get(handlers::entity::handle_function_detail),
        )
        .route(
            "/api/project/{project_id}/function/{id}/calls",
            get(handlers::entity::handle_function_calls),
        )
        .route(
            "/api/project/{project_id}/function/{id}/callers",
            get(handlers::entity::handle_function_callers),
        )
        .route(
            "/api/project/{project_id}/call-chain/{id}",
            get(handlers::entity::handle_call_chain),
        )
        .route(
            "/api/project/{project_id}/call-path",
            get(handlers::entity::handle_call_path),
        )
        .route(
            "/api/project/{project_id}/class/{id}/inheritance",
            get(handlers::entity::handle_class_inheritance),
        )
        .route(
            "/api/project/{project_id}/class/{id}/implementations",
            get(handlers::entity::handle_class_implementations),
        )
        // Classification queries
        .route(
            "/api/project/{project_id}/relations/classification/stats",
            get(handlers::entity::get_classification_stats),
        )
        .route(
            "/api/project/{project_id}/relations/classification/{classification}",
            get(handlers::entity::get_relations_by_classification),
        )
        // Metrics
        .route("/api/metrics", get(handlers::metrics::handle_get_metrics))
        .route(
            "/api/metrics/json",
            get(handlers::metrics::handle_get_metrics_json),
        )
        .route(
            "/api/metrics/history",
            get(handlers::metrics::handle_get_metrics_history),
        )
        .route(
            "/api/metrics/cleanup",
            delete(handlers::metrics::handle_cleanup_metrics),
        )
        // Watch (hot reload) - project-scoped
        .route(
            "/api/project/{project_id}/watch/start",
            post(handlers::watch::handle_start_watch),
        )
        .route(
            "/api/project/{project_id}/watch/stop",
            post(handlers::watch::handle_stop_watch),
        )
        .route(
            "/api/project/{project_id}/watch/status",
            get(handlers::watch::handle_watch_status),
        )
        // Configuration management
        .route(
            "/api/config/reload",
            post(handlers::config::handle_config_reload),
        )
        .route("/api/config", get(handlers::config::handle_config_info))
        .route(
            "/api/config/validate",
            get(handlers::config::handle_config_validate),
        )
        // Qdrant process lifecycle management
        .route(
            "/api/qdrant/process/status",
            get(handlers::qdrant_admin::handle_qdrant_process_status),
        )
        .route(
            "/api/qdrant/process/start",
            post(handlers::qdrant_admin::handle_qdrant_process_start),
        )
        .route(
            "/api/qdrant/process/stop",
            post(handlers::qdrant_admin::handle_qdrant_process_stop),
        )
        .route(
            "/api/qdrant/process/restart",
            post(handlers::qdrant_admin::handle_qdrant_process_restart),
        )
        // Search
        .route("/api/search", post(handlers::search::handle_search))
        // Aggregated search (multi-query with parallel retrieval)
        .route(
            "/api/search/aggregated",
            post(handlers::search::handle_aggregated_search),
        )
        // Entity search (FTS5)
        .route(
            "/api/entities/search",
            post(handlers::entity_search::handle_entity_search),
        )
        // Tools API
        .route(
            "/api/tools/compress",
            post(handlers::tools::handle_compress),
        )
        .route(
            "/api/tools/compress/batch",
            post(handlers::tools::handle_compress_batch),
        )
        .route(
            "/api/tools/diagnose",
            post(handlers::tools::handle_diagnose),
        )
        .route(
            "/api/tools/keyword-search",
            post(handlers::tools::handle_keyword_search),
        )
        .route(
            "/api/tools/symbols",
            post(handlers::tools::handle_get_symbols),
        )
        .route(
            "/api/tools/references",
            post(handlers::tools::handle_find_references),
        )
        .route(
            "/api/tools/definition",
            post(handlers::tools::handle_goto_definition),
        )
        // Health monitoring
        .route("/api/health", get(handlers::health::handle_health))
        .route(
            "/api/health/qdrant",
            get(handlers::health::handle_qdrant_health),
        )
        .route(
            "/api/health/embedding",
            get(handlers::health::handle_embedding_health),
        )
        .route(
            "/api/health/bm25",
            get(handlers::health::handle_bm25_health),
        )
        // Retry queue management
        .route(
            "/api/retry-queue",
            get(handlers::health::handle_retry_queue_status),
        )
        .route(
            "/api/retry-queue/process",
            post(handlers::health::handle_retry_queue_process),
        )
        .route(
            "/api/retry-queue",
            delete(handlers::health::handle_retry_queue_clear),
        )
        // Inject state
        .with_state(state)
        // Apply metrics middleware to all routes
        .layer(middleware::from_fn(metrics_middleware(http_metrics)))
}
