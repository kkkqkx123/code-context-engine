use super::*;

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
    assert_eq!(merged.text, "text a\n\ntext b\nIn file test.");
    assert_eq!(
        merged.token_count,
        crate::ast_to_nl::chunker::boundary::cost(&merged.text, ChunkPath::Embedding)
    );
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
fn test_merge_two_chunks_propagates_truncated_flag() {
    let mut first = make_test_chunk(
        "g1_emb_0",
        "g1",
        ChunkPath::Embedding,
        "first",
        5,
        vec![],
        Span::default(),
    );
    first.truncated = true;
    let second = make_test_chunk(
        "g2_emb_0",
        "g2",
        ChunkPath::Embedding,
        "second",
        6,
        vec![],
        Span::default(),
    );
    let merged = super::merge_two_chunks(&first, &second, 11);
    assert!(
        merged.truncated,
        "merging a truncated side must keep the loss signal"
    );
}
