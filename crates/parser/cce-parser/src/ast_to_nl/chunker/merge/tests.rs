use super::*;
use crate::ast_to_nl::chunker::{ChunkContentType, ChunkPath, SplitReason};
use crate::grouper::GroupType;
use cce_types::entity::EntityKind;

use std::collections::HashMap;

fn make_test_chunk(
    id: &str,
    group_id: &str,
    path: ChunkPath,
    text: &str,
    token_count: usize,
    _content_entity_ids: Vec<EntityId>,
    source_span: Span,
) -> ChunkedResult {
    ChunkedResult {
        chunk_id: id.to_string(),
        source_group_id: group_id.to_string(),
        path,
        group_type: GroupType::Standalone,
        chunk_index: 0,
        total_chunks: 1,
        text: text.to_string(),
        bm25_title: None,
        bm25_keywords: vec![],
        token_count,
        start_byte: source_span.start_byte,
        end_byte: source_span.end_byte,
        prev_overlap: None,
        next_overlap: None,
        related_groups: vec![],
        self_contained: false,
        metadata: ChunkMetadata {
            file_path: "test.rs".to_string(),
            source_span,
            source_ranges: vec![source_span],
            source_span_kind: SourceSpanKind::ExactEntities,
            ..Default::default()
        },
    }
}

#[allow(clippy::too_many_arguments)]
fn make_test_chunk_with_code_meta(
    id: &str,
    group_id: &str,
    path: ChunkPath,
    text: &str,
    token_count: usize,
    content_entity_ids: Vec<EntityId>,
    context_entity_ids: Vec<EntityId>,
    keywords: Vec<String>,
    modifiers: Vec<String>,
    overlap_entities: Vec<EntityId>,
    source_span: Span,
    is_fragment: bool,
    fragment_index: Option<usize>,
    total_fragments: Option<usize>,
) -> ChunkedResult {
    let code_metadata = CodeSpecificMetadata {
        content_entity_ids: content_entity_ids.clone(),
        content_entity_names: Vec::new(),
        context_entity_ids,
        entity_kind: EntityKind::Function,
        modifiers,
        split_reason: SplitReason::MemberBoundary,
        overlap_entities,
        has_overlap: false,
        is_fragment,
        fragment_index,
        total_fragments,
        original_entity_id: None,
        pattern_info: None,
    };

    ChunkedResult {
        chunk_id: id.to_string(),
        source_group_id: group_id.to_string(),
        path,
        group_type: GroupType::Standalone,
        chunk_index: 0,
        total_chunks: 1,
        text: text.to_string(),
        bm25_title: None,
        bm25_keywords: keywords,
        token_count,
        start_byte: source_span.start_byte,
        end_byte: source_span.end_byte,
        prev_overlap: None,
        next_overlap: None,
        related_groups: vec![],
        self_contained: false,
        metadata: ChunkMetadata {
            content_type: ChunkContentType::Code {
                language: cce_types::language::Language::Rust,
            },
            file_path: "test.rs".to_string(),
            source_span,
            source_ranges: vec![source_span],
            source_span_kind: SourceSpanKind::ExactEntities,
            bm25_word_count: None,
            segment_id: String::new(),
            merged_group_ids: Vec::new(),
            code_metadata: Some(code_metadata),
            doc_metadata: None,
            ..ChunkMetadata::default()
        },
    }
}

fn make_test_chunk_with_words(
    id: &str,
    group_id: &str,
    path: ChunkPath,
    text: &str,
    word_count: usize,
    source_span: Span,
) -> ChunkedResult {
    let mut chunk = make_test_chunk(id, group_id, path, text, word_count, vec![], source_span);
    chunk.metadata.bm25_word_count = Some(word_count);
    chunk
}

#[test]
fn test_merge_two_chunks_preserves_identity() {
    let a = make_test_chunk(
        "group_a_emb_0",
        "group_a",
        ChunkPath::Embedding,
        "text a",
        50,
        vec![EntityId(1)],
        Span::new(0, 10, 0, 0, 0, 0),
    );

    let b = make_test_chunk(
        "group_b_emb_0",
        "group_b",
        ChunkPath::Embedding,
        "text b",
        60,
        vec![EntityId(2)],
        Span::new(10, 20, 0, 0, 0, 0),
    );

    let merged = merge_two_chunks(&a, &b, 110);

    assert_eq!(merged.chunk_id, "group_a_emb_0");
    assert_eq!(merged.source_group_id, "group_a");
    assert_eq!(merged.path, ChunkPath::Embedding);
    assert_eq!(merged.token_count, 110);
    assert_eq!(merged.text, "text a\n\ntext b");
}

