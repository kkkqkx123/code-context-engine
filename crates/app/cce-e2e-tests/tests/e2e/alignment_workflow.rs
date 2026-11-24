//! Hybrid fusion alignment workflow tests.
//!
//! Alignment correctness is the product of three concerns with different
//! responsibilities, tested in layers:
//!
//! - **Key generation** (parse / grouper / chunker) — A-class.
//! - **Key transport** (payload / BM25 schema fields) — covered by storage
//!   unit tests and the orchestrator `bm25_alignment_integration.rs`.
//! - **Key consumption** (fusion alignment / dedup / determinism) — covered by
//!   fusion unit tests and the B-class pipeline smoke below.
//!
//! ## A-class: alignment structure (no DB, no embedding, no BM25)
//!
//! Real fixtures are fed straight through `FileProcessor::process_file_complete`
//! and the chunk-metadata invariants are asserted. This is millisecond-level and
//! exercises the real grouper/chunker boundary behaviour that hard-coded
//! integration cases cannot.
//!
//! ## B-class: pipeline smoke (local backend by default)
//!
//! Exercises index → query → fusion wiring without asserting alignment math.
//! The deterministic mock embedding server provides vectors and the embedded
//! local engine provides vector search, so no external service is needed. The
//! hybrid fixtures write the vector score threshold off (`min_score = 0`)
//! because the mock embedder produces only weak lexical similarity (0.1~0.45).
//!
//! ## C-class: remote Qdrant verification (service optional)
//!
//! `test_empty_data_point_pipeline_smoke` is the only test in this file that
//! talks to a real Qdrant service. It probes `localhost:6333` first and skips
//! (passes without asserting) when the service is unreachable.

use cce_storage_common::FulltextDocument;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use cce_codegraph::{CallChainQuery, RelationIndex};
use cce_config::project_registry::ProjectScope;
use cce_e2e_tests::mock_embedding_server::{MOCK_EMBEDDING_DIMENSION, MockEmbeddingServer};
use cce_llm_client::OpenAICompatibleProvider;
use cce_metrics::{MetricsRegistry, SearchMetrics};
use cce_orchestrator::index::FileProcessor;
use cce_orchestrator::query::IndexCapabilities;
use cce_orchestrator::query::types::{QueryOptions, SearchConfig};
use cce_orchestrator::{QueryCoordinator, SearchResult, SearchSources};
use cce_parser::ast_to_nl::chunker::result::ChunkedResult;
use cce_scanner::FileEntry;
use cce_storage_bm25::{Bm25Client, Bm25Config};
use cce_storage_vector_qdrant::QdrantConfig;
use cce_storage_vector_qdrant::{QdrantClient, generate_project_group_id};
use cce_types::language::LanguageInfo;
use cce_types::{OutputMode, PointKind};

use crate::helper::{
    EmptyFixture, QueryWorkflowTest, TestFixture, init_minimal_logging, mock_embedding,
};

// ============================================================================
// A-class: alignment structure (pure parse, no external services)
// ============================================================================

/// Process one file through the real parse → grouper → chunker pipeline and
/// return the produced chunks. No storage, embedding, or BM25 involvement.
async fn parse_chunks(root: &Path, relative: &str) -> Vec<ChunkedResult> {
    parse_chunks_with_config(root, relative, &cce_config::AstToNlConfig::default()).await
}

/// Like `parse_chunks` but with an explicit AST-to-NL configuration (used to
/// enable cross-group merging, which defaults to off).
async fn parse_chunks_with_config(
    root: &Path,
    relative: &str,
    config: &cce_config::AstToNlConfig,
) -> Vec<ChunkedResult> {
    let path = root.join(relative);
    let language_info = LanguageInfo::detect_from_path(relative);
    let file_entry = FileEntry::new(path, PathBuf::from(relative), 0, chrono::Utc::now())
        .with_language_info(language_info);

    let mut processor = FileProcessor::with_config(config).with_project_id(1);
    let result = processor
        .process_file_complete(&file_entry, OutputMode::Both)
        .await
        .expect("file must process through the real pipeline");
    result.chunks
}

