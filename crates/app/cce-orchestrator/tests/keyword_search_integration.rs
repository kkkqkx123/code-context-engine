//! Keyword search tool integration tests (regression, local-only)
//!
//! Exercises the unified storage-layer `Bm25Retrieval` path through the
//! `KeywordSearchTool`: BM25 matching, SQLite chunk enrichment, highlight
//! generation and filtering. No external services (Qdrant/LLM) required.

use std::sync::Arc;

use cce_orchestrator::tools::keyword_search::{
    KeywordSearchError, KeywordSearchRequest, KeywordSearchTool,
};
use cce_storage_bm25::{Bm25Client, Bm25Config, Bm25Document};
use cce_storage_sqlite::{
    ChunkRecord, ChunkRepository, NewProjectRecord, ProjectRepository, SqliteClient,
};

struct TestEnv {
    bm25: Arc<tokio::sync::Mutex<Bm25Client>>,
    sqlite: Arc<SqliteClient>,
    project_id: i64,
    _tmpdir: tempfile::TempDir,
}

async fn setup() -> TestEnv {
    let tmpdir = tempfile::tempdir().expect("Failed to create temp dir");
    let index_path = tmpdir
        .path()
        .join("keyword_search_bm25")
        .to_string_lossy()
        .to_string();

    let mut config = Bm25Config::default()
        .enabled()
        .with_index_name("test_index");
    config.index_path = Some(index_path);

    let mut client = Bm25Client::new(config);
    client
        .connect()
        .await
        .expect("Failed to connect to BM25 index");

    let sqlite = SqliteClient::in_memory().expect("Failed to create SQLite client");
    let project_id = sqlite
        .with_transaction(|tx| {
            let project = NewProjectRecord::new(
                "keyword-search-test".to_string(),
                tmpdir.path().to_string_lossy().to_string(),
            );
            ProjectRepository::insert(tx, &project)
        })
        .expect("Failed to create project");

    TestEnv {
        bm25: Arc::new(tokio::sync::Mutex::new(client)),
        sqlite: Arc::new(sqlite),
        project_id,
        _tmpdir: tmpdir,
    }
}

/// Create a chunk record and materialize its source snippet on disk under
/// the project root, mirroring the lazy source read performed at query time.
fn chunk(env: &TestEnv, chunk_id: &str, path: &str, content: &str, raw_code: &str) -> ChunkRecord {
    let file = env._tmpdir.path().join(path);
    std::fs::create_dir_all(file.parent().expect("parent exists")).expect("create dirs");
    std::fs::write(&file, raw_code).expect("write source file");
    ChunkRecord::new(
        chunk_id.to_string(),
        path.to_string(),
        content.to_string(),
        0,
        1024,
    )
    .with_chunk_type("function".to_string())
    .with_project_id(env.project_id)
}

fn doc(document_id: &str, fields: &[(&str, &str)]) -> Bm25Document {
    let mut doc = Bm25Document::new(document_id);
    for (name, value) in fields {
        doc = doc.with_field(*name, *value);
    }
    doc
}

fn tool(env: &TestEnv) -> KeywordSearchTool {
    KeywordSearchTool::new(env.bm25.clone()).with_sqlite(env.sqlite.clone())
}

fn request(query: &str, top_n: usize, project_id: i64, epoch: Option<i64>) -> KeywordSearchRequest {
    KeywordSearchRequest {
        query: query.to_string(),
        top_n,
        project_id,
        epoch,
        term_operator: Default::default(),
    }
}

