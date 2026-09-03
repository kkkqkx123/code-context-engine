use std::sync::Arc;

use super::EngineError;
use cce_config::AppConfig;
use cce_config::modules::RerankMode;
use cce_config::modules::summary::SummaryGenerationStrategy as SummaryStrategy;
use cce_llm_client::services::rerank::{
    CohereRerankProvider, CohereRerankRequestHandler, GenerativeRerankProvider,
    GenerativeRerankRequestHandler, ProductionRerankHandler,
};
use cce_llm_client::{ChatClientHandle, build_chat_client};
use cce_metrics_infra::{LlmRetryMetrics, MetricsRegistry};
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
/// `metrics_registry` enables registry-backed LLM retry metrics; `None`
/// disables them (used by unit tests).
pub(crate) fn build_chat_handle(
    config: &AppConfig,
    metrics_registry: Option<&Arc<MetricsRegistry>>,
) -> Result<Option<ChatClientHandle>, EngineError> {
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

    let retry_metrics = metrics_registry.map(|registry| {
        let provider_id = config
            .resolve_llm_connection(chat_key, cce_config::modules::ServiceType::Chat)
            .map(|connection| connection.provider_id)
            .unwrap_or_else(|_| chat_key.to_string());
        LlmRetryMetrics::new(registry, &provider_id)
    });

    let handle = build_chat_client(config, chat_key, retry_metrics).map_err(|e| {
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

    // Build the shared LLM client from the resolved provider connection.
    // The client routes requests to the endpoint matching the configured mode
    // (chat-completions for `generative`, the dedicated `/rerank` endpoint for
    // `cross_encoder`).
    let retry_metrics = {
        let provider_id = config
            .resolve_llm_connection(
                &config.rerank.model,
                cce_config::modules::ServiceType::Rerank,
            )
            .map(|connection| connection.provider_id)
            .unwrap_or_else(|_| config.rerank.model.clone());
        LlmRetryMetrics::new(metrics_registry, &provider_id)
    };
    let llm_client = cce_llm_client::build_rerank_client(
        config,
        &config.rerank.model,
        rerank_model_config.mode,
        Some(retry_metrics),
    )
    .map_err(EngineError::Llm)?;

    let rerank_metrics =
        cce_metrics_infra::RerankMetrics::new(metrics_registry, &config.rerank.model);

    // Select the provider implementation by the model's configured mode.
    let handler = match rerank_model_config.mode {
        RerankMode::Generative => {
            let provider = Arc::new(GenerativeRerankProvider::new(
                llm_client,
                rerank_model_config.model.clone(),
            ));
            let handler = Arc::new(
                GenerativeRerankRequestHandler::new(provider).with_rerank_metrics(rerank_metrics),
            );
            ProductionRerankHandler::Generative(handler)
        }
        RerankMode::CrossEncoder => {
            let provider = Arc::new(CohereRerankProvider::new(
                llm_client,
                rerank_model_config.model.clone(),
            ));
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

    /// Captures one HTTP request (request line + body) and replies with a
    /// fixed JSON body.
    async fn spawn_capturing_server(
        reply_body: String,
    ) -> (
        String,
        Arc<std::sync::Mutex<Option<String>>>,
        Arc<std::sync::Mutex<Option<String>>>,
    ) {
        use std::sync::Mutex;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::{TcpListener, TcpStream};

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test server");
        let addr = listener.local_addr().expect("local addr");
        let captured_body = Arc::new(Mutex::new(None::<String>));
        let captured_request_line = Arc::new(Mutex::new(None::<String>));

        let task_listener = Arc::new(listener);
        let captured_body_for_task = captured_body.clone();
        let captured_line_for_task = captured_request_line.clone();
        tokio::spawn(async move {
            loop {
                let (stream, _) = match task_listener.accept().await {
                    Ok(conn) => conn,
                    Err(_) => return,
                };
                let captured_body = captured_body_for_task.clone();
                let captured_line = captured_line_for_task.clone();
                let reply_body = reply_body.clone();
                tokio::spawn(async move {
                    let _ = serve_one(stream, &captured_body, &captured_line, &reply_body).await;
                });
            }
        });

        async fn serve_one(
            mut stream: TcpStream,
            captured_body: &Arc<Mutex<Option<String>>>,
            captured_line: &Arc<Mutex<Option<String>>>,
            reply_body: &str,
        ) -> std::io::Result<()> {
            let mut buf = Vec::with_capacity(4096);
            let mut tmp = [0u8; 4096];
            loop {
                let read = match stream.read(&mut tmp).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => n,
                };
                buf.extend_from_slice(&tmp[..read]);
                if let Some((header_end, content_length)) = parse_request_length(&buf) {
                    let body_start = header_end + 4;
                    if buf.len() >= body_start + content_length {
                        let body =
                            String::from_utf8_lossy(&buf[body_start..body_start + content_length])
                                .to_string();
                        let request_line = String::from_utf8_lossy(&buf[..buf.len()])
                            .lines()
                            .next()
                            .unwrap_or_default()
                            .to_string();
                        let mut body_guard = captured_body
                            .lock()
                            .map_err(|_| std::io::Error::other("capture mutex poisoned"))?;
                        *body_guard = Some(body);
                        let mut line_guard = captured_line
                            .lock()
                            .map_err(|_| std::io::Error::other("capture mutex poisoned"))?;
                        *line_guard = Some(request_line);
                        break;
                    }
                }
            }
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                reply_body.len(),
                reply_body
            );
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.flush().await;
            Ok(())
        }

        fn parse_request_length(buf: &[u8]) -> Option<(usize, usize)> {
            let haystack = buf.windows(4).position(|w| w == b"\r\n\r\n")?;
            let header = String::from_utf8_lossy(&buf[..haystack]);
            let content_length = header.lines().find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.trim()
                    .eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())
                    .flatten()
            })?;
            Some((haystack, content_length))
        }

        let _ = addr;
        let _ = TcpStream::connect(addr).await;
        let _ = tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        (
            format!("http://127.0.0.1:{}", addr.port()),
            captured_body,
            captured_request_line,
        )
    }

    /// The generative rerank handler must send the real model
    /// name (from `[llm.rerank_models.<key>].model`) instead of the registry
    /// key referenced by `[rerank].model`.
    #[tokio::test]
    async fn test_rerank_handler_uses_real_model_name() {
        use cce_config::modules::RerankModelConfig;
        use cce_llm_client::RerankProvider;
        use cce_llm_client::services::rerank::types::{
            RerankCandidate, RerankRequest, RerankRuntimeConfig as ServiceRerankConfig,
        };

        // The mock LLM replies with a rerank JSON array for the candidate.
        let reply = r#"{"choices":[{"message":{"content":"[{\"id\": \"cand-1\", \"score\": 0.95, \"reasoning\": \"relevant\"}]"}}],"usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15}}"#;
        let (base_url, captured_body, captured_line) =
            spawn_capturing_server(reply.to_string()).await;

        let mut config = AppConfig::default();
        config.rerank.enabled = true;
        // Registry key differs from the real model name.
        config.rerank.model = "registry-key-a".to_string();
        config.llm.providers.insert(
            "mock-provider".to_string(),
            ProviderConfig {
                id: "mock-provider".to_string(),
                name: "Mock".to_string(),
                base_url,
                api_keys: vec!["test-key".to_string()],
                ..ProviderConfig::default()
            },
        );
        let mut rerank_models = HashMap::new();
        rerank_models.insert(
            "registry-key-a".to_string(),
            RerankModelConfig {
                provider_id: "mock-provider".to_string(),
                model: "real-model-b".to_string(),
                ..Default::default()
            },
        );
        config.llm.rerank_models = rerank_models;

        let registry = Arc::new(MetricsRegistry::new());
        let handler = build_rerank_handler(&config, &registry)
            .expect("handler build must succeed")
            .expect("handler must be present when rerank is enabled");

        let request = RerankRequest {
            query: "test query".to_string(),
            candidates: vec![RerankCandidate {
                id: "cand-1".to_string(),
                content: "fn main() {}".to_string(),
                file_path: "src/main.rs".to_string(),
                initial_score: 0.5,
                entity_type: Some("function".to_string()),
                metadata: HashMap::new(),
            }],
            config: ServiceRerankConfig {
                max_candidates: 1,
                ..ServiceRerankConfig::default()
            },
        };

        handler.rerank(&request).await.expect("rerank must succeed");

        let line = captured_line
            .lock()
            .expect("capture lock")
            .clone()
            .expect("request line must be captured");
        assert!(
            line.starts_with("POST /chat/completions"),
            "generative rerank must target the chat-completions endpoint, got request line: {line}"
        );

        let body = captured_body
            .lock()
            .expect("capture lock")
            .clone()
            .expect("request body must be captured");
        assert!(
            body.contains("real-model-b"),
            "provider must receive the real model name, got body: {body}"
        );
        assert!(
            !body.contains("registry-key-a"),
            "provider must not receive the registry key, got body: {body}"
        );
    }

    /// A `cross_encoder` rerank model must call the dedicated
    /// `/rerank` endpoint (Cohere-compatible schema) instead of
    /// `chat/completions`.
    #[tokio::test]
    async fn test_cross_encoder_rerank_uses_dedicated_endpoint() {
        use cce_config::modules::{RerankMode, RerankModelConfig};
        use cce_llm_client::RerankProvider;
        use cce_llm_client::services::rerank::types::{
            RerankCandidate, RerankRequest, RerankRuntimeConfig as ServiceRerankConfig,
        };

        let reply =
            r#"{"results":[{"index":0,"relevance_score":0.95}],"usage":{"total_tokens":10}}"#;
        let (base_url, captured_body, captured_line) =
            spawn_capturing_server(reply.to_string()).await;

        let mut config = AppConfig::default();
        config.rerank.enabled = true;
        config.rerank.model = "registry-key-a".to_string();
        config.llm.providers.insert(
            "mock-provider".to_string(),
            ProviderConfig {
                id: "mock-provider".to_string(),
                name: "Mock".to_string(),
                base_url,
                api_keys: vec!["test-key".to_string()],
                ..ProviderConfig::default()
            },
        );
        let mut rerank_models = HashMap::new();
        rerank_models.insert(
            "registry-key-a".to_string(),
            RerankModelConfig {
                provider_id: "mock-provider".to_string(),
                model: "BAAI/bge-reranker-v2-m3".to_string(),
                mode: RerankMode::CrossEncoder,
            },
        );
        config.llm.rerank_models = rerank_models;

        let registry = Arc::new(MetricsRegistry::new());
        let handler = build_rerank_handler(&config, &registry)
            .expect("handler build must succeed")
            .expect("handler must be present when rerank is enabled");

        let request = RerankRequest {
            query: "test query".to_string(),
            candidates: vec![RerankCandidate {
                id: "cand-1".to_string(),
                content: "fn main() {}".to_string(),
                file_path: "src/main.rs".to_string(),
                initial_score: 0.5,
                entity_type: Some("function".to_string()),
                metadata: HashMap::new(),
            }],
            config: ServiceRerankConfig {
                max_candidates: 1,
                ..ServiceRerankConfig::default()
            },
        };

        handler.rerank(&request).await.expect("rerank must succeed");

        let line = captured_line
            .lock()
            .expect("capture lock")
            .clone()
            .expect("request line must be captured");
        assert!(
            line.starts_with("POST /rerank"),
            "cross-encoder rerank must target the dedicated /rerank endpoint, got request line: {line}"
        );

        let body = captured_body
            .lock()
            .expect("capture lock")
            .clone()
            .expect("request body must be captured");
        assert!(
            body.contains("BAAI/bge-reranker-v2-m3"),
            "provider must receive the real model name, got body: {body}"
        );
        assert!(
            body.contains("\"documents\""),
            "request must carry the documents array, got body: {body}"
        );
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
