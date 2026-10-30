//! Embedding store integration tests
//!
//! Drive `StorageCoordinator` batching, deferred rate-limit retries, and the
//! stage deadline against an in-process HTTP stand-in for Qdrant plus a
//! scripted stand-in for the embeddings endpoint. These are integration
//! tests because they exercise real network round-trips through the Qdrant
//! client and the llm-suite-backed embedder rather than pure in-process
//! logic.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use cce_config::modules::{DistanceMetric, QdrantConfig};
use cce_llm_client::OpenAICompatibleProvider;
use cce_orchestrator::OrchestratorError;
use cce_orchestrator::index::StorageCoordinator;
use cce_parser::ast_to_nl::chunker::{
    ChunkMetadata, ChunkPath, ChunkedResult, CodeSpecificMetadata,
};
use cce_storage_metadb_sqlite::ChunkRecord;
use cce_storage_vector_qdrant::QdrantClient;
use cce_types::ast_to_nl::FileCategory;
use cce_types::entity::{EntityId, EntityKind};
use cce_types::{Language, Span};

/// One scripted embeddings response, consumed in queue order.
#[derive(Clone)]
enum ScriptedResponse {
    /// Successful embedding response with the given dimension.
    Success { dimension: usize },
    /// 429 rate-limit response.
    RateLimit,
    /// Sleep before responding normally.
    Delayed { delay: Duration, dimension: usize },
}

/// Scripted stand-in for the OpenAI-compatible `/embeddings` endpoint.
///
/// Responses are consumed FIFO; an empty queue answers success. The server
/// task is detached on purpose: it lives until the test runtime shuts down,
/// mirroring the previous mock's fire-and-forget threads.
struct ScriptedEmbeddingServer {
    base_url: String,
    responses: Arc<std::sync::Mutex<Vec<ScriptedResponse>>>,
    request_count: Arc<AtomicUsize>,
}

