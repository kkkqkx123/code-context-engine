use super::*;

#[test]
fn test_merge_small_chunks_chain_merging() {
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

    // Each chunk has ~200 chars → ~50 tokens (below min_threshold=150).
    // g1+g2 merged: ~400 chars → ~100 tokens (below min_threshold=150, chain continues).
    // g1+g2+g3 merged: ~600 chars → ~150 tokens (>= min_threshold, chain stops).
    let chunk_text = || "hello world foo bar baz qux ".repeat(7);

    let chunks = vec![
        make_test_chunk(
            "g1_emb_0",
            "g1",
            ChunkPath::Embedding,
            &chunk_text(),
            50,
            vec![EntityId(1)],
            Span::new(0, 10, 0, 0, 0, 0),
        ),
        make_test_chunk(
            "g2_emb_0",
            "g2",
            ChunkPath::Embedding,
            &chunk_text(),
            50,
            vec![EntityId(2)],
            Span::new(10, 20, 0, 0, 0, 0),
        ),
        make_test_chunk(
            "g3_emb_0",
            "g3",
            ChunkPath::Embedding,
            &chunk_text(),
            50,
            vec![EntityId(3)],
            Span::new(20, 30, 0, 0, 0, 0),
        ),
    ];

    let result = merge_small_chunks_cross_group(chunks, &group_spans, &config);

    assert_eq!(
        result.len(),
        1,
        "Chain merge should combine all 3 chunks into 1"
    );
    assert!(
        result[0].token_count >= 100,
        "Merged chunk should have combined tokens"
    );
    assert_eq!(result[0].source_group_id, "g1");
    assert_eq!(result[0].chunk_index, 0);
    assert_eq!(result[0].total_chunks, 1);
}

#[test]
fn test_merge_small_chunks_threshold_respected() {
    let config = ChunkingConfig {
        min_chunk_tokens: 150,
        max_tokens: 512,
        cross_group_merge_threshold: 200,
        ..Default::default()
    };
    let group_spans = HashMap::from([
        ("g1".to_string(), Span::new(0, 10, 0, 0, 0, 0)),
        ("g2".to_string(), Span::new(10, 20, 0, 0, 0, 0)),
    ]);

    // Each chunk has ~500 chars → ~125 tokens (below min_threshold=150).
    // Combined: ~1000+ chars → ~250 tokens (> cross_group_merge_threshold=200).
    let chunk_text = || "hello world foo bar baz qux ".repeat(18);

    let chunks = vec![
        make_test_chunk(
            "g1_emb_0",
            "g1",
            ChunkPath::Embedding,
            &chunk_text(),
            125,
            vec![EntityId(1)],
            Span::new(0, 10, 0, 0, 0, 0),
        ),
        make_test_chunk(
            "g2_emb_0",
            "g2",
            ChunkPath::Embedding,
            &chunk_text(),
            125,
            vec![EntityId(2)],
            Span::new(10, 20, 0, 0, 0, 0),
        ),
    ];

    let result = merge_small_chunks_cross_group(chunks, &group_spans, &config);

    assert_eq!(
        result.len(),
        2,
        "Combined exceeds cross_group_merge_threshold, no merge"
    );
}

#[test]
fn test_merge_small_chunks_no_merge_when_above_threshold() {
    let config = ChunkingConfig {
        min_chunk_tokens: 150,
        max_tokens: 512,
        ..Default::default()
    };
    let group_spans = HashMap::from([
        ("g1".to_string(), Span::new(0, 10, 0, 0, 0, 0)),
        ("g2".to_string(), Span::new(10, 20, 0, 0, 0, 0)),
    ]);

    // Each chunk has ~1000 chars → ~250 tokens (above min_threshold=150).
    let chunk_text = || "hello world foo bar baz qux ".repeat(36);

    let chunks = vec![
        make_test_chunk(
            "g1_emb_0",
            "g1",
            ChunkPath::Embedding,
            &chunk_text(),
            250,
            vec![EntityId(1)],
            Span::new(0, 10, 0, 0, 0, 0),
        ),
        make_test_chunk(
            "g2_emb_0",
            "g2",
            ChunkPath::Embedding,
            &chunk_text(),
            250,
            vec![EntityId(2)],
            Span::new(10, 20, 0, 0, 0, 0),
        ),
    ];

    let result = merge_small_chunks_cross_group(chunks, &group_spans, &config);

    assert_eq!(result.len(), 2, "All chunks above threshold, no merge");
}

