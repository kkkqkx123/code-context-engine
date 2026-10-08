//! Embedding store integration tests
//!
//! Drive `StorageCoordinator` batching, deferred rate-limit retries, and the
//! stage deadline against an in-process HTTP stand-in for Qdrant. These are
//! integration tests because they exercise real network round-trips through
//! the Qdrant client rather than pure in-process logic.

use std::sync::Arc;

use cce_config::modules::{DistanceMetric, QdrantConfig};
use cce_llm_client::OpenAICompatibleProvider;
use cce_llm_client::services::embedding::mock_server::{MockEmbeddingServer, MockResponse};
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

fn create_test_embedder(server: &MockEmbeddingServer) -> Arc<OpenAICompatibleProvider> {
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
    let server = MockEmbeddingServer::start();
    server.queue_response(MockResponse::RateLimit);

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
    let server = MockEmbeddingServer::start();
    server.queue_response(MockResponse::Delayed {
        delay: std::time::Duration::from_secs(3600),
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
    let server = MockEmbeddingServer::start();
    server.queue_response(MockResponse::RateLimit);
    server.queue_response(MockResponse::RateLimit);
    server.queue_response(MockResponse::RateLimit);
    server.queue_response(MockResponse::Success { dimension: 2 });

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
    let server = MockEmbeddingServer::start();

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
