use std::ops::Range;

use cce_config::modules::ChunkingConfig;
use cce_types::entity::EntityId;

use crate::grouper::EntityGroup;

use super::super::boundary::{ChunkBoundary, NlEntityBoundary, SplitReason, cost};
use super::super::result::ChunkPath;
use super::lines::extend_or_push_trailing;

/// Partition `text[range]` at located entity-name spans.
///
/// Pure strategy: returns a complete partition of the interval (entity spans
/// plus the gaps between them) and never falls back to another strategy.
/// Returns an empty list when no entity boundary data is available in the
/// range — the caller (`split_range`) descends to a coarser boundary source.
pub fn split_by_members(
    text: &str,
    group: &EntityGroup,
    path: ChunkPath,
    config: &ChunkingConfig,
    range: Range<usize>,
) -> Vec<ChunkBoundary> {
    let nl_boundaries = super::super::boundary::locate_entities_in_nl_text(text, group);
    split_by_members_with_nl(text, group, path, config, &nl_boundaries, range)
}

/// Partition `text[range]` at NL-text-relative entity spans (accumulating
/// entities into chunks up to the path limit).
///
/// Pure strategy: returns a complete partition of the interval and never
/// falls back to another strategy. An oversized entity is emitted as a single
/// piece; re-splitting it is the caller's (`split_range`) job.
pub fn split_by_members_with_nl(
    text: &str,
    _group: &EntityGroup,
    path: ChunkPath,
    config: &ChunkingConfig,
    nl_boundaries: &[NlEntityBoundary],
    range: Range<usize>,
) -> Vec<ChunkBoundary> {
    let base = range.start;
    let end = range.end;

    let mut spans: Vec<&NlEntityBoundary> = nl_boundaries
        .iter()
        .filter(|b| b.start_byte < end && b.end_byte > base)
        .collect();
    if spans.is_empty() {
        return Vec::new();
    }

    // Locate-based spans can overlap when one entity name occurs inside
    // another; normalize to non-overlapping spans (earlier span wins) so the
    // partition stays monotonic.
    spans.sort_by_key(|b| (b.start_byte, b.end_byte));
    let spans: Vec<&NlEntityBoundary> = spans.into_iter().fold(Vec::new(), |mut acc, s| {
        if acc
            .last()
            .is_none_or(|last: &&NlEntityBoundary| s.start_byte >= last.end_byte)
        {
            acc.push(s);
        }
        acc
    });
    if spans.is_empty() {
        return Vec::new();
    }

    let limit = path_limit(path, config);
    let mut boundaries = Vec::new();
    let mut chunk_start = base;
    let mut chunk_cost = 0usize;
    let mut chunk_entities: Vec<EntityId> = Vec::new();

    for (idx, span) in spans.iter().enumerate() {
        let start_byte = span.start_byte.max(base).min(end);
        let end_byte = span.end_byte.max(base).min(end);
        if start_byte >= end_byte {
            continue;
        }
        let next_start = spans
            .get(idx + 1)
            .map(|s| s.start_byte.max(base).min(end))
            .unwrap_or(end);

        let entity_text = &text[start_byte..end_byte];
        let entity_cost = cost(entity_text, path);

        if entity_cost > limit {
            if !chunk_entities.is_empty() {
                boundaries.push(
                    ChunkBoundary::new(chunk_start, start_byte, SplitReason::MemberBoundary)
                        .with_token_count(chunk_cost)
                        .with_entity_ids(std::mem::take(&mut chunk_entities)),
                );
            }
            // The oversized entity becomes its own (oversized) piece; the
            // recursion sub-splits it at a coarser boundary source. Leading
            // gap text (zero-cost, never flushed) is folded into the piece
            // so the partition covers the whole range.
            let piece_start = if boundaries.is_empty() {
                chunk_start
            } else {
                start_byte
            };
            boundaries.push(
                ChunkBoundary::new(piece_start, next_start, SplitReason::MemberBoundary)
                    .with_token_count(cost(&text[piece_start..next_start], path))
                    .with_entity_ids(vec![span.entity_id]),
            );
            chunk_start = next_start;
            chunk_cost = 0;
            chunk_entities.clear();
            continue;
        }

        if chunk_cost + entity_cost > limit && !chunk_entities.is_empty() {
            let min_cost: usize = match path {
                ChunkPath::Bm25 => config.min_chunk_bm25_words,
                ChunkPath::Embedding => config.min_chunk_tokens,
            };

            // If current chunk is below min size and the next entity fits,
            // continue accumulating instead of creating a tiny chunk.
            if chunk_cost < min_cost && chunk_cost + entity_cost <= limit {
                chunk_cost += entity_cost;
                chunk_entities.push(span.entity_id);
                continue;
            }

            boundaries.push(
                ChunkBoundary::new(chunk_start, start_byte, SplitReason::MemberBoundary)
                    .with_token_count(chunk_cost)
                    .with_entity_ids(std::mem::take(&mut chunk_entities)),
            );

            chunk_start = start_byte;
            chunk_cost = entity_cost;
            chunk_entities = vec![span.entity_id];
        } else {
            chunk_cost += entity_cost;
            chunk_entities.push(span.entity_id);
        }
    }

    let covered = if !chunk_entities.is_empty() {
        boundaries.push(
            ChunkBoundary::new(chunk_start, end, SplitReason::MemberBoundary)
                .with_token_count(chunk_cost)
                .with_entity_ids(chunk_entities),
        );
        true
    } else {
        false
    };

    if !covered {
        extend_or_push_trailing(
            &mut boundaries,
            text,
            chunk_start,
            end,
            path,
            SplitReason::MemberBoundary,
        );
    }

    boundaries
}

