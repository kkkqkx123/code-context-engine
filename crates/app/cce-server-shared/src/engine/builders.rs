use std::sync::Arc;

use super::EngineError;
use cce_config::AppConfig;
use cce_config::modules::RerankMode;
use cce_config::modules::summary::SummaryGenerationStrategy as SummaryStrategy;
use cce_llm_client::services::rerank::{
    CohereRerankRequestHandler, GenerativeRerankRequestHandler, ProductionRerankHandler,
};
use cce_llm_client::{ChatClientHandle, build_chat_client};
use cce_metrics::MetricsRegistry;
use cce_parser::summary::{ModelEnhancedGenerator, RuleBasedGenerator, SummaryGenerator};

/// Build the generative rerank handler for a project when `[rerank] enabled`
/// is true; returns `None` when reranking is disabled.
///
/// The LLM provider receives the real model name from the resolved
/// `[llm.rerank_models.<key>]` entry (not the registry key).
/// Build the chat client handle for a project config, when applicable.
///
/// Returns `None` when the summary strategy does not use a model (`RuleBased`
/// / `Minimal`) or when no chat model is configured in `llm.defaults.chat`.
///
/// Chat retries run inside the shared gateway, so no per-client retry
/// metrics are attached; `metrics_registry` is retained for signature
/// stability and currently unused.
pub(crate) fn build_chat_handle(
    config: &AppConfig,
    metrics_registry: Option<&Arc<MetricsRegistry>>,
) -> Result<Option<ChatClientHandle>, EngineError> {
    let _ = metrics_registry;
    if !config.llm.enabled {
        return Ok(None);
    }

    if !matches!(
        config.summary.strategy,
        SummaryStrategy::Auto | SummaryStrategy::ModelEnhanced
    ) {
        return Ok(None);
    }

    let Some(chat_key) = config.llm.defaults.chat.as_deref() else {
        return Ok(None);
    };

    let handle = build_chat_client(config, chat_key).map_err(|e| {
        EngineError::Config(format!(
            "Failed to build chat client for '{}': {}",
            chat_key, e
        ))
    })?;

    tracing::info!(
        model = %handle.config.model,
        strategy = ?config.summary.strategy,
        "Model-enhanced summary generator initialized"
    );

    Ok(Some(handle))
}

/// Build the summary generator for a project config.
///
/// Uses `ModelEnhancedGenerator` when the strategy is `Auto`/`ModelEnhanced`
/// and a chat model is configured; otherwise falls back to `RuleBasedGenerator`.
///
/// `metrics_registry` enables registry-backed LLM retry metrics; `None`
/// disables them (used by unit tests).
pub(crate) fn build_summary_generator(
    config: &AppConfig,
    metrics_registry: Option<&Arc<MetricsRegistry>>,
) -> Result<Arc<dyn SummaryGenerator>, EngineError> {
    if let Some(handle) = build_chat_handle(config, metrics_registry)? {
        return Ok(Arc::new(ModelEnhancedGenerator::with_config(
            handle.client,
            handle.config,
            config.summary.clone(),
        )));
    }
    Ok(Arc::new(RuleBasedGenerator::with_config(
        config.summary.clone(),
    )))
}