/// Every chunk must carry a non-empty `segment_id`; the embedding and BM25
/// chunks of one group must share the same segment.
#[tokio::test]
async fn test_a_code_chunks_carry_segment_and_entity_keys() {
    init_minimal_logging();
    let fixture = TestFixture::rust_basic().expect("rust/basic fixture");
    let root = fixture.root_path().to_path_buf();

    for relative in ["src/lib.rs", "src/main.rs"] {
        let chunks = parse_chunks(&root, relative).await;
        assert!(!chunks.is_empty(), "{} must produce chunks", relative);

        // Group chunks by their logical group and verify segment consistency.
        let mut by_group: HashMap<String, Vec<&ChunkedResult>> = HashMap::new();
        for chunk in &chunks {
            assert!(
                !chunk.metadata.segment_id.is_empty(),
                "{} chunk '{}' must carry a non-empty segment_id",
                relative,
                chunk.chunk_id
            );
            by_group
                .entry(chunk.source_group_id.clone())
                .or_default()
                .push(chunk);
        }

        for (group_id, group_chunks) in &by_group {
            let first_segment = &group_chunks[0].metadata.segment_id;
            for chunk in group_chunks {
                assert_eq!(
                    &chunk.metadata.segment_id, first_segment,
                    "chunks of group '{}' must share one segment_id",
                    group_id
                );
            }
        }
    }

    // Code chunks: segment_id is the group id, and body entities are present.
    for relative in ["src/lib.rs", "src/main.rs"] {
        let chunks = parse_chunks(&root, relative).await;
        let code_chunks: Vec<&ChunkedResult> =
            chunks.iter().filter(|c| c.metadata.is_code()).collect();
        assert!(
            !code_chunks.is_empty(),
            "{} must contain code chunks",
            relative
        );
        for chunk in &code_chunks {
            assert_eq!(
                chunk.metadata.segment_id, chunk.source_group_id,
                "code chunk '{}' must align on source_group_id",
                chunk.chunk_id
            );
        }
        let with_entities = code_chunks
            .iter()
            .filter(|c| !c.metadata.content_entity_ids().is_empty())
            .count();
        assert!(
            with_entities > 0,
            "{} must contain at least one code chunk with body entities",
            relative
        );
    }
}

/// Non-code chunks (txt/log/md) carry a segment key but no entity ids, and
/// their title falls back to a heading or the file stem. Generic text
/// (txt/log) uses the `PlainText` payload, markdown the `Document` one; both
/// are non-code, which is the invariant asserted here.
#[tokio::test]
async fn test_a_document_chunks_carry_segment_without_entities() {
    init_minimal_logging();
    let fixture = TestFixture::documents().expect("documents fixture");
    let root = fixture.root_path().to_path_buf();

    for relative in ["notes.txt", "server.log", "README.md"] {
        let chunks = parse_chunks(&root, relative).await;
        assert!(!chunks.is_empty(), "{} must produce chunks", relative);
        for chunk in &chunks {
            assert!(
                !chunk.metadata.is_code(),
                "{} chunk '{}' must be a non-code (document/config/plaintext) chunk, got {:?}",
                relative,
                chunk.chunk_id,
                chunk.metadata.content_type
            );
            assert!(
                !chunk.metadata.segment_id.is_empty(),
                "{} chunk '{}' must carry a segment_id",
                relative,
                chunk.chunk_id
            );
            assert!(
                chunk.metadata.content_entity_ids().is_empty(),
                "{} chunk '{}' must not carry entity ids",
                relative,
                chunk.chunk_id
            );
            assert!(
                chunk.bm25_title.as_deref().is_some_and(|t| !t.is_empty()),
                "{} chunk '{}' must carry a title fallback",
                relative,
                chunk.chunk_id
            );
        }
    }
}

