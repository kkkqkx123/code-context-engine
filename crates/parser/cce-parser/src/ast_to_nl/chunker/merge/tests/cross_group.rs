use super::*;

/// A chunk the cross-group pass never folded into a neighbour must keep its
/// intra-group fragment bookkeeping.
///
/// The pass renumbers the output list (`chunk_index`/`total_chunks`), which is
/// a different concern from "is this chunk one piece of a split entity". It
/// used to clear `is_fragment` for every chunk on its stack, so a group split
/// into many pieces reported `is_fragment == false` on all of them even though
/// nothing had been merged.
#[test]
fn test_cross_group_merge_keeps_flags_of_untouched_fragments() {
    let config = ChunkingConfig {
        min_chunk_tokens: 1,
        max_tokens: 512,
        ..Default::default()
    };
    let group_spans = HashMap::from([("g1".to_string(), Span::new(0, 300, 0, 0, 0, 0))]);
    let body = "fn alpha() -> i32 { 1 }\n".repeat(200);

    let chunks = vec![
        make_test_chunk_with_code_meta(
            "g1_emb_0",
            "g1",
            ChunkPath::Embedding,
            &body,
            200,
            vec![EntityId(1)],
            vec![],
            vec![],
            vec![],
            vec![],
            Span::new(0, 100, 0, 0, 0, 0),
            true,
            Some(0),
            Some(2),
        ),
        make_test_chunk_with_code_meta(
            "g1_emb_1",
            "g1",
            ChunkPath::Embedding,
            &body,
            200,
            vec![EntityId(1)],
            vec![],
            vec![],
            vec![],
            vec![],
            Span::new(100, 300, 0, 0, 0, 0),
            true,
            Some(1),
            Some(2),
        ),
    ];

    let merged = merge_small_chunks_cross_group(chunks, &group_spans, &config);
    assert_eq!(merged.len(), 2, "nothing is small enough to merge here");
    for chunk in &merged {
        assert!(
            chunk.metadata.is_fragment(),
            "chunk '{}' is still one piece of a split entity",
            chunk.chunk_id
        );
        assert_eq!(
            chunk
                .metadata
                .as_code()
                .and_then(|code| code.fragment_index),
            Some(chunk.chunk_index),
            "fragment index must stay in step with the split, not the output list"
        );
    }
}
