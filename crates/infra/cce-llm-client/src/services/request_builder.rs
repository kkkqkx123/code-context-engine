//! OpenAI-compatible request builder shared by LLM services.

use serde_json::{Value, json};

/// Request builder for constructing LLM API request bodies
pub struct RequestBuilder {
    /// Base request body
    body: Value,
}

impl RequestBuilder {
    /// Create a new request builder with model and basic parameters
    pub fn new(model: &str) -> Self {
        Self {
            body: json!({
                "model": model
            }),
        }
    }

    /// Add input texts (for embedding requests)
    pub fn with_input(mut self, input: &[&str]) -> Self {
        self.body["input"] = json!(input);
        self
    }

    /// Add messages (for chat requests)
    pub fn with_messages(mut self, messages: Vec<Value>) -> Self {
        self.body["messages"] = json!(messages);
        self
    }

    /// Add max tokens parameter
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.body["max_tokens"] = json!(max_tokens);
        self
    }

    /// Add temperature parameter
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.body["temperature"] = json!(temperature);
        self
    }

    /// Add top_p parameter
    pub fn with_top_p(mut self, top_p: f32) -> Self {
        self.body["top_p"] = json!(top_p);
        self
    }

    /// Add frequency penalty parameter (optional)
    pub fn with_frequency_penalty(mut self, penalty: Option<f32>) -> Self {
        if let Some(p) = penalty {
            self.body["frequency_penalty"] = json!(p);
        }
        self
    }

    /// Add presence penalty parameter (optional)
    pub fn with_presence_penalty(mut self, penalty: Option<f32>) -> Self {
        if let Some(p) = penalty {
            self.body["presence_penalty"] = json!(p);
        }
        self
    }

    /// Add stop sequences (optional)
    pub fn with_stop_sequences(mut self, sequences: &[String]) -> Self {
        if !sequences.is_empty() {
            self.body["stop"] = json!(sequences);
        }
        self
    }

    /// Add seed parameter (optional)
    pub fn with_seed(mut self, seed: Option<i64>) -> Self {
        if let Some(s) = seed {
            self.body["seed"] = json!(s);
        }
        self
    }

    /// Add encoding format (for embedding requests)
    pub fn with_encoding_format(mut self, format: &str) -> Self {
        self.body["encoding_format"] = json!(format);
        self
    }

    /// Add dimensions parameter (for embedding requests)
    pub fn with_dimensions(mut self, dimensions: usize) -> Self {
        self.body["dimensions"] = json!(dimensions);
        self
    }

    /// Add response format (for chat requests)
    pub fn with_response_format(mut self, format_type: &str) -> Self {
        self.body["response_format"] = json!({
            "type": format_type
        });
        self
    }

    /// Build the final request body
    pub fn build(self) -> Value {
        self.body
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_builder_embedding() {
        let body = RequestBuilder::new("text-embedding-3-small")
            .with_input(&["hello", "world"])
            .with_encoding_format("float")
            .build();

        assert_eq!(body["model"], "text-embedding-3-small");
        assert_eq!(body["input"][0], "hello");
        assert_eq!(body["input"][1], "world");
        assert_eq!(body["encoding_format"], "float");
    }

    #[test]
    fn test_request_builder_chat() {
        let messages = vec![
            json!({"role": "system", "content": "You are helpful"}),
            json!({"role": "user", "content": "Hello"}),
        ];

        let body = RequestBuilder::new("gpt-4")
            .with_messages(messages)
            .with_max_tokens(1000)
            .with_temperature(0.7)
            .with_top_p(1.0)
            .build();

        assert_eq!(body["model"], "gpt-4");
        assert_eq!(body["max_tokens"], 1000);
        let temp = body["temperature"]
            .as_f64()
            .expect("temperature should be a number");
        assert!((temp - 0.7).abs() < 0.001);
        let top_p = body["top_p"].as_f64().expect("top_p should be a number");
        assert!((top_p - 1.0).abs() < 0.001);
        assert_eq!(
            body["messages"]
                .as_array()
                .expect("messages should be an array")
                .len(),
            2
        );
    }

    #[test]
    fn test_request_builder_optional_params() {
        let body = RequestBuilder::new("gpt-4")
            .with_frequency_penalty(Some(0.5))
            .with_presence_penalty(None)
            .with_seed(Some(42))
            .build();

        assert_eq!(body["frequency_penalty"], 0.5);
        assert!(
            !body
                .as_object()
                .expect("request body should be an object")
                .contains_key("presence_penalty")
        );
        assert_eq!(body["seed"], 42);
    }
}