#[test]
fn test_merge_small_chunks_blocks_different_kinds() {
    // A merged chunk keeps the left chunk's identity, so a macro group must
    // not absorb the following function: the unit would be titled after the
    // macro while its body is an unrelated function.
    let config = ChunkingConfig {
        min_chunk_tokens: 150,
        max_tokens: 512,
        ..Default::default()
    };
    let group_spans = HashMap::from([
        ("g1".to_string(), Span::new(0, 10, 0, 0, 0, 0)),
        ("g2".to_string(), Span::new(10, 20, 0, 0, 0, 0)),
    ]);

    // Small enough to be rescued by a merge, so the kind affinity is the
    // only reason the two chunks stay apart.
    let chunk_text = || "hello world foo bar ".repeat(3);

    let mut macro_chunk = make_test_chunk_with_code_meta(
        "g1_emb_0",
        "g1",
        ChunkPath::Embedding,
        &chunk_text(),
        125,
        vec![EntityId(1)],
        vec![],
        vec![],
        vec![],
        vec![],
        Span::new(0, 10, 0, 0, 0, 0),
        false,
        None,
        None,
    );
    macro_chunk.metadata.as_code_mut().unwrap().entity_kind = EntityKind::Macro;

    let mut function_chunk = make_test_chunk_with_code_meta(
        "g2_emb_0",
        "g2",
        ChunkPath::Embedding,
        &chunk_text(),
        125,
        vec![EntityId(2)],
        vec![],
        vec![],
        vec![],
        vec![],
        Span::new(10, 20, 0, 0, 0, 0),
        false,
        None,
        None,
    );
    function_chunk.metadata.as_code_mut().unwrap().entity_kind = EntityKind::Function;

    let result =
        merge_small_chunks_cross_group(vec![macro_chunk, function_chunk], &group_spans, &config);

    assert_eq!(
        result.len(),
        2,
        "chunks of different kinds must not merge across groups"
    );
}

#[test]
fn test_merge_small_chunks_merges_same_kinds() {
    let config = ChunkingConfig {
        min_chunk_tokens: 150,
        max_tokens: 512,
        ..Default::default()
    };
    let group_spans = HashMap::from([
        ("g1".to_string(), Span::new(0, 10, 0, 0, 0, 0)),
        ("g2".to_string(), Span::new(10, 20, 0, 0, 0, 0)),
    ]);

    // Small enough to be rescued by a merge.
    let chunk_text = || "hello world foo bar ".repeat(3);

    let chunks = vec![
        make_test_chunk_with_code_meta(
            "g1_emb_0",
            "g1",
            ChunkPath::Embedding,
            &chunk_text(),
            15,
            vec![EntityId(1)],
            vec![],
            vec![],
            vec![],
            vec![],
            Span::new(0, 10, 0, 0, 0, 0),
            false,
            None,
            None,
        ),
        make_test_chunk_with_code_meta(
            "g2_emb_0",
            "g2",
            ChunkPath::Embedding,
            &chunk_text(),
            15,
            vec![EntityId(2)],
            vec![],
            vec![],
            vec![],
            vec![],
            Span::new(10, 20, 0, 0, 0, 0),
            false,
            None,
            None,
        ),
    ];

    let result = merge_small_chunks_cross_group(chunks, &group_spans, &config);

    assert_eq!(
        result.len(),
        1,
        "peers of the same kind still merge into one unit"
    );
}