/// Multi-entity chunks (e.g. a struct group) surface every entity and keep a
/// deterministic primary key (first element) within the group.
#[tokio::test]
async fn test_a_multi_entity_chunks_surface_all_entities() {
    init_minimal_logging();
    let fixture = EmptyFixture::new().expect("fixture");
    fixture
        .add_file(
            "src/queue.rs",
            r#"
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
"#,
        )
        .expect("write fixture");

    let chunks = parse_chunks(fixture.root(), "src/queue.rs").await;

    // Every group chunk with ≥2 entities must list all of them, and the
    // primary key (first element) must be identical across the group's chunks.
    let mut by_group: HashMap<String, Vec<&ChunkedResult>> = HashMap::new();
    for chunk in &chunks {
        if chunk.metadata.content_entity_ids().len() >= 2 {
            by_group
                .entry(chunk.source_group_id.clone())
                .or_default()
                .push(chunk);
        }
    }
    assert!(
        !by_group.is_empty(),
        "expected a multi-entity (struct+impl) group, chunks: {:?}",
        chunks
            .iter()
            .map(|c| (
                c.source_group_id.as_str(),
                c.metadata.content_entity_ids().to_vec()
            ))
            .collect::<Vec<_>>()
    );

    for (group_id, group_chunks) in &by_group {
        let primary = group_chunks[0]
            .metadata
            .content_entity_ids()
            .first()
            .copied();
        for chunk in group_chunks {
            assert!(
                !chunk.metadata.content_entity_ids().is_empty(),
                "group '{}' chunk '{}' must list entities",
                group_id,
                chunk.chunk_id
            );
            assert_eq!(
                chunk.metadata.content_entity_ids().first().copied(),
                primary,
                "group '{}' must keep a single primary entity",
                group_id
            );
        }
    }
}

/// Merged chunks record every contributing group in `merged_group_ids` and
/// keep the segment of their primary group. Cross-group merging is enabled
/// explicitly (it defaults to off) so the invariant is actually exercised.
#[tokio::test]
async fn test_a_merged_chunks_record_group_attribution() {
    init_minimal_logging();
    let fixture = EmptyFixture::new().expect("fixture");
    fixture
        .add_file(
            "src/lib.rs",
            r#"
/// A small configuration struct
pub struct SmallConfig {
    pub retry: u32,
}

/// A second small struct with one method
pub struct TinyHandle {
    pub ready: bool,
}

impl TinyHandle {
    /// Check readiness
    pub fn is_ready(&self) -> bool {
        self.ready
    }
}
"#,
        )
        .expect("write fixture");

    // Two struct groups stay separate through the grouper (only standalone
    // fragments are pre-merged), so their undersized chunks must be cross-group
    // merged when the merge thresholds are configured.
    let mut config = cce_config::AstToNlConfig::default();
    config.chunking.min_chunk_tokens = 60;
    config.chunking.cross_group_merge_threshold = 300;

    let chunks = parse_chunks_with_config(fixture.root(), "src/lib.rs", &config).await;
    assert!(!chunks.is_empty(), "fixture must produce chunks");

    // The fixture is small enough that cross-group merging must actually occur.
    let merged: Vec<&ChunkedResult> = chunks
        .iter()
        .filter(|c| !c.metadata.merged_group_ids.is_empty())
        .collect();
    assert!(
        !merged.is_empty(),
        "expected cross-group merging, chunks: {:?}",
        chunks
            .iter()
            .map(|c| (
                c.source_group_id.as_str(),
                c.metadata.merged_group_ids.clone()
            ))
            .collect::<Vec<_>>()
    );

    for chunk in &chunks {
        assert!(
            !chunk.metadata.segment_id.is_empty(),
            "chunk '{}' must carry a segment_id",
            chunk.chunk_id
        );
    }

    for merged_chunk in merged {
        assert!(
            !merged_chunk
                .metadata
                .merged_group_ids
                .contains(&merged_chunk.source_group_id),
            "merged chunk must not list its own primary group in merged_group_ids"
        );
        assert!(
            !merged_chunk.metadata.merged_group_ids.is_empty(),
            "merged chunk must record at least one contributing group"
        );
        assert_eq!(
            merged_chunk.metadata.segment_id, merged_chunk.source_group_id,
            "merged chunk keeps the primary group's segment"
        );
    }
}

