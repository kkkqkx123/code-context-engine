//! HTTP router definition
//!
//! Defines all API routes and maps them to handler functions.

use axum::{
    Router, middleware,
    routing::{delete, get, post, put},
};

use super::handlers;
use super::middleware::metrics_middleware;
use super::openapi::openapi_json;
use super::state::AppState;
use cce_api::paths as api_paths;
use cce_metrics::HttpMetrics;

/// Serve the OpenAPI document (debug builds only).
async fn serve_openapi() -> ([(&'static str, &'static str); 1], String) {
    ([("Content-Type", "application/json")], openapi_json())
}

/// Shared route table backing the local router and the admission router.
///
/// Neither state nor layers are applied here so both shapes serve the same
/// endpoints and differ only in middleware.
pub(crate) fn api_routes() -> Router<AppState> {
    let mut router = Router::new()
        // Index operations
        .route(api_paths::INDEX, post(handlers::index::handle_index))
        .route(
            api_paths::INDEX_INCREMENTAL,
            post(handlers::index::handle_incremental),
        )
        .route(api_paths::PARSE, post(handlers::index::handle_parse))
        // Summary generation (temporary, no storage)
        .route(api_paths::SUMMARY, post(handlers::summary::handle_summary))
        // Storage management
        .route(
            api_paths::INDEX,
            delete(handlers::storage::handle_clear_index),
        )
        .route(
            api_paths::INDEX_FILE_TEMPLATE,
            delete(handlers::storage::handle_delete_file),
        )
        .route(
            api_paths::INDEX_ENTITY_TEMPLATE,
            delete(handlers::storage::handle_delete_entity),
        )
        .route(
            api_paths::INDEX_BATCH,
            delete(handlers::storage::handle_batch_delete),
        )
        .route(
            api_paths::INDEX_STATS,
            get(handlers::storage::handle_index_stats),
        )
        .route(
            api_paths::STORAGE_STATUS,
            get(handlers::storage::handle_storage_status),
        )
        // Project management
        .route(
            api_paths::PROJECT,
            post(handlers::project::handle_create_project),
        )
        .route(
            api_paths::PROJECT,
            get(handlers::project::handle_list_projects),
        )
        .route(
            api_paths::PROJECT_DETAIL_TEMPLATE,
            get(handlers::project::handle_get_project),
        )
        .route(
            api_paths::PROJECT_DETAIL_TEMPLATE,
            put(handlers::project::handle_update_project),
        )
        .route(
            api_paths::PROJECT_DETAIL_TEMPLATE,
            delete(handlers::project::handle_delete_project),
        )
        .route(
            api_paths::PROJECT_INDEX_TEMPLATE,
            post(handlers::project::handle_project_index),
        )
        .route(
            api_paths::PROJECT_DEAD_LETTERS_TEMPLATE,
            get(handlers::project::handle_dead_letter_list),
        )
        .route(
            api_paths::PROJECT_DEAD_LETTERS_RETRY_TEMPLATE,
            post(handlers::project::handle_dead_letter_files_retry),
        )
        .route(
            api_paths::PROJECT_DEAD_LETTERS_ACK_TEMPLATE,
            post(handlers::project::handle_dead_letter_acknowledge),
        )
        .route(
            api_paths::PROJECT_RELOAD_TEMPLATE,
            post(handlers::project::handle_reload_project_config),
        )
        .route(
            api_paths::PROJECT_CONFIG_TEMPLATE,
            get(handlers::project::handle_get_project_config)
                .put(handlers::project::handle_update_project_config),
        )
        // Project-scoped entity queries
        .route(
            api_paths::FUNCTION_DETAIL_TEMPLATE,
            get(handlers::entity::handle_function_detail),
        )
        .route(
            api_paths::FUNCTION_CALLS_TEMPLATE,
            get(handlers::entity::handle_function_calls),
        )
        .route(
            api_paths::FUNCTION_CALLERS_TEMPLATE,
            get(handlers::entity::handle_function_callers),
        )
        .route(
            api_paths::CALL_CHAIN_TEMPLATE,
            get(handlers::entity::handle_call_chain),
        )
        .route(
            api_paths::CALL_PATH_TEMPLATE,
            get(handlers::entity::handle_call_path),
        )
        .route(
            api_paths::CLASS_INHERITANCE_TEMPLATE,
            get(handlers::entity::handle_class_inheritance),
        )
        .route(
            api_paths::CLASS_IMPLEMENTATIONS_TEMPLATE,
            get(handlers::entity::handle_class_implementations),
        )
        // Classification queries
        .route(
            api_paths::CLASSIFICATION_STATS_TEMPLATE,
            get(handlers::entity::get_classification_stats),
        )
        .route(
            api_paths::CLASSIFICATION_DETAIL_TEMPLATE,
            get(handlers::entity::get_relations_by_classification),
        )
        // Graph traversal (independent retrieval path)
        .route(
            api_paths::GRAPH_EGO_TEMPLATE,
            get(handlers::graph::handle_graph_ego),
        )
        .route(
            api_paths::GRAPH_PATH_TEMPLATE,
            get(handlers::graph::handle_graph_path),
        )
        .route(
            api_paths::GRAPH_SUBGRAPH_TEMPLATE,
            get(handlers::graph::handle_graph_subgraph),
        )
        .route(
            api_paths::GRAPH_COMPONENTS_TEMPLATE,
            get(handlers::graph::handle_graph_components),
        )
        .route(
            api_paths::GRAPH_EXPORT_TEMPLATE,
            get(handlers::graph::handle_graph_export),
        )
        .route(
            api_paths::GRAPH_IMPACT_TEMPLATE,
            get(handlers::graph::handle_graph_impact),
        )
        .route(
            api_paths::GRAPH_ENTITY_IMPACT_TEMPLATE,
            get(handlers::graph::handle_graph_entity_impact),
        )
        .route(
            api_paths::GRAPH_CYCLES_TEMPLATE,
            get(handlers::graph::handle_graph_cycles),
        )
        .route(
            api_paths::GRAPH_STRUCTURAL_TEMPLATE,
            get(handlers::graph::handle_graph_structural),
        )
        .route(
            api_paths::GRAPH_MODULE_TEMPLATE,
            get(handlers::graph::handle_graph_module),
        )
        // Metrics
        .route(
            api_paths::METRICS,
            get(handlers::metrics::handle_get_metrics),
        )
        .route(
            api_paths::METRICS_JSON,
            get(handlers::metrics::handle_get_metrics_json),
        )
        .route(
            api_paths::METRICS_HISTORY,
            get(handlers::metrics::handle_get_metrics_history),
        )
        .route(
            api_paths::METRICS_CLEANUP,
            delete(handlers::metrics::handle_cleanup_metrics),
        )
        // Watch (hot reload) - project-scoped
        .route(
            api_paths::WATCH_START_TEMPLATE,
            post(handlers::watch::handle_start_watch),
        )
        .route(
            api_paths::WATCH_STOP_TEMPLATE,
            post(handlers::watch::handle_stop_watch),
        )
        .route(
            api_paths::WATCH_STATUS_TEMPLATE,
            get(handlers::watch::handle_watch_status),
        )
        // Configuration management
        .route(
            api_paths::CONFIG_RELOAD,
            post(handlers::config::handle_config_reload),
        )
        .route(
            api_paths::CONFIG_INFO,
            get(handlers::config::handle_config_info),
        )
        .route(
            api_paths::CONFIG_VALIDATE,
            get(handlers::config::handle_config_validate),
        )
        // Qdrant process lifecycle management
        .route(
            api_paths::QDRANT_PROCESS_STATUS,
            get(handlers::qdrant_admin::handle_qdrant_process_status),
        )
        .route(
            api_paths::QDRANT_PROCESS_START,
            post(handlers::qdrant_admin::handle_qdrant_process_start),
        )
        .route(
            api_paths::QDRANT_PROCESS_STOP,
            post(handlers::qdrant_admin::handle_qdrant_process_stop),
        )
        .route(
            api_paths::QDRANT_PROCESS_RESTART,
            post(handlers::qdrant_admin::handle_qdrant_process_restart),
        )
        // Search
        .route(api_paths::SEARCH, post(handlers::search::handle_search))
        // Aggregated search (multi-query with parallel retrieval)
        .route(
            api_paths::SEARCH_AGGREGATED,
            post(handlers::search::handle_aggregated_search),
        )
        // Entity search (FTS5)
        .route(
            api_paths::ENTITY_SEARCH,
            post(handlers::entity_search::handle_entity_search),
        )
        // Tools API
        .route(
            api_paths::TOOLS_COMPRESS,
            post(handlers::tools::handle_compress),
        )
        .route(
            api_paths::TOOLS_COMPRESS_BATCH,
            post(handlers::tools::handle_compress_batch),
        )
        .route(
            api_paths::TOOLS_DIAGNOSE,
            post(handlers::tools::handle_diagnose),
        )
        .route(api_paths::TOOLS_FOLD, post(handlers::tools::handle_fold))
        .route(
            api_paths::TOOLS_FOLD_BATCH,
            post(handlers::tools::handle_fold_batch),
        )
        .route(
            api_paths::TOOLS_KEYWORD_SEARCH,
            post(handlers::tools::handle_keyword_search),
        )
        .route(
            api_paths::TOOLS_SYMBOLS,
            post(handlers::tools::handle_get_symbols),
        )
        .route(
            api_paths::TOOLS_REFERENCES,
            post(handlers::tools::handle_find_references),
        )
        .route(
            api_paths::TOOLS_DEFINITION,
            post(handlers::tools::handle_goto_definition),
        )
        // Health monitoring
        .route(api_paths::HEALTH, get(handlers::health::handle_health))
        .route(
            api_paths::HEALTH_QDRANT,
            get(handlers::health::handle_qdrant_health),
        )
        .route(
            api_paths::HEALTH_EMBEDDING,
            get(handlers::health::handle_embedding_health),
        )
        .route(
            api_paths::HEALTH_BM25,
            get(handlers::health::handle_bm25_health),
        )
        // Retry queue management
        .route(
            api_paths::RETRY_QUEUE,
            get(handlers::health::handle_retry_queue_status),
        )
        .route(
            api_paths::RETRY_QUEUE_DEAD,
            get(handlers::health::handle_retry_queue_dead_list),
        )
        .route(
            api_paths::RETRY_QUEUE_DEAD,
            delete(handlers::health::handle_retry_queue_dead_clear),
        )
        .route(
            api_paths::RETRY_QUEUE_PROCESS,
            post(handlers::health::handle_retry_queue_process),
        )
        .route(
            api_paths::RETRY_QUEUE,
            delete(handlers::health::handle_retry_queue_clear),
        );

    // Debug-only self-hosted OpenAPI document; excluded from the contract tests.
    #[cfg(debug_assertions)]
    {
        router = router.route("/api-docs/openapi.json", get(serve_openapi));
    }

    router
}

/// Create API router with all routes
pub fn create_router(state: AppState) -> Router {
    // Create HTTP metrics wrapper for middleware. The metrics layer stays
    // outermost so it also observes admission rejections on the remote
    // router assembled beside this one.
    let http_metrics = HttpMetrics::new(state.engine.metrics_registry());

    api_routes()
        // Inject state
        .with_state(state)
        // Apply metrics middleware to all routes
        .layer(middleware::from_fn(metrics_middleware(http_metrics)))
}
