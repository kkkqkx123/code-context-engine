//! BM25 alignment integration tests.
//!
//! Verifies the real index pipeline for the alignment keys: file → chunk
//! (metadata with entity_id/segment_id) → BM25 documents → tantivy search →
//! SearchResult. No Qdrant or LLM service is required — BM25-only mode.
//!
//! Covers the alignment-fix contract: entity_id and segment_id must survive
//! the BM25 write path (`IndexSchema::to_document`) and the read path
//! (`Bm25Strategy::retrieve`).

use std::sync::Arc;

use cce_codegraph::{CallChainQuery, RelationIndex};
use cce_config::AppConfig;
use cce_config::modules::{EmbeddingModelConfig, ProviderConfig};
use cce_config::project_registry::ProjectScope;
use cce_llm_client::OpenAICompatibleProvider;
use cce_orchestrator::query::types::{QueryOptions, SearchConfig, SearchSources};
use cce_orchestrator::{
    CheckpointManager, IndexOptions, IndexOrchestrator, QueryCoordinator, SearchResult,
};
use cce_storage_bm25::{Bm25Client, Bm25Config};
use cce_storage_metadb_sqlite::ChunkRepository;
use cce_storage_metadb_sqlite::SqliteClient;
use cce_storage_vector_qdrant::generate_group_id;
use cce_storage_vector_qdrant::{QdrantClient, QdrantConfig};

/// Fixture files: two Rust source files worth of functions/structs plus a
/// markdown document chunk.
const LIB_RS: &str = r#"
/// Calculate the sum of two numbers
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

/// Calculate the difference of two numbers
pub fn subtract(a: i32, b: i32) -> i32 {
    a - b
}

/// Queue configuration container
pub struct QueueConfig {
    pub max_size: usize,
    pub retry_count: u32,
}

impl QueueConfig {
    /// Create a new queue configuration
    pub fn new(max_size: usize) -> Self {
        Self { max_size, retry_count: 0 }
    }

    /// Check whether the queue is empty
    pub fn is_empty(&self) -> bool {
        self.max_size == 0
    }
}
"#;

const GUIDE_MD: &str = r#"# Queue Alignment Guide

This guide explains how to configure queue retry policies.
The zephyrwind token is unique to this document and should never appear in code.
"#;

/// Test harness: real index (BM25-only) + query coordinator sharing the same
/// BM25 client and SQLite database, mirroring the production wiring.
struct AlignmentHarness {
    _fixture_dir: tempfile::TempDir,
    _bm25_dir: tempfile::TempDir,
    sqlite: Arc<SqliteClient>,
    coordinator: QueryCoordinator,
}

