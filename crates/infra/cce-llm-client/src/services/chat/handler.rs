//! Chat Request Handler

use crate::core::config::ChatConfig;
use crate::core::error::LlmError;
use crate::services::chat::types::{ChatResult, Message};
use crate::suite::SuiteChatClient;
use cce_llm::LlmClient;

/// Chat Request Handler - sends chat requests through the shared gateway.
pub struct ChatRequestHandler {
    /// Gateway-backed chat client
    inner: SuiteChatClient,
}

impl ChatRequestHandler {
    /// Create a new chat request handler
    pub fn new(client: SuiteChatClient) -> Self {
        Self { inner: client }
    }

    /// Send chat request
    pub async fn chat(
        &self,
        messages: &[Message],
        config: &ChatConfig,
    ) -> Result<ChatResult, LlmError> {
        self.inner.chat(messages, config).await
    }
}