#[test]
fn test_merge_small_chunks_single_chunk() {
    let config = ChunkingConfig::default();
    let group_spans = HashMap::new();

    let chunks = vec![make_test_chunk(
        "g1_emb_0",
        "g1",
        ChunkPath::Embedding,
        "text",
        50,
        vec![],
        Span::default(),
    )];

    let result = merge_small_chunks_cross_group(chunks, &group_spans, &config);
    assert_eq!(result.len(), 1);
}

#[test]
fn test_merge_small_chunks_empty() {
    let config = ChunkingConfig::default();
    let group_spans = HashMap::new();

    let result = merge_small_chunks_cross_group(vec![], &group_spans, &config);
    assert!(result.is_empty());
}

#[test]
fn test_merge_small_chunks_fragment_fields_cleared() {
    let config = ChunkingConfig {
        min_chunk_tokens: 150,
        max_tokens: 512,
        ..Default::default()
    };
    let group_spans = HashMap::from([
        ("g1".to_string(), Span::new(0, 10, 0, 0, 0, 0)),
        ("g2".to_string(), Span::new(10, 20, 0, 0, 0, 0)),
    ]);

    let chunks = vec![
        make_test_chunk_with_code_meta(
            "g1_emb_0",
            "g1",
            ChunkPath::Embedding,
            &"A ".repeat(25),
            50,
            vec![EntityId(1)],
            vec![],
            vec![],
            vec![],
            vec![],
            Span::new(0, 10, 0, 0, 0, 0),
            true,
            Some(0),
            Some(1),
        ),
        make_test_chunk_with_code_meta(
            "g2_emb_0",
            "g2",
            ChunkPath::Embedding,
            &"B ".repeat(30),
            60,
            vec![EntityId(2)],
            vec![],
            vec![],
            vec![],
            vec![],
            Span::new(10, 20, 0, 0, 0, 0),
            true,
            Some(0),
            Some(1),
        ),
    ];

    let result = merge_small_chunks_cross_group(chunks, &group_spans, &config);

    assert_eq!(result.len(), 1);
    let code = result[0].metadata.as_code().unwrap();
    assert!(!code.is_fragment);
    assert_eq!(code.fragment_index, None);
    assert_eq!(code.total_fragments, None);
}

#[test]
fn test_merge_small_chunks_bm25_path() {
    let config = ChunkingConfig {
        min_chunk_bm25_terms: 80,
        max_bm25_terms: 150,
        ..Default::default()
    };
    let group_spans = HashMap::from([
        ("g1".to_string(), Span::new(0, 10, 0, 0, 0, 0)),
        ("g2".to_string(), Span::new(10, 20, 0, 0, 0, 0)),
    ]);

    let chunks = vec![
        make_test_chunk(
            "g1_bm25_0",
            "g1",
            ChunkPath::Bm25,
            "hello world foo bar baz qux ",
            30,
            vec![EntityId(1)],
            Span::new(0, 10, 0, 0, 0, 0),
        ),
        make_test_chunk(
            "g2_bm25_0",
            "g2",
            ChunkPath::Bm25,
            "one two three four five six ",
            30,
            vec![EntityId(2)],
            Span::new(10, 20, 0, 0, 0, 0),
        ),
    ];

    let result = merge_small_chunks_cross_group(chunks, &group_spans, &config);

    assert_eq!(result.len(), 1, "BM25 chunks should merge");
    assert_eq!(result[0].path, ChunkPath::Bm25);
}

