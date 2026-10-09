//! Bridge from CCE configuration and error types to llm-suite.
//!
//! This module is the single assembly point for llm-suite objects: it owns
//! the process-wide [`LlmGateway`], converts resolved CCE connections into
//! provider definitions and profiles, and converts llm-suite errors back
//! into [`LlmError`] so existing callers keep their error handling.

use std::collections::HashMap;
use std::sync::OnceLock;

use cce_config::global::{ResolvedChatConfig, ResolvedLlmConnection};
use cce_config::modules::ServiceType;
use cce_llm::{ChatConfig, ChatResult, LlmClient, LlmError, Message, MessageRole};
use cce_types::error::common::TimeoutError;

/// Process-wide gateway. Breakers and rate limiters live in the gateway
/// keyed by base URL, so sharing one instance preserves the previous
/// per-endpoint sharing across embedder, chat and rerank clients.
pub fn global_gateway() -> &'static llm_gateway::LlmGateway {
    static GATEWAY: OnceLock<llm_gateway::LlmGateway> = OnceLock::new();
    GATEWAY.get_or_init(|| {
        let gateway = llm_gateway::LlmGateway::new();
        match TOKEN_SINK.get() {
            Some(sink) => gateway.with_token_metrics(std::sync::Arc::new(sink.clone())),
            None => gateway,
        }
    })
}

/// Registry-backed implementation of the llm-suite gateway metrics sinks.
///
/// Installed once at engine startup via [`init_global_token_metrics`]; the
/// gateway reports token usage, request latency/outcome and retry counts
/// through it under the `llm_gateway_*` dashboard caliber.
#[derive(Debug, Clone)]
pub struct GatewayMetricsSink {
    registry: cce_metrics::MetricsRegistry,
}

/// Latency histogram buckets (milliseconds), shared by the gateway latency
/// instruments.
const GATEWAY_LATENCY_BUCKETS: [f64; 8] = [10.0, 50.0, 100.0, 250.0, 500.0, 1000.0, 2500.0, 5000.0];

impl GatewayMetricsSink {
    /// Builds a sink writing into the given registry.
    pub fn new(registry: &cce_metrics::MetricsRegistry) -> Self {
        Self {
            registry: registry.clone(),
        }
    }
}

impl llm_client::TokenUsageSink for GatewayMetricsSink {
    fn record_token_usage(
        &self,
        prompt_tokens: u64,
        completion_tokens: u64,
        _total_cost: Option<f64>,
        model: Option<&str>,
    ) {
        let model = model.unwrap_or("unknown");
        self.registry
            .counter("llm_gateway_tokens_prompt_total", &[("model", model)])
            .add(prompt_tokens);
        self.registry
            .counter("llm_gateway_tokens_completion_total", &[("model", model)])
            .add(completion_tokens);
    }
}

impl llm_client::LlmMetricsSink for GatewayMetricsSink {
    fn record_first_byte(&self, duration_ms: f64, model: Option<&str>) {
        let model = model.unwrap_or("unknown");
        self.registry
            .histogram(
                "llm_gateway_first_byte_latency_ms",
                GATEWAY_LATENCY_BUCKETS.to_vec(),
                &[("model", model)],
            )
            .observe(duration_ms);
    }

    fn record_request(
        &self,
        duration_ms: f64,
        success: bool,
        error_kind: Option<&str>,
        model: Option<&str>,
    ) {
        let model = model.unwrap_or("unknown");
        let status = if success {
            "ok"
        } else {
            error_kind.unwrap_or("error")
        };
        self.registry
            .counter(
                "llm_gateway_requests_total",
                &[("model", model), ("status", status)],
            )
            .increment();
        self.registry
            .histogram(
                "llm_gateway_request_latency_ms",
                GATEWAY_LATENCY_BUCKETS.to_vec(),
                &[("model", model)],
            )
            .observe(duration_ms);
    }

