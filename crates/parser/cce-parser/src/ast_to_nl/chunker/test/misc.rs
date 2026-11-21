use super::*;

#[test]
fn test_add_relations_empty() {
    let tracker = GroupTracker::new();
    let mut chunks: Vec<crate::ast_to_nl::chunker::ChunkedResult> = vec![];
    crate::ast_to_nl::chunker::chunker::add_relations(&tracker, &mut chunks);
    assert!(chunks.is_empty());
}

#[test]
fn test_chunk_groups_empty() {
    let chunker = GroupChunker::new(ChunkingConfig::default());
    let chunks = chunker.chunk_groups(&[], "test.rs").expect("chunk").chunks;
    assert!(chunks.is_empty());
}

// ── Chunk override three-tier chain (above → builtin → below) ──

#[test]
fn test_single_entity_bm25_truncates_to_extended_bound() {
    // Single-entity BM25 stays whole in the splitter to avoid blank-only
    // fragments; beyond three times the hard limit it truncates to the
    // extended bound. Embedding still subdivides to the base hard limit.
    let config = ChunkingConfig {
        max_tokens: 40,
        max_bm25_terms: 40,
        min_chunk_tokens: 5,
        min_chunk_bm25_terms: 5,
        ..Default::default()
    };
    let mut parent = create_small_group();
    parent.group_id = CompactString::from("parent_long");
    parent.name = CompactString::from("LongParentName Word ".repeat(50));
    parent.kind = EntityKind::Class;
    let mut child = create_small_group();
    child.group_id = CompactString::from("child_tiny");
    child.group_type = GroupType::ClassWithMethods;
    child.kind = EntityKind::Class;
    child.name = CompactString::from("Tiny");
    child.header = Some(GroupedEntity::new(
        EntityId(21),
        EntityKind::Class,
        "Tiny".to_string(),
        "class Tiny".to_string(),
    ));
    child.header_id = Some(EntityId(21));
    child.parent_group_id = Some(CompactString::from("parent_long"));
    let long_text = "w ".repeat(400);
    let header_conv = ConversionResult {
        entity_id: EntityId(21),
        kind: EntityKind::Class,
        name: "Tiny".to_string(),
        file_path: "tiny_parent.cs".to_string(),
        bm25_text: Some(long_text.clone()),
        embedding_text: Some(long_text.clone()),
        ..Default::default()
    };
    let member_conv = ConversionResult {
        entity_id: EntityId(22),
        kind: EntityKind::Method,
        name: "run".to_string(),
        file_path: "tiny_parent.cs".to_string(),
        bm25_text: Some(long_text.clone()),
        embedding_text: Some(long_text),
        ..Default::default()
    };
    let conversions = vec![
        GroupConversions {
            group: parent.clone(),
            header_conversion: Some(ConversionResult {
                entity_id: EntityId(20),
                kind: EntityKind::Class,
                name: parent.name.to_string(),
                file_path: "tiny_parent.cs".to_string(),
                bm25_text: Some("parent".to_string()),
                embedding_text: Some("parent".to_string()),
                ..Default::default()
            }),
            member_conversions: vec![],
        },
        GroupConversions {
            group: child.clone(),
            header_conversion: Some(header_conv),
            member_conversions: vec![member_conv],
        },
    ];
    let chunker = GroupChunker::new(config.clone());
    let output = chunker
        .chunk_groups(&conversions, "tiny_parent.cs")
        .expect("tiny budget must not drop the file");
    assert!(!output.chunks.is_empty(), "file must keep its chunks");
    // BM25 wholes truncate to three times hard; embedding subdivides to hard.
    for chunk in &output.chunks {
        if chunk.path == ChunkPath::Bm25 {
            assert!(
                config.cost(&chunk.text, chunk.path) <= 120,
                "BM25 chunk {} must fit the extended bound",
                chunk.chunk_id
            );
        } else {
            assert!(
                config.cost(&chunk.text, chunk.path) <= config.limit(chunk.path),
                "embedding chunk {} must fit the hard limit",
                chunk.chunk_id
            );
        }
    }
}
