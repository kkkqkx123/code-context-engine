// Chunk merge tests grouped by merge scenario.
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
        truncated: false,
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
        content_entity_kinds: Vec::new(),
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
        truncated: false,
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

fn make_small_self_contained_chunk(
    id: &str,
    group_id: &str,
    content_entity_ids: Vec<EntityId>,
) -> ChunkedResult {
    let mut chunk = make_test_chunk_with_code_meta(
        id,
        group_id,
        ChunkPath::Embedding,
        &"hello world foo bar baz qux ".repeat(11),
        75,
        content_entity_ids,
        vec![],
        vec![],
        vec![],
        vec![],
        Span::new(0, 10, 0, 0, 0, 0),
        false,
        None,
        None,
    );
    chunk.self_contained = true;
    chunk
}

mod cross_group;
mod merge_small;
mod merge_two;
mod threshold;