    fn record_retry(&self, model: Option<&str>) {
        let model = model.unwrap_or("unknown");
        self.registry
            .counter("llm_gateway_retries_total", &[("model", model)])
            .increment();
    }
}

static TOKEN_SINK: OnceLock<GatewayMetricsSink> = OnceLock::new();

/// Installs the process-wide gateway metrics sink.
///
/// Call once during engine startup before any chat request; a second call
/// is ignored so late initialization can never replace an active sink.
pub fn init_global_token_metrics(registry: &cce_metrics::MetricsRegistry) {
    let _ = TOKEN_SINK.set(GatewayMetricsSink::new(registry));
}

/// Stable profile-id namespace: one profile per model key and service so
/// re-registering one model never clobbers another.
pub fn profile_id_for(model_key: &str, service: ServiceType) -> String {
    let slug = match service {
        ServiceType::Embedding => "embedding",
        ServiceType::Chat => "chat",
        ServiceType::Rerank => "rerank",
        ServiceType::Completion => "completion",
    };
    format!("cce-{slug}-{model_key}")
}

/// Joins a provider base URL and an endpoint path the way the previous
/// HTTP layer did.
pub fn full_endpoint_url(base_url: &str, endpoint_path: &str) -> String {
    format!(
        "{}/{}",
        base_url.trim_end_matches('/'),
        endpoint_path.trim_start_matches('/')
    )
}

/// Converts a per-minute provider budget into a token-bucket config.
/// A zero budget means unlimited and yields no limiter.
pub fn rate_limit_config(per_minute: u32) -> Option<llm_types::llm::RateLimitConfig> {
    if per_minute == 0 {
        return None;
    }
    let capped = per_minute.min(10_000) as f64;
    let requests_per_second = capped / 60.0;
    Some(llm_types::llm::RateLimitConfig {
        requests_per_second,
        burst: requests_per_second.ceil() as u32,
    })
}

/// Converts the count-based CCE breaker settings into the sampling-based
/// llm-suite config. The failure count becomes the minimum sample window
/// with a majority-failure threshold, which approximates the previous
/// consecutive-failure trip point.
pub fn circuit_breaker_config(
    config: &cce_config::modules::CircuitBreakerConfig,
) -> Option<llm_types::llm::CircuitBreakerConfig> {
    if !config.enabled {
        return None;
    }
    Some(llm_types::llm::CircuitBreakerConfig {
        min_samples: config.failure_threshold.max(1),
        failure_threshold: 0.5,
        open_duration_ms: config.recovery_timeout_secs.max(1) * 1000,
        half_open_max_probes: 1,
    })
}

fn metadata_from_headers(headers: &HashMap<String, String>) -> Option<llm_types::Metadata> {
    if headers.is_empty() {
        return None;
    }
    Some(
        headers
            .iter()
            .map(|(key, value)| (key.clone(), serde_json::Value::String(value.clone())))
            .collect(),
    )
}

fn metadata_from_params(
    params: &HashMap<String, serde_json::Value>,
) -> Option<llm_types::Metadata> {
    if params.is_empty() {
        return None;
    }
    Some(params.clone())
}

/// Builds the shared provider definition for a resolved connection.
pub fn provider_definition(
    connection: &ResolvedLlmConnection,
) -> llm_types::llm::LlmProviderDefinition {
    llm_types::llm::LlmProviderDefinition {
        id: connection.provider_id.clone(),
        name: None,
        description: None,
        base_url: Some(connection.base_url.clone()),
        auth_type: None,
        default_headers: metadata_from_headers(&connection.extra_headers),
        format: llm_types::llm::LlmFormat::OpenaiChat,
        model_discovery: None,
        api_version: None,
        metadata: None,
        proxy: connection.proxy_url.clone(),
        rate_limit: rate_limit_config(connection.rate_limit),
    }
}

