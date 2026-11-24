//! Deterministic mock embedding server for self-contained e2e tests.
//!
//! Serves the OpenAI-compatible `/embeddings` endpoint over plain TCP so the
//! real `OpenAICompatibleProvider` can be exercised without an external LLM
//! service. Vectors are derived from a token-bag-of-words hash scheme, so
//! texts sharing tokens receive similar vectors — good enough for structural
//! alignment assertions (hybrid fusion keys), not for semantic quality.
//!
//! The server is spawned on an ephemeral port; the provider must be wired to
//! `http://127.0.0.1:{port}` via the embedding model config.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{oneshot, watch};

/// Deterministic embedding dimension (matches the mock EmbeddingConfig).
pub const MOCK_EMBEDDING_DIMENSION: usize = 384;

/// A running mock embedding server.
pub struct MockEmbeddingServer {
    /// Base URL to configure the embedder with.
    pub base_url: String,
    /// Shutdown signal for the background task.
    shutdown: oneshot::Sender<()>,
    /// Join handle of the server task.
    task: tokio::task::JoinHandle<()>,
}

impl MockEmbeddingServer {
    /// Spawn the server on an ephemeral port and wait until it accepts.
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock embedding server");
        let port = listener.local_addr().expect("local addr").port();
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();
        let (live_tx, mut live_rx) = watch::channel(false);

        let task = tokio::spawn(async move {
            loop {
                let conn = tokio::select! {
                    conn = listener.accept() => conn,
                    _ = &mut shutdown_rx => break,
                };
                let (stream, _) = match conn {
                    Ok(conn) => conn,
                    Err(_) => break,
                };
                tokio::spawn(async move {
                    let _ = handle_connection(stream).await;
                });
            }
        });

        // The listener is bound: the server is immediately live.
        live_tx.send_replace(true);

        // Wait until the readiness signal propagates to this task's view.
        while !*live_rx.borrow() {
            if live_rx.changed().await.is_err() {
                break;
            }
        }

        Self {
            base_url: format!("http://127.0.0.1:{port}"),
            shutdown: shutdown_tx,
            task,
        }
    }

    /// Shut the server down and await its task.
    pub async fn stop(self) {
        let _ = self.shutdown.send(());
        let _ = self.task.await;
    }
}

/// Read one HTTP request (headers + body), serve it, and close the connection.
async fn handle_connection(mut stream: TcpStream) -> std::io::Result<()> {
    let mut buf = Vec::with_capacity(4096);
    let mut tmp = [0u8; 4096];

    loop {
        let read = stream.read(&mut tmp).await?;
        if read == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&tmp[..read]);
        // A request is complete once the header terminator and the full body
        // (per Content-Length) have been received.
        if let Some((header_end, content_length)) = parse_headers(&buf) {
            let body_start = header_end + 4;
            if buf.len() >= body_start + content_length {
                let body = &buf[body_start..body_start + content_length];
                let response = build_response(body);
                stream.write_all(&response).await?;
                return Ok(());
            }
        }
    }
}

/// Locate the header terminator (\r\n\r\n) and Content-Length value.
fn parse_headers(buf: &[u8]) -> Option<(usize, usize)> {
    let haystack = buf.windows(4).position(|w| w == b"\r\n\r\n")?;
    let header = String::from_utf8_lossy(&buf[..haystack]);
    let content_length = header
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.trim()
                .eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    Some((haystack, content_length))
}

/// Build the HTTP response for an embeddings request.
fn build_response(body: &[u8]) -> Vec<u8> {
    if is_embeddings_request(body) {
        if let Ok(json) = embed_response(body) {
            return format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                json.len(),
                json
            )
            .into_bytes();
        }
    }
    b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec()
}

/// Detect an embedding request from the JSON body shape.
fn is_embeddings_request(body: &[u8]) -> bool {
    serde_json::from_slice::<serde_json::Value>(body)
        .map(|v| v.get("input").is_some() && v.get("model").is_some())
        .unwrap_or(false)
}

/// Compute the OpenAI-compatible embedding response.
fn embed_response(body: &[u8]) -> Result<String, serde_json::Error> {
    #[derive(serde::Deserialize)]
    struct EmbedRequest {
        model: String,
        input: Vec<String>,
    }
    #[derive(serde::Serialize)]
    struct EmbedData {
        index: usize,
        embedding: Vec<f32>,
    }
    #[derive(serde::Serialize)]
    struct EmbedResponse {
        model: String,
        data: Vec<EmbedData>,
        usage: Usage,
    }
    #[derive(serde::Serialize)]
    struct Usage {
        prompt_tokens: usize,
        total_tokens: usize,
    }

    let request: EmbedRequest = serde_json::from_slice(body)?;
    let data: Vec<EmbedData> = request
        .input
        .iter()
        .enumerate()
        .map(|(index, text)| EmbedData {
            index,
            embedding: embed_text(text),
        })
        .collect();
    let tokens: usize = request
        .input
        .iter()
        .map(|t| t.split_whitespace().count())
        .sum();

    serde_json::to_string(&EmbedResponse {
        model: request.model,
        data,
        usage: Usage {
            prompt_tokens: tokens,
            total_tokens: tokens,
        },
    })
}