/// Boundary cases: empty file, heading-less plain text, nested headings,
/// structured deep nodes, and an oversized single segment.
#[tokio::test]
async fn test_a_boundary_cases() {
    init_minimal_logging();
    let fixture = TestFixture::documents().expect("documents fixture");
    let root = fixture.root_path().to_path_buf();

    // Empty file: must process without panic and produce no chunks.
    let empty = parse_chunks(&root, "empty.txt").await;
    assert!(empty.is_empty(), "empty file must produce no chunks");

    // Heading-less plain text: title falls back to the file stem.
    let untitled = parse_chunks(&root, "plain_untitled.txt").await;
    assert!(!untitled.is_empty(), "plain text must produce chunks");
    for chunk in &untitled {
        assert!(!chunk.metadata.segment_id.is_empty());
        assert!(chunk.metadata.content_entity_ids().is_empty());
        assert_eq!(
            chunk.bm25_title.as_deref(),
            Some("plain_untitled"),
            "no-heading text must use the file stem as title"
        );
    }

    // Nested markdown headings: every chunk keeps its segment, headings yield
    // document structure rather than entity ids.
    let nested_md = parse_chunks(&root, "README.md").await;
    assert!(!nested_md.is_empty(), "markdown must produce chunks");
    for chunk in &nested_md {
        assert!(!chunk.metadata.segment_id.is_empty());
        assert!(chunk.metadata.content_entity_ids().is_empty());
    }

    // Structured deep nodes (json/xml/toml): same invariants as plain text.
    for relative in [
        "json/nested.json",
        "xml/with-attributes.xml",
        "toml/with-tables.toml",
    ] {
        let chunks = parse_chunks(&root, relative).await;
        assert!(!chunks.is_empty(), "{} must produce chunks", relative);
        for chunk in &chunks {
            assert!(!chunk.metadata.segment_id.is_empty());
            assert!(chunk.metadata.content_entity_ids().is_empty());
        }
    }

    // Oversized single segment: a long function must be fragmented while every
    // fragment keeps a non-empty segment key.
    let mut long_fn = String::from("pub fn long_running() -> i32 {\n    let mut acc = 0;\n");
    for i in 0..400 {
        long_fn.push_str(&format!("    acc = acc.wrapping_add({i});\n"));
    }
    long_fn.push_str("    acc\n}\n");
    let long_fixture = EmptyFixture::new().expect("fixture");
    long_fixture
        .add_file("src/long.rs", long_fn)
        .expect("write long file");
    let chunks = parse_chunks(long_fixture.root(), "src/long.rs").await;
    assert!(
        chunks.iter().any(|c| c.metadata.is_fragment()),
        "a 400-statement function must be split into fragments"
    );
    for chunk in &chunks {
        assert!(
            !chunk.metadata.segment_id.is_empty(),
            "fragment '{}' must keep a segment_id",
            chunk.chunk_id
        );
    }
}

// ============================================================================
// B-class: pipeline smoke (Qdrant optional + mock embedding)
// ============================================================================

/// Search configuration with every score threshold disabled. The mock
/// embedder only yields weak lexical similarity (0.1~0.45), below the default
/// `vector.similarity_threshold` of 0.3; alignment assertions are structural, so the
/// vector path must not be silently filtered to zero.
fn no_threshold_config() -> SearchConfig {
    let mut config = SearchConfig::default();
    config.vector.similarity_threshold = 0.0;
    config.result.min_score = 0.0;
    config
}

/// Controlled fixture: code with a struct group plus a markdown document.
fn hybrid_fixture_content() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "src/lib.rs",
            r#"
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
"#,
        ),
        (
            "docs/guide.md",
            r#"# Queue Alignment Guide

This guide explains how to configure queue retry policies.
The zephyrwind token is unique to this document and never appears in code.
"#,
        ),
    ]
}

fn fill_fixture(fixture: &EmptyFixture) {
    for (path, content) in hybrid_fixture_content() {
        fixture
            .add_file(path, content)
            .expect("fixture file written");
    }
}

fn setup_hybrid_query_test(
    server: &MockEmbeddingServer,
    scenario: &str,
) -> QueryWorkflowTest<TestFixture> {
    let fixture = EmptyFixture::new().expect("fixture");
    fill_fixture(&fixture);

    let embedder = Arc::new(
        OpenAICompatibleProvider::from_model(
            &cce_e2e_tests::mock_embedding_server::mock_embedding_config(&server.base_url),
            "mock",
        )
        .expect("mock embedder"),
    );

    QueryWorkflowTest::new(fixture.into_test_fixture(), mock_embedding())
        .with_embedder(embedder)
        .with_extensions(vec!["rs".to_string(), "md".to_string()])
        .with_config(no_threshold_config())
        .with_scenario_name(scenario)
}

