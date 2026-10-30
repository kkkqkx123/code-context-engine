//! LLM chat configuration types

use serde::{Deserialize, Serialize};

/// Chat/Completion-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatConfig {
    /// Model to use for chat/completion
    pub model: String,

    /// Maximum tokens to generate
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,

    /// Temperature for sampling
    #[serde(default = "default_temperature")]
    pub temperature: f32,

    /// Top-p sampling parameter
    #[serde(default = "default_top_p")]
    pub top_p: f32,

    /// Frequency penalty
    #[serde(default)]
    pub frequency_penalty: Option<f32>,

    /// Presence penalty
    #[serde(default)]
    pub presence_penalty: Option<f32>,

    /// Stop sequences to end generation
    #[serde(default)]
    pub stop_sequences: Vec<String>,

    /// Seed for deterministic results
    #[serde(default)]
    pub seed: Option<i64>,

    /// Response format (e.g., "json_object" for JSON mode)
    #[serde(default)]
    pub response_format: Option<ResponseFormat>,

    /// Per-request retry budget override. `None` keeps the profile default.
    #[serde(default)]
    pub max_retries: Option<u32>,

    /// Per-request retry base delay in milliseconds. `None` keeps the profile default.
    #[serde(default)]
    pub retry_delay_ms: Option<u64>,

    /// Per-request wall-clock bound in milliseconds. `None` keeps the profile timeout.
    #[serde(default)]
    pub timeout_ms: Option<u64>,

    /// Per-request extra HTTP headers merged after codec-built headers.
    #[serde(default)]
    pub extra_headers: std::collections::HashMap<String, String>,
}

/// Response format configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseFormat {
    /// Type of response format (e.g., "json_object", "text")
    #[serde(rename = "type")]
    pub format_type: String,
}

impl Default for ChatConfig {
    fn default() -> Self {
        Self {
            model: String::new(),
            max_tokens: default_max_tokens(),
            temperature: default_temperature(),
            top_p: default_top_p(),
            frequency_penalty: None,
            presence_penalty: None,
            stop_sequences: Vec::new(),
            seed: None,
            response_format: None,
            max_retries: None,
            retry_delay_ms: None,
            timeout_ms: None,
            extra_headers: std::collections::HashMap::new(),
        }
    }
}

fn default_max_tokens() -> u32 {
    1024
}

fn default_temperature() -> f32 {
    0.3
}

fn default_top_p() -> f32 {
    1.0
}