#[test]
fn test_merge_small_chunks_self_contained_exemption() {
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

    let small = || "hello world foo bar baz qux ".repeat(11);
    let mut sc = make_test_chunk(
        "g2_emb_0",
        "g2",
        ChunkPath::Embedding,
        &small(),
        75,
        vec![EntityId(2)],
        Span::new(10, 20, 0, 0, 0, 0),
    );
    sc.self_contained = true;

    let chunks = vec![
        make_test_chunk(
            "g1_emb_0",
            "g1",
            ChunkPath::Embedding,
            &small(),
            75,
            vec![EntityId(1)],
            Span::new(0, 10, 0, 0, 0, 0),
        ),
        sc,
        make_test_chunk(
            "g3_emb_0",
            "g3",
            ChunkPath::Embedding,
            &small(),
            75,
            vec![EntityId(3)],
            Span::new(20, 30, 0, 0, 0, 0),
        ),
    ];

    let result = merge_small_chunks_cross_group(chunks, &group_spans, &config);

    assert_eq!(
        result.len(),
        3,
        "self-contained chunks must not merge with neighbors on the Embedding path"
    );
    assert!(result[1].self_contained);
    assert_eq!(result[1].source_group_id, "g2");
}

#[test]
fn test_merge_small_chunks_large_self_contained_absorbs_fragment() {
    let config = ChunkingConfig {
        min_chunk_tokens: 150,
        max_tokens: 512,
        ..Default::default()
    };
    let group_spans = HashMap::from([
        ("g1".to_string(), Span::new(0, 10, 0, 0, 0, 0)),
        ("g2".to_string(), Span::new(10, 20, 0, 0, 0, 0)),
    ]);

    // Large documented chunk (~330 tokens, above min) followed by a tiny
    // field-only residue. The exemption protects undersized self-contained
    // chunks, not a large neighbor stranding a fragment.
    let mut large = make_test_chunk(
        "g1_emb_0",
        "g1",
        ChunkPath::Embedding,
        &"word ".repeat(220),
        330,
        vec![EntityId(1)],
        Span::new(0, 10, 0, 0, 0, 0),
    );
    large.self_contained = true;
    let chunks = vec![
        large,
        make_test_chunk(
            "g2_emb_0",
            "g2",
            ChunkPath::Embedding,
            "field debug.",
            3,
            vec![EntityId(2)],
            Span::new(10, 20, 0, 0, 0, 0),
        ),
    ];

    let result = merge_small_chunks_cross_group(chunks, &group_spans, &config);

    assert_eq!(
        result.len(),
        1,
        "large self-contained chunk should absorb a tiny fragment"
    );
    assert!(
        !result[0].self_contained,
        "absorbing a plain residue dilutes topic purity"
    );
}

#[test]
fn test_merge_small_chunks_sentence_fragment_has_no_topic_purity() {
    let config = ChunkingConfig {
        min_chunk_tokens: 150,
        max_tokens: 512,
        ..Default::default()
    };
    let group_spans = HashMap::from([
        ("g1".to_string(), Span::new(0, 10, 0, 0, 0, 0)),
        ("g2".to_string(), Span::new(10, 20, 0, 0, 0, 0)),
    ]);

    // Undersized self-contained chunk that is a sentence-level fragment of
    // a larger member: its descriptor lives in another chunk, so it must
    // not veto absorbing the tiny neighbor residue.
    let mut frag = make_test_chunk_with_code_meta(
        "g1_emb_0",
        "g1",
        ChunkPath::Embedding,
        &"word ".repeat(55),
        82,
        vec![EntityId(1)],
        vec![],
        vec![],
        vec![],
        vec![],
        Span::new(0, 10, 0, 0, 0, 0),
        true,
        Some(2),
        Some(3),
    );
    frag.self_contained = true;
    frag.metadata
        .as_code_mut()
        .expect("test chunk carries code metadata")
        .split_reason = SplitReason::SentenceBoundary;
    let chunks = vec![
        frag,
        make_test_chunk(
            "g2_emb_0",
            "g2",
            ChunkPath::Embedding,
            "field debug.",
            3,
            vec![EntityId(2)],
            Span::new(10, 20, 0, 0, 0, 0),
        ),
    ];

    let result = merge_small_chunks_cross_group(chunks, &group_spans, &config);

    assert_eq!(
        result.len(),
        1,
        "sentence fragment must not strand a tiny residue"
    );
}

