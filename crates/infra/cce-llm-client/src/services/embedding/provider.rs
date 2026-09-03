//! OpenAI-compatible API embedder

use std::sync::Arc;
use std::time::Instant;

use tracing::{debug, info, trace};

use crate::core::error::LlmError;
use crate::core::{EmbeddingConfig, HttpLlmClient};
use crate::services::embedding::handler::EmbeddingRequestHandler;
use crate::services::embedding::types::EmbeddingResult;
use cce_llm::Embedder;
use cce_metrics::{EmbeddingErrorType, EmbeddingMetrics};

use crate::services::embedding::preprocessor::{
    NomicPreprocessor, NomicTaskType, StellaPreprocessor, StellaTaskType, TemplatePreprocessor,
    TextPreprocessor,
};
use cce_config::PreprocessorConfig;

/// OpenAI-compatible API embedder
pub struct OpenAICompatibleProvider {
    /// Single LLM client used for embedding requests
    llm_client: Arc<HttpLlmClient>,
    embed_config: EmbeddingConfig,
    preprocessor: PreprocessorConfig,
    /// Monitoring metrics (optional)
    metrics: Option<Arc<EmbeddingMetrics>>,
}

impl std::fmt::Debug for OpenAICompatibleProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenAICompatibleProvider")
            .field("model", &self.embed_config.model)
            .field("provider_id", &self.llm_client.provider_id())
            .field("max_batch_tokens", &self.embed_config.max_batch_tokens)
            .field("max_item_tokens", &self.embed_config.max_item_tokens)
            .field("vector_dimension", &self.embed_config.vector_dimension)
            .finish_non_exhaustive()
    }
}

impl OpenAICompatibleProvider {
    /// Create embedder from global AppConfig and a single model name.
    pub fn from_model(
        global_config: &cce_config::AppConfig,
        model_name: &str,
    ) -> Result<Self, LlmError> {
        Self::from_model_with_retry_metrics(global_config, model_name, None)
    }

    /// Create embedder from global AppConfig and a single model name, attaching
    /// LLM retry/circuit-breaker metrics when a registry-backed instance is
    /// provided.
    pub fn from_model_with_retry_metrics(
        global_config: &cce_config::AppConfig,
        model_name: &str,
        retry_metrics: Option<std::sync::Arc<cce_metrics::LlmRetryMetrics>>,
    ) -> Result<Self, LlmError> {
        debug!(model = model_name, "Creating embedder from model registry");

        let resolved = global_config
            .resolve_embedding_config(model_name)
            .map_err(|e| {
                LlmError::config(format!("Failed to resolve model '{}': {}", model_name, e))
            })?;

        let llm_client = crate::factory::build_llm_client(
            global_config,
            model_name,
            cce_config::modules::ServiceType::Embedding,
            None,
            retry_metrics,
        )?;

        let embed_config = EmbeddingConfig {
            model: resolved.model.clone(),
            max_batch_tokens: resolved.max_batch_tokens,
            max_item_tokens: resolved.max_item_tokens,
            vector_dimension: Some(resolved.vector_dimension),
            use_base64: resolved.use_base64,
        };
        let preprocessor = resolved.preprocessor;

        info!(
            model = %embed_config.model,
            vector_dimension = ?embed_config.vector_dimension,
            provider = %resolved.base_url,
            "OpenAI-compatible embedder initialized"
        );

        Ok(Self {
            llm_client,
            embed_config,
            preprocessor,
            metrics: None,
        })
    }

    /// Create embeddings for texts
    pub async fn embed(&self, texts: &[&str]) -> Result<EmbeddingResult, LlmError> {
        if texts.is_empty() {
            return Ok(EmbeddingResult::default());
        }

        let start_time = Instant::now();

        let processed_texts = self.preprocess_texts(texts);
        let text_refs: Vec<&str> = processed_texts.iter().map(|s| s.as_str()).collect();

        let token_count = texts.iter().map(|t| t.len()).sum();
        let handler = EmbeddingRequestHandler::new(Arc::clone(&self.llm_client));
        let result = handler.embed(&text_refs, &self.embed_config).await;

        match result {
            Ok(embedding_result) => {
                if let Some(metrics) = &self.metrics {
                    let elapsed_ms = start_time.elapsed().as_secs_f64() * 1000.0;
                    metrics.record_request(elapsed_ms, token_count, true);
                }
                Ok(embedding_result)
            }
            Err(err) => {
                if let Some(metrics) = &self.metrics {
                    let elapsed_ms = start_time.elapsed().as_secs_f64() * 1000.0;
                    metrics.record_request(elapsed_ms, token_count, false);
                    metrics.record_error(Self::classify_error(&err));
                }
                Err(err)
            }
        }
    }

    /// Preprocess texts using the configured preprocessor
    fn preprocess_texts(&self, texts: &[&str]) -> Vec<String> {
        match &self.preprocessor {
            PreprocessorConfig::None => texts.iter().map(|s| s.to_string()).collect(),
            PreprocessorConfig::Prefix { prefix } => {
                trace!(prefix = %prefix, "Using prefix preprocessor");
                texts
                    .iter()
                    .map(|text| format!("{}{}", prefix, text))
                    .collect()
            }
            PreprocessorConfig::Template { template } => {
                trace!(template = %template, "Using template preprocessor");
                let preprocessor = TemplatePreprocessor::new(template.clone());
                preprocessor.process_batch(texts)
            }
            PreprocessorConfig::Nomic { task_type } => {
                let nomic_task_type = match task_type.as_str() {
                    "search_document" => NomicTaskType::SearchDocument,
                    "search_query" => NomicTaskType::SearchQuery,
                    "clustering" => NomicTaskType::Clustering,
                    "classification" => NomicTaskType::Classification,
                    _ => NomicTaskType::SearchDocument,
                };
                trace!(task_type = ?nomic_task_type, "Using Nomic preprocessor");
                let preprocessor = NomicPreprocessor::new(nomic_task_type);
                preprocessor.process_batch(texts)
            }
            PreprocessorConfig::Stella { task_type } => {
                let stella_task_type = match task_type.as_str() {
                    "s2p" => StellaTaskType::S2P,
                    "s2s" => StellaTaskType::S2S,
                    _ => StellaTaskType::S2P,
                };
                trace!(task_type = ?stella_task_type, "Using Stella preprocessor");
                let preprocessor = StellaPreprocessor::new(stella_task_type);
                preprocessor.process_batch(texts)
            }
        }
    }

