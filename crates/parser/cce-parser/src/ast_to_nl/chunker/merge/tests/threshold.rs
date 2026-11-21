use super::*;

#[test]
fn test_merge_bm25_uses_word_counts_not_tokens() {
    let config = ChunkingConfig {
        min_chunk_bm25_terms: 80,
        max_bm25_terms: 150,
        // A high embedding ceiling (cross_group_merge_threshold) must NOT widen the BM25 limit.
        ..Default::default()
    };
    let group_spans = HashMap::from([
        ("g1".to_string(), Span::new(0, 10, 0, 0, 0, 0)),
        ("g2".to_string(), Span::new(10, 20, 0, 0, 0, 0)),
    ]);

    // 90-word chunks: each below the 150-word limit, but their sum exceeds it.
    let text90 = || "word ".repeat(90);
    let chunks = vec![
        make_test_chunk_with_words(
            "g1_bm25_0",
            "g1",
            ChunkPath::Bm25,
            &text90(),
            90,
            Span::new(0, 10, 0, 0, 0, 0),
        ),
        make_test_chunk_with_words(
            "g2_bm25_0",
            "g2",
            ChunkPath::Bm25,
            &text90(),
            90,
            Span::new(10, 20, 0, 0, 0, 0),
        ),
    ];

    let result = merge_small_chunks_cross_group(chunks, &group_spans, &config);

    assert_eq!(
        result.len(),
        2,
        "90+90 words exceeds max_bm25_terms, must not merge"
    );
}

#[test]
fn test_merge_bm25_merges_when_combined_within_word_limit() {
    let config = ChunkingConfig {
        min_chunk_bm25_terms: 80,
        max_bm25_terms: 150,
        ..Default::default()
    };
    let group_spans = HashMap::from([
        ("g1".to_string(), Span::new(0, 10, 0, 0, 0, 0)),
        ("g2".to_string(), Span::new(10, 20, 0, 0, 0, 0)),
    ]);

    let text60 = || "word ".repeat(60);
    let chunks = vec![
        make_test_chunk_with_words(
            "g1_bm25_0",
            "g1",
            ChunkPath::Bm25,
            &text60(),
            60,
            Span::new(0, 10, 0, 0, 0, 0),
        ),
        make_test_chunk_with_words(
            "g2_bm25_0",
            "g2",
            ChunkPath::Bm25,
            &text60(),
            60,
            Span::new(10, 20, 0, 0, 0, 0),
        ),
    ];

    let result = merge_small_chunks_cross_group(chunks, &group_spans, &config);

    assert_eq!(result.len(), 1, "60+60 words fits within max_bm25_terms");
    assert_eq!(result[0].metadata.bm25_word_count, Some(120));
    assert_eq!(
        result[0].token_count, 120,
        "BM25 token_count is the merged word count"
    );
}

#[test]
fn test_merge_records_merged_group_ids() {
    let config = ChunkingConfig {
        min_chunk_tokens: 150,
        max_tokens: 512,
        ..Default::default()
    };
    let group_spans = HashMap::from([
        ("g1".to_string(), Span::new(0, 10, 0, 0, 0, 0)),
        ("g2".to_string(), Span::new(10, 20, 0, 0, 0, 0)),
        ("g3".to_string(), Span::new(20, 30, 0, 0, 0, 0)),
    ]);

    // 64-token chunks: g1+g2 ≈ 129 tokens (< min 150? no — chain keeps
    // merging while the accumulated prev chunk stays below min_threshold).
    let small = || "hello world foo bar baz qux ".repeat(8);
    let chunks = vec![
        make_test_chunk(
            "g1_emb_0",
            "g1",
            ChunkPath::Embedding,
            &small(),
            64,
            vec![EntityId(1)],
            Span::new(0, 10, 0, 0, 0, 0),
        ),
        make_test_chunk(
            "g2_emb_0",
            "g2",
            ChunkPath::Embedding,
            &small(),
            64,
            vec![EntityId(2)],
            Span::new(10, 20, 0, 0, 0, 0),
        ),
        make_test_chunk(
            "g3_emb_0",
            "g3",
            ChunkPath::Embedding,
            &small(),
            64,
            vec![EntityId(3)],
            Span::new(20, 30, 0, 0, 0, 0),
        ),
    ];

    let result = merge_small_chunks_cross_group(chunks, &group_spans, &config);

    assert_eq!(result.len(), 1, "chain merge should combine all three");
    let merged = &result[0];
    assert_eq!(merged.source_group_id, "g1");
    assert!(
        merged.metadata.merged_group_ids.contains(&"g2".to_string())
            && merged.metadata.merged_group_ids.contains(&"g3".to_string()),
        "merged chunk must record all contributing groups, got {:?}",
        merged.metadata.merged_group_ids
    );
}

#[test]
fn test_merge_combines_related_groups() {
    let config = ChunkingConfig {
        min_chunk_tokens: 150,
        max_tokens: 512,
        ..Default::default()
    };
    let group_spans = HashMap::from([
        ("g1".to_string(), Span::new(0, 10, 0, 0, 0, 0)),
        ("g2".to_string(), Span::new(10, 20, 0, 0, 0, 0)),
    ]);

    let small = || "hello world foo bar baz qux ".repeat(11);
    let mut a = make_test_chunk(
        "g1_emb_0",
        "g1",
        ChunkPath::Embedding,
        &small(),
        75,
        vec![EntityId(1)],
        Span::new(0, 10, 0, 0, 0, 0),
    );
    a.related_groups = vec![crate::ast_to_nl::chunker::GroupRelation {
        group_id: "g9".to_string(),
        relation_type: crate::ast_to_nl::chunker::GroupRelationType::Caller,
        strength: 0.9,
    }];
    let b = make_test_chunk(
        "g2_emb_0",
        "g2",
        ChunkPath::Embedding,
        &small(),
        75,
        vec![EntityId(2)],
        Span::new(10, 20, 0, 0, 0, 0),
    );

    let chunks = vec![a, b];
    let result = merge_small_chunks_cross_group(chunks, &group_spans, &config);

    assert_eq!(result.len(), 1);
    assert_eq!(
        result[0].related_groups.len(),
        1,
        "b has no relations, a's must survive"
    );
}
