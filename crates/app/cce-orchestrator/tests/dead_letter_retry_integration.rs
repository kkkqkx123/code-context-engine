//! Dead-letter truncate-retry integration tests
//!
//! Drives the full repair pass — re-chunk from disk, budget truncation, and
//! vector storage — against an in-process HTTP stand-in for Qdrant plus a
//! real SQLite generation layout. Integration scope: it exercises the real
//! storage backends end to end, not in-process logic only.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use cce_config::{AstToNlConfig, NestProcessorConfig};
use cce_llm_client::OpenAICompatibleProvider;
use cce_orchestrator::hot_update::FileChangeType;
use cce_orchestrator::index::IndexOrchestrator;
use cce_orchestrator::index::{MetadataStore, in_memory_metadata_store};
use cce_orchestrator::index_state::{
    ModuleType, ModuleUpdateState, TOKEN_LIMIT_ERROR_CODE, TrackerFailure,
};
use cce_storage_common::MetadataStorage;
use cce_storage_common::metadb::EntityRecord;

/// Success-only stand-in for the OpenAI-compatible `/embeddings` endpoint.
///
/// The server task is detached on purpose: it lives until the test runtime
/// shuts down. Vectors are constant; this test asserts repair wiring (chunk
/// records, truncation markers, queue state), never vector values.
struct SuccessEmbeddingServer {
    base_url: String,
}