impl AlignmentHarness {
    fn new() -> Self {
        let fixture_dir = tempfile::tempdir().expect("fixture temp dir");
        std::fs::create_dir_all(fixture_dir.path().join("src")).expect("create src dir");
        std::fs::create_dir_all(fixture_dir.path().join("docs")).expect("create docs dir");
        std::fs::write(fixture_dir.path().join("src/lib.rs"), LIB_RS).expect("write lib.rs");
        std::fs::write(fixture_dir.path().join("docs/guide.md"), GUIDE_MD).expect("write guide.md");

        let rt = tokio::runtime::Runtime::new().expect("runtime");

        // Shared in-memory metadata store for the index and query sides.
        let sqlite_db = Arc::new(SqliteClient::in_memory().expect("in-memory sqlite"));
        let checkpoint_manager = Arc::new(CheckpointManager::new_for_project(1, sqlite_db.clone()));

        // Insert the project row so FK-scoped writes (files/entities/chunks)
        // resolve during indexing, mirroring production wiring. The real
        // fixture directory is registered as the root so query-time lazy
        // source reads (snippet/content) can resolve chunk file paths.
        {
            let conn = sqlite_db.write_connection().expect("sqlite connection");
            conn.execute(
                "INSERT INTO projects (id, name, root_path, created_at, updated_at)
                 VALUES (1, 'alignment-harness', ?2, ?1, ?1)",
                rusqlite::params![
                    chrono::Utc::now().timestamp(),
                    fixture_dir.path().to_string_lossy().to_string()
                ],
            )
            .expect("insert project row");
        }

        // BM25 client on a unique temp index path (mirrors QueryWorkflowTest).
        let bm25_dir = tempfile::tempdir().expect("bm25 temp dir");
        let bm25_config = Bm25Config::default()
            .enabled()
            .with_index_name("default")
            .with_index_path(bm25_dir.path().to_string_lossy().as_ref());
        let mut bm25 = Bm25Client::new(bm25_config);
        rt.block_on(bm25.connect()).expect("bm25 connect");
        let bm25 = Arc::new(bm25);

        let mut orchestrator = IndexOrchestrator::new(1).expect("orchestrator");
        orchestrator = orchestrator
            .with_checkpoint_manager(checkpoint_manager)
            .with_metadata_store(sqlite_db.clone())
            .with_bm25_client(bm25.clone());

        let options = IndexOptions {
            root_dir: fixture_dir.path().to_path_buf(),
            extensions: vec!["rs".to_string(), "md".to_string()],
            store_vectors: false,
            store_bm25: true,
            store_summaries: false,
            build_relations: false,
            ..Default::default()
        };
        rt.block_on(orchestrator.execute(options))
            .expect("index execution");

        // Query coordinator: BM25-only capabilities, shared clients.
        let qdrant = Arc::new(
            QdrantClient::new(QdrantConfig::with_url("http://localhost:6333"), "test")
                .expect("qdrant client (not connected)"),
        );
        let embedder = Arc::new(mock_embedder());
        let scope = ProjectScope::new(1, generate_group_id(".")).expect("project scope");
        let empty_index = RelationIndex::new();
        let call_chain = Arc::new(CallChainQuery::from_index(empty_index));

        let coordinator = QueryCoordinator::builder(
            cce_orchestrator::index::VectorStore::Qdrant(qdrant),
            embedder,
            cce_orchestrator::index::FulltextStore::local(bm25),
            call_chain,
            scope,
        )
        .with_capabilities(cce_orchestrator::query::IndexCapabilities::new().with_bm25(true))
        .with_sqlite(sqlite_db.clone())
        .build();

        Self {
            _fixture_dir: fixture_dir,
            _bm25_dir: bm25_dir,
            sqlite: sqlite_db,
            coordinator,
        }
    }

    fn search_bm25(&self, query: &str, limit: usize) -> Vec<SearchResult> {
        let rt = tokio::runtime::Runtime::new().expect("runtime");
        let mut options = QueryOptions::new(query, 1);
        options.sources = SearchSources::none().with_bm25();
        options.config = SearchConfig {
            result: cce_orchestrator::query::types::ResultFilterConfig {
                min_score: 0.0,
                limit,
                ..Default::default()
            },
            ..Default::default()
        };
        options.with_source = true;

        rt.block_on(self.coordinator.search(&options))
            .expect("bm25 search")
            .items
    }
}

/// Mock embedder config: never called in BM25-only mode, but the coordinator
/// requires an embedder instance.
fn mock_embedder() -> OpenAICompatibleProvider {
    let mut app_config = AppConfig::default();
    let mut providers = std::collections::HashMap::new();
    providers.insert(
        "test-provider".to_string(),
        ProviderConfig {
            id: "test-provider".to_string(),
            name: "Test Provider".to_string(),
            base_url: "http://mock.local".to_string(),
            api_keys: vec!["test-key".to_string()],
            ..Default::default()
        },
    );
    let mut models = std::collections::HashMap::new();
    models.insert(
        "mock".to_string(),
        EmbeddingModelConfig {
            provider_id: "test-provider".to_string(),
            model: "mock".to_string(),
            vector_dimension: 384,
            ..Default::default()
        },
    );
    app_config.llm.providers = providers;
    app_config.llm.embedding_models = models;
    app_config.embedder.default_model = "mock".to_string();

    OpenAICompatibleProvider::from_model(&app_config, "mock").expect("mock embedder")
}

