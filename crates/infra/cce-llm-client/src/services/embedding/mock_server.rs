//! Test-only mock HTTP server for embedding requests.
//!
//! Provides a lightweight HTTP server that can be configured to return
//! specific responses (success, rate limit, delay) for testing the
//! `OpenAICompatibleProvider` and `CachedEmbedder` without external
//! dependencies.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// A mock HTTP server for embedding requests.
pub struct MockEmbeddingServer {
    #[allow(dead_code)]
    listener: TcpListener,
    responses: Arc<Mutex<Vec<MockResponse>>>,
    request_count: Arc<AtomicUsize>,
    port: u16,
}

/// A mock response to return for the next request.
#[derive(Clone)]
pub enum MockResponse {
    /// Return a successful embedding response with the given dimension.
    Success { dimension: usize },
    /// Return a 429 rate limit response.
    RateLimit,
    /// Return a 500 server error response.
    ServerError,
    /// Delay the response by the given duration, then return success.
    Delayed { delay: Duration, dimension: usize },
}

impl MockEmbeddingServer {
    /// Start a new mock server on a random available port.
    pub fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let port = listener.local_addr().expect("local_addr").port();
        let responses = Arc::new(Mutex::new(Vec::new()));
        let request_count = Arc::new(AtomicUsize::new(0));

        let server = Self {
            listener: listener.try_clone().expect("clone listener"),
            responses: Arc::clone(&responses),
            request_count: Arc::clone(&request_count),
            port,
        };

        thread::spawn(move || {
            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => {
                        let responses = Arc::clone(&responses);
                        let request_count = Arc::clone(&request_count);
                        thread::spawn(move || {
                            handle_connection(stream, responses, request_count);
                        });
                    }
                    Err(_) => break,
                }
            }
        });

        server
    }

    /// Get the base URL for this mock server.
    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// Queue a response to be returned for the next request.
    pub fn queue_response(&self, response: MockResponse) {
        self.responses
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(response);
    }

    /// Get the number of requests received so far.
    pub fn request_count(&self) -> usize {
        self.request_count.load(Ordering::SeqCst)
    }

    /// Create an `AppConfig` pointing to this mock server.
    pub fn app_config(&self, model_name: &str, dimension: usize) -> cce_config::AppConfig {
        let mut config = cce_config::AppConfig::default();

        let mut providers = HashMap::new();
        providers.insert(
            "mock".to_string(),
            cce_config::modules::ProviderConfig {
                id: "mock".to_string(),
                name: "Mock".to_string(),
                base_url: self.base_url(),
                api_keys: vec!["sk-mock".to_string()],
                ..Default::default()
            },
        );
        config.llm.providers = providers;

        let mut models = HashMap::new();
        models.insert(
            model_name.to_string(),
            cce_config::modules::EmbeddingModelConfig {
                provider_id: "mock".to_string(),
                model: model_name.to_string(),
                vector_dimension: dimension,
                ..Default::default()
            },
        );
        config.llm.embedding_models = models;
        config.embedder.default_model = model_name.to_string();

        config
    }
}

fn handle_connection(
    mut stream: TcpStream,
    responses: Arc<Mutex<Vec<MockResponse>>>,
    request_count: Arc<AtomicUsize>,
) {
    let mut buffer = [0u8; 4096];
    let bytes_read = stream.read(&mut buffer).unwrap_or(0);
    let _request = String::from_utf8_lossy(&buffer[..bytes_read]);

    request_count.fetch_add(1, Ordering::SeqCst);

    let response = {
        let mut queue = responses
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if queue.is_empty() {
            MockResponse::Success { dimension: 3 }
        } else {
            queue.remove(0)
        }
    };

    let (status_line, body) = match response {
        MockResponse::Success { dimension } => {
            let embeddings: Vec<Vec<f32>> = vec![vec![0.5; dimension]; 1];
            let body = serde_json::json!({
                "data": [{"index": 0, "embedding": embeddings[0]}],
                "usage": {"prompt_tokens": 0, "total_tokens": 0}
            });
            ("200 OK", body.to_string())
        }
        MockResponse::RateLimit => {
            let body = serde_json::json!({
                "error": {"message": "rate limit exceeded", "type": "rate_limit_error"}
            });
            ("429 Too Many Requests", body.to_string())
        }
        MockResponse::ServerError => {
            let body = serde_json::json!({
                "error": {"message": "internal server error", "type": "server_error"}
            });
            ("500 Internal Server Error", body.to_string())
        }
        MockResponse::Delayed { delay, dimension } => {
            thread::sleep(delay);
            let embeddings: Vec<Vec<f32>> = vec![vec![0.5; dimension]; 1];
            let body = serde_json::json!({
                "data": [{"index": 0, "embedding": embeddings[0]}],
                "usage": {"prompt_tokens": 0, "total_tokens": 0}
            });
            ("200 OK", body.to_string())
        }
    };

    let response_text = format!(
        "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status_line,
        body.len(),
        body
    );

    stream.write_all(response_text.as_bytes()).ok();
    stream.flush().ok();
}
