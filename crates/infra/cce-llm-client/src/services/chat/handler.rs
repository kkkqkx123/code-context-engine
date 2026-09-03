//! Chat Request Handler

use crate::core::client::HttpLlmClient;
use crate::core::config::ChatConfig;
use crate::core::error::LlmError;
use crate::services::chat::types::{ChatResult, Message};
use crate::services::request_builder::RequestBuilder;
use cce_config::modules::ServiceType;
use serde_json::json;
use std::sync::Arc;

/// Chat Request Handler - handles chat requests
pub struct ChatRequestHandler {
    /// Underlying HTTP client
    inner: Arc<HttpLlmClient>,
}

impl ChatRequestHandler {
    /// Create a new chat request handler
    pub fn new(client: Arc<HttpLlmClient>) -> Self {
        Self { inner: client }
    }

    /// Send chat request
    pub async fn chat(
        &self,
        messages: &[Message],
        config: &ChatConfig,
    ) -> Result<ChatResult, LlmError> {
        let api_messages: Vec<serde_json::Value> = messages
            .iter()
            .map(|msg| {
                let role_str = match msg.role {
                    crate::services::chat::types::MessageRole::System => "system",
                    crate::services::chat::types::MessageRole::User => "user",
                    crate::services::chat::types::MessageRole::Assistant => "assistant",
                };
                json!({
                    "role": role_str,
                    "content": msg.content
                })
            })
            .collect();

        let mut builder = RequestBuilder::new(&config.model)
            .with_messages(api_messages)
            .with_max_tokens(config.max_tokens)
            .with_temperature(config.temperature)
            .with_top_p(config.top_p)
            .with_frequency_penalty(config.frequency_penalty)
            .with_presence_penalty(config.presence_penalty)
            .with_stop_sequences(&config.stop_sequences)
            .with_seed(config.seed);

        if let Some(ref response_format) = config.response_format {
            builder = builder.with_response_format(&response_format.format_type);
        }

        let request_body = builder.build();

        #[derive(serde::Deserialize)]
        struct ChatApiResponse {
            choices: Vec<ChatApiChoice>,
            #[serde(default)]
            usage: Option<crate::services::embedding::response_parser::TokenUsage>,
        }

        #[derive(serde::Deserialize)]
        struct ChatApiChoice {
            message: ChatApiMessage,
        }

        #[derive(serde::Deserialize)]
        struct ChatApiMessage {
            content: String,
        }

        let response: ChatApiResponse = self
            .inner
            .request(&self.inner.endpoint_path(ServiceType::Chat), &request_body)
            .await?;

        let content = response
            .choices
            .first()
            .map(|choice| choice.message.content.clone())
            .ok_or_else(|| LlmError::invalid_response("Chat response contains no choices"))?;

        let usage = response.usage.unwrap_or_default();
        let completion_tokens = usage.total_tokens.saturating_sub(usage.prompt_tokens);

        Ok(ChatResult {
            content,
            prompt_tokens: usage.prompt_tokens,
            completion_tokens,
            total_tokens: usage.total_tokens,
        })
    }
}
