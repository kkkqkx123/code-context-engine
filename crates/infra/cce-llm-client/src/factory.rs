//! LLM client factory

use std::collections::HashMap;
use std::sync::Arc;

use cce_config::AppConfig;
use cce_config::global::ResolvedLlmConnection;
use cce_config::modules::{RerankMode, ServiceType};

use crate::core::client::HttpLlmClient;
use crate::core::config::{ChatConfig, LlmConfig};
use crate::core::error::LlmError;
use crate::core::retry::RetryPolicy;
use crate::rate_limiter_registry::global_rate_limiter_registry;
use cce_metrics::LlmRetryMetrics;

fn build_client_with_endpoints(
    connection: &ResolvedLlmConnection,
    endpoints: HashMap<ServiceType, String>,
    max_input_tokens: Option<usize>,
    retry_metrics: Option<Arc<LlmRetryMetrics>>,
) -> Result<Arc<HttpLlmClient>, LlmError> {
    let llm_config = LlmConfig {
        api_keys: connection.api_keys.clone(),
        base_url: connection.base_url.clone(),
        timeout_secs: connection.timeout_secs,
        max_retries: connection.max_retries,
        retry_delay_ms: connection.retry_delay_ms,
        retry_jitter: connection.retry_jitter,
        rate_limit_max_retries: connection.rate_limit_max_retries,
        rate_limit_max_delay_ms: connection.rate_limit_max_delay_ms,
        circuit_breaker: connection.circuit_breaker.clone(),
        proxy_url: connection.proxy_url.clone(),
        extra_headers: connection.extra_headers.clone(),
        extra_params: connection.extra_params.clone(),
        endpoints,
    };

    let registry = global_rate_limiter_registry();
    let rate_limiter = registry.limiter_for(&connection.base_url, connection.rate_limit);
    let circuit_breaker =
        registry.circuit_breaker_for(&connection.base_url, &connection.circuit_breaker);

    let retry_policy = RetryPolicy::new(connection.max_retries, connection.retry_delay_ms)
        .with_jitter_ratio(connection.retry_jitter)
        .with_rate_limit_budget(
            connection.rate_limit_max_retries,
            connection.rate_limit_max_delay_ms,
        );

    let mut builder = HttpLlmClient::builder()
        .with_config(llm_config)
        .with_provider_id(connection.provider_id.clone())
        .with_rate_limiter(rate_limiter)
        .with_retry_policy(retry_policy)
        .with_circuit_breaker(circuit_breaker)
        .with_retry_metrics(retry_metrics);
    if let Some(max_input_tokens) = max_input_tokens {
        builder = builder.with_max_input_tokens(max_input_tokens);
    }
    let client = Arc::new(builder.build()?);

    Ok(client)
}

/// Build an HTTP LLM client for a registered model of the given service.
pub fn build_llm_client(
    global_config: &AppConfig,
    model_key: &str,
    service: ServiceType,
    max_input_tokens: Option<usize>,
    retry_metrics: Option<Arc<LlmRetryMetrics>>,
) -> Result<Arc<HttpLlmClient>, LlmError> {
    let connection = global_config
        .resolve_llm_connection(model_key, service)
        .map_err(|e| {
            LlmError::config(format!(
                "Failed to resolve model '{}' for {:?}: {}",
                model_key, service, e
            ))
        })?;

    let endpoints = HashMap::from([(service, connection.endpoint_path.clone())]);
    let client =
        build_client_with_endpoints(&connection, endpoints, max_input_tokens, retry_metrics)
            .map_err(|e| {
                LlmError::config(format!(
                    "Failed to create LLM client for '{}': {}",
                    model_key, e
                ))
            })?;

    tracing::info!(
        model = %model_key,
        provider = %connection.base_url,
        endpoint = %connection.endpoint_path,
        "LLM client initialized for {:?}",
        service
    );

    Ok(client)
}

/// Build an HTTP LLM client for a rerank model registered in
/// `[llm.rerank_models]`.
pub fn build_rerank_client(
    global_config: &AppConfig,
    model_key: &str,
    mode: RerankMode,
    retry_metrics: Option<Arc<LlmRetryMetrics>>,
) -> Result<Arc<HttpLlmClient>, LlmError> {
    let connection = global_config
        .resolve_llm_connection(model_key, ServiceType::Rerank)
        .map_err(|e| {
            LlmError::config(format!(
                "Failed to resolve rerank model '{}': {}",
                model_key, e
            ))
        })?;

    let provider = global_config
        .llm
        .providers
        .get(&connection.provider_id)
        .ok_or_else(|| {
            LlmError::config(format!(
                "Provider '{}' not found for rerank model '{}'",
                connection.provider_id, model_key
            ))
        })?;

    let mut endpoints = HashMap::new();
    endpoints.insert(
        ServiceType::Chat,
        provider.get_endpoint_path(ServiceType::Chat),
    );
    endpoints.insert(
        ServiceType::Rerank,
        provider.get_endpoint_path(ServiceType::Rerank),
    );

    let client =
        build_client_with_endpoints(&connection, endpoints, None, retry_metrics).map_err(|e| {
            LlmError::config(format!(
                "Failed to create LLM client for '{}': {}",
                model_key, e
            ))
        })?;

    let endpoint = provider.get_endpoint_path(match mode {
        RerankMode::Generative => ServiceType::Chat,
        RerankMode::CrossEncoder => ServiceType::Rerank,
    });

    tracing::info!(
        model = %model_key,
        provider = %connection.base_url,
        endpoint = %endpoint,
        mode = ?mode,
        "LLM client initialized for rerank"
    );

    Ok(client)
}

/// A chat client together with the resolved chat call configuration.
#[derive(Debug, Clone)]
pub struct ChatClientHandle {
    /// HTTP client bound to the resolved provider connection.
    pub client: Arc<HttpLlmClient>,
    /// Chat call parameters (model, temperature, max tokens, ...).
    pub config: ChatConfig,
}

/// Build a chat client and its call configuration from the global config.
pub fn build_chat_client(
    global_config: &AppConfig,
    model_key: &str,
    retry_metrics: Option<Arc<LlmRetryMetrics>>,
) -> Result<ChatClientHandle, LlmError> {
    let resolved = global_config.resolve_chat_config(model_key).map_err(|e| {
        LlmError::config(format!(
            "Failed to resolve chat model '{}': {}",
            model_key, e
        ))
    })?;

    let client = build_llm_client(
        global_config,
        model_key,
        ServiceType::Chat,
        Some(resolved.max_input_tokens),
        retry_metrics,
    )?;

    let config = ChatConfig {
        model: resolved.model.clone(),
        temperature: resolved.temperature,
        max_tokens: resolved.max_tokens,
        top_p: resolved.top_p,
        ..Default::default()
    };

    Ok(ChatClientHandle { client, config })
}
