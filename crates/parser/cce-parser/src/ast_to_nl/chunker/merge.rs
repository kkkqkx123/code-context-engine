//! Cross-group merging of undersized chunks.
//!
//! After per-group chunking, chunks below the min threshold are merged with
//! adjacent chunks (source-position order, adjacent groups only). The merged
//! chunk keeps the left chunk's identity and records every contributing group
//! in `ChunkMetadata::merged_group_ids`, so merged groups survive in relation
//! graphs and source attribution.

mod merge_strategy;

use std::collections::HashMap;

use cce_types::Span;
use cce_types::entity::EntityId;

use super::boundary::SplitReason;
use super::config::ChunkingConfig;
use super::result::{ChunkMetadata, ChunkedResult, CodeSpecificMetadata, SourceSpanKind};

/// Remove duplicates while preserving the first-occurrence order.
///
/// `Vec::dedup` only removes consecutive duplicates; lists built by
/// concatenating several already-sorted entity id sequences (header +
/// member ids + source ids) can carry the same id in non-adjacent
/// positions, so a membership-set dedup is required.
fn dedup_preserving_order<T>(items: &mut Vec<T>)
where
    T: Eq + std::hash::Hash + Clone,
{
    let mut seen = std::collections::HashSet::new();
    items.retain(|item| seen.insert(item.clone()));
}

/// Merge the source coverage of two adjacent chunks.
///
/// The merged text is `a.text + b.text`, so the merged coverage is the union
/// of both chunks' source ranges — merged by adjacency/overlap — with an
/// enclosing span derived from the outermost endpoints. This is always
/// correct regardless of whether the chunks carry resolvable entity ids.
fn merge_source_coverage(
    a: &ChunkMetadata,
    b: &ChunkMetadata,
) -> (Span, Vec<Span>, SourceSpanKind) {
    let mut ranges: Vec<Span> = a.source_ranges.to_vec();
    ranges.extend_from_slice(&b.source_ranges);
    ranges.sort_by_key(|span| (span.start_byte, span.end_byte));

    let mut merged: Vec<Span> = Vec::with_capacity(ranges.len());
    for span in ranges {
        if let Some(last) = merged.last_mut() {
            if span.start_byte <= last.end_byte.saturating_add(1) {
                if span.end_byte > last.end_byte {
                    last.end_byte = span.end_byte;
                    last.end_position = span.end_position;
                }
                continue;
            }
        }
        merged.push(span);
    }

    if merged.is_empty() {
        return (Span::default(), vec![], SourceSpanKind::GroupFallback);
    }

    let first = merged[0];
    let source_span = merged.iter().skip(1).fold(first, |combined, current| {
        let start = if current.start_byte < combined.start_byte {
            current
        } else {
            &combined
        };
        let end = if current.end_byte > combined.end_byte {
            current
        } else {
            &combined
        };
        Span::new(
            start.start_byte,
            end.end_byte,
            start.start_position.row,
            start.start_position.column,
            end.end_position.row,
            end.end_position.column,
        )
    });

    let kind = match (a.source_span_kind, b.source_span_kind) {
        (SourceSpanKind::ExactEntities, SourceSpanKind::ExactEntities) => {
            SourceSpanKind::ExactEntities
        }
        (SourceSpanKind::DocumentRange, SourceSpanKind::DocumentRange) => {
            SourceSpanKind::DocumentRange
        }
        _ => SourceSpanKind::GroupFallback,
    };

    (source_span, merged, kind)
}

