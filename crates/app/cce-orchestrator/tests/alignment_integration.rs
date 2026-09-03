//! Integration tests for hybrid fusion alignment.
//!
//! Verifies the end-to-end flow: chunking → storage → retrieval → fusion,
//! ensuring that entity_id and segment_id are correctly propagated through
//! the pipeline and that fusion correctly matches results across paths.

use cce_orchestrator::query::retrieval::post_processing::fusion::{
    HybridFusionConfig, compute_alignment_coverage, fuse_hybrid_results,
};
use cce_orchestrator::query::types::SearchResult;
use cce_types::entity::EntityId;

fn make_vector_result(
    id: &str,
    entity_id: Option<u64>,
    segment_id: Option<&str>,
    score: f32,
) -> SearchResult {
    SearchResult {
        id: id.to_string(),
        entity_ids: entity_id.map(EntityId).into_iter().collect(),
        segment_id: segment_id.map(String::from),
        score,
        original_score: score,
        vector_score: score,
        bm25_score: None,
        sources: vec!["vector".to_string()],
        ..Default::default()
    }
}

fn make_bm25_result(
    id: &str,
    entity_id: Option<u64>,
    segment_id: Option<&str>,
    score: f32,
) -> SearchResult {
    SearchResult {
        id: id.to_string(),
        entity_ids: entity_id.map(EntityId).into_iter().collect(),
        segment_id: segment_id.map(String::from),
        score,
        original_score: score,
        vector_score: 0.0,
        bm25_score: Some(score),
        sources: vec!["bm25".to_string()],
        ..Default::default()
    }
}

#[test]
fn test_end_to_end_code_chunk_alignment() {
    let vector = vec![
        make_vector_result("emb_func1", Some(100), Some("group_1"), 0.95),
        make_vector_result("emb_func2", Some(200), Some("group_2"), 0.85),
    ];
    let bm25 = vec![
        make_bm25_result("bm25_func1", Some(100), Some("group_1"), 0.9),
        make_bm25_result("bm25_func2", Some(200), Some("group_2"), 0.8),
    ];

    let config = HybridFusionConfig {
        vector_weight: 0.5,
        bm25_weight: 0.5,
        include_single_path: true,
        min_score: 0.0,
        dedup_by_chunk: false,
    };

    let fused = fuse_hybrid_results(vector, bm25, &config);

    assert_eq!(fused.len(), 2);

    let func1 = fused
        .iter()
        .find(|r| r.entity_ids.first() == Some(&EntityId(100)))
        .unwrap();
    assert!(func1.score >= 0.5);
    assert_eq!(func1.sources, vec!["hybrid"]);

    let func2 = fused
        .iter()
        .find(|r| r.entity_ids.first() == Some(&EntityId(200)))
        .unwrap();
    assert!(func2.score >= 0.0);
}

#[test]
fn test_end_to_end_document_chunk_alignment() {
    let vector = vec![
        make_vector_result("emb_sec1", None, Some("section_1"), 0.95),
        make_vector_result("emb_sec2", None, Some("section_2"), 0.85),
        make_vector_result("emb_sec3", None, Some("section_3"), 0.75),
    ];
    let bm25 = vec![
        make_bm25_result("bm25_sec1", None, Some("section_1"), 0.8),
        make_bm25_result("bm25_sec2", None, Some("section_2"), 0.9),
    ];

    let config = HybridFusionConfig {
        vector_weight: 0.5,
        bm25_weight: 0.5,
        include_single_path: true,
        min_score: 0.0,
        dedup_by_chunk: false,
    };

    let fused = fuse_hybrid_results(vector, bm25, &config);

    assert_eq!(fused.len(), 3);

    let sec1 = fused
        .iter()
        .find(|r| r.segment_id.as_deref() == Some("section_1"))
        .unwrap();
    assert!(sec1.score >= 0.5);

    let sec2 = fused
        .iter()
        .find(|r| r.segment_id.as_deref() == Some("section_2"))
        .unwrap();
    assert!(sec2.score >= 0.5);

    let sec3 = fused
        .iter()
        .find(|r| r.segment_id.as_deref() == Some("section_3"))
        .unwrap();
    assert!(sec3.score < 0.5);
}

