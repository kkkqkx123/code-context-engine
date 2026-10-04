//! Dead-letter truncate-retry integration tests
//!
//! Drives the full repair pass — re-chunk from disk, budget truncation, and
//! vector storage — against an in-process HTTP stand-in for Qdrant plus a
//! real SQLite generation layout. Integration scope: it exercises the real
//! storage backends end to end, not in-process logic only.

use std::path::Path;
use std::sync::Arc;

use cce_config::{AstToNlConfig, NestProcessorConfig};
use cce_llm_client::OpenAICompatibleProvider;
use cce_llm_client::services::embedding::mock_server::MockEmbeddingServer;
use cce_orchestrator::hot_update::FileChangeType;
use cce_orchestrator::index::IndexOrchestrator;
use cce_orchestrator::index_state::{
    ModuleType, ModuleUpdateState, TOKEN_LIMIT_ERROR_CODE, TrackerFailure,
};
use cce_storage_sqlite::{
    NewProjectRecord, ProjectIndexManifestRepository, ProjectRepository, SqliteClient,
};

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

fn mock_qdrant_client(url: &str) -> Arc<cce_storage_qdrant::QdrantClient> {
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
        cce_storage_qdrant::QdrantClient::new(qdrant_config, ".")
            .expect("qdrant client must build"),
    )
}

fn create_test_embedder(server: &MockEmbeddingServer) -> Arc<OpenAICompatibleProvider> {
    let config = server.app_config("test-model", 2);
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

    let database = Arc::new(SqliteClient::in_memory().expect("in-memory database"));
    database
        .with_transaction(|tx| {
            ProjectRepository::insert(
                tx,
                &NewProjectRecord::new("test".to_string(), dir.path().display().to_string()),
            )?;
            ProjectIndexManifestRepository::activate(tx, 1, 1, 0, "initial", None)?;
            tx.execute(
                "INSERT INTO files
                    (path, language, last_modified, created_at, project_id, content_hash, epoch, batch_id)
                 VALUES ('src/lib.rs', 'Rust', 1, 1, 1, ?1, 1, 0)",
                rusqlite::params![hash],
            )
            .map_err(|error| cce_types::StorageError::insert("files", error.to_string()))?;
            let file_id = tx.last_insert_rowid();

            // Seed the entity records the embedding store path links chunks
            // to. In a real run they are written before vectors; the store
            // refuses to link a chunk whose epoch-scoped entity record is
            // absent, so a repaired file must carry the same precondition as
            // an interrupted run.
            for entity in &parsed.entities {
                tx.execute(
                    "INSERT INTO entities
                        (name, kind, file_id, signature, metadata, project_id, epoch, batch_id)
                     VALUES (?1, ?2, ?3, ?4, ?5, 1, 1, 0)",
                    rusqlite::params![
                        entity.name,
                        entity.kind.to_string(),
                        file_id,
                        entity.signature,
                        format!(r#"{{"__source_entity_id":"{}"}}"#, entity.id.0),
                    ],
                )
                .map_err(|error| {
                    cce_types::StorageError::insert("entities", error.to_string())
                })?;
            }
            Ok(())
        })
        .expect("initial generation should be created");

    let qdrant_url = spawn_mock_qdrant_url().await;
    let mut orchestrator = IndexOrchestrator::new(1)
        .expect("valid project")
        .with_metadata_store(database.clone())
        .with_embedder(create_test_embedder(&MockEmbeddingServer::start()))
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
    let conn = database.read_connection().expect("read connection");
    let (total, truncated): (i64, i64) = conn
        .query_row(
            "SELECT COUNT(*), COALESCE(SUM(truncated), 0) FROM chunks
             WHERE project_id = 1 AND epoch = 1 AND path = 'emb'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("chunk counts should be queryable");
    assert!(total > 0, "embedding chunk records should be stored");
    assert!(
        truncated > 0,
        "over-budget chunks should be stored with the truncated marker"
    );
}