#[tokio::test]
async fn test_keyword_search_full_flow_with_highlights() {
    let env = setup().await;
    let chunks = vec![
        chunk(
            &env,
            "chunk_parse_query",
            "src/parser.rs",
            "pub fn parse_query(input: &str) -> Result<Query>",
            "pub fn parse_query(input: &str) -> Result<Query> {\n    // parse here\n}",
        ),
        chunk(
            &env,
            "chunk_format_output",
            "src/output.rs",
            "pub fn format_output(data: &JsonValue) -> String",
            "pub fn format_output(data: &JsonValue) -> String {\n    serde_json::to_string(data)\n}",
        ),
    ];
    env.sqlite
        .with_transaction(|tx| ChunkRepository::insert_batch(tx, &chunks))
        .expect("Failed to insert chunks");

    let bm25_docs = vec![
        doc(
            "chunk:parse_query",
            &[
                ("title", "parse_query"),
                ("content", "Parses incoming query strings"),
                ("chunk_id", "chunk_parse_query"),
                ("file_path", "src/parser.rs"),
                ("project_id", &env.project_id.to_string()),
                ("epoch", "0"),
            ],
        ),
        doc(
            "chunk:format_output",
            &[
                ("title", "format_output"),
                ("content", "Formats output into JSON"),
                ("chunk_id", "chunk_format_output"),
                ("file_path", "src/output.rs"),
                ("project_id", &env.project_id.to_string()),
                ("epoch", "0"),
            ],
        ),
    ];
    env.bm25
        .lock()
        .await
        .batch_index("test_index", &bm25_docs)
        .await
        .expect("Failed to index docs");

    let response = tool(&env)
        .search(request("parse_query", 10, env.project_id, None))
        .await
        .expect("Search should succeed");

    assert_eq!(
        response.total, 1,
        "Only parse_query chunk should match content"
    );
    let item = &response.results[0];
    assert_eq!(item.chunk_id, "chunk_parse_query");
    assert_eq!(item.file_path, "src/parser.rs");
    assert_eq!(item.title, "parse_query");
    assert_eq!(item.start_line, 0);
    assert_eq!(item.end_line, 1024);
    assert!(
        item.highlighted_snippet.contains("<mark>"),
        "Snippet should contain highlights: {}",
        item.highlighted_snippet
    );
    assert!(item.score > 0.0);
}

#[tokio::test]
async fn test_keyword_search_project_scoping() {
    let env = setup().await;
    let other_project_id = env
        .sqlite
        .with_transaction(|tx| {
            let project =
                NewProjectRecord::new("other-project".to_string(), "/tmp/other".to_string());
            ProjectRepository::insert(tx, &project)
        })
        .expect("Failed to create other project");

    let chunks = vec![chunk(
        &env,
        "chunk_shared",
        "src/lib.rs",
        "fn shared_helper() {}",
        "fn shared_helper() -> u8 { 42 }",
    )];
    env.sqlite
        .with_transaction(|tx| ChunkRepository::insert_batch(tx, &chunks))
        .expect("Failed to insert chunks");

    let bm25_docs = vec![
        doc(
            "1::shared",
            &[
                ("title", "shared_helper"),
                ("content", "shared helper implementation"),
                ("chunk_id", "chunk_shared"),
                ("project_id", &env.project_id.to_string()),
                ("epoch", "0"),
            ],
        ),
        doc(
            "2::shared",
            &[
                ("title", "shared_helper"),
                ("content", "shared helper implementation"),
                ("chunk_id", "chunk_shared"),
                ("project_id", &other_project_id.to_string()),
                ("epoch", "0"),
            ],
        ),
    ];
    env.bm25
        .lock()
        .await
        .batch_index("test_index", &bm25_docs)
        .await
        .expect("Failed to index docs");

    let response = tool(&env)
        .search(request("shared_helper", 10, env.project_id, None))
        .await
        .expect("Search should succeed");
    assert_eq!(
        response.total, 1,
        "Only project-scoped result should be returned"
    );
}

#[tokio::test]
async fn test_keyword_search_epoch_filtering() {
    let env = setup().await;
    // Chunk rows carry their generation's epoch, mirroring production writes;
    // the SQLite lookup resolves strictly inside the requested epoch view.
    let mk_chunk =
        |env: &TestEnv, id: &str, path: &str, content: &str, raw_code: &str, epoch: i64| {
            chunk(env, id, path, content, raw_code)
                .with_epoch(epoch)
                .with_project_id(env.project_id)
        };
    let chunks = vec![
        mk_chunk(
            &env,
            "chunk_v1",
            "src/parser.rs",
            "fn parse_v1() {}",
            "fn parse_v1() -> u8 { 1 }",
            1,
        ),
        mk_chunk(
            &env,
            "chunk_v2",
            "src/parser.rs",
            "fn parse_v2() {}",
            "fn parse_v2() -> u8 { 2 } // parser variant",
            2,
        ),
    ];
    env.sqlite
        .with_transaction(|tx| ChunkRepository::insert_batch(tx, &chunks))
        .expect("Failed to insert chunks");

    let bm25_docs = vec![
        doc(
            "chunk:parse_v1",
            &[
                ("title", "parse_v1"),
                ("content", "version one parser"),
                ("chunk_id", "chunk_v1"),
                ("epoch", "1"),
                ("project_id", &env.project_id.to_string()),
            ],
        ),
        doc(
            "chunk:parse_v2",
            &[
                ("title", "parse_v2"),
                ("content", "version two parser"),
                ("chunk_id", "chunk_v2"),
                ("epoch", "2"),
                ("project_id", &env.project_id.to_string()),
            ],
        ),
    ];
    env.bm25
        .lock()
        .await
        .batch_index("test_index", &bm25_docs)
        .await
        .expect("Failed to index docs");

    let response = tool(&env)
        .search(request("parser", 10, env.project_id, Some(2)))
        .await
        .expect("Search should succeed");
    assert_eq!(response.total, 1, "Epoch 2 should filter out v1");
    assert_eq!(response.results[0].chunk_id, "chunk_v2");
}