/// Shared setup for hybrid tests (embedded local backend, no service needed).
async fn hybrid_harness(
    with_metrics: bool,
    scenario: &str,
) -> (
    QueryWorkflowTest<TestFixture>,
    MockEmbeddingServer,
    Option<Arc<SearchMetrics>>,
) {
    init_minimal_logging();
    let server = MockEmbeddingServer::start().await;

    let mut query_test = setup_hybrid_query_test(&server, scenario);
    let metrics = if with_metrics {
        let registry = Arc::new(MetricsRegistry::new());
        let handle = SearchMetrics::new(&registry, 1);
        query_test = query_test.with_metrics_registry(registry);
        Some(handle)
    } else {
        None
    };

    query_test.index().await.expect("hybrid index failed");
    (query_test, server, metrics)
}

/// Derive the fusion alignment key, delegating to the shared `cce_types`
/// derivation also consumed by the production fusion pipeline.
fn alignment_key_of(item: &SearchResult) -> String {
    cce_types::alignment_key(&item.entity_ids, item.segment_id.as_deref(), &item.id)
        .unwrap_or_else(|| format!("c:{}", item.id))
}

/// B1: pipeline smoke over a real fixture. After the F0 fix the vector path
/// must return results, and the hybrid path must fuse keys from both paths.
#[tokio::test]
async fn test_hybrid_smoke_rust_basic() {
    init_minimal_logging();
    let server = MockEmbeddingServer::start().await;

    let fixture = TestFixture::rust_basic().expect("rust/basic fixture");
    let embedder = Arc::new(
        OpenAICompatibleProvider::from_model(
            &cce_e2e_tests::mock_embedding_server::mock_embedding_config(&server.base_url),
            "mock",
        )
        .expect("mock embedder"),
    );
    let mut query_test = QueryWorkflowTest::new(fixture, mock_embedding())
        .with_embedder(embedder)
        .with_config(no_threshold_config())
        .with_scenario_name("alignment-smoke-rust-basic");

    let index_result = query_test.index().await.expect("index failed");
    assert!(
        index_result.total_entities >= 1,
        "fixture must index entities"
    );

    // Vector path must return real results (would be 0 before the F0 fix).
    let vector = query_test
        .search_vector("process internal result", 20)
        .await
        .expect("vector search failed");
    assert!(
        vector.total > 0,
        "vector path must return results after the retrieval fix"
    );
    let vector_items: Vec<SearchResult> = vector.items.clone();

    let bm25 = query_test
        .search_bm25("process internal result", 20)
        .await
        .expect("bm25 search failed");
    assert!(bm25.total > 0, "expected BM25 results");
    let bm25_items: Vec<SearchResult> = bm25.items.clone();

    let hybrid = query_test
        .search_hybrid("process internal result", 20)
        .await
        .expect("hybrid search failed");
    assert!(hybrid.total > 0, "expected hybrid results");

    // Structural overlap: both paths must surface at least one shared key.
    let vector_keys: std::collections::HashSet<String> =
        vector_items.iter().map(alignment_key_of).collect();
    let bm25_keys: std::collections::HashSet<String> =
        bm25_items.iter().map(alignment_key_of).collect();
    let overlap: Vec<&String> = vector_keys.intersection(&bm25_keys).collect();
    assert!(
        !overlap.is_empty(),
        "vector and BM25 must share at least one alignment key"
    );

    query_test.cleanup().await;
    server.stop().await;
}

/// Build a bare QueryCoordinator (no index pipeline) over manually injected
/// storage: used for the empty-data-point smoke.
async fn build_bare_coordinator(
    qdrant: Arc<QdrantClient>,
    bm25: Arc<Bm25Client>,
    embedder: Arc<OpenAICompatibleProvider>,
    project_id: i64,
    group_id: &str,
) -> QueryCoordinator {
    let scope = ProjectScope::new(project_id, group_id.to_string()).expect("project scope");
    let metadata_store =
        cce_orchestrator::index::in_memory_metadata_store().expect("in-memory store");
    let empty_index = RelationIndex::new();
    let call_chain = Arc::new(CallChainQuery::from_index(empty_index));

    QueryCoordinator::builder(
        cce_orchestrator::index::VectorStore::Qdrant(qdrant),
        embedder,
        cce_orchestrator::index::FulltextStore::local(bm25),
        call_chain,
        scope,
    )
    .with_capabilities(IndexCapabilities::new().with_vectors(true).with_bm25(true))
    .with_metadata_store(metadata_store)
    .build()
}

