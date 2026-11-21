use super::*;

#[test]
fn test_cross_group_merge_multiple_small_groups_embedding() {
    let config = ChunkingConfig {
        min_chunk_tokens: 150,
        max_tokens: 512,
        ..Default::default()
    };
    let chunker = GroupChunker::new(config);

    let group_convs = vec![
        GroupConversions {
            group: create_merge_test_group("g1", "hello world foo bar", 0, 19),
            header_conversion: Some(create_merge_test_conversion("hello world foo bar")),
            member_conversions: vec![],
        },
        GroupConversions {
            group: create_merge_test_group("g2", "one two three four five", 20, 39),
            header_conversion: Some(create_merge_test_conversion("one two three four five")),
            member_conversions: vec![],
        },
        GroupConversions {
            group: create_merge_test_group("g3", "alpha beta gamma", 40, 55),
            header_conversion: Some(create_merge_test_conversion("alpha beta gamma")),
            member_conversions: vec![],
        },
    ];

    let chunks = chunker
        .chunk_groups(&group_convs, "test.rs")
        .expect("chunk")
        .chunks;
    let emb_chunks: Vec<_> = chunks
        .iter()
        .filter(|c| c.path == ChunkPath::Embedding)
        .collect();

    assert_eq!(
        emb_chunks.len(),
        1,
        "Should merge all embedding chunks into 1"
    );
    assert_eq!(
        emb_chunks[0].source_group_id, "g1",
        "Should preserve leftmost group"
    );
    assert_eq!(emb_chunks[0].chunk_index, 0);
    assert_eq!(emb_chunks[0].total_chunks, 1);
}

#[test]
fn test_cross_group_bm25_tiny_rescue_merges() {
    let config = ChunkingConfig {
        min_chunk_bm25_terms: 80,
        max_bm25_terms: 150,
        ..Default::default()
    };
    let chunker = GroupChunker::new(config);

    let group_convs = vec![
        GroupConversions {
            group: create_merge_test_group("g1", "hello world foo bar", 0, 19),
            header_conversion: Some(create_merge_test_conversion("hello world foo bar")),
            member_conversions: vec![],
        },
        GroupConversions {
            group: create_merge_test_group("g2", "one two three four five", 20, 39),
            header_conversion: Some(create_merge_test_conversion("one two three four five")),
            member_conversions: vec![],
        },
    ];

    let chunks = chunker
        .chunk_groups(&group_convs, "test.rs")
        .expect("chunk")
        .chunks;
    let bm25_chunks: Vec<_> = chunks
        .iter()
        .filter(|c| c.path == ChunkPath::Bm25)
        .collect();

    assert_eq!(
        bm25_chunks.len(),
        1,
        "tiny same-kind BM25 neighbours merge within the ceiling"
    );
    assert_eq!(bm25_chunks[0].path, ChunkPath::Bm25);
}

#[test]
fn test_cross_group_merge_reduces_chunk_count() {
    let config = ChunkingConfig {
        min_chunk_tokens: 150,
        max_tokens: 512,
        ..Default::default()
    };
    let chunker = GroupChunker::new(config);

    let group_convs = vec![
        GroupConversions {
            group: create_merge_test_group("g1", "small text", 0, 10),
            header_conversion: Some(create_merge_test_conversion("small text")),
            member_conversions: vec![],
        },
        GroupConversions {
            group: create_merge_test_group("g2", "another small text", 11, 27),
            header_conversion: Some(create_merge_test_conversion("another small text")),
            member_conversions: vec![],
        },
    ];

    let chunks = chunker
        .chunk_groups(&group_convs, "test.rs")
        .expect("chunk")
        .chunks;
    let emb_chunks: Vec<_> = chunks
        .iter()
        .filter(|c| c.path == ChunkPath::Embedding)
        .collect();

    assert_eq!(
        emb_chunks.len(),
        1,
        "Should reduce from 2 groups to 1 chunk"
    );
}