#[test]
fn test_merge_two_chunks_deduplicates_entities() {
    let a = make_test_chunk_with_code_meta(
        "a",
        "g1",
        ChunkPath::Embedding,
        "text a",
        50,
        vec![EntityId(1), EntityId(2)],
        vec![EntityId(10)],
        vec!["hello".to_string(), "world".to_string()],
        vec!["pub".to_string()],
        vec![EntityId(100)],
        Span::new(0, 10, 0, 0, 0, 0),
        false,
        None,
        None,
    );

    let b = make_test_chunk_with_code_meta(
        "b",
        "g2",
        ChunkPath::Embedding,
        "text b",
        60,
        vec![EntityId(2), EntityId(3)],
        vec![EntityId(10), EntityId(20)],
        vec!["world".to_string(), "foo".to_string()],
        vec!["pub".to_string(), "unsafe".to_string()],
        vec![EntityId(100), EntityId(200)],
        Span::new(10, 20, 0, 0, 0, 0),
        false,
        None,
        None,
    );

    let merged = merge_two_chunks(&a, &b, 110);

    let code = merged.metadata.as_code().unwrap();
    assert_eq!(
        code.content_entity_ids,
        vec![EntityId(1), EntityId(2), EntityId(3)]
    );
    assert_eq!(code.context_entity_ids, vec![EntityId(10), EntityId(20)]);

    assert_eq!(merged.bm25_keywords, vec!["hello", "world", "foo"]);

    assert_eq!(code.modifiers, vec!["pub", "unsafe"]);
    assert_eq!(code.overlap_entities, vec![EntityId(100), EntityId(200)]);
}

#[test]
fn test_merge_two_chunks_source_span_exact() {
    let a = make_test_chunk_with_code_meta(
        "a",
        "g1",
        ChunkPath::Embedding,
        "text a",
        50,
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

    let b = make_test_chunk_with_code_meta(
        "b",
        "g2",
        ChunkPath::Embedding,
        "text b",
        60,
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

    let merged = merge_two_chunks(&a, &b, 110);

    assert_eq!(
        merged.metadata.source_span_kind,
        SourceSpanKind::ExactEntities
    );
    assert_eq!(merged.metadata.source_span.start_byte, 0);
    assert_eq!(merged.metadata.source_span.end_byte, 20);
}

#[test]
fn test_merge_two_chunks_source_span_uses_chunk_ranges() {
    // Merged coverage comes from the two chunks' own source ranges,
    // never from entity-id resolution — so it stays correct even when
    // the chunks carry no resolvable entity ids.
    let mut a = make_test_chunk_with_code_meta(
        "a",
        "g1",
        ChunkPath::Embedding,
        "text a",
        50,
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        Span::new(0, 10, 0, 0, 0, 0),
        false,
        None,
        None,
    );
    a.metadata.source_span_kind = SourceSpanKind::GroupFallback;

    let b = make_test_chunk_with_code_meta(
        "b",
        "g2",
        ChunkPath::Embedding,
        "text b",
        60,
        vec![EntityId(2)],
        vec![],
        vec![],
        vec![],
        vec![],
        Span::new(30, 40, 0, 0, 0, 0),
        false,
        None,
        None,
    );

    let merged = merge_two_chunks(&a, &b, 110);

    assert_eq!(
        merged.metadata.source_span_kind,
        SourceSpanKind::GroupFallback,
        "non-exact inputs must propagate GroupFallback"
    );
    assert_eq!(merged.metadata.source_span.start_byte, 0);
    assert_eq!(merged.metadata.source_span.end_byte, 40);
    assert_eq!(
        merged.metadata.source_ranges,
        vec![Span::new(0, 10, 0, 0, 0, 0), Span::new(30, 40, 0, 0, 0, 0),],
        "disjoint chunk ranges must both survive the merge"
    );
}

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
        min_chunk_bm25_words: 80,
        max_bm25_words: 150,
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
fn test_merge_small_chunks_self_contained_bm25_unaffected() {
    let config = ChunkingConfig {
        min_chunk_bm25_words: 80,
        max_bm25_words: 150,
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
    // g1+g2 merged: ~600 chars → ~150 tokens (>= min_threshold=150, chain stops).
    // g3: ~400 chars → ~100 tokens estimated (below min_threshold, but chain already stopped).
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

    assert_eq!(result.len(), 2, "g1+g2 should merge, g3 stays independent");
    assert_eq!(result[0].source_group_id, "g1");
    assert_eq!(result[1].source_group_id, "g3");
}

#[test]
fn test_merge_bm25_uses_word_counts_not_tokens() {
    let config = ChunkingConfig {
        min_chunk_bm25_words: 80,
        max_bm25_words: 150,
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
        "90+90 words exceeds max_bm25_words, must not merge"
    );
}

#[test]
fn test_merge_bm25_merges_when_combined_within_word_limit() {
    let config = ChunkingConfig {
        min_chunk_bm25_words: 80,
        max_bm25_words: 150,
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

    assert_eq!(result.len(), 1, "60+60 words fits within max_bm25_words");
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