/// Merge two adjacent chunks into one.
///
/// The merged chunk keeps `a`'s identity (chunk_id, source_group_id, entity
/// kind) and records every group that contributed content in
/// `metadata.merged_group_ids`, so merged groups survive in relation graphs
/// and source attribution.
pub(crate) fn merge_two_chunks(
    a: &ChunkedResult,
    b: &ChunkedResult,
    combined_cost: usize,
) -> ChunkedResult {
    let merged_text = format!("{}\n\n{}", a.text, b.text);

    let mut content_entity_ids = a.metadata.content_entity_ids().to_vec();
    content_entity_ids.extend_from_slice(b.metadata.content_entity_ids());
    dedup_preserving_order(&mut content_entity_ids);

    // Merge display names positionally aligned with the deduplicated ID list.
    // Chunks with missing/legacy name lists contribute empty entries, which
    // downstream consumers treat as "name unknown".
    fn aligned_names(chunk: &ChunkedResult) -> Vec<(EntityId, String)> {
        let code = chunk.metadata.as_code();
        let ids = chunk.metadata.content_entity_ids();
        let names = code
            .map(|m| m.content_entity_names.as_slice())
            .unwrap_or(&[]);
        ids.iter()
            .enumerate()
            .map(|(i, id)| (*id, names.get(i).cloned().unwrap_or_default()))
            .collect()
    }
    let mut name_by_id: Vec<(EntityId, String)> = aligned_names(a);
    name_by_id.extend(aligned_names(b));
    name_by_id.dedup_by(|a, b| a.0 == b.0);
    let content_entity_names: Vec<String> = content_entity_ids
        .iter()
        .map(|id| {
            name_by_id
                .iter()
                .find(|(nid, _)| nid == id)
                .map(|(_, name)| name.clone())
                .unwrap_or_default()
        })
        .collect();

    let mut context_entity_ids = a.metadata.context_entity_ids().to_vec();
    context_entity_ids.extend_from_slice(b.metadata.context_entity_ids());
    dedup_preserving_order(&mut context_entity_ids);

    let mut modifiers = a
        .metadata
        .as_code()
        .map(|m| m.modifiers.clone())
        .unwrap_or_default();
    if let Some(b_code) = b.metadata.as_code() {
        modifiers.extend_from_slice(&b_code.modifiers);
    }
    dedup_preserving_order(&mut modifiers);

    let mut overlap_entities = a
        .metadata
        .as_code()
        .map(|m| m.overlap_entities.clone())
        .unwrap_or_default();
    if let Some(b_code) = b.metadata.as_code() {
        overlap_entities.extend_from_slice(&b_code.overlap_entities);
    }
    dedup_preserving_order(&mut overlap_entities);

    let mut related_groups = a.related_groups.clone();
    for rel in &b.related_groups {
        let is_dup = related_groups
            .iter()
            .any(|r| r.group_id == rel.group_id && r.relation_type == rel.relation_type);
        if !is_dup {
            related_groups.push(rel.clone());
        }
    }

    let mut merged_group_ids = a.metadata.merged_group_ids.clone();
    if !merged_group_ids.contains(&b.source_group_id) {
        merged_group_ids.push(b.source_group_id.clone());
    }
    for gid in &b.metadata.merged_group_ids {
        if !merged_group_ids.contains(gid) {
            merged_group_ids.push(gid.clone());
        }
    }

    let (source_span, source_ranges, source_span_kind) =
        merge_source_coverage(&a.metadata, &b.metadata);

    // Any source group marked `Test` makes the merged chunk `Test`
    // (High confidence overrides Medium).
    let test_info = a.metadata.test_info.merge(&b.metadata.test_info);

    let mut bm25_keywords = a.bm25_keywords.clone();
    bm25_keywords.extend_from_slice(&b.bm25_keywords);
    bm25_keywords.dedup();

    let bm25_word_count = merged_text
        .split_whitespace()
        .filter(|w| !w.is_empty())
        .count();

    let code_metadata = CodeSpecificMetadata {
        content_entity_ids,
        content_entity_names,
        context_entity_ids,
        entity_kind: a.metadata.entity_kind().unwrap_or_default(),
        modifiers,
        split_reason: SplitReason::MemberBoundary,
        overlap_entities,
        has_overlap: false,
        is_fragment: false,
        fragment_index: None,
        total_fragments: None,
        original_entity_id: a.metadata.as_code().and_then(|m| m.original_entity_id),
        pattern_info: a.metadata.as_code().and_then(|m| m.pattern_info.clone()),
    };

    ChunkedResult {
        chunk_id: a.chunk_id.clone(),
        source_group_id: a.source_group_id.clone(),
        path: a.path,
        group_type: a.group_type,
        chunk_index: a.chunk_index,
        total_chunks: a.total_chunks,
        text: merged_text,
        bm25_title: a.bm25_title.clone(),
        bm25_keywords,
        token_count: combined_cost,
        start_byte: a.start_byte,
        end_byte: b.end_byte,
        prev_overlap: None,
        next_overlap: None,
        related_groups,
        self_contained: a.self_contained || b.self_contained,
        metadata: ChunkMetadata {
            content_type: a.metadata.content_type.clone(),
            file_path: a.metadata.file_path.clone(),
            source_span,
            source_ranges,
            source_span_kind,
            bm25_word_count: Some(bm25_word_count),
            segment_id: a.metadata.segment_id.clone(),
            merged_group_ids,
            test_info,
            file_category: a.metadata.file_category,
            code_metadata: Some(code_metadata),
            doc_metadata: a.metadata.doc_metadata.clone(),
        },
    }
}

/// Merge small chunks cross-group.
///
/// Sorts chunks by source position (using group span), then merges adjacent chunks
/// where the leading chunk is below the min threshold and the combined size fits
/// within the merge ceiling. The decision is delegated to
/// `merge_strategy::should_merge`, which wraps the single shared rule
/// `boundary::can_merge` plus the Embedding-only self-contained exemption,
/// identical to the intra-splitter pass in `splitter::TextSplitter`.
///
/// Cost is path-dependent: the Embedding path measures estimated tokens
/// (re-estimated on the combined text), the BM25 path measures actual word
/// counts (exact sum, since the separator adds no words). The BM25 merge
/// ceiling is hard-capped at `max_bm25_words`: `cross_group_merge_threshold`
/// (an embedding-token setting) must never widen the BM25 limit.
///
/// Uses a stack-based approach to enable chain merging: after merging two chunks,
/// the resulting chunk is checked against the next chunk for further merging.
pub(crate) fn merge_small_chunks_cross_group(
    mut chunks: Vec<ChunkedResult>,
    group_spans: &HashMap<String, Span>,
    config: &ChunkingConfig,
) -> Vec<ChunkedResult> {
    if chunks.len() <= 1 {
        return chunks;
    }

    let path = chunks[0].path;

    merge_strategy::sort_chunks_by_span(&mut chunks, group_spans);

    let mut stack: Vec<ChunkedResult> = Vec::with_capacity(chunks.len());

    for chunk in chunks {
        stack.push(chunk);

        while stack.len() >= 2 {
            let len = stack.len();
            let top = &stack[len - 1];
            let prev = &stack[len - 2];

            if merge_strategy::should_merge(prev, top, path, config) {
                let combined = merge_strategy::combined_cost(prev, top, path);
                let merged = merge_two_chunks(prev, top, combined);
                stack.pop();
                stack.pop();
                stack.push(merged);
            } else {
                break;
            }
        }
    }

    let total = stack.len();
    for (idx, chunk) in stack.iter_mut().enumerate() {
        chunk.chunk_index = idx;
        chunk.total_chunks = total;
        if let Some(code) = chunk.metadata.as_code_mut() {
            code.fragment_index = None;
            code.total_fragments = None;
        }
    }

    stack
}

#[cfg(test)]
mod tests;