#[test]
fn test_merge_small_chunks_first_block_header_context_keeps_veto() {
    let config = ChunkingConfig {
        min_chunk_tokens: 150,
        max_tokens: 512,
        ..Default::default()
    };
    let group_spans = HashMap::from([
        ("g1".to_string(), Span::new(0, 10, 0, 0, 0, 0)),
        ("g2".to_string(), Span::new(10, 20, 0, 0, 0, 0)),
    ]);

    // First-block chunk: group header (context) plus one documented member.
    // Its topic is the member's own, so the self-contained veto must hold
    // even though two entity ids are attributed to the chunk.
    let mut first = make_test_chunk_with_code_meta(
        "g1_emb_0",
        "g1",
        ChunkPath::Embedding,
        &"word ".repeat(60),
        90,
        vec![EntityId(1), EntityId(2)],
        vec![EntityId(1)],
        vec![],
        vec![],
        vec![],
        Span::new(0, 10, 0, 0, 0, 0),
        false,
        None,
        None,
    );
    first.self_contained = true;
    let chunks = vec![
        first,
        make_test_chunk(
            "g2_emb_0",
            "g2",
            ChunkPath::Embedding,
            &"word ".repeat(140),
            210,
            vec![EntityId(3)],
            Span::new(10, 20, 0, 0, 0, 0),
        ),
    ];

    let result = merge_small_chunks_cross_group(chunks, &group_spans, &config);

    assert_eq!(
        result.len(),
        2,
        "header-plus-one-member block keeps its topic veto"
    );
}

#[test]
fn test_merge_small_chunks_self_contained_bm25_unaffected() {
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
    let mut sc = make_test_chunk(
        "g2_bm25_0",
        "g2",
        ChunkPath::Bm25,
        &text60(),
        60,
        vec![EntityId(2)],
        Span::new(10, 20, 0, 0, 0, 0),
    );
    sc.self_contained = true;

    let chunks = vec![
        make_test_chunk(
            "g1_bm25_0",
            "g1",
            ChunkPath::Bm25,
            &text60(),
            60,
            vec![EntityId(1)],
            Span::new(0, 10, 0, 0, 0, 0),
        ),
        sc,
    ];

    let result = merge_small_chunks_cross_group(chunks, &group_spans, &config);

    assert_eq!(
        result.len(),
        1,
        "self_contained is an Embedding-only concept; BM25 merging is unchanged"
    );
}

#[test]
fn test_merge_small_chunks_sorted_by_group_span() {
    let config = ChunkingConfig {
        min_chunk_tokens: 150,
        max_tokens: 512,
        ..Default::default()
    };
    let group_spans = HashMap::from([
        ("g1".to_string(), Span::new(100, 200, 0, 0, 0, 0)),
        ("g2".to_string(), Span::new(0, 50, 0, 0, 0, 0)),
    ]);

    let chunks = vec![
        make_test_chunk(
            "g1_emb_0",
            "g1",
            ChunkPath::Embedding,
            &"A ".repeat(25),
            50,
            vec![EntityId(1)],
            Span::new(100, 200, 0, 0, 0, 0),
        ),
        make_test_chunk(
            "g2_emb_0",
            "g2",
            ChunkPath::Embedding,
            &"B ".repeat(25),
            50,
            vec![EntityId(2)],
            Span::new(0, 50, 0, 0, 0, 0),
        ),
    ];

    let result = merge_small_chunks_cross_group(chunks, &group_spans, &config);

    assert_eq!(
        result.len(),
        1,
        "Should merge even when input order is reversed"
    );
    assert_eq!(
        result[0].source_group_id, "g2",
        "Should preserve leftmost chunk's identity after sort"
    );
}