fn mock_vector(dim: usize, seed: u32) -> Vec<f32> {
    let mut v = Vec::with_capacity(dim);
    for i in 0..dim {
        v.push(((i as u32).wrapping_mul(seed).wrapping_add(seed) % 7) as f32 / 7.0);
    }
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in &mut v {
            *x /= norm;
        }
    }
    v
}

/// B2 (remote Qdrant verification): empty-data-point pipeline smoke. Fake
/// chunks carrying only alignment keys are written straight to Qdrant + BM25
/// (no parse/embedding), then the QueryCoordinator must recall them: vector
/// path returns results (F0 fix), fusion keys land in the known key set, and
/// epoch/group filters apply. Skips when no Qdrant service is reachable.
#[tokio::test]
async fn test_empty_data_point_pipeline_smoke() {
    init_minimal_logging();
    let server = MockEmbeddingServer::start().await;

    let project_id = 1;
    // Stable scenario key so repeated runs reuse one partition of the shared
    // collection instead of growing it without bound. Stale points from a
    // previous (possibly failed) run are cleared before injecting.
    let group_id = generate_project_group_id(project_id, "/empty/data/points");

    // --- Qdrant: ensure collection, clear stale points, inject fake points ---
    let mut qdrant_config = QdrantConfig::with_url("http://localhost:6333");
    qdrant_config.vector_size = MOCK_EMBEDDING_DIMENSION;
    let qdrant = Arc::new(QdrantClient::new(qdrant_config, "test").expect("qdrant client"));
    if let Err(e) = qdrant.initialize().await {
        eprintln!("skipping remote Qdrant verification (localhost:6333 unreachable): {e}");
        server.stop().await;
        return;
    }
    qdrant.delete_by_group(&group_id).await.ok();

    use cce_storage_vector_qdrant::{Payload, VectorPoint};
    let point = |id: &str, file: &str, entity: Option<i64>, segment: Option<&str>| VectorPoint {
        id: id.to_string(),
        vector: mock_vector(MOCK_EMBEDDING_DIMENSION, fnv_seed(id)),
        payload: {
            let mut p = Payload::new(file)
                .with_source_id(id)
                .with_group_id(&group_id)
                .with_type(PointKind::Chunk)
                .with_epoch(0);
            if let Some(e) = entity {
                p = p.with_entity_ids(vec![e]);
            }
            if let Some(s) = segment {
                p = p.with_segment_id(s);
            }
            p
        },
    };

    let in_scope_points = vec![
        point("chunk_a", "alpha.rs", Some(101), Some("seg_alpha")),
        point("chunk_b", "beta.rs", Some(102), Some("seg_beta")),
        point("chunk_s", "docs/guide.md", None, Some("seg_guide")),
    ];
    // Out-of-scope points: different group, and stale epoch.
    let mut stale_group = point("chunk_x", "other.rs", Some(999), Some("seg_other"));
    stale_group.payload = stale_group
        .payload
        .with_group_id("project-2-some-other-group");
    let mut stale_epoch = point("chunk_y", "old.rs", Some(888), Some("seg_old"));
    stale_epoch.payload = stale_epoch.payload.with_group_id(&group_id).with_epoch(5);

    qdrant
        .upsert_points(&in_scope_points)
        .await
        .expect("upsert in-scope points");
    qdrant
        .upsert_points(&[stale_group, stale_epoch])
        .await
        .expect("upsert stale points");

    // --- BM25: inject fake documents with matching keys ---
    let bm25_dir = tempfile::tempdir().expect("bm25 temp dir");
    let bm25_config = Bm25Config::default()
        .enabled()
        .with_index_name("default")
        .with_index_path(bm25_dir.path().to_string_lossy().as_ref());
    let mut bm25 = Bm25Client::new(bm25_config);
    bm25.connect().await.expect("bm25 connect");
    let bm25 = Arc::new(bm25);

    let mut docs: Vec<FulltextDocument> = Vec::new();
    for (id, title, entity, segment) in [
        ("chunk_a", "alpha function", Some("101"), Some("seg_alpha")),
        ("chunk_b", "beta function", Some("102"), Some("seg_beta")),
        ("chunk_s", "queue guide", None, Some("seg_guide")),
    ] {
        let mut doc = FulltextDocument::new(id)
            .with_field("chunk_id", id)
            .with_field("title", title)
            .with_field("content", format!("{title} zephyrwind queue retry policy"))
            .with_field("file_path", id)
            .with_field("project_id", project_id.to_string())
            .with_field("epoch", "0");
        if let Some(e) = entity {
            doc = doc.with_field("entity_id", e);
        }
        if let Some(s) = segment {
            doc = doc.with_field("segment_id", s);
        }
        docs.push(doc);
    }
    {
        bm25.batch_index("default", &docs)
            .await
            .expect("bm25 index");
    }

    // --- Query through the coordinator ---
    let embedder = Arc::new(
        OpenAICompatibleProvider::from_model(
            &cce_e2e_tests::mock_embedding_server::mock_embedding_config(&server.base_url),
            "mock",
        )
        .expect("mock embedder"),
    );
    let coordinator =
        build_bare_coordinator(qdrant.clone(), bm25, embedder, project_id, &group_id).await;

    let mut options = QueryOptions::new("zephyrwind queue retry policy", project_id);
    options.sources = SearchSources::default();
    options.config = no_threshold_config();
    options.config.vector.top_k = 20;
    options.config.result.limit = 20;
    options.with_source = true;

    // Vector-only path must recall the injected points (F0 regression guard).
    let mut vector_options = options.clone();
    vector_options.sources = SearchSources::none().with_vector();
    let vector_result = coordinator
        .search(&vector_options)
        .await
        .expect("vector search");
    assert!(
        vector_result.total > 0,
        "vector path must return the injected points (retrieval fix)"
    );

    // Hybrid path: every fused key must be one we injected, and both paths
    // must contribute at least one shared key.
    let hybrid = coordinator.search(&options).await.expect("hybrid search");
    let known_keys = ["e:101", "e:102", "s:seg_guide"];
    for item in &hybrid.items {
        let key = alignment_key_of(item);
        assert!(
            known_keys.contains(&key.as_str()),
            "fused key '{}' must come from the injected data",
            key
        );
    }

    let vector_keys: std::collections::HashSet<String> =
        vector_result.items.iter().map(alignment_key_of).collect();
    let bm25_keys: std::collections::HashSet<String> = {
        let mut bm25_opts = options.clone();
        bm25_opts.sources = SearchSources::none().with_bm25();
        coordinator
            .search(&bm25_opts)
            .await
            .expect("bm25 search")
            .items
            .iter()
            .map(alignment_key_of)
            .collect()
    };
    assert!(
        !vector_keys
            .intersection(&bm25_keys)
            .collect::<Vec<_>>()
            .is_empty(),
        "vector and BM25 must share an injected key"
    );

    // No out-of-scope data may leak: foreign group and stale epoch keys absent.
    let foreign_keys: Vec<String> = hybrid
        .items
        .iter()
        .map(|i| i.id.clone())
        .filter(|id| id == "chunk_x" || id == "chunk_y")
        .collect();
    assert!(
        foreign_keys.is_empty(),
        "out-of-scope points must be filtered: {:?}",
        foreign_keys
    );

    qdrant.delete_by_group(&group_id).await.ok();
    // Remove the injected out-of-scope point so it never leaks into the shared
    // collection across runs.
    qdrant
        .delete_by_group("project-2-some-other-group")
        .await
        .ok();
    server.stop().await;
}

