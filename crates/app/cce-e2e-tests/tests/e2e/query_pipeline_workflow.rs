//! Query pipeline workflow tests
//!
//! Tests for detailed query pipeline components: ranking, fusion,
//! and configuration feedback loop. These tests verify the internal query
//! pipeline behavior beyond the basic workflow tests in query_workflow.rs.

use cce_orchestrator::query::{
    IndexCapabilities, ScoreSorter, SearchConfig, SearchResult, ThresholdFilter,
    types::ResultFilterConfig,
};
use cce_types::EntityId;

use crate::helper::init_minimal_logging;

// ---------------------------------------------------------------------------
// Ranking
// ---------------------------------------------------------------------------

fn entity_id_for_test(id: &str) -> Option<EntityId> {
    // Derive a unique EntityId from the id string for fusion tests.
    // Each unique id should map to a unique u64 value so fusion doesn't
    // merge distinct entities that happen to share a None entity_id.
    let idx = match id {
        "a" => 1,
        "b" => 2,
        "c" => 3,
        "d" => 4,
        "e" => 5,
        _ => id
            .bytes()
            .fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64)),
    };
    Some(EntityId(idx))
}

/// Helper to create a SearchResult with given score
fn make_search_result(id: &str, score: f32, file: &str) -> SearchResult {
    SearchResult {
        id: id.to_string(),
        score,
        original_score: score,
        vector_score: score,
        bm25_score: None,
        file_path: file.to_string(),
        name: format!("entity_{}", id),
        kind: "function".to_string(),
        entity_ids: entity_id_for_test(id).into_iter().collect(),
        ..Default::default()
    }
}

/// Query result ranking and threshold filtering
#[test]
fn test_ranking_threshold_filtering() {
    init_minimal_logging();

    let config = SearchConfig {
        result: ResultFilterConfig {
            min_score: 0.5,
            limit: usize::MAX,
            ..Default::default()
        },
        ..Default::default()
    };

    let results = vec![
        make_search_result("a", 0.9, "src/main.rs"),
        make_search_result("b", 0.3, "src/lib.rs"),
        make_search_result("c", 0.7, "src/utils.rs"),
        make_search_result("d", 0.1, "src/low.rs"),
        make_search_result("e", 0.6, "src/medium.rs"),
    ];

    let filter = ThresholdFilter::new();
    let filtered = filter.apply(results, &config).unwrap();

    // Items below 0.5 threshold should be filtered out
    assert_eq!(filtered.len(), 3);
    assert!(filtered.iter().all(|r| r.score >= 0.5));
}

#[test]
fn test_ranking_with_empty_results() {
    init_minimal_logging();

    let filter = ThresholdFilter::new();
    let config = SearchConfig::default();
    let filtered = filter.apply(vec![], &config).unwrap();

    assert!(filtered.is_empty());
}

#[test]
fn test_score_sorter_order() {
    init_minimal_logging();

    let results = vec![
        make_search_result("a", 0.7, "src/main.rs"),
        make_search_result("b", 0.9, "src/lib.rs"),
        make_search_result("c", 0.8, "src/utils.rs"),
    ];

    let sorter = ScoreSorter::new();
    let sorted = sorter.sort(results);

    assert_eq!(sorted[0].id, "b");
    assert_eq!(sorted[1].id, "c");
    assert_eq!(sorted[2].id, "a");
}

// ---------------------------------------------------------------------------
// Fusion
// ---------------------------------------------------------------------------

/// Hybrid search fusion (vector + BM25)
#[test]
fn test_hybrid_fusion_weighted() {
    init_minimal_logging();

    use cce_orchestrator::query::retrieval::{HybridFusionConfig, fuse_hybrid_results};

    let config = HybridFusionConfig {
        vector_weight: 0.5,
        bm25_weight: 0.5,
        include_vector_only_keys: true,
        include_bm25_only_keys: true,
        min_score: 0.0,
        dedup_by_chunk: false,
        ..HybridFusionConfig::default()
    };

    // Vector results
    let vector_results = vec![
        make_search_result("a", 0.95, "src/main.rs"),
        make_search_result("b", 0.85, "src/lib.rs"),
        make_search_result("c", 0.75, "src/utils.rs"),
    ];

    // BM25 results (different ordering and scores)
    let mut bm25_a = make_search_result("a", 0.6, "src/main.rs");
    bm25_a.bm25_score = Some(0.6);
    bm25_a.vector_score = 0.0;

    let mut bm25_c = make_search_result("c", 0.8, "src/utils.rs");
    bm25_c.bm25_score = Some(0.8);
    bm25_c.vector_score = 0.0;

    let mut bm25_d = make_search_result("d", 0.7, "src/extra.rs");
    bm25_d.bm25_score = Some(0.7);
    bm25_d.vector_score = 0.0;

    let bm25_results = vec![bm25_c, bm25_d, bm25_a];

    let fused = fuse_hybrid_results(vector_results, bm25_results, &config);

    // Should produce fused results with both sources contributing
    assert!(!fused.is_empty(), "Fused results should not be empty");

    let ids: Vec<&str> = fused.iter().map(|r| r.id.as_str()).collect();
    assert!(ids.contains(&"a"), "Result 'a' should be in fused output");
    assert!(ids.contains(&"c"), "Result 'c' should be in fused output");
    assert!(ids.contains(&"d"), "Result 'd' should be in fused output");
    assert!(ids.contains(&"b"), "Result 'b' should be in fused output");
}

#[test]
fn test_fusion_with_empty_bm25() {
    init_minimal_logging();

    use cce_orchestrator::query::retrieval::{HybridFusionConfig, fuse_hybrid_results};

    let config = HybridFusionConfig {
        vector_weight: 1.0,
        bm25_weight: 0.0,
        include_vector_only_keys: true,
        include_bm25_only_keys: true,
        min_score: 0.0,
        dedup_by_chunk: false,
        ..HybridFusionConfig::default()
    };

    let vector_results = vec![
        make_search_result("a", 0.9, "src/main.rs"),
        make_search_result("b", 0.8, "src/lib.rs"),
    ];

    let fused = fuse_hybrid_results(vector_results, vec![], &config);

    assert_eq!(fused.len(), 2);
}

// ---------------------------------------------------------------------------
// Configuration Feedback
// ---------------------------------------------------------------------------

/// Query configuration feedback loop
#[test]
fn test_capability_driven_config() {
    init_minimal_logging();

    // Simulate an index that only has BM25 (no vectors)
    let caps = IndexCapabilities::new()
        .with_vectors(false)
        .with_bm25(true)
        .with_relations(true);

    assert!(!caps.has_vectors());
    assert!(caps.has_bm25());
    assert!(caps.has_relations());

    // With all capabilities available
    let caps_all = IndexCapabilities::all();
    assert!(caps_all.has_vectors());
    assert!(caps_all.has_bm25());
    assert!(caps_all.has_relations());

    let mut config = SearchConfig::default();
    config.vector.top_k = if caps_all.has_vectors() { 10 } else { 0 };
    assert_eq!(config.vector.top_k, 10);
}