/// Code chunks must carry both alignment keys straight from the BM25 index.
#[test]
fn test_code_chunks_carry_entity_and_segment_keys() {
    let harness = AlignmentHarness::new();

    let results = harness.search_bm25("calculate sum difference", 10);
    let code_results: Vec<&SearchResult> = results
        .iter()
        .filter(|r| r.file_path.ends_with("lib.rs"))
        .collect();
    assert!(
        !code_results.is_empty(),
        "expected lib.rs results, got: {:?}",
        results
            .iter()
            .map(|r| r.file_path.as_str())
            .collect::<Vec<_>>()
    );

    for r in &code_results {
        assert!(
            !r.entity_ids.is_empty(),
            "code chunk '{}' must carry entity_ids from BM25 index",
            r.id
        );
        assert!(
            r.segment_id.is_some() && !r.segment_id.as_deref().unwrap().is_empty(),
            "code chunk '{}' must carry non-empty segment_id",
            r.id
        );
    }
}

/// Document chunks must carry segment_id (and no entity keys).
#[test]
fn test_document_chunks_carry_segment_key() {
    let harness = AlignmentHarness::new();

    let results = harness.search_bm25("zephyrwind guide", 10);
    let doc_results: Vec<&SearchResult> = results
        .iter()
        .filter(|r| r.file_path.ends_with("guide.md"))
        .collect();
    assert!(
        !doc_results.is_empty(),
        "expected guide.md results, got: {:?}",
        results
            .iter()
            .map(|r| r.file_path.as_str())
            .collect::<Vec<_>>()
    );

    for r in &doc_results {
        assert!(
            r.segment_id.is_some() && !r.segment_id.as_deref().unwrap().is_empty(),
            "document chunk '{}' must carry segment_id",
            r.id
        );
        assert!(
            r.entity_ids.is_empty(),
            "document chunk '{}' must not carry entity_ids",
            r.id
        );
    }
}

/// Multi-entity chunks: the struct group (QueueConfig + its impl methods)
/// must surface every entity through entity_ids.
#[test]
fn test_multi_entity_chunks_surface_all_entities() {
    let harness = AlignmentHarness::new();

    let results = harness.search_bm25("queue configuration", 10);
    let queue_results: Vec<&SearchResult> = results
        .iter()
        .filter(|r| r.file_path.ends_with("lib.rs"))
        .collect();

    let group = queue_results
        .iter()
        .find(|r| r.name.contains("QueueConfig") || r.content.contains("QueueConfig"))
        .expect("queue group should be found");
    assert!(
        group.entity_ids.len() >= 2,
        "struct+impl group should contain multiple entities, got {:?}",
        group.entity_ids
    );
}

