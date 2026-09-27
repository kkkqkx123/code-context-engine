//! Embedding store integration tests
//!
//! Drive `StorageCoordinator` batching, deferred rate-limit retries, and the
//! stage deadline against an in-process HTTP stand-in for Qdrant. These are
//! integration tests because they exercise real network round-trips through
//! the Qdrant client rather than pure in-process logic.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use cce_config::modules::{DistanceMetric, QdrantConfig};
use cce_llm::{Embedder, EmbeddingResult, LlmError};
use cce_orchestrator::OrchestratorError;
use cce_orchestrator::index::StorageCoordinator;
use cce_parser::ast_to_nl::chunker::{
    ChunkMetadata, ChunkPath, ChunkedResult, CodeSpecificMetadata,
};
use cce_storage_qdrant::QdrantClient;
use cce_storage_sqlite::ChunkRecord;
use cce_types::ast_to_nl::FileCategory;
use cce_types::entity::{EntityId, EntityKind};
use cce_types::{Language, Span};

/// Minimal in-process Qdrant stand-in: answers every request with a
/// successful upsert response.
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

/// Build a Qdrant client pointed at a mock server (`2`-dimensional vectors,
/// matching the stub embedders used here).
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

/// Embedder stub: rate-limits the first `rate_limit_calls` invocations and
/// succeeds afterwards with fixed-dimension vectors.
struct StubEmbedder {
    calls: AtomicU32,
    rate_limit_calls: u32,
}

#[async_trait::async_trait]
impl Embedder for StubEmbedder {
    async fn embed(&self, texts: &[&str]) -> Result<EmbeddingResult, LlmError> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        if call < self.rate_limit_calls {
            return Err(LlmError::rate_limit_exceeded(5));
        }
        Ok(EmbeddingResult {
            embeddings: texts.iter().map(|_| vec![0.5_f32, 0.5_f32]).collect(),
            prompt_tokens: 0,
            total_tokens: 0,
        })
    }

    async fn embed_one(&self, text: &str) -> Result<Vec<f32>, LlmError> {
        self.embed(&[text])
            .await
            .map(|r| r.embeddings.first().cloned().unwrap_or_default())
    }

    async fn embed_vectors(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, LlmError> {
        self.embed(texts).await.map(|r| r.embeddings)
    }

    fn dimension(&self) -> usize {
        2
    }

    fn model_name(&self) -> &str {
        "stub-embedder"
    }

    fn is_healthy(&self) -> bool {
        true
    }
}

/// Embedder stub that never answers: proves the stage deadline fires.
struct HangingEmbedder;

#[async_trait::async_trait]
impl Embedder for HangingEmbedder {
    async fn embed(&self, _texts: &[&str]) -> Result<EmbeddingResult, LlmError> {
        tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
        unreachable!("the deadline must cancel this call first")
    }

    async fn embed_one(&self, text: &str) -> Result<Vec<f32>, LlmError> {
        self.embed(&[text]).await.map(|r| r.embeddings[0].clone())
    }

    async fn embed_vectors(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, LlmError> {
        self.embed(texts).await.map(|r| r.embeddings)
    }

    fn dimension(&self) -> usize {
        2
    }

    fn model_name(&self) -> &str {
        "hanging-embedder"
    }

    fn is_healthy(&self) -> bool {
        true
    }
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

fn storage_with(qdrant_url: &str, embedder: Arc<dyn Embedder>) -> StorageCoordinator {
    StorageCoordinator::new(7)
        .expect("valid project ID")
        .with_project_group_id("project-7-root")
        .with_qdrant(mock_qdrant_client(qdrant_url))
        .with_embedder(embedder)
}

#[tokio::test]
async fn rate_limited_batch_is_deferred_and_retried_after_other_batches() {
    let qdrant_url = spawn_mock_qdrant_url().await;
    let stub = Arc::new(StubEmbedder {
        calls: AtomicU32::new(0),
        rate_limit_calls: 1,
    });
    let embedder: Arc<dyn Embedder> = stub.clone();
    let storage = storage_with(&qdrant_url, embedder.clone());

    let chunks = [
        embedding_chunk("a", "first"),
        embedding_chunk("b", "second"),
    ];
    let stored = storage
        .store_vectors_batched(&chunks, 1, 0)
        .await
        .expect("store must succeed");

    // Batch "a" was rate limited on its first attempt, deferred, and
    // retried after batch "b"; both batches end up stored.
    assert_eq!(stored, 2);
    assert_eq!(
        stub.calls.load(Ordering::SeqCst),
        3,
        "expected: batch a (429) + batch b + batch a retry"
    );
}

#[tokio::test]
async fn embedding_stage_deadline_surfaces_as_identifiable_error() {
    let qdrant_url = spawn_mock_qdrant_url().await;
    let embedder: Arc<dyn Embedder> = Arc::new(HangingEmbedder);
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
    // Both the initial attempt and the deferred retry are rate limited.
    let stub = Arc::new(StubEmbedder {
        calls: AtomicU32::new(0),
        rate_limit_calls: 3,
    });
    let embedder: Arc<dyn Embedder> = stub.clone();
    let storage = storage_with(&qdrant_url, embedder.clone());

    let chunks = [
        embedding_chunk("a", "first"),
        embedding_chunk("b", "second"),
    ];
    let result = storage.store_vectors_batched(&chunks, 1, 0).await;

    // Both batches are rate limited on the first pass and deferred. On the
    // retry pass batch "a" fails again while batch "b" succeeds. The
    // retry-pass failure must not be reported as Ok: the caller stops
    // advancing the checkpoint boundary so the uncommitted work unit is
    // replayed on resume.
    assert!(result.is_err(), "deferred retry failure must surface");
    assert_eq!(
        stub.calls.load(Ordering::SeqCst),
        4,
        "expected: batch a (429) + batch b (429) + both retries"
    );
}

#[tokio::test]
async fn reembed_vectors_from_records_upserts_every_chunk() {
    let qdrant_url = spawn_mock_qdrant_url().await;
    let stub = Arc::new(StubEmbedder {
        calls: AtomicU32::new(0),
        rate_limit_calls: 0,
    });
    let embedder: Arc<dyn Embedder> = stub.clone();
    let storage = storage_with(&qdrant_url, embedder);

    let records = [stored_record("a"), stored_record("b")];
    let stored = storage
        .reembed_vectors_from_records(&records, 1)
        .await
        .expect("re-embed must succeed");

    assert_eq!(stored, 2);
    assert_eq!(
        stub.calls.load(Ordering::SeqCst),
        2,
        "one call per microbatch"
    );
}