impl ScriptedEmbeddingServer {
    async fn start() -> Self {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock embedding port");
        let addr = listener.local_addr().expect("local addr");
        let responses = Arc::new(std::sync::Mutex::new(Vec::new()));
        let request_count = Arc::new(AtomicUsize::new(0));

        let task_responses = Arc::clone(&responses);
        let task_count = Arc::clone(&request_count);
        tokio::spawn(async move {
            loop {
                let (mut socket, _) = match listener.accept().await {
                    Ok(accepted) => accepted,
                    Err(_) => break,
                };
                let responses = Arc::clone(&task_responses);
                let count = Arc::clone(&task_count);
                tokio::spawn(async move {
                    let mut buf = Vec::with_capacity(4096);
                    let mut tmp = [0u8; 4096];
                    let (input_count, requested_dimension) = loop {
                        match socket.read(&mut tmp).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => {
                                buf.extend_from_slice(&tmp[..n]);
                                if let Some((header_end, content_length)) = parse_headers(&buf) {
                                    let body_start = header_end + 4;
                                    if buf.len() >= body_start + content_length {
                                        break parse_embedding_request(
                                            &buf[body_start..body_start + content_length],
                                        );
                                    }
                                }
                            }
                        }
                    };
                    count.fetch_add(1, Ordering::SeqCst);
                    let scripted = {
                        let mut queue = responses
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner());
                        if queue.is_empty() {
                            None
                        } else {
                            Some(queue.remove(0))
                        }
                    };
                    let response = scripted.unwrap_or(ScriptedResponse::Success { dimension: 2 });
                    let (status_line, body) = match response {
                        ScriptedResponse::Success { dimension } => {
                            let dimension = requested_dimension.unwrap_or(dimension);
                            ("200 OK", success_body(input_count, dimension))
                        }
                        ScriptedResponse::RateLimit => {
                            let body = serde_json::json!({
                                "error": {"message": "rate limit exceeded", "type": "rate_limit_error"}
                            });
                            ("429 Too Many Requests", body.to_string())
                        }
                        ScriptedResponse::Delayed { delay, dimension } => {
                            tokio::time::sleep(delay).await;
                            let dimension = requested_dimension.unwrap_or(dimension);
                            ("200 OK", success_body(input_count, dimension))
                        }
                    };
                    let reply = format!(
                        "HTTP/1.1 {status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len(),
                    );
                    let _ = socket.write_all(reply.as_bytes()).await;
                });
            }
        });

        Self {
            base_url: format!("http://{addr}"),
            responses,
            request_count,
        }
    }

    fn queue_response(&self, response: ScriptedResponse) {
        self.responses
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(response);
    }

    fn request_count(&self) -> usize {
        self.request_count.load(Ordering::SeqCst)
    }

    fn app_config(&self, model_name: &str, dimension: usize) -> cce_config::AppConfig {
        let mut config = cce_config::AppConfig::default();
        let mut providers = HashMap::new();
        providers.insert(
            "mock".to_string(),
            cce_config::modules::ProviderConfig {
                id: "mock".to_string(),
                name: "Mock".to_string(),
                base_url: self.base_url.clone(),
                api_keys: vec!["sk-mock".to_string()],
                max_retries: 0,
                retry_delay_ms: 0,
                rate_limit: 0,
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

fn parse_embedding_request(body: &[u8]) -> (usize, Option<usize>) {
    let value = serde_json::from_slice::<serde_json::Value>(body).ok();
    let input_count = value
        .as_ref()
        .and_then(|value| value.get("input"))
        .map(|input| match input {
            serde_json::Value::Array(items) => items.len(),
            serde_json::Value::String(_) => 1,
            _ => 1,
        })
        .unwrap_or(1);
    let dimension = value
        .as_ref()
        .and_then(|value| value.get("dimensions"))
        .and_then(|dimension| dimension.as_u64())
        .map(|dimension| dimension as usize);
    (input_count, dimension)
}

fn success_body(input_count: usize, dimension: usize) -> String {
    let data: Vec<serde_json::Value> = (0..input_count)
        .map(|index| {
            serde_json::json!({
                "index": index,
                "embedding": vec![0.5f32; dimension]
            })
        })
        .collect();
    serde_json::json!({
        "data": data,
        "usage": {"prompt_tokens": 0, "total_tokens": 0}
    })
    .to_string()
}

async fn spawn_mock_qdrant_url() -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock qdrant port");
    let addr = listener.local_addr().expect("local addr");
    tokio::spawn(async move {
        loop {
            let (mut socket, _) = match listener.accept().await {
                Ok(accepted) => accepted,
                Err(_) => break,
            };
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                let _ = socket.read(&mut buf).await;
                let body = r#"{"result":{"operation_id":1,"status":"ok"}}"#;
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-length: {}\r\ncontent-type: application/json\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = socket.write_all(response.as_bytes()).await;
            });
        }
    });
    format!("http://{addr}")
}

fn mock_qdrant_client(url: &str) -> Arc<QdrantClient> {
    let qdrant_config = QdrantConfig {
        url: url.to_string(),
        vector_size: 2,
        distance_metric: DistanceMetric::Cosine,
        timeout_ms: 5000,
        max_retries: 0,
        retry_delay_ms: 10,
        enabled: true,
        ..Default::default()
    };
    Arc::new(QdrantClient::new(qdrant_config, ".").expect("qdrant client must build"))
}

fn create_test_embedder(server: &ScriptedEmbeddingServer) -> Arc<OpenAICompatibleProvider> {
    let config = server.app_config("test-model", 2);
    let provider =
        OpenAICompatibleProvider::from_model(&config, "test-model").expect("create embedder");
    Arc::new(provider)
}

fn embedding_chunk(id: &str, text: &str) -> ChunkedResult {
    let mut chunk = ChunkedResult::new(
        id.to_string(),
        format!("{id}-source"),
        ChunkPath::Embedding,
        0,
        1,
    );
    chunk.text = text.to_string();
    chunk.metadata = ChunkMetadata::for_code(
        format!("{id}.rs"),
        Span::default(),
        Language::Rust,
        CodeSpecificMetadata {
            content_entity_ids: vec![EntityId(7)],
            entity_kind: EntityKind::Function,
            ..Default::default()
        },
    );
    chunk
}