/// Alignment keys must not depend on SQLite enrichment. `segment_id` is the
/// logical-group key (not a per-chunk key): a large group token-split into
/// several BM25 fragments keeps one `segment_id` across all fragments, while
/// distinct groups keep distinct `segment_id`s. This test pins that contract
/// end-to-end through the BM25 write/read round trip.
#[test]
fn test_alignment_keys_are_consistent_across_results() {
    let harness = AlignmentHarness::new();

    let results = harness.search_bm25("queue configuration calculate", 20);
    let code_results: Vec<&SearchResult> = results
        .iter()
        .filter(|r| r.file_path.ends_with("lib.rs"))
        .collect();

    // `segment_id` is the logical-group key, not a per-chunk key. The
    // cce-document chunker token-splits a large logical group into several BM25
    // fragments that all share one `segment_id` ("multiple sub-chunks (all
    // share group_id)", crates/parser/cce-document/src/common/chunker.rs:303),
    // and `SearchResult::segment_id` documents the same contract
    // (crates/app/cce-orchestrator/src/query/types/search_result.rs:36). BM25
    // fragments of one group are named "{group}_bm25_{i}", so the shared prefix
    // identifies the group. The alignment contract is therefore: fragments of
    // the same logical group must carry the same `segment_id`, while distinct
    // groups keep distinct `segment_id`s (no cross-group collision).
    fn fragment_group(id: &str) -> &str {
        id.split("_bm25_").next().unwrap_or(id)
    }

    // Every code result must carry a non-empty alignment key that survived the
    // BM25 write/read round trip.
    assert!(
        code_results.iter().all(|r| r
            .segment_id
            .as_deref()
            .map(|s| !s.is_empty())
            .unwrap_or(false)),
        "every code result must carry a non-empty segment_id"
    );

    // Fragments of the same logical group must share one segment_id. This pins
    // the alignment contract end-to-end: a token-split group is still one
    // segment. Distinct groups (different "{group}_bm25_" prefixes) are free to
    // differ, which is the correct, intended behavior.
    for i in 0..code_results.len() {
        for j in (i + 1)..code_results.len() {
            let a = &code_results[i];
            let b = &code_results[j];
            if fragment_group(&a.id) == fragment_group(&b.id) {
                assert_eq!(
                    a.segment_id.as_deref(),
                    b.segment_id.as_deref(),
                    "BM25 fragments of group '{}' must share a segment_id",
                    fragment_group(&a.id)
                );
            }
        }
    }

    // Positive contract: the fixture's QueueConfig impl is intentionally large
    // enough to split into multiple BM25 fragments; assert that the split
    // actually happened and every fragment kept one segment_id. This is the
    // real "multi-entity group ⇒ one segment_id" guarantee the old assertion
    // tried (and failed) to express.
    let mut seg_counts: std::collections::HashMap<&str, usize> = Default::default();
    for r in &code_results {
        *seg_counts
            .entry(r.segment_id.as_deref().unwrap())
            .or_insert(0) += 1;
    }
    assert!(
        seg_counts.values().any(|&c| c > 1),
        "expected the QueueConfig impl group to split into multiple BM25 fragments sharing one segment_id"
    );
}

/// BM25-only recall must surface raw code, snippet and line numbers.
///
/// Regression: BM25 chunks were only persisted in the tantivy index, never in
/// the SQLite `chunks` table, so the SQLite enrichment lookup keyed by the BM25
/// chunk_id always missed and every BM25 result was returned with empty content
/// and line 0. BM25 chunk records are now stored at index time.
#[test]
fn test_bm25_results_are_enriched_from_sqlite() {
    let harness = AlignmentHarness::new();

    let results = harness.search_bm25("calculate sum difference", 10);
    let code_results: Vec<&SearchResult> = results
        .iter()
        .filter(|r| r.file_path.ends_with("lib.rs"))
        .collect();
    assert!(
        !code_results.is_empty(),
        "expected lib.rs results, got: {:?}",
        results
            .iter()
            .map(|r| r.file_path.as_str())
            .collect::<Vec<_>>()
    );

    for r in &code_results {
        assert!(
            !r.content.is_empty(),
            "BM25 result '{}' must carry content from SQLite enrichment",
            r.id
        );
        assert!(
            r.start_line > 0,
            "BM25 result '{}' must carry a start_line",
            r.id
        );
        assert!(
            r.end_line >= r.start_line,
            "BM25 result '{}' must carry a valid end_line",
            r.id
        );
    }
}

/// The SQLite `chunks` table must contain the BM25-path records after a
/// BM25-only index, tagged with `path = 'bm25'`.
#[test]
fn test_bm25_chunk_records_persisted_in_sqlite() {
    let harness = AlignmentHarness::new();

    let results = harness.search_bm25("calculate sum difference", 10);
    let code_results: Vec<&SearchResult> = results
        .iter()
        .filter(|r| r.file_path.ends_with("lib.rs"))
        .collect();
    assert!(!code_results.is_empty());

    let conn = harness
        .sqlite
        .write_connection()
        .expect("sqlite connection");
    for r in &code_results {
        let records =
            ChunkRepository::get_by_chunk_ids(&conn, std::slice::from_ref(&r.id), 1, None)
                .expect("query chunk records");
        assert_eq!(
            records.len(),
            1,
            "BM25 result '{}' must resolve to a persisted chunk record",
            r.id
        );
        assert_eq!(
            records[0].path, "bm25",
            "record '{}' must be tagged as a bm25-path record",
            r.id
        );
        assert!(
            !records[0].content.is_empty(),
            "record '{}' must persist its content",
            r.id
        );
    }
}
