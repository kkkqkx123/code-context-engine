//! Project configuration handlers
//!
//! Handles configuration update and reload operations. Validation, persistence
//! and the post-write side effects live in the shared project service; the
//! handlers only decode the wire payloads and map service errors.

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
use cce_server_shared::services::project::{
    LlmProbeService, ProjectLlmUpdateSpec, ProjectServiceError, probe_project_llm,
    read_project_profile, registry_error, update_project_llm, update_project_profile,
};

/// Map a project service failure onto the HTTP error contract.
fn project_error(error: ProjectServiceError) -> ErrorResponse {
    let (code, message) = match error {
        ProjectServiceError::MetadataUnavailable => (
            error_codes::INTERNAL_ERROR,
            "metadata store not initialized".to_string(),
        ),
        ProjectServiceError::NotFound(id) => (
            error_codes::ENTITY_NOT_FOUND,
            format!("Project {id} not found"),
        ),
        ProjectServiceError::Invalid(message) => (error_codes::INVALID_INPUT, message),
        ProjectServiceError::Conflict(message) => (error_codes::CONFLICT, message),
        ProjectServiceError::Storage(message) => (error_codes::STORAGE_ERROR, message),
    };
    ErrorResponse::new(code, message)
}

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
    State(state): State<cce_server_shared::context::SharedContext>,
    Path(project_id): Path<i64>,
    Json(payload): Json<ProjectConfigUpdateRequest>,
) -> ApiResult<ProjectConfigUpdateResponse> {
    match update_project_profile(&state, project_id, payload.config).await {
        Ok(outcome) => ApiResult::Success(ProjectConfigUpdateResponse {
            success: true,
            hot_reload_applied: outcome.hot_reload_applied,
            message: if outcome.hot_reload_applied {
                "Configuration updated and hot reload triggered successfully".to_string()
            } else {
                "Configuration updated, but hot reload failed. Manual restart may be required for some components.".to_string()
            },
            config_version: outcome.config_version,
        }),
        Err(error) => {
            tracing::error!(project_id, error = %error, "Failed to update project profile");
            ApiResult::Error(project_error(error))
        }
    }
}

/// Handle read stored project profile request
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
    State(state): State<cce_server_shared::context::SharedContext>,
    Path(project_id): Path<i64>,
) -> ApiResult<ProjectConfigGetResponse> {
    match read_project_profile(&state, project_id).await {
        Ok((config, version)) => ApiResult::Success(ProjectConfigGetResponse {
            success: true,
            project_id,
            config_version: version,
            config,
        }),
        Err(error) => ApiResult::Error(project_error(error)),
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
    State(state): State<cce_server_shared::context::SharedContext>,
    Path(id): Path<i64>,
) -> ApiResult<ProjectConfigReloadResponse> {
    let project_registry = state.engine.project_registry();
    let _ = project_registry.invalidate_cache(Some(id)).await;

    if let Err(error) = state.engine.reload_project_config(id).await {
        tracing::warn!(
            project_id = id,
            error = %error,
            "Failed to reload engine components (non-critical)"
        );
    }

    match project_registry.get_or_load(id).await {
        Ok(entry) => ApiResult::Success(ProjectConfigReloadResponse {
            success: true,
            message: "Configuration cache invalidated. Effective configuration is rebuilt from the global configuration plus the project record on next access."
                .to_string(),
            project_id: id,
            config_version: entry.version,
        }),
        Err(error) => {
            tracing::error!(project_id = id, error = %error, "Failed to reload project config");
            ApiResult::Error(project_error(registry_error(error)))
        }
    }
}

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
    State(state): State<cce_server_shared::context::SharedContext>,
    Path(project_id): Path<i64>,
) -> ApiResult<ProjectLlmOverviewResponse> {
    let registry = state.engine.project_registry();
    let entry = match registry.get_or_load(project_id).await {
        Ok(entry) => entry,
        Err(e) => return ApiResult::Error(project_error(registry_error(e))),
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
        ephemeral_note: "Project LLM selections are persisted in the project profile; provider connection details stay global.".to_string(),
    })
}

/// Handle typed project LLM update request.
///
/// Only project-effective chat and rerank selections are accepted. The update
/// is validated and persisted as part of the project profile document, so a
/// restart keeps the values. Provider connection details and per-model
/// tuning stay global and are rejected here.
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
    State(state): State<cce_server_shared::context::SharedContext>,
    Path(project_id): Path<i64>,
    Json(payload): Json<ProjectLlmUpdateRequest>,
) -> ApiResult<ProjectLlmUpdateResponse> {
    let spec = ProjectLlmUpdateSpec {
        chat_model_params_present: payload
            .chat_model_params
            .as_ref()
            .is_some_and(|params| !params.is_empty()),
        default_chat: payload.default_chat,
        default_rerank: payload.default_rerank,
        rerank_enabled: payload.rerank_enabled,
        rerank_max_candidates: payload.rerank_max_candidates,
    };
    match update_project_llm(&state, project_id, spec).await {
        Ok(outcome) => ApiResult::Success(ProjectLlmUpdateResponse {
            success: true,
            project_id,
            config_version: outcome.config_version,
            hot_reload_applied: outcome.hot_reload_applied,
            ephemeral: false,
            message: if outcome.hot_reload_applied {
                "Project LLM profile persisted and hot reload triggered successfully".to_string()
            } else {
                "Project LLM profile persisted, but hot reload failed. Manual restart may be required for some components".to_string()
            },
        }),
        Err(error) => ApiResult::Error(project_error(error)),
    }
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
    State(state): State<cce_server_shared::context::SharedContext>,
    Path(project_id): Path<i64>,
    Json(payload): Json<LlmTestRequest>,
) -> ApiResult<LlmTestResponse> {
    let Some(service) = LlmProbeService::parse(&payload.service) else {
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_INPUT,
            "service must be \"embedding\" or \"chat\"".to_string(),
        ));
    };
    match probe_project_llm(&state, project_id, service, &payload.model_key).await {
        Ok(outcome) => ApiResult::Success(LlmTestResponse {
            success: outcome.success,
            service: outcome.service.label().to_string(),
            model_key: outcome.model_key,
            latency_ms: outcome.latency_ms,
            dimension: outcome.dimension,
            message: outcome.message,
        }),
        Err(error) => ApiResult::Error(project_error(error)),
    }
}