/// Registers the provider definition on the global gateway. Registering is
/// idempotent and replaces the previous entry under the same id.
pub fn ensure_provider_registered(connection: &ResolvedLlmConnection) -> Result<(), LlmError> {
    global_gateway()
        .register_provider_definition(provider_definition(connection))
        .map_err(|err| LlmError::config(format!("Failed to register LLM provider: {err}")))
}

/// Builds the chat profile for a resolved chat model. Per-call generation
/// parameters stay on the request; only connection-level settings land here.
pub fn chat_profile(
    model_key: &str,
    resolved: &ResolvedChatConfig,
    connection: &ResolvedLlmConnection,
) -> llm_types::llm::LlmProfile {
    llm_types::llm::LlmProfile {
        id: profile_id_for(model_key, ServiceType::Chat),
        name: model_key.to_string(),
        format: llm_types::llm::LlmFormat::OpenaiChat,
        provider_id: Some(connection.provider_id.clone()),
        model: resolved.model.clone(),
        api_key: connection.api_keys.first().cloned(),
        base_url: Some(connection.base_url.clone()),
        parameters: None,
        generation: None,
        timeout: Some(connection.timeout_secs),
        max_retries: Some(connection.max_retries),
        retry_delay: Some(connection.retry_delay_ms),
        headers: None,
        metadata: None,
        tool_call_protocol: None,
        auth_type: None,
        custom_headers: metadata_from_headers(&connection.extra_headers),
        custom_body: None,
        custom_body_enabled: None,
        query_params: metadata_from_params(&connection.extra_params),
        stream_options: None,
        context_window_size: None,
        proxy: connection.proxy_url.clone(),
        circuit_breaker: circuit_breaker_config(&connection.circuit_breaker),
    }
}

/// Registers the chat profile on the global gateway and returns its id.
pub fn ensure_chat_profile(
    model_key: &str,
    resolved: &ResolvedChatConfig,
    connection: &ResolvedLlmConnection,
) -> Result<String, LlmError> {
    ensure_provider_registered(connection)?;
    let profile = chat_profile(model_key, resolved, connection);
    let id = profile.id.clone();
    global_gateway()
        .register_profile(profile)
        .map_err(|err| LlmError::config(format!("Failed to register chat profile: {err}")))?;
    Ok(id)
}

/// Renders a body/query parameter value as a plain string.
pub fn query_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        _ => value.to_string(),
    }
}

/// Builds the llm-suite rerank config for a dedicated `/rerank` endpoint.
pub fn rerank_endpoint_config(
    connection: &ResolvedLlmConnection,
    endpoint_url: impl Into<String>,
    model: impl Into<String>,
) -> llm_rerank::RerankConfig {
    let mut config = llm_rerank::RerankConfig::new(endpoint_url, model);
    config.timeout_secs = connection.timeout_secs;
    config.api_key = connection.api_keys.first().cloned();
    config.proxy = connection.proxy_url.clone();
    config.headers = connection.extra_headers.clone();
    config.query_params = connection
        .extra_params
        .iter()
        .map(|(key, value)| (key.clone(), query_string(value)))
        .collect();
    config
}

/// Builds the llm-suite chat endpoint for generative reranking.
pub fn generative_chat_endpoint(
    connection: &ResolvedLlmConnection,
    endpoint_url: impl Into<String>,
    model: impl Into<String>,
) -> llm_rerank::GenerativeChatEndpoint {
    let mut endpoint = llm_rerank::GenerativeChatEndpoint::new(endpoint_url, model);
    endpoint.api_key = connection.api_keys.first().cloned();
    endpoint.proxy = connection.proxy_url.clone();
    endpoint.headers = connection.extra_headers.clone();
    endpoint.query_params = connection
        .extra_params
        .iter()
        .map(|(key, value)| (key.clone(), query_string(value)))
        .collect();
    endpoint
}
/// Body substrings that mark a 429 as account/billing exhaustion rather
/// than throttling. Mirrors the previous HTTP layer's quota signals.
fn is_quota_message(message: &str) -> bool {
    let lower = message.to_lowercase();
    [
        "quota",
        "billing",
        "insufficient",
        "credit",
        "payment",
        "topup",
    ]
    .iter()
    .any(|signal| lower.contains(signal))
}

