//! Project configuration handlers
//!
//! Handles configuration update and reload operations.

use axum::{
    Json,
    extract::{Path, State},
};

use crate::api::response::ApiResult;
use cce_api::models::error_codes;
use cce_api::models::{
    ChatModelView, EmbeddingModelView, ErrorResponse, LlmDimensionStatus, LlmProviderSummary,
    LlmTestRequest, LlmTestResponse, ProjectConfigGetResponse, ProjectConfigReloadResponse,
    ProjectConfigUpdateRequest, ProjectConfigUpdateResponse, ProjectLlmOverviewResponse,
    ProjectLlmUpdateRequest, ProjectLlmUpdateResponse, RerankModelView,
};
use cce_config::global::AppConfig;
use cce_config::project_registry::RegistryError;
use cce_config::validation::Validate;
use cce_llm::LlmClient;

/// Handle update project config request
#[utoipa::path(
    put, path = "/api/project/{project_id}/config", tag = "Project",
    params(("project_id" = i64, Path, description = "Project id")),
    request_body = ProjectConfigUpdateRequest,
    responses(
        (status = 200, body = ProjectConfigUpdateResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_update_project_config(
    State(state): State<crate::api::state::AppState>,
    Path(project_id): Path<i64>,
    Json(payload): Json<ProjectConfigUpdateRequest>,
) -> ApiResult<ProjectConfigUpdateResponse> {
    // Check if project registry is available
    let registry = state.engine.project_registry();
    let registry = &**registry;

    // Validate the submitted config against the application schema
    let config = match serde_json::from_value::<AppConfig>(payload.config) {
        Ok(config) => config,
        Err(e) => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::INVALID_INPUT,
                format!("Invalid configuration format: {}", e),
            ));
        }
    };

    // `update_config` mutates the in-memory entry, so the project must be
    // resident first: loading also surfaces a genuine not-found as 404
    // instead of silently reporting a cache miss as a missing project.
    if let Err(e) = registry.get_or_load(project_id).await {
        let (code, message) = match &e {
            RegistryError::ProjectNotFound(id) => (
                error_codes::ENTITY_NOT_FOUND,
                format!("Project {} not found", id),
            ),
            other => (
                error_codes::STORAGE_ERROR,
                format!("Failed to load project: {}", other),
            ),
        };
        return ApiResult::Error(ErrorResponse::new(code, message));
    }

    // 1. Update configuration
    if let Err(e) = registry.update_config(project_id, config).await {
        // Log the error with context
        tracing::error!(
            project_id,
            error = %e,
            "Failed to update project config"
        );

        let (code, message) = match &e {
            RegistryError::ProjectNotFound(id) => (
                error_codes::ENTITY_NOT_FOUND,
                format!("Project {} not found", id),
            ),
            RegistryError::PathNotFound(path) => (
                error_codes::INVALID_INPUT,
                format!("Path does not exist: {:?}", path),
            ),
            RegistryError::DuplicatePath(path) => (
                error_codes::CONFLICT,
                format!("Path already registered: {:?}", path),
            ),
            RegistryError::Validation(msg) => (
                error_codes::INVALID_INPUT,
                format!("Configuration validation failed: {}", msg),
            ),
            RegistryError::Configuration(msg) => (
                error_codes::INVALID_INPUT,
                format!("Configuration error: {}", msg),
            ),
            RegistryError::Serialization(msg) => (
                error_codes::INVALID_INPUT,
                format!("Invalid configuration format: {}", msg),
            ),
            RegistryError::Deserialization(msg) => (
                error_codes::INVALID_INPUT,
                format!("Invalid configuration format: {}", msg),
            ),
            RegistryError::Io(err) => (error_codes::STORAGE_ERROR, format!("IO error: {}", err)),
            RegistryError::Database(err) => (
                error_codes::STORAGE_ERROR,
                format!("Database error: {}", err),
            ),
        };

        return ApiResult::Error(ErrorResponse::new(code, message));
    }

    // 2. Notify HotUpdateCoordinator to reload (if exists)
    let mut hot_reload_success = true;
    let engine = &state.engine;
    match engine.get_hot_update_coordinator(project_id).await {
        Ok(coordinator) => {
            let mut coord = coordinator.lock().await;
            if let Err(e) = coord.reload_project_config().await {
                tracing::error!(
                    project_id,
                    error = %e,
                    "Failed to reload HotUpdateCoordinator config after update"
                );
                hot_reload_success = false;
                // Don't return error - main config was updated successfully
            }
        }
        Err(e) => {
            tracing::warn!(
                project_id,
                error = %e,
                "HotUpdateCoordinator not available, config updated but hot reload skipped"
            );
            hot_reload_success = false;
        }
    }

    // 3. Return success response with hot reload status
    ApiResult::Success(ProjectConfigUpdateResponse {
        success: true,
        hot_reload_applied: hot_reload_success,
        message: if hot_reload_success {
            "Configuration updated and hot reload triggered successfully".to_string()
        } else {
            "Configuration updated, but hot reload failed. Manual restart may be required for some components.".to_string()
        },
    })
}