fn path_limit(path: ChunkPath, config: &ChunkingConfig) -> usize {
    match path {
        ChunkPath::Bm25 => config.max_bm25_words,
        ChunkPath::Embedding => config.max_tokens,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use cce_config::modules::ChunkingConfig;
    use cce_types::Span;
    use cce_types::entity::{EntityId, EntityKind, GroupedEntity};

    use crate::grouper::types::{EntityGroup, GroupType};

    use super::super::super::result::ChunkPath;
    use super::*;

    #[test]
    fn test_split_by_members_with_entity_spans() {
        let config = ChunkingConfig::default();

        let text = "Test method1 entity3";
        let span_default = Span::default();
        let group = EntityGroup {
            group_id: compact_str::CompactString::from("test"),
            group_type: GroupType::ClassWithMethods,
            header: Some(GroupedEntity {
                id: EntityId(1),
                kind: EntityKind::Class,
                name: "Test".to_string(),
                ..Default::default()
            }),
            header_id: Some(EntityId(1)),
            members: smallvec::smallvec![GroupedEntity {
                id: EntityId(2),
                kind: EntityKind::Method,
                name: "method1".to_string(),
                ..Default::default()
            },],
            member_ids: smallvec::smallvec![EntityId(2)],
            entity_spans: HashMap::from([
                (
                    EntityId(1),
                    Span {
                        start_byte: 0,
                        end_byte: 6,
                        ..span_default
                    },
                ),
                (
                    EntityId(2),
                    Span {
                        start_byte: 7,
                        end_byte: 15,
                        ..span_default
                    },
                ),
            ]),
            kind: EntityKind::Class,
            name: compact_str::CompactString::from("Test"),
            ..Default::default()
        };

        let boundaries = split_by_members(text, &group, ChunkPath::Bm25, &config, 0..text.len());
        assert!(!boundaries.is_empty());
        // Partition covers the whole range.
        assert_eq!(boundaries.first().unwrap().start_byte, 0);
        assert_eq!(boundaries.last().unwrap().end_byte, text.len());
    }

    #[test]
    fn test_split_by_members_no_located_spans_returns_empty() {
        let config = ChunkingConfig::default();

        let text = "short";
        let group = EntityGroup {
            group_id: compact_str::CompactString::from("test"),
            group_type: GroupType::ClassWithMethods,
            header: Some(GroupedEntity {
                id: EntityId(1),
                kind: EntityKind::Class,
                name: "Test".to_string(),
                ..Default::default()
            }),
            header_id: Some(EntityId(1)),
            entity_spans: HashMap::from([(
                EntityId(1),
                Span {
                    start_byte: 0,
                    end_byte: 999,
                    ..Default::default()
                },
            )]),
            ..Default::default()
        };

        // No entity name occurs in the text → no boundary data → empty list
        // (the caller descends to a coarser boundary source).
        let boundaries = split_by_members(text, &group, ChunkPath::Bm25, &config, 0..text.len());
        assert!(boundaries.is_empty());
    }

    #[test]
    fn test_split_by_members_with_nl_basic() {
        let config = ChunkingConfig {
            max_bm25_words: 2,
            ..Default::default()
        };
        let text = "alpha one two beta three four gamma five";
        let nlb = vec![
            NlEntityBoundary {
                entity_id: EntityId(1),
                start_byte: 0,
                end_byte: 5,
            },
            NlEntityBoundary {
                entity_id: EntityId(2),
                start_byte: 14,
                end_byte: 18,
            },
            NlEntityBoundary {
                entity_id: EntityId(3),
                start_byte: 30,
                end_byte: 35,
            },
        ];
        let group = EntityGroup::default();
        let boundaries =
            split_by_members_with_nl(text, &group, ChunkPath::Bm25, &config, &nlb, 0..text.len());
        assert!(!boundaries.is_empty());
        // Partition covers the whole range contiguously.
        let mut prev = 0;
        for b in &boundaries {
            assert_eq!(b.start_byte, prev);
            assert!(b.start_byte < b.end_byte);
            prev = b.end_byte;
        }
        assert_eq!(prev, text.len());
    }

    #[test]
    fn test_split_by_members_with_nl_empty_spans_returns_empty() {
        let config = ChunkingConfig::default();
        let group = EntityGroup::default();
        let boundaries =
            split_by_members_with_nl("some text", &group, ChunkPath::Bm25, &config, &[], 0..9);
        assert!(boundaries.is_empty());
    }
}