fn map_status_error(status: Option<u16>, message: String) -> LlmError {
    match status {
        Some(401) | Some(403) => LlmError::Auth(message),
        Some(404) => LlmError::ModelNotFound(message),
        Some(code) => LlmError::HttpStatus {
            status: code,
            message,
        },
        None => LlmError::Api(message),
    }
}

/// Converts a gateway chat error into the CCE error contract.
pub fn map_chat_error(error: llm_codec::error::LlmError) -> LlmError {
    use llm_codec::error::LlmError as SuiteError;
    match error {
        SuiteError::RateLimited { retry_after_ms } => {
            LlmError::RateLimitExceeded(retry_after_ms.unwrap_or(5000))
        }
        SuiteError::CircuitOpen => {
            LlmError::CircuitBreakerOpen("llm-suite circuit breaker is open".to_string())
        }
        SuiteError::ProxyError(message) => LlmError::http(format!("proxy error: {message}")),
        SuiteError::Timeout(ms) => {
            LlmError::Timeout(TimeoutError(format!("LLM request timed out after {ms}ms")))
        }
        SuiteError::AuthError(message) => LlmError::Auth(message),
        SuiteError::ContextLengthExceeded(message) => LlmError::HttpStatus {
            status: 400,
            message,
        },
        SuiteError::InvalidResponse(message) => LlmError::InvalidResponse(message),
        SuiteError::ConfigError(message)
        | SuiteError::ProfileNotFound(message)
        | SuiteError::CodecNotFound(message) => LlmError::config(message),
        SuiteError::UnsupportedFormat(format) => {
            LlmError::config(format!("unsupported LLM format: {format:?}"))
        }
        SuiteError::ProviderError { status, message } => {
            if status == Some(429) {
                if is_quota_message(&message) {
                    LlmError::QuotaExhausted(message)
                } else {
                    LlmError::RateLimitExceeded(5000)
                }
            } else {
                map_status_error(status, message)
            }
        }
        SuiteError::HttpError(error) => {
            if error.is_timeout() {
                LlmError::Timeout(TimeoutError(format!("LLM request timed out: {error}")))
            } else {
                LlmError::http(error.to_string())
            }
        }
        SuiteError::SerializationError(error) => {
            LlmError::InvalidResponse(format!("failed to serialize LLM request: {error}"))
        }
        SuiteError::StreamError(message) => LlmError::InvalidResponse(message),
        SuiteError::ToolNotFound(message) => LlmError::InvalidInput(message),
        SuiteError::Cancelled => LlmError::Internal("LLM request was cancelled".to_string()),
    }
}

/// Converts an llm-suite embedding error into the CCE error contract.
pub fn map_embedding_error(error: llm_embedding::EmbeddingError) -> LlmError {
    use llm_embedding::EmbeddingError as SuiteError;
    match error {
        SuiteError::Config(message) => LlmError::config(message),
        SuiteError::InvalidRequest(message) => LlmError::InvalidInput(message),
        SuiteError::Decode(message) => LlmError::InvalidResponse(message),
        SuiteError::Transport(message) => LlmError::http(message),
        SuiteError::Timeout => {
            LlmError::Timeout(TimeoutError("embedding request timed out".to_string()))
        }
        SuiteError::Provider {
            status: 429,
            message,
            retry_after_ms,
        } => {
            if is_quota_message(&message) {
                LlmError::QuotaExhausted(message)
            } else {
                LlmError::RateLimitExceeded(retry_after_ms.unwrap_or(5000))
            }
        }
        SuiteError::Provider {
            status, message, ..
        } => map_status_error(Some(status), message),
    }
}