    /// Set monitoring metrics
    pub fn with_metrics(mut self, metrics: Arc<EmbeddingMetrics>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Check if the embedding provider is healthy
    pub fn is_healthy(&self) -> bool {
        self.llm_client.is_healthy()
    }

    /// Classify an LlmError into an EmbeddingErrorType for metrics tracking.
    fn classify_error(err: &LlmError) -> EmbeddingErrorType {
        match err {
            LlmError::Timeout(_) => EmbeddingErrorType::Timeout,
            LlmError::RateLimitExceeded(_) => EmbeddingErrorType::RateLimited,
            LlmError::Auth(_) => EmbeddingErrorType::Authentication,
            LlmError::InvalidInput(_) | LlmError::InvalidResponse(_) => {
                EmbeddingErrorType::InvalidRequest
            }
            LlmError::ModelNotFound(_) | LlmError::Http(_) | LlmError::Api(_) => {
                EmbeddingErrorType::ServiceUnavailable
            }
            LlmError::HttpStatus { status, .. } if (500..=599).contains(status) => {
                EmbeddingErrorType::ServiceUnavailable
            }
            LlmError::HttpStatus { .. } => EmbeddingErrorType::InvalidRequest,
            _ => EmbeddingErrorType::Unknown,
        }
    }

    /// Embed a single text (convenience method)
    pub async fn embed_one(&self, text: &str) -> Result<Vec<f32>, LlmError> {
        let mut embeddings = self.embed_vectors(&[text]).await?;
        embeddings
            .pop()
            .ok_or_else(|| LlmError::internal("No embedding returned"))
    }

    /// Embed texts and return only the dense vectors (convenience method)
    pub async fn embed_vectors(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, LlmError> {
        let result = self.embed(texts).await?;
        Ok(result.embeddings)
    }

    /// Get the embedding dimension for this provider
    pub fn dimension(&self) -> usize {
        self.embed_config.vector_dimension.unwrap_or(0)
    }

    /// Get the model name
    pub fn model_name(&self) -> &str {
        &self.embed_config.model
    }

    /// Get monitoring metrics (optional)
    pub fn get_metrics(&self) -> Option<Arc<EmbeddingMetrics>> {
        self.metrics.clone()
    }
}

#[async_trait::async_trait]
impl Embedder for OpenAICompatibleProvider {
    async fn embed(&self, texts: &[&str]) -> Result<EmbeddingResult, LlmError> {
        OpenAICompatibleProvider::embed(self, texts).await
    }

    async fn embed_one(&self, text: &str) -> Result<Vec<f32>, LlmError> {
        OpenAICompatibleProvider::embed_one(self, text).await
    }

    async fn embed_vectors(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, LlmError> {
        OpenAICompatibleProvider::embed_vectors(self, texts).await
    }

    fn dimension(&self) -> usize {
        OpenAICompatibleProvider::dimension(self)
    }

    fn model_name(&self) -> &str {
        OpenAICompatibleProvider::model_name(self)
    }

    fn is_healthy(&self) -> bool {
        OpenAICompatibleProvider::is_healthy(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_config::AppConfig;
    use cce_config::modules::{EmbeddingModelConfig, ProviderConfig};
    use std::collections::HashMap;

    fn test_global_config() -> AppConfig {
        let mut config = AppConfig::default();

        let mut providers = HashMap::new();
        providers.insert(
            "openai".to_string(),
            ProviderConfig {
                id: "openai".to_string(),
                name: "OpenAI".to_string(),
                base_url: "https://api.openai.com/v1".to_string(),
                api_keys: vec!["sk-test".to_string()],
                ..ProviderConfig::default()
            },
        );
        config.llm.providers = providers;

        let mut models = HashMap::new();
        models.insert(
            "text-embedding-3-small".to_string(),
            EmbeddingModelConfig {
                provider_id: "openai".to_string(),
                model: "text-embedding-3-small".to_string(),
                vector_dimension: 1536,
                ..EmbeddingModelConfig::default()
            },
        );
        config.llm.embedding_models = models;
        config.embedder.default_model = "text-embedding-3-small".to_string();

        config
    }

    #[test]
    fn test_create_embedder_from_model() {
        let config = test_global_config();
        let embedder = OpenAICompatibleProvider::from_model(&config, "text-embedding-3-small");
        assert!(embedder.is_ok());
    }

    #[test]
    fn test_embedding_provider_metadata() {
        let config = test_global_config();
        let provider = OpenAICompatibleProvider::from_model(&config, "text-embedding-3-small")
            .expect("create failed");

        assert_eq!(provider.model_name(), "text-embedding-3-small");
        assert_eq!(provider.dimension(), 1536);
    }

    #[test]
    fn test_from_model_invalid_model() {
        let global_config = AppConfig::default();
        let result = OpenAICompatibleProvider::from_model(&global_config, "non-existent-model");
        assert!(result.is_err());
    }
}