/// Deterministic token bag-of-words embedding, L2-normalized.
///
/// Each token maps to one dimension via a stable hash; overlapping tokens
/// between query and chunk produce overlapping vectors (cosine similarity
/// reflects lexical overlap). Perfectly deterministic across runs.
fn embed_text(text: &str) -> Vec<f32> {
    let mut vector = vec![0.0f32; MOCK_EMBEDDING_DIMENSION];
    for token in text.split_whitespace() {
        let token = token.to_lowercase();
        let hash = fnv1a(token.as_bytes());
        let index = (hash as usize) % MOCK_EMBEDDING_DIMENSION;
        vector[index] += 1.0;
    }
    let norm = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm > 0.0 {
        for value in &mut vector {
            *value /= norm;
        }
    }
    vector
}

/// FNV-1a 64-bit hash (deterministic, no std HashMap randomness involved).
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in bytes {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Shared handle allowing multiple embedders to resolve against the server.
pub type SharedMockServer = Arc<MockEmbeddingServer>;

/// Create an embedding AppConfig wired to the mock server, mirroring the
/// wiring done by `crate::query_test::QueryWorkflowTest`.
pub fn mock_embedding_config(base_url: &str) -> cce_config::AppConfig {
    use cce_config::modules::{EmbeddingModelConfig, ProviderConfig};

    let mut app_config = cce_config::AppConfig::default();
    let mut providers = HashMap::new();
    providers.insert(
        "mock-provider".to_string(),
        ProviderConfig {
            id: "mock-provider".to_string(),
            name: "Mock Provider".to_string(),
            base_url: base_url.to_string(),
            api_keys: vec!["test-key".to_string()],
            ..Default::default()
        },
    );
    let mut models = HashMap::new();
    models.insert(
        "mock".to_string(),
        EmbeddingModelConfig {
            provider_id: "mock-provider".to_string(),
            model: "mock".to_string(),
            vector_dimension: MOCK_EMBEDDING_DIMENSION,
            ..Default::default()
        },
    );
    app_config.llm.providers = providers;
    app_config.llm.embedding_models = models;
    app_config.embedder.default_model = "mock".to_string();
    app_config
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fnv1a_is_deterministic() {
        assert_eq!(fnv1a(b"add"), fnv1a(b"add"));
        assert_ne!(fnv1a(b"add"), fnv1a(b"sub"));
    }

    #[test]
    fn test_embed_text_is_normalized() {
        let v = embed_text("calculate sum of two numbers");
        let norm: f32 = v.iter().map(|x| x * x).sum();
        assert!((norm - 1.0).abs() < 1e-3);
        assert_eq!(v.len(), MOCK_EMBEDDING_DIMENSION);
    }

    #[test]
    fn test_embed_text_overlap_reflects_lexical_overlap() {
        let a = embed_text("queue configuration retry policy");
        let b = embed_text("queue configuration retry policy docs");
        let c = embed_text("calculate sum of numbers");
        let cosine = |x: &[f32], y: &[f32]| x.iter().zip(y).map(|(i, j)| i * j).sum::<f32>();
        assert!(
            cosine(&a, &b) > cosine(&a, &c),
            "lexically overlapping texts must be more similar"
        );
    }

    #[tokio::test]
    async fn test_server_serves_embeddings_endpoint() {
        let server = MockEmbeddingServer::start().await;
        let config = mock_embedding_config(&server.base_url);
        let provider = cce_llm_client::OpenAICompatibleProvider::from_model(&config, "mock")
            .expect("provider from mock config");
        let result = provider
            .embed(&["queue configuration", "calculate sum"])
            .await;
        let result = result.expect("embed against mock server");
        assert_eq!(result.embeddings.len(), 2);
        assert_eq!(result.embeddings[0].len(), MOCK_EMBEDDING_DIMENSION);
        let cosine = result.embeddings[0]
            .iter()
            .zip(&result.embeddings[1])
            .map(|(a, b)| a * b)
            .sum::<f32>();
        assert!(cosine < 0.9, "different texts must not be near-identical");
        server.stop().await;
    }
}