/// Converts an llm-suite rerank error into the CCE error contract.
pub fn map_rerank_error(error: llm_rerank::RerankError) -> LlmError {
    use llm_rerank::RerankError as SuiteError;
    match error {
        SuiteError::InvalidRequest(message) => LlmError::InvalidInput(message),
        SuiteError::Decode(message) => LlmError::InvalidResponse(message),
        SuiteError::Transport(message) => LlmError::http(message),
        SuiteError::Timeout => {
            LlmError::Timeout(TimeoutError("rerank request timed out".to_string()))
        }
        SuiteError::Provider {
            status: 429,
            message,
            retry_after_ms,
        } => {
            if is_quota_message(&message) {
                LlmError::QuotaExhausted(message)
            } else {
                LlmError::RateLimitExceeded(retry_after_ms.unwrap_or(5000))
            }
        }
        SuiteError::Provider {
            status, message, ..
        } => map_status_error(Some(status), message),
    }
}

fn suite_role(role: &MessageRole) -> llm_types::message::MessageRole {
    match role {
        MessageRole::System => llm_types::message::MessageRole::System,
        MessageRole::User => llm_types::message::MessageRole::User,
        MessageRole::Assistant => llm_types::message::MessageRole::Assistant,
    }
}

fn generation_params(config: &ChatConfig) -> llm_types::llm::LlmGenerationParams {
    let mut params = llm_types::llm::LlmGenerationParams {
        temperature: Some(f64::from(config.temperature)),
        max_tokens: Some(config.max_tokens),
        top_p: Some(f64::from(config.top_p)),
        frequency_penalty: config.frequency_penalty.map(f64::from),
        presence_penalty: config.presence_penalty.map(f64::from),
        seed: config.seed.and_then(|seed| u64::try_from(seed).ok()),
        ..Default::default()
    };
    if !config.stop_sequences.is_empty() {
        params.stop = Some(config.stop_sequences.clone());
    }
    if let Some(format) = &config.response_format {
        let kind = if format.format_type.contains("json") {
            llm_types::llm::ResponseFormatKind::JsonObject
        } else {
            llm_types::llm::ResponseFormatKind::Text
        };
        params.response_format = Some(llm_types::llm::LlmResponseFormat {
            kind: Some(kind),
            ..Default::default()
        });
    }
    params
}

/// Chat client backed by the shared llm-suite gateway. It implements the
/// CCE [`LlmClient`] port so existing consumers keep working unchanged.
#[derive(Debug, Clone)]
pub struct SuiteChatClient {
    profile_id: String,
}

impl SuiteChatClient {
    /// Creates a client bound to an already-registered gateway profile.
    pub fn new(profile_id: impl Into<String>) -> Self {
        Self {
            profile_id: profile_id.into(),
        }
    }

    /// Returns the bound gateway profile id.
    pub fn profile_id(&self) -> &str {
        &self.profile_id
    }
}