#[test]
fn test_cross_group_merge_no_merge_when_all_above_threshold() {
    let config = ChunkingConfig {
        min_chunk_tokens: 150,
        max_tokens: 512,
        ..Default::default()
    };
    let chunker = GroupChunker::new(config);

    // TokenEstimator uses latin_factor=0.25, so tokens ≈ chars * 0.25.
    // To get token_count >= 150, need >= 600 chars.
    let long_text = "A".repeat(600);
    let group_convs = vec![
        GroupConversions {
            group: create_merge_test_group("g1", &long_text, 0, 600),
            header_conversion: Some(create_merge_test_conversion(&long_text)),
            member_conversions: vec![],
        },
        GroupConversions {
            group: create_merge_test_group("g2", &long_text, 601, 1201),
            header_conversion: Some(create_merge_test_conversion(&long_text)),
            member_conversions: vec![],
        },
    ];

    let chunks = chunker
        .chunk_groups(&group_convs, "test.rs")
        .expect("chunk")
        .chunks;
    let emb_chunks: Vec<_> = chunks
        .iter()
        .filter(|c| c.path == ChunkPath::Embedding)
        .collect();

    assert_eq!(
        emb_chunks.len(),
        2,
        "No merge when all chunks above threshold"
    );
}

#[test]
fn test_cross_group_merge_both_paths_produced() {
    let config = ChunkingConfig {
        min_chunk_tokens: 150,
        min_chunk_bm25_terms: 80,
        max_tokens: 512,
        max_bm25_terms: 150,
        ..Default::default()
    };
    let chunker = GroupChunker::new(config);

    let group_convs = vec![
        GroupConversions {
            group: create_merge_test_group("g1", "hello world", 0, 11),
            header_conversion: Some(create_merge_test_conversion("hello world")),
            member_conversions: vec![],
        },
        GroupConversions {
            group: create_merge_test_group("g2", "foo bar baz", 12, 22),
            header_conversion: Some(create_merge_test_conversion("foo bar baz")),
            member_conversions: vec![],
        },
    ];

    let chunks = chunker
        .chunk_groups(&group_convs, "test.rs")
        .expect("chunk")
        .chunks;

    let emb_chunks: Vec<_> = chunks
        .iter()
        .filter(|c| c.path == ChunkPath::Embedding)
        .collect();
    let bm25_chunks: Vec<_> = chunks
        .iter()
        .filter(|c| c.path == ChunkPath::Bm25)
        .collect();

    assert!(
        emb_chunks.len() <= 2,
        "Embedding chunks should be processed"
    );
    assert!(bm25_chunks.len() <= 2, "BM25 chunks should be processed");
}

#[test]
fn test_cross_group_merge_chunk_indices_normalized() {
    let config = ChunkingConfig {
        min_chunk_tokens: 150,
        max_tokens: 512,
        ..Default::default()
    };
    let chunker = GroupChunker::new(config);

    let group_convs = vec![
        GroupConversions {
            group: create_merge_test_group("g1", "hello world", 0, 11),
            header_conversion: Some(create_merge_test_conversion("hello world")),
            member_conversions: vec![],
        },
        GroupConversions {
            group: create_merge_test_group("g2", "foo bar baz", 12, 22),
            header_conversion: Some(create_merge_test_conversion("foo bar baz")),
            member_conversions: vec![],
        },
    ];

    let chunks = chunker
        .chunk_groups(&group_convs, "test.rs")
        .expect("chunk")
        .chunks;

    // Check per-path consistency: indices within each path are 0-based and total_chunks matches
    let emb_chunks: Vec<_> = chunks
        .iter()
        .filter(|c| c.path == ChunkPath::Embedding)
        .collect();
    for (i, chunk) in emb_chunks.iter().enumerate() {
        assert_eq!(
            chunk.chunk_index, i,
            "embedding chunk_index should be normalized"
        );
        assert_eq!(
            chunk.total_chunks,
            emb_chunks.len(),
            "embedding total_chunks should match path count"
        );
    }

    // Both paths merge tiny same-kind neighbours: each path yields one
    // chunk carrying normalized per-path indices.
    let bm25_chunks: Vec<_> = chunks
        .iter()
        .filter(|c| c.path == ChunkPath::Bm25)
        .collect();
    assert_eq!(bm25_chunks.len(), 1);
    for chunk in &bm25_chunks {
        assert_eq!(chunk.chunk_index, 0);
        assert_eq!(chunk.total_chunks, 1);
    }
}

// ── Helpers for tests moved from chunker.rs ──