/// B3: hybrid output is deterministic across repeated queries.
#[tokio::test]
async fn test_hybrid_output_is_deterministic() {
    let (mut query_test, server, _metrics) = hybrid_harness(false, "alignment-determinism").await;

    let first = query_test
        .search_hybrid("queue configuration retry policy", 20)
        .await
        .expect("hybrid search failed");
    let ids_first: Vec<(String, f32)> = first
        .items
        .iter()
        .map(|r| (r.id.clone(), r.score))
        .collect();

    let second = query_test
        .search_hybrid("queue configuration retry policy", 20)
        .await
        .expect("hybrid search failed");
    let ids_second: Vec<(String, f32)> = second
        .items
        .iter()
        .map(|r| (r.id.clone(), r.score))
        .collect();

    assert_eq!(
        ids_first, ids_second,
        "identical queries must produce identical (id, score) sequences"
    );

    query_test.cleanup().await;
    server.stop().await;
}

/// B4: hybrid fusion surfaces one entry per alignment key, and no duplicate
/// (chunk id, entity id) pair (entity-level granularity).
#[tokio::test]
async fn test_hybrid_dedup_invariants() {
    let (mut query_test, server, _metrics) = hybrid_harness(false, "alignment-dedup").await;

    let hybrid = query_test
        .search_hybrid("queue configuration retry policy", 20)
        .await
        .expect("hybrid search failed");
    assert!(hybrid.total > 0, "hybrid query must return results");

    let mut seen_keys = std::collections::HashSet::new();
    let mut seen_pairs = std::collections::HashSet::new();
    for item in &hybrid.items {
        let key = alignment_key_of(item);
        assert!(
            seen_keys.insert(key.clone()),
            "alignment key '{key}' must appear at most once"
        );
        let pair = (
            item.id.clone(),
            item.entity_ids.first().map(|e| e.0).unwrap_or(u64::MAX),
        );
        assert!(
            seen_pairs.insert(pair),
            "duplicate (chunk id, entity id) pair for chunk '{}'",
            item.id
        );
    }

    query_test.cleanup().await;
    server.stop().await;
}

