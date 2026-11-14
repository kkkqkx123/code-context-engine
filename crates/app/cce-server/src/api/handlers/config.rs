//! Configuration reload handler
//!
//! This module provides handlers for:
//! - Manually triggering configuration reloads
//! - Inspecting current active configuration
//! - Validating configuration files

use axum::{extract::Query, extract::State};
use serde_json::json;

use cce_config::{AppConfig, ConfigWarning, DependencyParams, Settings, validate_all_dependencies};

use cce_api::models::{
    ConfigInfoResponse, ConfigReloadQuery, ConfigReloadResponse, ConfigValidateResponse,
    ConfigWarningInfo, ErrorResponse, error_codes,
};
use cce_server_shared::services::config;

use crate::api::response::ApiResult;

/// Handle config reload request
///
/// Manually triggers a reload of business configurations for all registered processors.
/// Uses invalidate-rebuild pattern for simplified configuration management.
///
/// This endpoint requires a `project_id` query parameter. For per-project config reload,
/// use `/api/project/{id}/reload` instead.
#[utoipa::path(
    post, path = "/api/config/reload", tag = "Config",
    params(ConfigReloadQuery),
    responses(
        (status = 200, body = ConfigReloadResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_config_reload(
    State(state): State<cce_server_shared::context::SharedContext>,
    Query(query): Query<ConfigReloadQuery>,
) -> ApiResult<ConfigReloadResponse> {
    // Without a project id this is a global safe reload. Dangerous section
    // changes fail closed; safe sections advance the global version so the
    // project registry invalidates cached effective configurations.
    let Some(project_id) = query.project_id else {
        return match config::reload_global(&state).await {
            Ok(version) => ApiResult::Success(ConfigReloadResponse {
                success: true,
                message: format!(
                    "Global configuration reloaded (version {version}); project caches invalidated by global version"
                ),
            }),
            Err(error) => ApiResult::Error(ErrorResponse::with_details(
                error_codes::INVALID_REQUEST,
                "Global reload refused; restart required for dangerous sections",
                error,
            )),
        };
    };

    match config::reload_project(&state, project_id).await {
        Ok(()) => ApiResult::Success(ConfigReloadResponse {
            success: true,
            message: format!("Configuration reloaded for project {project_id}"),
        }),
        Err(error) => ApiResult::Error(ErrorResponse::with_details(
            error_codes::INTERNAL_ERROR,
            "Failed to reload project config",
            error,
        )),
    }
}

/// GET /api/config
///
/// Return the current active configuration information.
/// Useful for verifying that the correct configuration has been loaded.
#[utoipa::path(
    get, path = "/api/config", tag = "Config",
    responses(
        (status = 200, body = ConfigInfoResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_config_info() -> ApiResult<ConfigInfoResponse> {
    let initialized = Settings::is_initialized();
    let (database, embedder, full_config, project_count) = if initialized {
        match Settings::global() {
            Ok(config) => {
                let db = serde_json::to_value(&config.database).unwrap_or_default();
                let emb = serde_json::to_value(&config.embedder).unwrap_or_default();
                let full = serde_json::to_value(&config).unwrap_or_default();
                (db, emb, full, 1)
            }
            Err(_) => (json!(null), json!(null), json!(null), 0),
        }
    } else {
        (json!(null), json!(null), json!(null), 0)
    };

    ApiResult::Success(ConfigInfoResponse {
        initialized,
        database,
        embedder,
        config: full_config,
        project_count,
    })
}

/// GET /api/config/validate
///
/// Validate the current configuration and return any warnings or errors.
/// This is a read-only check that does not reload or modify any state.
#[utoipa::path(
    get, path = "/api/config/validate", tag = "Config",
    responses(
        (status = 200, body = ConfigValidateResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_config_validate() -> ApiResult<ConfigValidateResponse> {
    let mut warnings: Vec<String> = Vec::new();
    let mut dep_warnings: Vec<ConfigWarning> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    if !Settings::is_initialized() {
        errors.push("Configuration has not been initialized".into());
        return ApiResult::Success(ConfigValidateResponse {
            valid: false,
            errors,
            warnings,
            dependency_warnings: Vec::new(),
        });
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
                vector_backend: config.database.vector_backend,
                relation_backend: config.database.relation_backend,
                fulltext_backend: config.database.fulltext_backend,
                allow_nonstandard_backends: config.database.allow_nonstandard_backends,
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

    ApiResult::Success(ConfigValidateResponse {
        valid: errors.is_empty(),
        errors,
        warnings,
        dependency_warnings: dep_warnings
            .into_iter()
            .map(|w| ConfigWarningInfo {
                severity: format!("{:?}", w.severity),
                field: w.field,
                depends_on: w.depends_on,
                suggestion: w.suggestion,
            })
            .collect(),
    })
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