impl SuccessEmbeddingServer {
    async fn start() -> Self {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock embedding port");
        let addr = listener.local_addr().expect("local addr");
        tokio::spawn(async move {
            loop {
                let (mut socket, _) = match listener.accept().await {
                    Ok(accepted) => accepted,
                    Err(_) => break,
                };
                tokio::spawn(async move {
                    let mut buf = Vec::with_capacity(4096);
                    let mut tmp = [0u8; 4096];
                    let input_count = loop {
                        match socket.read(&mut tmp).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => {
                                buf.extend_from_slice(&tmp[..n]);
                                if let Some((header_end, content_length)) = parse_headers(&buf) {
                                    let body_start = header_end + 4;
                                    if buf.len() >= body_start + content_length {
                                        break parse_input_count(
                                            &buf[body_start..body_start + content_length],
                                        );
                                    }
                                }
                            }
                        }
                    };
                    let data: Vec<serde_json::Value> = (0..input_count)
                        .map(|index| {
                            serde_json::json!({
                                "index": index,
                                "embedding": vec![0.5f32; 2]
                            })
                        })
                        .collect();
                    let body = serde_json::json!({
                        "data": data,
                        "usage": {"prompt_tokens": 0, "total_tokens": 0}
                    })
                    .to_string();
                    let reply = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len(),
                    );
                    let _ = socket.write_all(reply.as_bytes()).await;
                });
            }
        });
        Self {
            base_url: format!("http://{addr}"),
        }
    }

    fn app_config(&self, model_name: &str) -> cce_config::AppConfig {
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
                vector_dimension: 2,
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

fn parse_input_count(body: &[u8]) -> usize {
    serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .and_then(|value| value.get("input").cloned())
        .map(|input| match input {
            serde_json::Value::Array(items) => items.len(),
            serde_json::Value::String(_) => 1,
            _ => 1,
        })
        .unwrap_or(1)
}

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

fn mock_qdrant_client(url: &str) -> Arc<cce_storage_vector_qdrant::QdrantClient> {
    let qdrant_config = cce_config::modules::QdrantConfig {
        url: url.to_string(),
        vector_size: 2,
        distance_metric: cce_config::modules::DistanceMetric::Cosine,
        timeout_ms: 5000,
        max_retries: 0,
        retry_delay_ms: 10,
        enabled: true,
        ..Default::default()
    };
    Arc::new(
        cce_storage_vector_qdrant::QdrantClient::new(qdrant_config, ".")
            .expect("qdrant client must build"),
    )
}

fn create_test_embedder(server: &SuccessEmbeddingServer) -> Arc<OpenAICompatibleProvider> {
    let config = server.app_config("test-model");
    let provider =
        OpenAICompatibleProvider::from_model(&config, "test-model").expect("create embedder");
    Arc::new(provider)
}

/// End-to-end executor pass: a dead-lettered file is re-chunked from
/// disk, its over-budget embedding chunks are truncated, records land in
/// the active generation with the `truncated` marker, and the module
/// leaves the dead letter queue.
#[tokio::test]
async fn truncate_retry_repairs_dead_letter_embedding() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file_path = dir.path().join("src/lib.rs");
    std::fs::create_dir(dir.path().join("src")).expect("create src dir");
    let content = "pub fn alpha() -> u32 { let mut v: Vec<u32> = Vec::new(); \
         for i in 0..64 { v.push(i * i + 7); } v.iter().sum() }\n\
         pub fn beta(text: &str) -> usize { text.chars().filter(|c| c.is_alphabetic()).count() }\n";
    std::fs::write(&file_path, content).expect("write source file");
    let hash = cce_utils::hash::calculate_hash(content.as_bytes());

    // Parse once up front so the entity rows seeded below carry exactly the
    // source ids the retry re-derives when it re-chunks the same content.
    let parsed = cce_parser::parser::ParseCoordinator::new()
        .parse("src/lib.rs", content)
        .expect("parse test source");

    let database = in_memory_metadata_store().expect("in-memory store");
    database
        .ensure_project(1, &dir.path().display().to_string())
        .await
        .expect("project should be ensured");
    database
        .manifest_begin_building(1, 1, "initial", None)
        .await
        .expect("manifest should be created");
    database
        .manifest_activate(1, 1, 0, "initial", None)
        .await
        .expect("initial generation should be activated");
    database
        .files_record_hashes(1, 1, &[("src/lib.rs".to_string(), hash.clone())])
        .await
        .expect("initial generation should be created");
    let file = database
        .file_by_path(1, "src/lib.rs")
        .await
        .expect("file should load")
        .expect("file should exist");
    // Seed the entity records the embedding store path links chunks
    // to. In a real run they are written before vectors; the store
    // refuses to link a chunk whose epoch-scoped entity record is
    // absent, so a repaired file must carry the same precondition as
    // an interrupted run.
    let records: Vec<EntityRecord> = parsed
        .entities
        .iter()
        .map(|entity| EntityRecord {
            id: 0,
            name: entity.name.clone(),
            kind: entity.kind.to_string(),
            file_id: file.id,
            signature: Some(entity.signature.clone()),
            span_start_row: None,
            span_end_row: None,
            span_start_column: None,
            span_end_column: None,
            span_start_byte: None,
            span_end_byte: None,
            scoped_name: None,
            overload_discriminator: String::new(),
            depth: None,
            parent_id: None,
            metadata: Some(format!(r#"{{"__source_entity_id":"{}"}}"#, entity.id.0)),
            parameters_json: None,
            return_type: None,
            doc_comment: None,
            modifiers_json: None,
            project_id: 1,
            epoch: 1,
            batch_id: 0,
            rank: 0.0,
        })
        .collect();
    database
        .entities_upsert(&records)
        .await
        .expect("entities should seed");

    let qdrant_url = spawn_mock_qdrant_url().await;
    let mut orchestrator = IndexOrchestrator::new(1)
        .expect("valid project")
        .with_metadata_store(database.clone())
        .with_embedder(create_test_embedder(&SuccessEmbeddingServer::start().await))
        .with_qdrant_client(mock_qdrant_client(&qdrant_url))
        .with_project_fingerprint("project-1-root".to_string())
        .with_file_processor_configs(
            NestProcessorConfig::default(),
            &AstToNlConfig::both(),
            &cce_config::LicenseHeaderConfig::default(),
        )
        .with_dead_letter_config(true, 8);

    // Drive Embedding into the dead letter queue for the recorded file
    // with the token-limit classification that makes it a truncate target.
    let recorded = file_path.to_string_lossy().to_string();
    {
        let tracker = orchestrator.state_tracker();
        tracker
            .create_update(Path::new(&recorded), FileChangeType::Modified)
            .await;
        tracker
            .mark_failed(
                Path::new(&recorded),
                ModuleType::Embedding,
                TrackerFailure {
                    message: "Token limit exceeded: 9000 > 8192".to_string(),
                    code: Some(TOKEN_LIMIT_ERROR_CODE.to_string()),
                    retryable: false,
                },
            )
            .await
            .expect("state exists");
        assert_eq!(tracker.get_truncate_retry_candidates().await.len(), 1);
    }

    let report = orchestrator
        .retry_dead_letter_with_truncation()
        .await
        .expect("retry pass should run");
    assert_eq!(report.retried, 1);
    assert_eq!(report.succeeded, 1);
    assert_eq!(report.still_failed, 0);
    assert!(
        report.truncated_chunks > 0,
        "over-budget chunks should be counted in the report"
    );

    // The module left the dead letter queue and consumed its one attempt.
    let tracker = orchestrator.state_tracker();
    let state = tracker
        .get_state(Path::new(&recorded))
        .await
        .expect("state exists");
    let record = state.get_module_state(ModuleType::Embedding);
    assert!(matches!(record.state, ModuleUpdateState::Success));
    assert!(record.truncated);
    assert!(tracker.get_truncate_retry_candidates().await.is_empty());

    // Stored active-generation records carry the truncate marker.
    let (total, truncated): (i64, i64) = database
        .chunks_stats(1, 1, "emb")
        .await
        .expect("chunk counts should be queryable");
    assert!(total > 0, "embedding chunk records should be stored");
    assert!(
        truncated > 0,
        "over-budget chunks should be stored with the truncated marker"
    );
}