#[test]
fn test_end_to_end_mixed_code_and_document() {
    let vector = vec![
        make_vector_result("emb_func1", Some(100), Some("group_1"), 0.9),
        make_vector_result("emb_doc1", None, Some("doc_section_1"), 0.7),
    ];
    let bm25 = vec![
        make_bm25_result("bm25_func1", Some(100), Some("group_1"), 0.8),
        make_bm25_result("bm25_doc1", None, Some("doc_section_1"), 0.6),
    ];

    let config = HybridFusionConfig {
        vector_weight: 0.5,
        bm25_weight: 0.5,
        include_single_path: true,
        min_score: 0.0,
        dedup_by_chunk: false,
    };

    let fused = fuse_hybrid_results(vector, bm25, &config);

    assert_eq!(fused.len(), 2);

    let func = fused
        .iter()
        .find(|r| r.entity_ids.first() == Some(&EntityId(100)))
        .unwrap();
    assert!(func.score > 0.5);

    let doc = fused
        .iter()
        .find(|r| r.segment_id.as_deref() == Some("doc_section_1"))
        .unwrap();
    assert!(doc.score >= 0.0);
}

#[test]
fn test_end_to_end_bm25_sub_chunks_align_with_embedding() {
    let vector = vec![make_vector_result(
        "emb_doc1",
        None,
        Some("doc_section_1"),
        0.9,
    )];

    let bm25 = vec![
        make_bm25_result("bm25_doc1_sub0", None, Some("doc_section_1"), 0.7),
        make_bm25_result("bm25_doc1_sub1", None, Some("doc_section_1"), 0.5),
    ];

    let config = HybridFusionConfig {
        vector_weight: 0.5,
        bm25_weight: 0.5,
        include_single_path: true,
        min_score: 0.0,
        dedup_by_chunk: false,
    };

    let fused = fuse_hybrid_results(vector, bm25, &config);

    assert_eq!(fused.len(), 1);

    let result = fused
        .iter()
        .find(|r| r.segment_id.as_deref() == Some("doc_section_1"))
        .unwrap();
    assert!(result.score > 0.5);
}

#[test]
fn test_end_to_end_different_segments_do_not_fuse() {
    let vector = vec![make_vector_result("emb_sec1", None, Some("section_1"), 0.9)];
    let bm25 = vec![make_bm25_result("bm25_sec2", None, Some("section_2"), 0.8)];

    let config = HybridFusionConfig {
        vector_weight: 0.5,
        bm25_weight: 0.5,
        include_single_path: true,
        min_score: 0.0,
        dedup_by_chunk: false,
    };

    let fused = fuse_hybrid_results(vector, bm25, &config);

    assert_eq!(fused.len(), 2);

    for r in &fused {
        assert!(r.score <= 0.5);
    }
}

#[test]
fn test_end_to_end_entity_id_takes_priority() {
    let vector = vec![make_vector_result(
        "emb_a",
        Some(100),
        Some("shared_seg"),
        0.9,
    )];
    let bm25 = vec![make_bm25_result(
        "bm25_b",
        Some(200),
        Some("shared_seg"),
        0.8,
    )];

    let config = HybridFusionConfig {
        vector_weight: 0.5,
        bm25_weight: 0.5,
        include_single_path: true,
        min_score: 0.0,
        dedup_by_chunk: false,
    };

    let fused = fuse_hybrid_results(vector, bm25, &config);

    assert_eq!(fused.len(), 2);

    for r in &fused {
        assert!(r.score <= 0.5);
    }
}

#[test]
fn test_end_to_end_exclude_single_path() {
    let vector = vec![
        make_vector_result("emb_func1", Some(100), Some("group_1"), 0.9),
        make_vector_result("emb_doc1", None, Some("doc_section_1"), 0.7),
    ];
    let bm25 = vec![make_bm25_result(
        "bm25_func1",
        Some(100),
        Some("group_1"),
        0.8,
    )];

    let config = HybridFusionConfig {
        vector_weight: 0.5,
        bm25_weight: 0.5,
        include_single_path: false,
        min_score: 0.0,
        dedup_by_chunk: false,
    };

    let fused = fuse_hybrid_results(vector, bm25, &config);

    assert_eq!(fused.len(), 1);

    let func = fused
        .iter()
        .find(|r| r.entity_ids.first() == Some(&EntityId(100)))
        .unwrap();
    assert!(func.score > 0.5);
}

