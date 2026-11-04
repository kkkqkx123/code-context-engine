//! Overview cards for members split across fragments.
//!
//! A member whose converted text exceeds the path budget is split into
//! fragments, so a single retrieval hit cannot cover the labelled range.
//! For every member whose coverage spreads over a chain of adjacent
//! chunks, this pass appends one overview card: the head of the earliest
//! pure fragment over the union of the chain's source spans. The card is
//! an ordinary indexed chunk, so one hit recovers the whole member.
//!
//! A pure single-entity fragment anchors the chain; adjacency tells a
//! real split with spillover apart from a far-away chunk that merely
//! names the same entity.

use std::collections::HashMap;

use cce_types::ast_to_nl::SplitReason;
use cce_types::entity::EntityId;

use super::boundary::cost;
use super::result::{ChunkPath, ChunkedResult, SourceSpanKind};
use super::strategies::sentences::find_sentence_boundaries;

/// A split member shows up as a chain of chunks that all cover it: at
/// least one pure single-entity fragment (the anchor) plus adjacent
/// chunks that also cover it. Adjacency is what tells a real split with
/// spillover apart from a far-away member-list chunk that merely names
/// the same entity. Mixed chunks whose body covers the entity count;
/// context-only mentions never carry content ids.
fn split_chain(indices: &[usize], chunks: &[ChunkedResult]) -> bool {
    if indices.len() < 2 {
        return false;
    }
    let mut ordered = indices.to_vec();
    ordered.sort_by_key(|index| chunks[*index].metadata.source_span.start_byte);
    let mut pure = false;
    for index in &ordered {
        if chunks[*index]
            .metadata
            .as_code()
            .map(|code| code.content_entity_ids.len() == 1)
            .unwrap_or(false)
        {
            pure = true;
            break;
        }
    }
    if !pure {
        return false;
    }
    for pair in ordered.windows(2) {
        let prev = chunks[pair[0]].metadata.source_span;
        let next = chunks[pair[1]].metadata.source_span;
        if next.start_byte < prev.start_byte {
            return false;
        }
        if next.start_position.row > prev.end_position.row.saturating_add(5) {
            return false;
        }
    }
    true
}

/// Head budget of an overview card in the path's own cost unit.
fn max_cost(path: ChunkPath) -> usize {
    match path {
        ChunkPath::Embedding => 200,
        ChunkPath::Bm25 => 150,
    }
}

/// Head-truncate text to the overview budget, preferring a sentence end.
fn head_text(text: &str, path: ChunkPath) -> String {
    head_text_with_budget(text, path, max_cost(path))
}

/// Head-truncate text to an explicit budget, preferring a sentence end.
fn head_text_with_budget(text: &str, path: ChunkPath, budget: usize) -> String {
    if cost(text, path) <= budget {
        return text.to_string();
    }
    let mut end = 0usize;
    for boundary in find_sentence_boundaries(text) {
        if cost(&text[..boundary], path) > budget {
            break;
        }
        end = boundary;
    }
    if end == 0 {
        for piece in text.split_inclusive('\n') {
            let candidate = end + piece.len();
            if cost(&text[..candidate], path) > budget {
                break;
            }
            end = candidate;
        }
    }
    if end == 0 {
        return text.split_whitespace().next().unwrap_or("").to_string();
    }
    text[..end].trim_end().to_string()
}

/// Fill a BM25 overview card with the union's vocabulary.
///
/// The card promises the union span but carries only the head fragment's
/// text; terms living in later fragments (fields after the struct head,
/// methods after the first) can never match it, so split truths covered
/// only by the card stay unrecoverable. Appending the head of every
/// same-group fragment inside the union span gives the card the union's
/// vocabulary within the same cost budget. Embedding cards keep the
/// single-head text: the similarity model degrades on stitched text.
fn union_heads(
    chunks: &[ChunkedResult],
    first: &ChunkedResult,
    path: ChunkPath,
    union_start: usize,
    union_end: usize,
) -> String {
    let budget = max_cost(path);
    let mut overview = head_text(&first.text, path);
    if cost(&overview, path) >= budget {
        return overview;
    }
    let mut seen = std::collections::HashSet::new();
    seen.insert(overview.clone());
    let mut siblings: Vec<&ChunkedResult> = chunks
        .iter()
        .filter(|chunk| {
            chunk.path == path
                && chunk.source_group_id == first.source_group_id
                && !std::ptr::eq(*chunk, first)
                && chunk.metadata.source_span.start_byte >= union_start
                && chunk.metadata.source_span.end_byte <= union_end
        })
        .collect();
    siblings.sort_by_key(|chunk| chunk.start_byte);
    for sibling in siblings {
        let remaining = budget.saturating_sub(cost(&overview, path));
        if remaining == 0 {
            break;
        }
        let head = head_text_with_budget(&sibling.text, path, remaining);
        if head.trim().is_empty() || !seen.insert(head.clone()) {
            continue;
        }
        // Skip a sibling head already contained in the card: repeating a
        // sentence already present only inflates term frequency and spends
        // budget without adding vocabulary.
        if overview.contains(&head) {
            continue;
        }
        overview.push('\n');
        overview.push_str(&head);
    }
    overview
}