impl LlmClient for SuiteChatClient {
    async fn chat(
        &self,
        messages: &[Message],
        config: &ChatConfig,
    ) -> Result<ChatResult, LlmError> {
        let request = llm_types::llm::LlmRequest {
            profile_id: self.profile_id.clone(),
            messages: messages
                .iter()
                .map(|message| {
                    llm_types::message::Message::text(
                        suite_role(&message.role),
                        message.content.as_str(),
                    )
                })
                .collect(),
            parameters: None,
            generation: Some(generation_params(config)),
            tools: None,
            tool_call_protocol: None,
            locked_tool_call_protocol: None,
            violation_policy: None,
            execution_id: None,
            stream: None,
            dead_loop_detection: None,
            protocol_auto_converted: None,
            timeout_ms: None,
        };
        let response = global_gateway()
            .generate(&request, None)
            .await
            .map_err(map_chat_error)?;
        let prompt_tokens = response
            .usage
            .as_ref()
            .map(|usage| usage.prompt_tokens)
            .unwrap_or(0);
        let total_tokens = response
            .usage
            .as_ref()
            .map(|usage| usage.total_tokens)
            .unwrap_or(0);
        Ok(ChatResult {
            content: response.content.unwrap_or_default(),
            prompt_tokens: u64::from(prompt_tokens),
            completion_tokens: u64::from(total_tokens.saturating_sub(prompt_tokens)),
            total_tokens: u64::from(total_tokens),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connection() -> ResolvedLlmConnection {
        ResolvedLlmConnection {
            provider_id: "acme".to_string(),
            api_keys: vec!["sk-test".to_string()],
            api_key_file: None,
            base_url: "https://api.acme.test/v1".to_string(),
            endpoint_path: "chat/completions".to_string(),
            timeout_secs: 30,
            max_retries: 3,
            retry_delay_ms: 1000,
            retry_jitter: 0.2,
            rate_limit_max_retries: 5,
            rate_limit_max_delay_ms: 60_000,
            rate_limit: 60,
            circuit_breaker: cce_config::modules::CircuitBreakerConfig::default(),
            proxy_url: None,
            extra_headers: HashMap::new(),
            extra_params: HashMap::new(),
        }
    }

    #[test]
    fn profile_ids_are_namespaced_by_service() {
        assert_eq!(profile_id_for("m", ServiceType::Chat), "cce-chat-m");
        assert_eq!(
            profile_id_for("m", ServiceType::Embedding),
            "cce-embedding-m"
        );
        assert_eq!(profile_id_for("m", ServiceType::Rerank), "cce-rerank-m");
    }

    #[test]
    fn rate_limit_converts_minutes_to_seconds() {
        assert!(rate_limit_config(0).is_none());
        let config = rate_limit_config(60).expect("limited");
        assert!((config.requests_per_second - 1.0).abs() < f64::EPSILON);
        assert_eq!(config.burst, 1);
    }

    #[test]
    fn disabled_breaker_yields_no_config() {
        let disabled = cce_config::modules::CircuitBreakerConfig {
            enabled: false,
            ..Default::default()
        };
        assert!(circuit_breaker_config(&disabled).is_none());
        let enabled = cce_config::modules::CircuitBreakerConfig {
            enabled: true,
            ..Default::default()
        };
        let config = circuit_breaker_config(&enabled).expect("enabled");
        assert_eq!(
            config.open_duration_ms,
            enabled.recovery_timeout_secs * 1000
        );
    }

    #[test]
    fn chat_error_mapping_covers_contract_variants() {
        use llm_codec::error::LlmError as SuiteError;
        assert!(matches!(
            map_chat_error(SuiteError::RateLimited {
                retry_after_ms: Some(1500)
            }),
            LlmError::RateLimitExceeded(1500)
        ));
        assert!(matches!(
            map_chat_error(SuiteError::CircuitOpen),
            LlmError::CircuitBreakerOpen(_)
        ));
        assert!(matches!(
            map_chat_error(SuiteError::AuthError("denied".to_string())),
            LlmError::Auth(_)
        ));
        assert!(matches!(
            map_chat_error(SuiteError::ProviderError {
                status: Some(429),
                message: "quota exceeded for account".to_string(),
            }),
            LlmError::QuotaExhausted(_)
        ));
        assert!(matches!(
            map_chat_error(SuiteError::ContextLengthExceeded("too long".to_string())),
            LlmError::HttpStatus { status: 400, .. }
        ));
    }

    #[test]
    fn provider_definition_carries_connection_settings() {
        let definition = provider_definition(&connection());
        assert_eq!(definition.id, "acme");
        assert_eq!(
            definition.base_url.as_deref(),
            Some("https://api.acme.test/v1")
        );
        assert!(definition.rate_limit.is_some());
    }
}