#[tokio::test]
async fn test_keyword_search_keeps_title_only_hits() {
    let env = setup().await;
    let chunks = vec![chunk(
        &env,
        "chunk_title_only",
        "src/misc.rs",
        "fn unrelated() {}",
        "fn unrelated() -> () {}",
    )];
    env.sqlite
        .with_transaction(|tx| ChunkRepository::insert_batch(tx, &chunks))
        .expect("Failed to insert chunks");

    let bm25_docs = vec![doc(
        "chunk:title_only",
        &[
            ("title", "needle_in_title_only"),
            ("content", "no matching content here"),
            ("chunk_id", "chunk_title_only"),
            ("project_id", &env.project_id.to_string()),
            ("epoch", "0"),
        ],
    )];
    env.bm25
        .lock()
        .await
        .batch_index("test_index", &bm25_docs)
        .await
        .expect("Failed to index docs");

    let response = tool(&env)
        .search(request("needle", 10, env.project_id, None))
        .await
        .expect("Search should succeed");
    // Title-only hits are kept: a title/keywords hit does not require the
    // query term to appear verbatim in the source snippet.
    assert_eq!(
        response.total, 1,
        "Title-only hits must be kept, got total: {}",
        response.total
    );
    assert_eq!(response.results[0].chunk_id, "chunk_title_only");
    assert_eq!(
        response.results[0].title, "needle_in_title_only",
        "title must stay plain text (no highlight markup)"
    );
}

#[tokio::test]
async fn test_keyword_search_sorted_by_score() {
    let env = setup().await;
    let chunks = vec![
        chunk(
            &env,
            "chunk_exact",
            "src/a.rs",
            "fn cache_invalidate() {}",
            "fn cache_invalidate() { self.cache.invalidate(); }",
        ),
        chunk(
            &env,
            "chunk_weak",
            "src/b.rs",
            "fn other() {}",
            "fn other() { let cache = vec![1, 2, 3]; }",
        ),
    ];
    env.sqlite
        .with_transaction(|tx| ChunkRepository::insert_batch(tx, &chunks))
        .expect("Failed to insert chunks");

    let bm25_docs = vec![
        doc(
            "chunk:exact",
            &[
                ("title", "cache_invalidate"),
                ("content", "cache invalidate helper for cache invalidation"),
                ("chunk_id", "chunk_exact"),
                ("project_id", &env.project_id.to_string()),
                ("epoch", "0"),
            ],
        ),
        doc(
            "chunk:weak",
            &[
                ("title", "other"),
                ("content", "a single cache mention"),
                ("chunk_id", "chunk_weak"),
                ("project_id", &env.project_id.to_string()),
                ("epoch", "0"),
            ],
        ),
    ];
    env.bm25
        .lock()
        .await
        .batch_index("test_index", &bm25_docs)
        .await
        .expect("Failed to index docs");

    let response = tool(&env)
        .search(request("cache", 10, env.project_id, None))
        .await
        .expect("Search should succeed");
    assert_eq!(response.total, 2);
    let scores: Vec<f32> = response.results.iter().map(|item| item.score).collect();
    assert!(
        scores.windows(2).all(|w| w[0] >= w[1]),
        "Results must be sorted by score descending: {scores:?}"
    );
}

#[tokio::test]
async fn test_keyword_search_validation_and_errors() {
    let env = setup().await;

    let empty_query = tool(&env)
        .search(request("  ", 10, env.project_id, None))
        .await;
    assert!(matches!(empty_query, Err(KeywordSearchError::Bm25(_))));

    let zero_project = tool(&env).search(request("query", 10, 0, None)).await;
    assert!(matches!(zero_project, Err(KeywordSearchError::Bm25(_))));

    let zero_top_n = tool(&env)
        .search(request("query", 0, env.project_id, None))
        .await;
    assert!(matches!(zero_top_n, Err(KeywordSearchError::Bm25(_))));

    let no_sqlite = KeywordSearchTool::new(env.bm25.clone())
        .search(request("query", 10, env.project_id, None))
        .await;
    assert!(matches!(
        no_sqlite,
        Err(KeywordSearchError::SqliteNotConfigured)
    ));
}