#[test]
fn test_end_to_end_fusion_preserves_metadata() {
    let mut vector = make_vector_result("emb_func1", Some(100), Some("group_1"), 0.9);
    vector.file_path = "src/lib.rs".to_string();
    vector.kind = "function".to_string();
    vector.name = "my_function".to_string();
    vector.start_line = 10;
    vector.end_line = 20;

    let bm25 = make_bm25_result("bm25_func1", Some(100), Some("group_1"), 0.8);

    let config = HybridFusionConfig {
        vector_weight: 0.5,
        bm25_weight: 0.5,
        include_single_path: true,
        min_score: 0.0,
        dedup_by_chunk: false,
    };

    let fused = fuse_hybrid_results(vec![vector], vec![bm25], &config);

    assert_eq!(fused.len(), 1);

    let result = &fused[0];
    assert_eq!(result.id, "emb_func1");
    assert_eq!(result.entity_ids.first(), Some(&EntityId(100)));
    assert_eq!(result.segment_id, Some("group_1".to_string()));
    assert_eq!(result.file_path, "src/lib.rs");
    assert_eq!(result.kind, "function");
    assert_eq!(result.name, "my_function");
    assert_eq!(result.start_line, 10);
    assert_eq!(result.end_line, 20);
    assert_eq!(result.sources, vec!["hybrid"]);
}

#[test]
fn test_end_to_end_empty_results() {
    let config = HybridFusionConfig::default();

    let fused = fuse_hybrid_results(vec![], vec![], &config);
    assert!(fused.is_empty());

    let fused = fuse_hybrid_results(
        vec![make_vector_result("a", Some(1), None, 0.9)],
        vec![],
        &config,
    );
    assert_eq!(fused.len(), 1);

    let fused = fuse_hybrid_results(
        vec![],
        vec![make_bm25_result("b", Some(2), None, 0.8)],
        &config,
    );
    assert_eq!(fused.len(), 1);
}

// ---------------------------------------------------------------------------
// Searcher pipeline composition: expand_multi_entity_results → fusion
// Mirrors searcher.rs (expand before fuse) so entity-level alignment is
// verified as the production chain, not just fusion in isolation.
// ---------------------------------------------------------------------------

/// Helper: multi-entity chunk result (one physical chunk, several entities).
fn make_multi_entity_result(
    id: &str,
    entities: &[u64],
    segment_id: Option<&str>,
    score: f32,
) -> SearchResult {
    SearchResult {
        id: id.to_string(),
        entity_ids: entities.iter().map(|e| EntityId(*e)).collect(),
        segment_id: segment_id.map(String::from),
        score,
        original_score: score,
        vector_score: score,
        bm25_score: None,
        sources: vec!["vector".to_string()],
        ..Default::default()
    }
}

#[test]
fn test_searcher_pipeline_expand_then_fuse_matches_across_paths() {
    use cce_orchestrator::query::searcher::expand_multi_entity_results;

    // One physical chunk containing two entities, recalled by both paths.
    let vector = vec![make_multi_entity_result(
        "chunk_x",
        &[100, 200],
        Some("group_1"),
        0.9,
    )];
    let bm25 = vec![
        make_bm25_result("bm25_e100", Some(100), Some("group_1"), 0.8),
        make_bm25_result("bm25_e200", Some(200), Some("group_1"), 0.6),
    ];

    let expanded_vector = expand_multi_entity_results(vector);
    let expanded_bm25 = expand_multi_entity_results(bm25);

    // Expansion produces one entry per entity before fusion.
    assert_eq!(expanded_vector.len(), 2);

    let stats = compute_alignment_coverage(&expanded_vector, &expanded_bm25);
    assert_eq!(stats.vector_keys, 2);
    assert_eq!(stats.bm25_keys, 2);
    assert_eq!(
        stats.matched_keys, 2,
        "both entities must align across paths"
    );

    let config = HybridFusionConfig {
        vector_weight: 0.5,
        bm25_weight: 0.5,
        include_single_path: true,
        min_score: 0.0,
        dedup_by_chunk: false,
    };
    let fused = fuse_hybrid_results(expanded_vector, expanded_bm25, &config);

    // Entity-level alignment: both entities surface with hybrid scores.
    assert_eq!(fused.len(), 2);
    let e100 = fused
        .iter()
        .find(|r| r.entity_ids.first() == Some(&EntityId(100)))
        .unwrap();
    assert_eq!(e100.sources, vec!["hybrid"]);
    assert!(e100.bm25_score.is_some());
    let e200 = fused
        .iter()
        .find(|r| r.entity_ids.first() == Some(&EntityId(200)))
        .unwrap();
    assert_eq!(e200.sources, vec!["hybrid"]);
}