/// Build the rerank handler for a project when `[rerank] enabled` is true;
/// returns `None` when reranking is disabled.
///
/// The LLM provider receives the real model name from the resolved
/// `[llm.rerank_models.<key>]` entry (not the registry key).
/// The client is built against the endpoint that matches the configured mode:
/// chat-completions for `generative`, the dedicated `/rerank` endpoint for
/// `cross_encoder`.
pub(crate) fn build_rerank_handler(
    config: &AppConfig,
    metrics_registry: &Arc<MetricsRegistry>,
) -> Result<Option<Arc<ProductionRerankHandler>>, EngineError> {
    if !config.rerank.enabled {
        tracing::debug!("Reranking capability disabled for project");
        return Ok(None);
    }

    tracing::info!(
        model = config.rerank.model,
        max_candidates = config.rerank.max_candidates,
        "Reranking capability enabled for project"
    );

    // Resolve rerank model configuration from project config
    let rerank_model_config = config
        .llm
        .rerank_models
        .get(&config.rerank.model)
        .ok_or_else(|| {
            EngineError::Config(format!(
                "Rerank model '{}' not found in llm.rerank_models",
                config.rerank.model
            ))
        })?;

    // Build the rerank provider matching the configured mode
    // (generative scores through chat-completions, cross-encoder calls the
    // dedicated `/rerank` endpoint).
    let rerank_metrics = cce_metrics::RerankMetrics::new(metrics_registry, &config.rerank.model);

    // Select the provider implementation by the model's configured mode.
    let handler = match rerank_model_config.mode {
        RerankMode::Generative => {
            let provider = Arc::new(
                cce_llm_client::build_generative_rerank_provider(config, &config.rerank.model)
                    .map_err(EngineError::Llm)?,
            );
            let handler = Arc::new(
                GenerativeRerankRequestHandler::new(provider).with_rerank_metrics(rerank_metrics),
            );
            ProductionRerankHandler::Generative(handler)
        }
        RerankMode::CrossEncoder => {
            let provider = Arc::new(
                cce_llm_client::build_cohere_rerank_provider(config, &config.rerank.model)
                    .map_err(EngineError::Llm)?,
            );
            let handler = Arc::new(
                CohereRerankRequestHandler::new(provider).with_rerank_metrics(rerank_metrics),
            );
            ProductionRerankHandler::CrossEncoder(handler)
        }
    };

    tracing::info!(
        registry_key = config.rerank.model,
        mode = ?rerank_model_config.mode,
        "Reranking handler initialized with model: {}",
        rerank_model_config.model
    );
    Ok(Some(Arc::new(handler)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_config::AppConfig;
    use cce_config::modules::ProviderConfig;
    use std::collections::HashMap;

    /// Builds a rerank app config where the registry key differs from the
    /// real model name, mirroring production naming. The provider address is
    /// never contacted: factory construction performs no network input.
    fn rerank_config_with_model(
        registry_key: &str,
        real_model: &str,
        mode: cce_config::modules::RerankMode,
    ) -> AppConfig {
        use cce_config::modules::RerankModelConfig;

        let mut config = AppConfig::default();
        config.rerank.enabled = true;
        config.rerank.model = registry_key.to_string();
        config.llm.providers.insert(
            "mock-provider".to_string(),
            ProviderConfig {
                id: "mock-provider".to_string(),
                name: "Mock".to_string(),
                base_url: "http://127.0.0.1:9".to_string(),
                api_keys: vec!["test-key".to_string()],
                ..ProviderConfig::default()
            },
        );
        let mut rerank_models = HashMap::new();
        rerank_models.insert(
            registry_key.to_string(),
            RerankModelConfig {
                provider_id: "mock-provider".to_string(),
                model: real_model.to_string(),
                mode,
                proxy_url: None,
            },
        );
        config.llm.rerank_models = rerank_models;
        config
    }

    /// Builds a two-candidate rerank request for handler behavior tests.
    fn single_candidate_request(
        first_id: &str,
        first_score: f32,
        second_id: &str,
        second_score: f32,
    ) -> cce_llm_client::RerankRequest {
        use cce_llm_client::{
            RerankCandidate, RerankRequest, RerankRuntimeConfig as ServiceRerankConfig,
        };

        RerankRequest {
            query: "test query".to_string(),
            candidates: vec![
                RerankCandidate {
                    id: first_id.to_string(),
                    content: "fn main() {}".to_string(),
                    file_path: "src/main.rs".to_string(),
                    initial_score: first_score,
                    entity_type: Some("function".to_string()),
                    metadata: HashMap::new(),
                },
                RerankCandidate {
                    id: second_id.to_string(),
                    content: "pub fn start() {}".to_string(),
                    file_path: "src/app.rs".to_string(),
                    initial_score: second_score,
                    entity_type: Some("function".to_string()),
                    metadata: HashMap::new(),
                },
            ],
            config: ServiceRerankConfig::default(),
        }
    }

    /// The generative factory must carry the real model name (from
    /// `[llm.rerank_models.<key>].model`) instead of the registry key, and
    /// resolve the chat-completions endpoint. Construction performs no
    /// network input, so no test server is needed.
    #[test]
    fn test_generative_factory_uses_real_model_name() {
        let config = rerank_config_with_model(
            "registry-key-a",
            "real-model-b",
            cce_config::modules::RerankMode::Generative,
        );

        let provider = cce_llm_client::build_generative_rerank_provider(&config, "registry-key-a")
            .expect("provider build must succeed");

        assert_eq!(provider.endpoint().model, "real-model-b");
        assert!(
            provider.endpoint().base_url.contains("chat/completions"),
            "generative rerank must target the chat-completions endpoint, got: {}",
            provider.endpoint().base_url
        );
    }

    /// A `cross_encoder` rerank model must resolve the dedicated
    /// `/rerank` endpoint instead of `chat/completions`.
    #[test]
    fn test_cross_encoder_factory_uses_dedicated_endpoint() {
        use cce_config::modules::RerankMode;

        let config = rerank_config_with_model(
            "registry-key-a",
            "BAAI/bge-reranker-v2-m3",
            RerankMode::CrossEncoder,
        );

        let provider = cce_llm_client::build_cohere_rerank_provider(&config, "registry-key-a")
            .expect("provider build must succeed");

        assert_eq!(provider.config().model, "BAAI/bge-reranker-v2-m3");
        assert!(
            provider.config().base_url.ends_with("/rerank"),
            "cross-encoder rerank must target the dedicated /rerank endpoint, got: {}",
            provider.config().base_url
        );
    }

    /// Ordering through the injected suite mock must be observable without
    /// any network stub.
    #[tokio::test]
    async fn test_mock_rerank_orders_by_configured_ids() {
        use cce_llm_client::services::rerank::{DelegatingRerankProvider, RerankRequestHandler};
        use llm_rerank::mock::MockRerankProvider;

        let provider = Arc::new(DelegatingRerankProvider::new(
            MockRerankProvider::with_order(vec!["cand-2".to_string(), "cand-1".to_string()]),
        ));
        let handler = RerankRequestHandler::new(provider);

        let result = handler
            .rerank(&single_candidate_request("cand-1", 0.9, "cand-2", 0.1))
            .await
            .expect("rerank must succeed");

        assert_eq!(result.reranked_candidates.len(), 2);
        assert_eq!(result.reranked_candidates[0].id, "cand-2");
    }

    /// Candidate limiting happens before the provider call: only the
    /// top-scored candidate reaches the mock.
    #[tokio::test]
    async fn test_mock_rerank_limits_candidates_before_provider() {
        use cce_llm_client::RerankRuntimeConfig as ServiceRerankConfig;
        use cce_llm_client::services::rerank::{DelegatingRerankProvider, RerankRequestHandler};
        use llm_rerank::mock::MockRerankProvider;

        let provider = Arc::new(DelegatingRerankProvider::new(MockRerankProvider::by_score()));
        let handler = RerankRequestHandler::new(provider);

        let mut request = single_candidate_request("cand-1", 0.9, "cand-2", 0.1);
        request.config = ServiceRerankConfig {
            max_candidates: 1,
            ..ServiceRerankConfig::default()
        };

        let result = handler.rerank(&request).await.expect("rerank must succeed");

        assert_eq!(result.reranked_candidates.len(), 1);
        assert_eq!(result.reranked_candidates[0].id, "cand-1");
    }

    /// An empty query is rejected before any provider call.
    #[tokio::test]
    async fn test_mock_rerank_rejects_empty_query() {
        use cce_llm_client::services::rerank::{DelegatingRerankProvider, RerankRequestHandler};
        use llm_rerank::mock::MockRerankProvider;

        let provider = Arc::new(DelegatingRerankProvider::new(MockRerankProvider::by_score()));
        let handler = RerankRequestHandler::new(provider);

        let mut request = single_candidate_request("cand-1", 0.9, "cand-2", 0.1);
        request.query.clear();

        assert!(handler.rerank(&request).await.is_err());
    }

    fn chat_config_with_model() -> AppConfig {
        use cce_config::modules::ChatModelConfig;

        let mut config = AppConfig::default();
        config.llm.enabled = true;
        config.llm.providers.insert(
            "mock-provider".to_string(),
            ProviderConfig {
                id: "mock-provider".to_string(),
                name: "Mock".to_string(),
                base_url: "https://api.mock.example.com/v1".to_string(),
                api_keys: vec!["test-key".to_string()],
                ..ProviderConfig::default()
            },
        );
        config.llm.chat_models.insert(
            "chat-model".to_string(),
            ChatModelConfig {
                provider_id: "mock-provider".to_string(),
                model: "mock-chat".to_string(),
                ..ChatModelConfig::default()
            },
        );
        config.llm.defaults.chat = Some("chat-model".to_string());
        config.summary.strategy = SummaryStrategy::ModelEnhanced;
        config
    }

    /// A configured chat model + `ModelEnhanced` strategy must
    /// produce a chat handle carrying the resolved model parameters.
    #[test]
    fn test_build_chat_handle_uses_chat_model_when_configured() {
        let config = chat_config_with_model();
        let handle = build_chat_handle(&config, None)
            .expect("handle must build")
            .expect("handle must be present");
        assert_eq!(handle.config.model, "mock-chat");
    }

    /// Without a configured chat model the chat wiring is
    /// skipped, so the summary pipeline falls back to rule-based generation.
    #[test]
    fn test_build_chat_handle_falls_back_without_chat_model() {
        let mut config = chat_config_with_model();
        config.llm.defaults.chat = None;
        let handle = build_chat_handle(&config, None).expect("no error when unconfigured");
        assert!(handle.is_none(), "no chat model must suppress chat wiring");
    }

    /// Disabling `llm.enabled` must suppress chat client creation
    /// even when a chat model is configured.
    #[test]
    fn test_build_chat_handle_respects_llm_enabled_flag() {
        let mut config = chat_config_with_model();
        config.llm.enabled = false;
        let handle = build_chat_handle(&config, None).expect("no error when disabled");
        assert!(
            handle.is_none(),
            "llm.enabled=false must suppress chat wiring"
        );
    }
}