/// Append one overview card per member that owns two or more single-entity
/// fragments. Existing chunks keep their ids and indices; only the
/// per-group per-path totals grow.
pub fn synthesize_overviews(chunks: &mut Vec<ChunkedResult>) {
    let mut by_entity: HashMap<(ChunkPath, EntityId), Vec<usize>> = HashMap::new();
    for (index, chunk) in chunks.iter().enumerate() {
        let covers = chunk
            .metadata
            .as_code()
            .map(|code| code.content_entity_ids.as_slice())
            .unwrap_or(&[]);
        for entity in covers {
            by_entity
                .entry((chunk.path, *entity))
                .or_default()
                .push(index);
        }
    }

    let mut counts: HashMap<(String, ChunkPath), usize> = HashMap::new();
    for chunk in chunks.iter() {
        *counts
            .entry((chunk.source_group_id.clone(), chunk.path))
            .or_default() += 1;
    }

    let mut cards: Vec<ChunkedResult> = Vec::new();
    let mut ordered: Vec<((ChunkPath, EntityId), Vec<usize>)> = by_entity.into_iter().collect();
    ordered.sort_by_key(|(_, indices)| indices[0]);
    for ((path, _), mut indices) in ordered {
        if !split_chain(&indices, chunks) {
            continue;
        }
        indices.sort_by_key(|index| chunks[*index].start_byte);
        // The overview leads with the earliest pure fragment: the head of
        // a mixed spillover chunk belongs to the neighbour, not the member.
        let first = indices
            .iter()
            .find(|index| {
                chunks[**index]
                    .metadata
                    .as_code()
                    .map(|code| code.content_entity_ids.len() == 1)
                    .unwrap_or(false)
            })
            .and_then(|index| chunks.get(*index))
            .unwrap_or(&chunks[indices[0]]);
        let mut start_byte = usize::MAX;
        let mut end_byte = 0usize;
        let mut start_row = usize::MAX;
        let mut end_row = 0usize;
        let mut start_col = 0usize;
        let mut end_col = 0usize;
        for index in &indices {
            let span = chunks[*index].metadata.source_span;
            if span.start_byte < start_byte {
                start_byte = span.start_byte;
                start_row = span.start_position.row;
                start_col = span.start_position.column;
            }
            if span.end_byte > end_byte {
                end_byte = span.end_byte;
                end_row = span.end_position.row;
                end_col = span.end_position.column;
            }
        }
        if start_byte == usize::MAX || end_byte <= start_byte {
            continue;
        }
        let union_span =
            cce_types::Span::new(start_byte, end_byte, start_row, start_col, end_row, end_col);
        // The card text is built after the union span is known so the BM25
        // union fill can admit only fragments inside the promised span.
        let overview = if path == ChunkPath::Bm25 {
            union_heads(chunks, first, path, start_byte, end_byte)
        } else {
            head_text(&first.text, path)
        };
        if overview.trim().is_empty() {
            continue;
        }

        let key = (first.source_group_id.clone(), path);
        let counter = counts.entry(key).or_insert(0);
        let index = *counter;
        *counter += 1;
        let mut card = first.clone();
        card.chunk_id = format!("{}_{}_overview_{index}", first.source_group_id, path);
        card.chunk_index = index;
        card.total_chunks = index + 1;
        card.text = overview;
        card.token_count = cost(&card.text, path);
        card.start_byte = start_byte;
        card.end_byte = end_byte;
        card.prev_overlap = None;
        card.next_overlap = None;
        card.self_contained = false;
        card.truncated = false;
        card.metadata.source_span = union_span;
        card.metadata.source_ranges = vec![union_span];
        card.metadata.source_span_kind = SourceSpanKind::ExactEntities;
        if path == ChunkPath::Bm25 {
            card.metadata.bm25_word_count = Some(cost(&card.text, path));
        }
        if let Some(code) = card.metadata.code_metadata.as_mut() {
            code.has_overlap = false;
            code.is_fragment = false;
            code.fragment_index = None;
            code.total_fragments = None;
            code.original_entity_id = None;
            code.split_reason = SplitReason::Overview;
        }
        cards.push(card);
    }

    chunks.extend(cards);
    let mut totals: HashMap<(String, ChunkPath), usize> = HashMap::new();
    for chunk in chunks.iter() {
        *totals
            .entry((chunk.source_group_id.clone(), chunk.path))
            .or_default() += 1;
    }
    for chunk in chunks.iter_mut() {
        if let Some(total) = totals.get(&(chunk.source_group_id.clone(), chunk.path)) {
            chunk.total_chunks = *total;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::super::{ChunkMetadata, CodeSpecificMetadata};
    use cce_types::GroupType;

    #[allow(clippy::too_many_arguments)]
    fn chunk(
        group: &str,
        path: ChunkPath,
        index: usize,
        total: usize,
        entity: Option<EntityId>,
        start: usize,
        end: usize,
        text: &str,
    ) -> ChunkedResult {
        let code_metadata = entity.map(|id| CodeSpecificMetadata {
            content_entity_ids: vec![id],
            split_reason: SplitReason::MemberBoundary,
            is_fragment: true,
            fragment_index: Some(index),
            total_fragments: Some(total),
            original_entity_id: Some(id),
            ..Default::default()
        });
        let metadata = ChunkMetadata {
            source_span: cce_types::Span::new(start, end, 1, 0, 2, 0),
            code_metadata,
            ..Default::default()
        };
        ChunkedResult {
            chunk_id: format!("{group}_{path}_{index}"),
            source_group_id: group.to_string(),
            path,
            group_type: GroupType::Standalone,
            chunk_index: index,
            total_chunks: total,
            text: text.to_string(),
            bm25_title: Some(format!("{group} title")),
            bm25_keywords: vec![group.to_string()],
            token_count: cost(text, path),
            start_byte: start,
            end_byte: end,
            prev_overlap: None,
            next_overlap: None,
            related_groups: Vec::new(),
            self_contained: false,
            truncated: false,
            metadata,
        }
    }

    #[test]
    fn split_member_gains_overview_with_union_span() {
        let head = "method App.register_blueprint (self, blueprint).\nRegister a class on the application.\nCalls the blueprint method after recording.\n";
        let tail = "blueprint.register\n(self, options)\n";
        let mut chunks = vec![
            chunk(
                "g",
                ChunkPath::Embedding,
                0,
                2,
                Some(EntityId(7)),
                100,
                200,
                head,
            ),
            chunk(
                "g",
                ChunkPath::Embedding,
                1,
                2,
                Some(EntityId(7)),
                200,
                260,
                tail,
            ),
        ];
        synthesize_overviews(&mut chunks);
        assert_eq!(chunks.len(), 3, "one card appended");
        let card = chunks.last().expect("card present");
        assert!(card.chunk_id.contains("overview"), "id marks the card");
        assert!(card.text.contains("Register a class"), "head text kept");
        assert!(!card.text.contains("self, options"), "tail cut off");
        assert_eq!(card.start_byte, 100, "union starts at first fragment");
        assert_eq!(card.end_byte, 260, "union ends at last fragment");
        assert_eq!(
            card.metadata.source_span_kind,
            SourceSpanKind::ExactEntities,
            "card claims the whole entity"
        );
        assert_eq!(card.metadata.source_ranges.len(), 1, "single union range");
        assert_eq!(chunks[0].total_chunks, 3, "sibling totals grow");
        assert_eq!(chunks[0].chunk_id, "g_emb_0", "sibling ids stable");
    }

    #[test]
    fn single_and_merged_chunks_gain_no_card() {
        let mut chunks = vec![
            chunk(
                "g",
                ChunkPath::Embedding,
                0,
                2,
                Some(EntityId(7)),
                0,
                50,
                "head text here",
            ),
            chunk(
                "g",
                ChunkPath::Embedding,
                1,
                2,
                Some(EntityId(9)),
                50,
                90,
                "other member",
            ),
            chunk(
                "h",
                ChunkPath::Embedding,
                0,
                1,
                Some(EntityId(11)),
                0,
                40,
                "whole member",
            ),
        ];
        synthesize_overviews(&mut chunks);
        assert_eq!(chunks.len(), 3, "no card without a split member");
    }

    #[test]
    fn adjacent_spillover_produces_card() {
        let mut chunks = vec![
            chunk(
                "g",
                ChunkPath::Bm25,
                0,
                2,
                Some(EntityId(7)),
                0,
                50,
                "is match returns true",
            ),
            chunk(
                "g",
                ChunkPath::Bm25,
                1,
                2,
                Some(EntityId(7)),
                50,
                90,
                "candidate amortizes",
            ),
        ];
        chunks[1]
            .metadata
            .code_metadata
            .as_mut()
            .expect("code metadata present")
            .content_entity_ids
            .push(EntityId(8));
        synthesize_overviews(&mut chunks);
        assert_eq!(chunks.len(), 3, "spillover tail still chains");
        let card = chunks.last().expect("card present");
        assert_eq!(card.start_byte, 0, "union covers the head");
        assert_eq!(card.end_byte, 90, "union covers the spillover");
    }

    #[test]
    fn bm25_card_fills_union_vocabulary_from_sibling_fragments() {
        let mut chunks = vec![
            chunk(
                "g",
                ChunkPath::Bm25,
                0,
                2,
                Some(EntityId(7)),
                0,
                50,
                "is match returns true",
            ),
            chunk(
                "g",
                ChunkPath::Bm25,
                1,
                2,
                Some(EntityId(7)),
                50,
                90,
                "candidate amortizes",
            ),
        ];
        synthesize_overviews(&mut chunks);
        assert_eq!(chunks.len(), 3, "one card appended");
        let card = chunks.last().expect("card present");
        assert!(
            card.text.contains("is match returns true"),
            "head leads: {}",
            card.text
        );
        assert!(
            card.text.contains("candidate amortizes"),
            "sibling vocabulary fills the union card: {}",
            card.text
        );
    }

    #[test]
    fn bm25_card_skips_sibling_head_already_contained() {
        let mut chunks = vec![
            chunk(
                "g",
                ChunkPath::Bm25,
                0,
                2,
                Some(EntityId(7)),
                0,
                50,
                "alpha beta gamma",
            ),
            chunk(
                "g",
                ChunkPath::Bm25,
                1,
                2,
                Some(EntityId(7)),
                50,
                90,
                "beta gamma",
            ),
        ];
        synthesize_overviews(&mut chunks);
        assert_eq!(chunks.len(), 3, "one card appended");
        let card = chunks.last().expect("card present");
        assert_eq!(
            card.text.matches("beta gamma").count(),
            1,
            "contained sibling head adds no second copy: {}",
            card.text
        );
    }

    #[test]
    fn embedding_card_keeps_single_head_text() {
        let mut chunks = vec![
            chunk(
                "g",
                ChunkPath::Embedding,
                0,
                2,
                Some(EntityId(7)),
                0,
                50,
                "is match returns true",
            ),
            chunk(
                "g",
                ChunkPath::Embedding,
                1,
                2,
                Some(EntityId(7)),
                50,
                90,
                "candidate amortizes",
            ),
        ];
        synthesize_overviews(&mut chunks);
        assert_eq!(chunks.len(), 3, "one card appended");
        let card = chunks.last().expect("card present");
        assert!(
            !card.text.contains("candidate amortizes"),
            "embedding cards keep the single-head text: {}",
            card.text
        );
    }

    #[test]
    fn distant_name_match_produces_no_card() {
        let mut head = chunk(
            "g",
            ChunkPath::Embedding,
            0,
            2,
            Some(EntityId(7)),
            0,
            50,
            "class header names the member",
        );
        head.metadata.source_span = cce_types::Span::new(0, 50, 1, 0, 3, 0);
        let mut body = chunk(
            "g",
            ChunkPath::Embedding,
            1,
            2,
            Some(EntityId(7)),
            600,
            660,
            "member body text here",
        );
        body.metadata.source_span = cce_types::Span::new(600, 660, 60, 0, 66, 0);
        let mut chunks = vec![head, body];
        synthesize_overviews(&mut chunks);
        assert_eq!(chunks.len(), 2, "far-apart chunks never chain");
    }
}