#[test]
fn test_searcher_pipeline_dedup_collapses_expanded_entities() {
    use cce_orchestrator::query::searcher::expand_multi_entity_results;

    // Vector path sees a multi-entity chunk, BM25 path only one of them.
    let vector = vec![make_multi_entity_result(
        "chunk_x",
        &[100, 200],
        Some("group_1"),
        0.9,
    )];
    let bm25 = vec![make_bm25_result(
        "bm25_e100",
        Some(100),
        Some("group_1"),
        0.8,
    )];

    let expanded_vector = expand_multi_entity_results(vector);
    let expanded_bm25 = expand_multi_entity_results(bm25);

    // Without dedup: both entities surface at entity granularity.
    let config = HybridFusionConfig {
        vector_weight: 0.5,
        bm25_weight: 0.5,
        include_single_path: true,
        min_score: 0.0,
        dedup_by_chunk: false,
    };
    let fused = fuse_hybrid_results(expanded_vector.clone(), expanded_bm25.clone(), &config);
    assert_eq!(fused.len(), 2);

    // With dedup_by_chunk: one entry per physical chunk id.
    let config_dedup = HybridFusionConfig {
        dedup_by_chunk: true,
        ..config
    };
    let fused_dedup = fuse_hybrid_results(expanded_vector, expanded_bm25, &config_dedup);
    assert_eq!(fused_dedup.len(), 1);
    assert_eq!(fused_dedup[0].id, "chunk_x");
    assert!(
        fused_dedup[0].score >= fused[0].score,
        "dedup keeps the best-scoring entry"
    );
}

#[test]
fn test_searcher_pipeline_unkeyed_results_keep_distinct_keys() {
    use cce_orchestrator::query::searcher::expand_multi_entity_results;

    // Summary-like results without entity_id/segment_id must survive as
    // individual single-path entries (no collapse to one shared key), and
    // coverage statistics must count them.
    let vector = vec![
        SearchResult {
            id: "summary_a".to_string(),
            score: 0.9,
            original_score: 0.9,
            vector_score: 0.9,
            bm25_score: None,
            sources: vec!["vector".to_string()],
            ..Default::default()
        },
        SearchResult {
            id: "summary_b".to_string(),
            score: 0.7,
            original_score: 0.7,
            vector_score: 0.7,
            bm25_score: None,
            sources: vec!["vector".to_string()],
            ..Default::default()
        },
    ];
    let bm25 = vec![make_bm25_result("bm25_a", Some(100), Some("group_1"), 0.8)];

    let expanded_vector = expand_multi_entity_results(vector);
    let expanded_bm25 = expand_multi_entity_results(bm25);

    let stats = compute_alignment_coverage(&expanded_vector, &expanded_bm25);
    assert_eq!(
        stats.vector_keys, 2,
        "unkeyed chunks count as distinct keys"
    );
    assert_eq!(stats.matched_keys, 0);

    let config = HybridFusionConfig {
        vector_weight: 0.5,
        bm25_weight: 0.5,
        include_single_path: true,
        min_score: 0.0,
        dedup_by_chunk: false,
    };
    let fused = fuse_hybrid_results(expanded_vector, expanded_bm25, &config);
    assert_eq!(fused.len(), 3, "unkeyed results must not collapse");
    let ids: Vec<&str> = fused.iter().map(|r| r.id.as_str()).collect();
    for id in ["summary_a", "summary_b", "bm25_a"] {
        assert_eq!(ids.iter().filter(|i| **i == id).count(), 1);
    }
}