/// B5: the hybrid alignment coverage metric records a nonzero ratio when both
/// paths align.
#[tokio::test]
async fn test_hybrid_alignment_coverage_metric() {
    let (mut query_test, server, metrics) = hybrid_harness(true, "alignment-coverage").await;
    let metrics = metrics.expect("metrics handle");

    query_test
        .search_hybrid("queue configuration retry policy", 20)
        .await
        .expect("hybrid search failed");

    assert!(
        metrics.hybrid_alignment_match_ratio.get_count() > 0,
        "hybrid query must record the alignment coverage histogram"
    );
    let average = metrics.hybrid_alignment_match_ratio.get_average();
    assert!(
        average > 0.0,
        "alignment coverage ratio must be > 0 when both paths align, got {average}"
    );
    assert!(
        metrics.hybrid_alignment_single_path_ratio.get_count() > 0
            && metrics.hybrid_alignment_unalignable_ratio.get_count() > 0,
        "hybrid query must record the single-path and unalignable histograms"
    );
    let single_path = metrics.hybrid_alignment_single_path_ratio.get_average();
    let unalignable = metrics.hybrid_alignment_unalignable_ratio.get_average();
    assert!(
        (0.0..=1.0).contains(&single_path) && (0.0..=1.0).contains(&unalignable),
        "alignment ratios must stay in [0, 1], got {single_path} / {unalignable}"
    );

    query_test.cleanup().await;
    server.stop().await;
}

/// Mock server sanity: the embedder must be reachable through the mock
/// endpoint at the configured dimension.
#[tokio::test]
async fn test_mock_embedder_produces_expected_dimension() {
    init_minimal_logging();
    let server = MockEmbeddingServer::start().await;
    let embedder = OpenAICompatibleProvider::from_model(
        &cce_e2e_tests::mock_embedding_server::mock_embedding_config(&server.base_url),
        "mock",
    )
    .expect("mock embedder");
    let result = embedder
        .embed(&["queue configuration retry policy"])
        .await
        .expect("embed");
    assert_eq!(result.embeddings[0].len(), MOCK_EMBEDDING_DIMENSION);
    server.stop().await;
}

/// Small deterministic FNV seed for stable fake vector generation.
fn fnv_seed(text: &str) -> u32 {
    let mut hash: u32 = 0x811c9dc5;
    for b in text.as_bytes() {
        hash ^= *b as u32;
        hash = hash.wrapping_mul(0x01000193);
    }
    hash
}
