//! Configuration reload handler
//!
//! This module provides handlers for:
//! - Manually triggering configuration reloads
//! - Inspecting current active configuration
//! - Validating configuration files

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::Deserialize;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;

use cce_config::{
    AppConfig, ConfigWarning, DependencyParams, RelationConfig, Settings, validate_all_dependencies,
};
use cce_orchestrator::hot_update::processors::ProcessorCollection;
use cce_orchestrator::hot_update::processors::factory::{ProcessorConfig, ProcessorFactory};

use cce_api::models::ConfigInfoResponse;

use cce_api::models::error_codes;

/// Query parameters for config reload
#[derive(Debug, Deserialize)]
pub struct ConfigReloadQuery {
    /// Project ID to reload (optional, uses cached projects if not specified)
    pub project_id: Option<i64>,
}

/// Handle config reload request
///
/// Manually triggers a reload of business configurations for all registered processors.
/// Uses invalidate-rebuild pattern for simplified configuration management.
///
/// This endpoint requires a `project_id` query parameter. For per-project config reload,
/// use `/api/project/{id}/reload` instead.
pub async fn handle_config_reload(
    State(state): State<crate::api::state::AppState>,
    Query(query): Query<ConfigReloadQuery>,
) -> impl IntoResponse {
    // Get project_id from query param, or return error
    let project_id = match query.project_id {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!(cce_api::models::ErrorResponse::new(
                    error_codes::INVALID_REQUEST,
                    "project_id query parameter is required. Use /api/project/{id}/reload for per-project reload."
                ))),
            );
        }
    };

    // Use engine to reload project config (clears all component caches)
    let engine = &state.engine;
    if let Err(e) = engine.reload_project_config(project_id).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!(cce_api::models::ErrorResponse::with_details(
                error_codes::INTERNAL_ERROR,
                "Failed to reload project config",
                e.to_string(),
            ))),
        );
    }

    // Attempt hot-update coordinator reload if available
    match engine.get_hot_update_coordinator(project_id).await {
        Ok(coordinator) => {
            let coord = coordinator.lock().await;
            let relation_config = match engine.project_registry().get_or_load(project_id).await {
                Ok(entry) => entry.config.relation.clone(),
                Err(error) => {
                    tracing::warn!(
                        project_id,
                        error = %error,
                        "Using default relation configuration during config reload"
                    );
                    RelationConfig::default()
                }
            };
            let project_root = coord
                .watched_dirs()
                .first()
                .cloned()
                .unwrap_or_else(|| PathBuf::from("."));
            let project_group_id = cce_storage_qdrant::generate_project_group_id(
                project_id,
                &project_root.to_string_lossy(),
            );

            let factory = ProcessorFactory::new();
            let relation_publisher = match engine.get_relation_snapshot_publisher(project_id).await
            {
                Ok(publisher) => Some(publisher),
                Err(error) => {
                    tracing::warn!(
                        project_id,
                        error = %error,
                        "Relation processor will remain unavailable during config reload"
                    );
                    None
                }
            };
            // Build the processor set from the project configuration so export
            // and summary processors are reflected in the reloaded processors
            // (export gated on `export.enabled`, summary on `store_summaries`).
            let project_config = match engine.project_registry().get_or_load(project_id).await {
                Ok(entry) => entry.config.clone(),
                Err(error) => {
                    tracing::warn!(
                        project_id,
                        error = %error,
                        "Using default configuration during config reload"
                    );
                    cce_config::AppConfig::default()
                }
            };
            let mut processor_config = ProcessorConfig::new();
            processor_config.enable_export = project_config.export.enabled;
            processor_config.enable_summary = project_config.orchestrator.indexer.store_summaries;
            processor_config.export_config = if project_config.export.enabled {
                Some(cce_orchestrator::ExportConfig::from_module_config(
                    &project_config.export,
                    project_root.clone(),
                    project_id,
                ))
            } else {
                None
            };
            // The summary generator follows the project summary config (same as
            // the full-index path) so hot-update summaries stay consistent.
            let summary_generator: Option<Arc<dyn cce_parser::summary::SummaryGenerator>> =
                processor_config.enable_summary.then(|| {
                    let generator: Arc<dyn cce_parser::summary::SummaryGenerator> =
                        Arc::new(cce_parser::summary::RuleBasedGenerator::with_config(
                            project_config.summary.clone(),
                        ));
                    generator
                });
            let processors_result = factory.create_all_processors(
                state.qdrant.clone(),
                state.bm25.clone(),
                state
                    .metadata_store
                    .as_ref()
                    .and_then(|client| client.for_project(project_id).ok())
                    .or_else(|| state.metadata_store.clone()),
                state.embedder.clone(),
                Some(project_group_id),
                project_id,
                relation_publisher,
                &relation_config,
                coord.checkpoint_manager().await,
                summary_generator,
                Some(&project_config.ast_to_nl),
                &project_config.grouper,
                Some(&project_config.summary),
                &processor_config,
                engine.get_plugin_registry(project_id).await,
                None,
                Some(cce_metrics_infra::HotUpdateStorageMetrics::new(
                    state.engine.metrics_registry(),
                    project_id,
                )),
            );
            let (processors, _storage_coordinator) = match processors_result {
                Ok(value) => value,
                Err(error) => {
                    tracing::warn!(
                        project_id,
                        error = %error,
                        "Failed to rebuild processors during config reload; keeping the previous set"
                    );
                    return (
                        StatusCode::OK,
                        Json(json!({
                            "success": true,
                            "message": format!("Configuration reloaded for project {} (processors kept unchanged)", project_id)
                        })),
                    );
                }
            };
            let processor_refs: Vec<
                &dyn cce_orchestrator::hot_update::processors::UpdateProcessor,
            > = processors.enabled_processors();
            if let Err(e) = coord.process_pending_config_changes(&processor_refs).await {
                tracing::warn!(
                    project_id,
                    error = %e,
                    "Hot-update config reload failed (non-critical)"
                );
            }
        }
        Err(e) => {
            tracing::warn!(
                project_id,
                error = %e,
                "HotUpdateCoordinator not available, config cache cleared but runtime not hot-reloaded"
            );
        }
    }

    (
        StatusCode::OK,
        Json(json!({
            "success": true,
            "message": format!("Configuration reloaded for project {}", project_id)
        })),
    )
}