/// Handle read merged project config request
#[utoipa::path(
    get, path = "/api/project/{project_id}/config", tag = "Project",
    params(("project_id" = i64, Path, description = "Project id")),
    responses(
        (status = 200, body = ProjectConfigGetResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_get_project_config(
    State(state): State<crate::api::state::AppState>,
    Path(project_id): Path<i64>,
) -> ApiResult<ProjectConfigGetResponse> {
    let registry = state.engine.project_registry();
    match registry.get_or_load(project_id).await {
        Ok(entry) => {
            let config = serde_json::to_value(&entry.config).unwrap_or_default();
            ApiResult::Success(ProjectConfigGetResponse {
                success: true,
                project_id,
                config_version: entry.version,
                config,
            })
        }
        Err(e) => {
            let (code, message) = match e {
                RegistryError::ProjectNotFound(_) => (
                    error_codes::ENTITY_NOT_FOUND,
                    format!("Project {} not found", project_id),
                ),
                RegistryError::Configuration(msg) | RegistryError::Validation(msg) => {
                    (error_codes::INVALID_INPUT, msg)
                }
                RegistryError::Io(err) => (error_codes::STORAGE_ERROR, err.to_string()),
                RegistryError::Database(err) => (error_codes::STORAGE_ERROR, err),
                RegistryError::PathNotFound(path) => (
                    error_codes::ENTITY_NOT_FOUND,
                    format!("Path does not exist: {}", path),
                ),
                RegistryError::DuplicatePath(path) => (
                    error_codes::CONFLICT,
                    format!("Path already registered: {}", path),
                ),
                RegistryError::Serialization(msg) | RegistryError::Deserialization(msg) => {
                    (error_codes::INVALID_INPUT, msg)
                }
            };
            ApiResult::Error(ErrorResponse::new(code, message))
        }
    }
}

/// Handle reload project config request (hot reload)
#[utoipa::path(
    post, path = "/api/project/{project_id}/reload", tag = "Project",
    params(("project_id" = i64, Path, description = "Project id")),
    responses(
        (status = 200, body = ProjectConfigReloadResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_reload_project_config(
    State(state): State<crate::api::state::AppState>,
    Path(id): Path<i64>,
) -> ApiResult<ProjectConfigReloadResponse> {
    let project_registry = state.engine.project_registry();
    let project_registry = &**project_registry;

    let _ = project_registry.invalidate_cache(Some(id)).await;

    if let Err(e) = state.engine.reload_project_config(id).await {
        tracing::warn!(
            project_id = id,
            error = %e,
            "Failed to reload engine components (non-critical)"
        );
    }

    match project_registry.get_or_load(id).await {
        Ok(entry) => ApiResult::Success(ProjectConfigReloadResponse {
            success: true,
            message: "Configuration cache invalidated. Will reload from file on next access."
                .to_string(),
            project_id: id,
            config_version: entry.version,
        }),
        Err(e) => {
            tracing::error!(
                project_id = id,
                error = %e,
                "Failed to reload project config"
            );

            let (code, message) = match e {
                RegistryError::ProjectNotFound(_) | RegistryError::PathNotFound(_) => (
                    error_codes::ENTITY_NOT_FOUND,
                    format!("Project {} not found", id),
                ),
                RegistryError::Validation(msg)
                | RegistryError::Configuration(msg)
                | RegistryError::Serialization(msg)
                | RegistryError::Deserialization(msg) => (error_codes::INVALID_INPUT, msg),
                RegistryError::Io(err) => (error_codes::STORAGE_ERROR, err.to_string()),
                RegistryError::Database(err) => (error_codes::STORAGE_ERROR, err.to_string()),
                RegistryError::DuplicatePath(path) => (
                    error_codes::CONFLICT,
                    format!("Path already registered: {:?}", path),
                ),
            };

            ApiResult::Error(ErrorResponse::new(code, message))
        }
    }
}

/// Whether a stored key still holds an unresolved `${VAR}` placeholder.
fn has_unresolved_placeholder(keys: &[String]) -> bool {
    keys.iter()
        .any(|key| key.contains("${") && key.contains('}'))
}

/// Effective default embedding key, preferring the embedder runtime setting.
fn effective_default_embedding(config: &AppConfig) -> Option<String> {
    if !config.embedder.default_model.trim().is_empty() {
        return Some(config.embedder.default_model.clone());
    }
    config.llm.defaults.embedding.clone()
}

/// Handle read project LLM overview request.
///
/// Returns masked provider summaries, typed model views and dimension
/// diagnostics derived from the merged runtime configuration. Secrets are
/// never returned.
#[utoipa::path(
    get, path = "/api/project/{project_id}/llm/overview", tag = "Project",
    params(("project_id" = i64, Path, description = "Project id")),
    responses(
        (status = 200, body = ProjectLlmOverviewResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_get_project_llm_overview(
    State(state): State<crate::api::state::AppState>,
    Path(project_id): Path<i64>,
) -> ApiResult<ProjectLlmOverviewResponse> {
    let registry = state.engine.project_registry();
    let entry = match registry.get_or_load(project_id).await {
        Ok(entry) => entry,
        Err(e) => {
            let (code, message) = match e {
                RegistryError::ProjectNotFound(_) => (
                    error_codes::ENTITY_NOT_FOUND,
                    format!("Project {} not found", project_id),
                ),
                other => (
                    error_codes::STORAGE_ERROR,
                    format!("Failed to load project: {}", other),
                ),
            };
            return ApiResult::Error(ErrorResponse::new(code, message));
        }
    };
    let config = &entry.config;

    let default_embedding = effective_default_embedding(config);
    let default_chat = config.llm.defaults.chat.clone();
    let default_rerank = config.llm.defaults.rerank.clone();

    let mut providers = Vec::new();
    for (id, provider) in &config.llm.providers {
        let mut embedding_model_count = 0usize;
        let mut chat_model_count = 0usize;
        let mut rerank_model_count = 0usize;
        for model in config.llm.embedding_models.values() {
            if &model.provider_id == id {
                embedding_model_count += 1;
            }
        }
        for model in config.llm.chat_models.values() {
            if &model.provider_id == id {
                chat_model_count += 1;
            }
        }
        for model in config.llm.rerank_models.values() {
            if &model.provider_id == id {
                rerank_model_count += 1;
            }
        }
        providers.push(LlmProviderSummary {
            id: id.clone(),
            name: provider.name.clone(),
            provider_type: format!("{:?}", provider.provider_type).to_lowercase(),
            base_url: provider.base_url.clone(),
            enabled: provider.enabled,
            has_api_keys: !provider.api_keys.is_empty(),
            key_count: provider.api_keys.len(),
            has_unresolved_placeholder: has_unresolved_placeholder(&provider.api_keys),
            api_key_file_set: provider.api_key_file.is_some(),
            timeout_secs: provider.timeout_secs,
            max_retries: provider.max_retries,
            rate_limit: provider.rate_limit,
            embedding_model_count,
            chat_model_count,
            rerank_model_count,
        });
    }
    providers.sort_by(|a, b| a.id.cmp(&b.id));

    let mut embedding_models = Vec::new();
    for (key, model) in &config.llm.embedding_models {
        let provider_name = config
            .llm
            .providers
            .get(&model.provider_id)
            .map(|provider| provider.name.clone())
            .unwrap_or_default();
        embedding_models.push(EmbeddingModelView {
            key: key.clone(),
            provider_id: model.provider_id.clone(),
            provider_name,
            model: model.model.clone(),
            vector_dimension: model.vector_dimension,
            max_batch_tokens: model.max_batch_tokens,
            max_item_tokens: model.max_item_tokens,
            is_default: default_embedding.as_deref() == Some(key.as_str()),
        });
    }
    embedding_models.sort_by(|a, b| a.key.cmp(&b.key));

    let mut chat_models = Vec::new();
    for (key, model) in &config.llm.chat_models {
        let provider_name = config
            .llm
            .providers
            .get(&model.provider_id)
            .map(|provider| provider.name.clone())
            .unwrap_or_default();
        chat_models.push(ChatModelView {
            key: key.clone(),
            provider_id: model.provider_id.clone(),
            provider_name,
            model: model.model.clone(),
            temperature: model.temperature,
            max_tokens: model.max_tokens,
            top_p: model.top_p,
            max_input_tokens: model.max_input_tokens,
            is_default: default_chat.as_deref() == Some(key.as_str()),
        });
    }
    chat_models.sort_by(|a, b| a.key.cmp(&b.key));

    let mut rerank_models = Vec::new();
    for (key, model) in &config.llm.rerank_models {
        let provider_name = config
            .llm
            .providers
            .get(&model.provider_id)
            .map(|provider| provider.name.clone())
            .unwrap_or_default();
        rerank_models.push(RerankModelView {
            key: key.clone(),
            provider_id: model.provider_id.clone(),
            provider_name,
            model: model.model.clone(),
            mode: format!("{:?}", model.mode).to_lowercase(),
            is_default: default_rerank.as_deref() == Some(key.as_str()),
        });
    }
    rerank_models.sort_by(|a, b| a.key.cmp(&b.key));

    let vector_backend = format!("{:?}", config.database.vector_backend).to_lowercase();
    let effective_vector_size = config.database.vector_dimension();
    let default_embedding_dimension = default_embedding.as_deref().and_then(|key| {
        config
            .llm
            .embedding_models
            .get(key)
            .map(|model| model.vector_dimension)
    });
    let matches = default_embedding_dimension
        .map(|dimension| dimension == effective_vector_size)
        .unwrap_or(true);
    let dimension = LlmDimensionStatus {
        vector_backend,
        effective_vector_size,
        default_embedding_model: default_embedding.clone(),
        default_embedding_dimension,
        matches,
        requires_reindex: !matches,
        message: if matches {
            "Default embedding dimension matches the active vector backend".to_string()
        } else {
            "Default embedding dimension differs from the vector backend; changing the embedding model requires a full reindex".to_string()
        },
    };

    let mut missing_api_key_providers = Vec::new();
    for (id, provider) in &config.llm.providers {
        if provider.enabled && provider.api_keys.is_empty() && provider.api_key_file.is_none() {
            missing_api_key_providers.push(id.clone());
        }
    }
    missing_api_key_providers.sort();

    let mut unknown_provider_refs = Vec::new();
    for (key, model) in &config.llm.embedding_models {
        if !config.llm.providers.contains_key(&model.provider_id) {
            unknown_provider_refs.push(format!("embedding:{key}->{}", model.provider_id));
        }
    }
    for (key, model) in &config.llm.chat_models {
        if !config.llm.providers.contains_key(&model.provider_id) {
            unknown_provider_refs.push(format!("chat:{key}->{}", model.provider_id));
        }
    }
    for (key, model) in &config.llm.rerank_models {
        if !config.llm.providers.contains_key(&model.provider_id) {
            unknown_provider_refs.push(format!("rerank:{key}->{}", model.provider_id));
        }
    }
    unknown_provider_refs.sort();

    ApiResult::Success(ProjectLlmOverviewResponse {
        success: true,
        project_id,
        config_version: entry.version,
        llm_enabled: config.llm.enabled,
        providers,
        embedding_models,
        chat_models,
        rerank_models,
        default_embedding,
        default_chat,
        default_rerank,
        rerank_enabled: config.rerank.enabled,
        rerank_max_candidates: config.rerank.max_candidates,
        dimension,
        missing_api_key_providers,
        unknown_provider_refs,
        ephemeral_note: "Project LLM edits apply to in-memory runtime state and are discarded on reload from file or restart. Provider connection details stay global.".to_string(),
    })
}

/// Handle typed project LLM update request.
///
/// Only project-effective chat and rerank fields are accepted. The update is
/// validated, stored in memory and component caches are cleared without
/// invalidating the registry entry.
#[utoipa::path(
    put, path = "/api/project/{project_id}/llm/config", tag = "Project",
    params(("project_id" = i64, Path, description = "Project id")),
    request_body = ProjectLlmUpdateRequest,
    responses(
        (status = 200, body = ProjectLlmUpdateResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_update_project_llm_config(
    State(state): State<crate::api::state::AppState>,
    Path(project_id): Path<i64>,
    Json(payload): Json<ProjectLlmUpdateRequest>,
) -> ApiResult<ProjectLlmUpdateResponse> {
    let registry = state.engine.project_registry();
    let entry = match registry.get_or_load(project_id).await {
        Ok(entry) => entry,
        Err(e) => {
            let (code, message) = match &e {
                RegistryError::ProjectNotFound(id) => (
                    error_codes::ENTITY_NOT_FOUND,
                    format!("Project {} not found", id),
                ),
                other => (
                    error_codes::STORAGE_ERROR,
                    format!("Failed to load project: {}", other),
                ),
            };
            return ApiResult::Error(ErrorResponse::new(code, message));
        }
    };
    let mut config = entry.config.clone();

    if let Some(ref default_chat) = payload.default_chat {
        if !config.llm.chat_models.contains_key(default_chat) {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::INVALID_INPUT,
                format!("Unknown chat model '{default_chat}'"),
            ));
        }
    }
    if let Some(ref default_rerank) = payload.default_rerank {
        if !config.llm.rerank_models.contains_key(default_rerank) {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::INVALID_INPUT,
                format!("Unknown rerank model '{default_rerank}'"),
            ));
        }
    }
    if let Some(max_candidates) = payload.rerank_max_candidates {
        if !(1..=1000).contains(&max_candidates) {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::INVALID_INPUT,
                "rerank_max_candidates must be within 1..=1000".to_string(),
            ));
        }
    }
    if let Some(ref params) = payload.chat_model_params {
        for (key, patch) in params {
            let model = match config.llm.chat_models.get(key) {
                Some(model) => model,
                None => {
                    return ApiResult::Error(ErrorResponse::new(
                        error_codes::INVALID_INPUT,
                        format!("Unknown chat model '{key}'"),
                    ));
                }
            };
            let temperature = patch.temperature.unwrap_or(model.temperature);
            let max_tokens = patch.max_tokens.unwrap_or(model.max_tokens);
            let top_p = patch.top_p.unwrap_or(model.top_p);
            let max_input_tokens = patch.max_input_tokens.unwrap_or(model.max_input_tokens);
            if !(0.0..=2.0).contains(&temperature) {
                return ApiResult::Error(ErrorResponse::new(
                    error_codes::INVALID_INPUT,
                    format!("chat model '{key}' temperature must be within 0.0..=2.0"),
                ));
            }
            if !(1..=128000).contains(&max_tokens) {
                return ApiResult::Error(ErrorResponse::new(
                    error_codes::INVALID_INPUT,
                    format!("chat model '{key}' max_tokens must be within 1..=128000"),
                ));
            }
            if !(0.0..=1.0).contains(&top_p) {
                return ApiResult::Error(ErrorResponse::new(
                    error_codes::INVALID_INPUT,
                    format!("chat model '{key}' top_p must be within 0.0..=1.0"),
                ));
            }
            if !(1..=1000000).contains(&max_input_tokens) {
                return ApiResult::Error(ErrorResponse::new(
                    error_codes::INVALID_INPUT,
                    format!("chat model '{key}' max_input_tokens must be within 1..=1000000"),
                ));
            }
        }
    }

    if let Some(default_chat) = payload.default_chat {
        config.llm.defaults.chat = Some(default_chat);
    }
    if let Some(default_rerank) = payload.default_rerank {
        config.llm.defaults.rerank = Some(default_rerank);
    }
    if let Some(enabled) = payload.rerank_enabled {
        config.rerank.enabled = enabled;
    }
    if let Some(max_candidates) = payload.rerank_max_candidates {
        config.rerank.max_candidates = max_candidates;
    }
    if let Some(params) = payload.chat_model_params {
        for (key, patch) in params {
            if let Some(model) = config.llm.chat_models.get_mut(&key) {
                if let Some(temperature) = patch.temperature {
                    model.temperature = temperature;
                }
                if let Some(max_tokens) = patch.max_tokens {
                    model.max_tokens = max_tokens;
                }
                if let Some(top_p) = patch.top_p {
                    model.top_p = top_p;
                }
                if let Some(max_input_tokens) = patch.max_input_tokens {
                    model.max_input_tokens = max_input_tokens;
                }
            }
        }
    }

    if let Err(e) = config.validate_structured() {
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_INPUT,
            format!("Configuration validation failed: {}", e),
        ));
    }

    let registry_ref = &**registry;
    if let Err(e) = registry_ref.update_config(project_id, config).await {
        let (code, message) = match &e {
            RegistryError::ProjectNotFound(id) => (
                error_codes::ENTITY_NOT_FOUND,
                format!("Project {} not found", id),
            ),
            other => (
                error_codes::STORAGE_ERROR,
                format!("Failed to update project LLM config: {}", other),
            ),
        };
        return ApiResult::Error(ErrorResponse::new(code, message));
    }

    state
        .engine
        .clear_project_component_caches(project_id)
        .await;

    let mut hot_reload_applied = true;
    match state.engine.get_hot_update_coordinator(project_id).await {
        Ok(coordinator) => {
            let mut coord = coordinator.lock().await;
            if let Err(e) = coord.reload_project_config().await {
                tracing::warn!(
                    project_id,
                    error = %e,
                    "Hot-update reload failed after LLM config update"
                );
                hot_reload_applied = false;
            }
        }
        Err(e) => {
            tracing::debug!(
                project_id,
                error = %e,
                "HotUpdateCoordinator not available after LLM config update"
            );
            hot_reload_applied = false;
        }
    }

    let version = registry_ref
        .get_or_load(project_id)
        .await
        .map(|entry| entry.version)
        .unwrap_or(0);

    ApiResult::Success(ProjectLlmUpdateResponse {
        success: true,
        project_id,
        config_version: version,
        hot_reload_applied,
        ephemeral: true,
        message: if hot_reload_applied {
            "Project LLM config updated in memory; reload from file or restart discards it"
                .to_string()
        } else {
            "Project LLM config updated in memory; hot reload skipped and restart may be needed for some components".to_string()
        },
    })
}