fn stored_record(id: &str) -> (ChunkRecord, u8) {
    (
        ChunkRecord::new(
            id.to_string(),
            "src/lib.rs".to_string(),
            format!("content of {id}"),
            0,
            9,
        )
        .with_project_id(7)
        .with_epoch(3)
        .with_batch_id(11)
        .with_entity_ids_json("[42]".to_string())
        .with_chunk_type("function".to_string())
        .with_segment_id(format!("{id}-source")),
        FileCategory::Code.as_u8(),
    )
}

fn storage_with(qdrant_url: &str, embedder: Arc<OpenAICompatibleProvider>) -> StorageCoordinator {
    StorageCoordinator::new(7)
        .expect("valid project ID")
        .with_project_group_id("project-7-root")
        .with_qdrant(mock_qdrant_client(qdrant_url))
        .with_embedder(embedder)
}

#[tokio::test]
async fn rate_limited_batch_is_deferred_and_retried_after_other_batches() {
    let qdrant_url = spawn_mock_qdrant_url().await;
    let server = ScriptedEmbeddingServer::start().await;
    server.queue_response(ScriptedResponse::RateLimit);

    let embedder = create_test_embedder(&server);
    let storage = storage_with(&qdrant_url, embedder);

    let chunks = [
        embedding_chunk("a", "first"),
        embedding_chunk("b", "second"),
    ];
    let stored = storage
        .store_vectors_batched(&chunks, 1, 0)
        .await
        .expect("store must succeed");

    assert_eq!(stored, 2);
    assert_eq!(
        server.request_count(),
        3,
        "expected: batch a (429) + batch b + batch a retry"
    );
}

#[tokio::test]
async fn embedding_stage_deadline_surfaces_as_identifiable_error() {
    let qdrant_url = spawn_mock_qdrant_url().await;
    let server = ScriptedEmbeddingServer::start().await;
    server.queue_response(ScriptedResponse::Delayed {
        delay: Duration::from_secs(3600),
        dimension: 2,
    });

    let embedder = create_test_embedder(&server);
    let mut storage = storage_with(&qdrant_url, embedder);
    storage.set_embedding_stage_timeout(1);

    let chunks = [embedding_chunk("a", "first")];
    let error = storage
        .store_vectors_batched(&chunks, 1, 0)
        .await
        .expect_err("a wedged embedder must hit the stage deadline");
    match error {
        OrchestratorError::Index { operation, .. } => {
            assert_eq!(operation, "EMBEDDING_STAGE_TIMEOUT");
        }
        other => panic!("expected an index-stage timeout, got {other}"),
    }
}

#[tokio::test]
async fn rate_limited_retry_failure_surfaces_as_batch_error() {
    let qdrant_url = spawn_mock_qdrant_url().await;
    let server = ScriptedEmbeddingServer::start().await;
    server.queue_response(ScriptedResponse::RateLimit);
    server.queue_response(ScriptedResponse::RateLimit);
    server.queue_response(ScriptedResponse::RateLimit);
    server.queue_response(ScriptedResponse::Success { dimension: 2 });

    let embedder = create_test_embedder(&server);
    let storage = storage_with(&qdrant_url, embedder);

    let chunks = [
        embedding_chunk("a", "first"),
        embedding_chunk("b", "second"),
    ];
    let result = storage.store_vectors_batched(&chunks, 1, 0).await;

    assert!(result.is_err(), "deferred retry failure must surface");
    assert_eq!(
        server.request_count(),
        4,
        "expected: batch a (429) + batch b (429) + both retries"
    );
}

#[tokio::test]
async fn reembed_vectors_from_records_upserts_every_chunk() {
    let qdrant_url = spawn_mock_qdrant_url().await;
    let server = ScriptedEmbeddingServer::start().await;

    let embedder = create_test_embedder(&server);
    let storage = storage_with(&qdrant_url, embedder);

    let records = [stored_record("a"), stored_record("b")];
    let stored = storage
        .reembed_vectors_from_records(&records, 1)
        .await
        .expect("re-embed must succeed");

    assert_eq!(stored, 2);
    assert_eq!(server.request_count(), 2, "one call per microbatch");
}