#[test]
fn test_merge_small_chunks_partial_merge() {
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

    // g1 and g2: ~300 chars each → ~75 tokens estimated.
    // g1+g2 merged: ~600 chars → ~150 tokens.
    // g3: ~400 chars → ~100 tokens estimated (below min, so bidirectional
    // merge absorbs it as well when the combined cost fits the ceiling).
    let small_text = || "hello world foo bar baz qux ".repeat(11);
    let large_text = || "hello world foo bar baz qux ".repeat(14);

    let chunks = vec![
        make_test_chunk(
            "g1_emb_0",
            "g1",
            ChunkPath::Embedding,
            &small_text(),
            75,
            vec![EntityId(1)],
            Span::new(0, 10, 0, 0, 0, 0),
        ),
        make_test_chunk(
            "g2_emb_0",
            "g2",
            ChunkPath::Embedding,
            &small_text(),
            75,
            vec![EntityId(2)],
            Span::new(10, 20, 0, 0, 0, 0),
        ),
        make_test_chunk(
            "g3_emb_0",
            "g3",
            ChunkPath::Embedding,
            &large_text(),
            100,
            vec![EntityId(3)],
            Span::new(20, 30, 0, 0, 0, 0),
        ),
    ];

    let result = merge_small_chunks_cross_group(chunks, &group_spans, &config);

    assert_eq!(
        result.len(),
        1,
        "bidirectional merge absorbs trailing small g3"
    );
    assert_eq!(result[0].source_group_id, "g1");
}

#[test]
fn test_mixed_topic_small_chunk_does_not_veto_merge() {
    let config = ChunkingConfig::default();
    let mixed =
        make_small_self_contained_chunk("mixed_emb_0", "g1", vec![EntityId(1), EntityId(2)]);
    let mut neighbor = make_small_self_contained_chunk("next_emb_0", "g2", vec![EntityId(3)]);
    neighbor.self_contained = false;
    assert!(
        merge_strategy::should_merge(&mixed, &neighbor, ChunkPath::Embedding, &config),
        "a below-min chunk with several content entities is already mixed-topic and must merge"
    );
}

#[test]
fn test_single_topic_small_chunk_keeps_veto() {
    let config = ChunkingConfig::default();
    let pure = make_small_self_contained_chunk("pure_emb_0", "g1", vec![EntityId(1)]);
    let mut neighbor = make_small_self_contained_chunk("next_emb_0", "g2", vec![EntityId(3)]);
    neighbor.self_contained = false;
    assert!(
        !merge_strategy::should_merge(&pure, &neighbor, ChunkPath::Embedding, &config),
        "a below-min single-entity self-contained chunk keeps its pure topic"
    );
}

#[test]
fn test_tiny_residue_merges_across_kinds() {
    let config = ChunkingConfig::default();
    // 14-token sentence splinter next to a same-group neighbor of a
    // different kind: the kind gate must not strand a fragment of the
    // unit it was split from.
    let mut splinter = make_test_chunk_with_code_meta(
        "tiny_emb_0",
        "g1",
        ChunkPath::Embedding,
        "field debug.",
        14,
        vec![EntityId(1)],
        vec![],
        vec![],
        vec![],
        vec![],
        Span::new(0, 10, 0, 0, 0, 0),
        false,
        None,
        None,
    );
    splinter.self_contained = false;
    let mut neighbor = make_small_self_contained_chunk("next_emb_0", "g1", vec![EntityId(3)]);
    neighbor.self_contained = false;
    neighbor
        .metadata
        .as_code_mut()
        .expect("code meta")
        .entity_kind = EntityKind::Struct;
    assert!(
        merge_strategy::should_merge(&splinter, &neighbor, ChunkPath::Embedding, &config),
        "a tiny same-group residue rejoins its unit regardless of kind"
    );

    // Same tiny residue from another group at a disjoint span: the kind
    // gate still applies.
    let mut stranger = make_small_self_contained_chunk("other_emb_0", "g2", vec![EntityId(4)]);
    stranger.self_contained = false;
    stranger.metadata.source_span = Span::new(10, 20, 0, 0, 0, 0);
    stranger
        .metadata
        .as_code_mut()
        .expect("code meta")
        .entity_kind = EntityKind::Struct;
    assert!(
        !merge_strategy::should_merge(&splinter, &stranger, ChunkPath::Embedding, &config),
        "a tiny residue must not glue unrelated groups"
    );
}