/// Handle project LLM connectivity probe.
///
/// Uses the stored project configuration and discards all transient client
/// state afterwards. No secrets are accepted or returned.
#[utoipa::path(
    post, path = "/api/project/{project_id}/llm/test", tag = "Project",
    params(("project_id" = i64, Path, description = "Project id")),
    request_body = LlmTestRequest,
    responses(
        (status = 200, body = LlmTestResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_test_project_llm(
    State(state): State<crate::api::state::AppState>,
    Path(project_id): Path<i64>,
    Json(payload): Json<LlmTestRequest>,
) -> ApiResult<LlmTestResponse> {
    let registry = state.engine.project_registry();
    let entry = match registry.get_or_load(project_id).await {
        Ok(entry) => entry,
        Err(e) => {
            let (code, message) = match &e {
                RegistryError::ProjectNotFound(id) => (
                    error_codes::ENTITY_NOT_FOUND,
                    format!("Project {} not found", id),
                ),
                other => (
                    error_codes::STORAGE_ERROR,
                    format!("Failed to load project: {}", other),
                ),
            };
            return ApiResult::Error(ErrorResponse::new(code, message));
        }
    };
    let project_config = entry.config.clone();
    let service = payload.service.trim().to_lowercase();
    let model_key = payload.model_key.trim().to_string();
    if model_key.is_empty() {
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_INPUT,
            "model_key must not be empty".to_string(),
        ));
    }

    if service == "embedding" {
        if !project_config.llm.embedding_models.contains_key(&model_key) {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::INVALID_INPUT,
                format!("Unknown embedding model '{model_key}'"),
            ));
        }
        let started = std::time::Instant::now();
        let probe =
            cce_llm_client::OpenAICompatibleProvider::from_model(&project_config, &model_key)
                .map_err(|e| {
                    ErrorResponse::new(
                        error_codes::INVALID_INPUT,
                        format!("Failed to resolve embedding model '{model_key}': {e}"),
                    )
                });
        let probe = match probe {
            Ok(probe) => probe,
            Err(error) => return ApiResult::Error(error),
        };
        match probe.embed_one("cce connectivity probe").await {
            Ok(vector) => {
                let dimension = vector.len();
                ApiResult::Success(LlmTestResponse {
                    success: true,
                    service: service.clone(),
                    model_key: model_key.clone(),
                    latency_ms: started.elapsed().as_millis() as u64,
                    dimension: Some(dimension),
                    message: format!("Embedding probe succeeded with {dimension} dimensions"),
                })
            }
            Err(e) => ApiResult::Success(LlmTestResponse {
                success: false,
                service: service.clone(),
                model_key: model_key.clone(),
                latency_ms: started.elapsed().as_millis() as u64,
                dimension: None,
                message: format!("Embedding probe failed: {e}"),
            }),
        }
    } else if service == "chat" {
        if !project_config.llm.chat_models.contains_key(&model_key) {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::INVALID_INPUT,
                format!("Unknown chat model '{model_key}'"),
            ));
        }
        let handle = cce_llm_client::factory::build_chat_client(&project_config, &model_key)
            .map_err(|e| {
                ErrorResponse::new(
                    error_codes::INVALID_INPUT,
                    format!("Failed to resolve chat model '{model_key}': {e}"),
                )
            });
        let handle = match handle {
            Ok(handle) => handle,
            Err(error) => return ApiResult::Error(error),
        };
        let started = std::time::Instant::now();
        let messages = vec![cce_llm::Message::user("Reply with ok.")];
        match handle.client.chat(&messages, &handle.config).await {
            Ok(result) => ApiResult::Success(LlmTestResponse {
                success: true,
                service: service.clone(),
                model_key: model_key.clone(),
                latency_ms: started.elapsed().as_millis() as u64,
                dimension: None,
                message: format!("Chat probe succeeded ({} chars)", result.content.len()),
            }),
            Err(e) => ApiResult::Success(LlmTestResponse {
                success: false,
                service: service.clone(),
                model_key: model_key.clone(),
                latency_ms: started.elapsed().as_millis() as u64,
                dimension: None,
                message: format!("Chat probe failed: {e}"),
            }),
        }
    } else {
        ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_INPUT,
            "service must be \"embedding\" or \"chat\"".to_string(),
        ))
    }
}
