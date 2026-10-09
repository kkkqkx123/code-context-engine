//! LLM client factory
//!
//! The factory resolves model keys from the global config and assembles
//! llm-suite objects: gateway chat profiles, embedding transports (built in
//! the embedding provider) and rerank providers.

use std::sync::Arc;

use cce_config::AppConfig;
use cce_config::modules::ServiceType;

use crate::core::config::ChatConfig;
use crate::core::error::LlmError;
use crate::services::rerank::{CohereRerankProvider, GenerativeRerankProvider};
use crate::suite::{
    SuiteChatClient, ensure_chat_profile, full_endpoint_url, generative_chat_endpoint,
    map_rerank_error, rerank_endpoint_config,
};

/// Builds the chat profile for a registered chat model and returns a client
/// bound to it together with the resolved call configuration.
///
/// Chat retries run inside the gateway; per-attempt retry accounting is
/// therefore gateway-owned and no metrics handle is accepted here.
pub fn build_chat_client(
    global_config: &AppConfig,
    model_key: &str,
) -> Result<ChatClientHandle, LlmError> {
    let resolved = global_config.resolve_chat_config(model_key).map_err(|e| {
        LlmError::config(format!(
            "Failed to resolve chat model '{}': {}",
            model_key, e
        ))
    })?;
    let connection = global_config
        .resolve_llm_connection(model_key, ServiceType::Chat)
        .map_err(|e| {
            LlmError::config(format!(
                "Failed to resolve model '{}' for Chat: {}",
                model_key, e
            ))
        })?;

    let profile_id = ensure_chat_profile(model_key, &resolved, &connection)?;

    let config = ChatConfig {
        model: resolved.model.clone(),
        temperature: resolved.temperature,
        max_tokens: resolved.max_tokens,
        top_p: resolved.top_p,
        ..Default::default()
    };

    tracing::info!(
        model = %model_key,
        provider = %connection.base_url,
        endpoint = %connection.endpoint_path,
        "LLM chat client initialized for Chat",
    );

    Ok(ChatClientHandle {
        client: Arc::new(SuiteChatClient::new(profile_id)),
        config,
    })
}

/// A chat client together with the resolved chat call configuration.
#[derive(Debug, Clone)]
pub struct ChatClientHandle {
    /// Gateway-backed client bound to the resolved provider connection.
    pub client: Arc<SuiteChatClient>,
    /// Chat call parameters (model, temperature, max tokens, ...).
    pub config: ChatConfig,
}

/// Builds a generative rerank provider for a registered rerank model.
pub fn build_generative_rerank_provider(
    global_config: &AppConfig,
    model_key: &str,
) -> Result<GenerativeRerankProvider, LlmError> {
    let connection = global_config
        .resolve_llm_connection(model_key, ServiceType::Rerank)
        .map_err(|e| {
            LlmError::config(format!(
                "Failed to resolve rerank model '{}': {}",
                model_key, e
            ))
        })?;
    let model_config =
        global_config
            .llm
            .rerank_models
            .get(model_key)
            .ok_or_else(|| {
                LlmError::config(format!(
                    "Rerank model '{model_key}' not found in llm.rerank_models"
                ))
            })?;
    let provider = global_config
        .llm
        .providers
        .get(&connection.provider_id)
        .ok_or_else(|| {
            LlmError::config(format!(
                "Provider '{}' not found for rerank model '{model_key}'",
                connection.provider_id
            ))
        })?;
    let chat_url = full_endpoint_url(
        &connection.base_url,
        &provider.get_endpoint_path(ServiceType::Chat),
    );
    let endpoint = generative_chat_endpoint(&connection, chat_url, model_config.model.clone());
    let inner = llm_rerank::GenerativeRerankProvider::new(endpoint, connection.timeout_secs)
        .map_err(map_rerank_error)?;

    tracing::info!(
        model = %model_key,
        provider = %connection.base_url,
        "LLM client initialized for generative rerank",
    );

    Ok(GenerativeRerankProvider::new(inner))
}

/// Builds a cross-encoder rerank provider for a registered rerank model.
pub fn build_cohere_rerank_provider(
    global_config: &AppConfig,
    model_key: &str,
) -> Result<CohereRerankProvider, LlmError> {
    let connection = global_config
        .resolve_llm_connection(model_key, ServiceType::Rerank)
        .map_err(|e| {
            LlmError::config(format!(
                "Failed to resolve rerank model '{}': {}",
                model_key, e
            ))
        })?;
    let model_config =
        global_config
            .llm
            .rerank_models
            .get(model_key)
            .ok_or_else(|| {
                LlmError::config(format!(
                    "Rerank model '{model_key}' not found in llm.rerank_models"
                ))
            })?;
    let rerank_url = full_endpoint_url(&connection.base_url, &connection.endpoint_path);
    let config = rerank_endpoint_config(&connection, rerank_url, model_config.model.clone());
    let inner =
        llm_rerank::CohereRerankProvider::new(config).map_err(map_rerank_error)?;

    tracing::info!(
        model = %model_key,
        provider = %connection.base_url,
        endpoint = %connection.endpoint_path,
        "LLM client initialized for cross-encoder rerank",
    );

    Ok(CohereRerankProvider::new(inner))
}