/// GET /api/config
///
/// Return the current active configuration information.
/// Useful for verifying that the correct configuration has been loaded.
pub async fn handle_config_info(
    State(state): State<crate::api::state::AppState>,
) -> Json<ConfigInfoResponse> {
    let initialized = Settings::is_initialized();
    let (database, embedder, project_count) = if initialized {
        match Settings::global() {
            Ok(config) => {
                let db = serde_json::to_value(&config.database).unwrap_or_default();
                let emb = serde_json::to_value(&config.embedder).unwrap_or_default();
                let pcount = state.project_registry.as_ref().map(|_| 1).unwrap_or(0);
                (db, emb, pcount)
            }
            Err(_) => (json!(null), json!(null), 0),
        }
    } else {
        (json!(null), json!(null), 0)
    };

    Json(ConfigInfoResponse {
        initialized,
        database,
        embedder,
        project_count,
    })
}

/// GET /api/config/validate
///
/// Validate the current configuration and return any warnings or errors.
/// This is a read-only check that does not reload or modify any state.
pub async fn handle_config_validate() -> Json<serde_json::Value> {
    let mut warnings: Vec<String> = Vec::new();
    let mut dep_warnings: Vec<ConfigWarning> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    if !Settings::is_initialized() {
        errors.push("Configuration has not been initialized".into());
        return Json(json!({
            "valid": false,
            "errors": errors,
            "warnings": warnings,
        }));
    }

    match Settings::global() {
        Ok(config) => {
            // Basic structural validation
            validate_config(&config, &mut warnings, &mut errors);

            // Cross-module dependency validation
            let params = DependencyParams {
                export_include_summary: config.export.include_summary,
                export_enable_relation_enhancement: config.export.enable_relation_enhancement,
                indexer_store_summaries: config.orchestrator.indexer.store_summaries,
                indexer_build_relations: config.orchestrator.indexer.build_relations,
                indexer_store_vectors: config.orchestrator.indexer.store_vectors,
                indexer_store_bm25: config.orchestrator.indexer.store_bm25,
                qdrant_enabled: config.database.qdrant.enabled,
                bm25_enabled: config.database.bm25.enabled,
                relation_index_enabled: config.relation.index.enabled,
                llm_enabled: config.llm.enabled,
                has_llm_provider: !config.llm.providers.is_empty(),
                has_chat_model: config
                    .llm
                    .defaults
                    .chat
                    .as_ref()
                    .is_some_and(|model| config.llm.chat_models.contains_key(model)),
            };
            dep_warnings = validate_all_dependencies(&params);
        }
        Err(e) => {
            errors.push(format!("Failed to read global config: {}", e));
        }
    }

    Json(json!({
        "valid": errors.is_empty(),
        "errors": errors,
        "warnings": warnings,
        "dependency_warnings": dep_warnings,
    }))
}

/// Perform basic validation of the configuration.
fn validate_config(config: &AppConfig, warnings: &mut Vec<String>, errors: &mut Vec<String>) {
    // Qdrant config
    let qdrant = &config.database.qdrant;
    if qdrant.url.is_empty() {
        errors.push("Qdrant URL is empty".into());
    }
    if qdrant.enabled && qdrant.api_key.as_deref().unwrap_or("") == "${CCE_DB_QDRANT_API_KEY}" {
        warnings.push("Qdrant API key references environment variable ${CCE_DB_QDRANT_API_KEY} which may not be set".into());
    }

    // Embedder config
    if config.embedder.default_model.is_empty() {
        errors.push("No default embedding model configured".into());
    }

    // SQLite config
    let sqlite = &config.database.sqlite;
    if sqlite.path.is_empty() {
        warnings.push("SQLite database path is empty, using default".into());
    }
}
