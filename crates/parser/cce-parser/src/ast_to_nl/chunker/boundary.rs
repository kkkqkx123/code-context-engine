//! Chunk boundary definitions
//!
//! Defines chunk boundaries and split reasons for text segmentation.

use cce_config::modules::ChunkingConfig;
use cce_types::ConversionResult;
use cce_types::entity::EntityId;
use cce_utils::token_estimation::estimate_tokens;

use crate::grouper::EntityGroup;

use super::result::ChunkPath;

/// Split reason (cross-layer contract, defined in `cce_core`).
pub use cce_types::ast_to_nl::SplitReason;

/// Compute the cost of `text` for `path`.
///
/// BM25 costs actual word count; Embedding costs estimated tokens. All
/// strategies, merge layers and chunk assembly use this single metric so
/// limit checks and accumulation are always measured against the path's own
/// unit.
pub fn cost(text: &str, path: ChunkPath) -> usize {
    match path {
        ChunkPath::Bm25 => text.split_whitespace().filter(|w| !w.is_empty()).count(),
        ChunkPath::Embedding => estimate_tokens(text),
    }
}

/// Resolve the `(min, ceiling)` merge thresholds for `path`.
///
/// The min threshold is the undersized-chunk trigger; the ceiling is the
/// largest combined cost a merge may produce. BM25 is hard-capped at
/// `max_bm25_words`; the Embedding ceiling is `cross_group_merge_threshold`
/// (0 = `max_tokens`). Shared by both merge layers so their limits can never
/// drift apart.
pub fn merge_limits(path: ChunkPath, config: &ChunkingConfig) -> (usize, usize) {
    match path {
        ChunkPath::Bm25 => (config.min_chunk_bm25_words, config.max_bm25_words),
        ChunkPath::Embedding => (
            config.min_chunk_tokens,
            if config.cross_group_merge_threshold == 0 {
                config.max_tokens
            } else {
                config.cross_group_merge_threshold
            },
        ),
    }
}

/// Whether `prev` may absorb `next` in a merge.
///
/// The single merge decision shared by the intra-splitter pass
/// (`splitter::TextSplitter`) and the cross-group pass
/// (`merge::merge_small_chunks_cross_group`): a merge happens only when the
/// leading chunk is still below the path's min threshold (the undersized
/// chunk is the one being rescued) and the combined cost stays within the
/// path's merge ceiling. Boundaries are preserved whenever the leading chunk
/// is already large enough — large chunks never absorb neighbors.
pub fn can_merge(prev: &str, next: &str, path: ChunkPath, config: &ChunkingConfig) -> bool {
    let (min_threshold, merge_threshold) = merge_limits(path, config);
    let prev_cost = cost(prev, path);
    prev_cost < min_threshold && prev_cost + cost(next, path) <= merge_threshold
}

/// Chunk boundary information
///
/// # Byte Range Semantics
///
/// - `start_byte` / `end_byte`: positions in the **BM25 NL text**. Used by
///   `TextSplitter` for splitting decisions and text extraction.
#[derive(Debug, Clone, PartialEq)]
pub struct ChunkBoundary {
    /// Start byte position (in BM25 NL text)
    pub start_byte: usize,
    /// End byte position (in BM25 NL text)
    pub end_byte: usize,
    /// Start line (0-indexed)
    pub start_line: usize,
    /// End line (0-indexed, exclusive)
    pub end_line: usize,
    /// Split reason
    pub split_reason: SplitReason,
    /// Entity IDs in this chunk
    pub entity_ids: Vec<EntityId>,
    /// Estimated token count
    pub token_count: usize,
    /// Nested group ID (if this chunk is a nested group)
    pub group_id: Option<String>,
    /// Nesting level of this chunk
    pub nesting_level: usize,
}

/// Chunk segment with extracted text
///
/// This is the output from TextSplitter - contains both boundary info and pre-extracted text.
#[derive(Debug, Clone, PartialEq)]
pub struct ChunkSegment {
    /// Boundary information
    pub boundary: ChunkBoundary,
    /// Extracted text content for this segment
    pub text: String,
}

impl ChunkBoundary {
    /// Create new boundary
    pub fn new(start_byte: usize, end_byte: usize, split_reason: SplitReason) -> Self {
        Self {
            start_byte,
            end_byte,
            start_line: 0,
            end_line: 0,
            split_reason,
            entity_ids: Vec::new(),
            token_count: 0,
            group_id: None,
            nesting_level: 0,
        }
    }

    /// Set line numbers
    pub fn with_lines(mut self, start_line: usize, end_line: usize) -> Self {
        self.start_line = start_line;
        self.end_line = end_line;
        self
    }

    /// Set entity IDs
    pub fn with_entity_ids(mut self, entity_ids: Vec<EntityId>) -> Self {
        self.entity_ids = entity_ids;
        self
    }

    /// Set token count
    pub fn with_token_count(mut self, token_count: usize) -> Self {
        self.token_count = token_count;
        self
    }

    /// Set group ID (for nested groups)
    pub fn with_group_id(mut self, group_id: Option<String>) -> Self {
        self.group_id = group_id;
        self
    }

    /// Set nesting level
    pub fn with_nesting_level(mut self, nesting_level: usize) -> Self {
        self.nesting_level = nesting_level;
        self
    }

    /// Get byte length
    pub fn byte_len(&self) -> usize {
        self.end_byte.saturating_sub(self.start_byte)
    }

    /// Get line count
    pub fn line_count(&self) -> usize {
        self.end_line.saturating_sub(self.start_line)
    }

    /// Check if contains byte position
    pub fn contains_byte(&self, byte_pos: usize) -> bool {
        self.start_byte <= byte_pos && byte_pos < self.end_byte
    }

    /// Check if this is a nested group chunk
    pub fn is_nested_group(&self) -> bool {
        self.group_id.is_some()
    }
}

impl ChunkSegment {
    /// Create new chunk segment from boundary and text
    pub fn new(boundary: ChunkBoundary, text: String) -> Self {
        Self { boundary, text }
    }
}

/// NL-text-relative entity boundary for member splitting.
///
/// Maps an entity to its byte range within the **combined NL text** produced by
/// `smart_chunk_with_header` (header + members concatenated with "\n\n").
#[derive(Debug, Clone)]
pub struct NlEntityBoundary {
    pub entity_id: EntityId,
    pub start_byte: usize,
    pub end_byte: usize,
}

/// Entity IDs whose NL text range intersects `[start_byte, end_byte)`.
///
/// Used to attribute entity ownership to fragments produced by coarse
/// strategies (sentences/lines/tokens) that carry no entity data of their own.
/// Empty when the fragment matches no entity, which callers may use as the
/// signal to fall back to the whole-group range.
pub fn intersect_entities_in_range(
    nl_boundaries: &[NlEntityBoundary],
    start_byte: usize,
    end_byte: usize,
) -> Vec<EntityId> {
    nl_boundaries
        .iter()
        .filter(|b| b.start_byte < end_byte && b.end_byte > start_byte)
        .map(|b| b.entity_id)
        .collect()
}

/// Compute NL-text-relative entity boundaries for a header + member sequence.
pub fn compute_nl_entity_boundaries(
    header_text: &str,
    header_entity_id: Option<EntityId>,
    members: &[ConversionResult],
    path: ChunkPath,
) -> Vec<NlEntityBoundary> {
    let mut boundaries = Vec::new();
    let mut current_offset = 0usize;

    if !header_text.is_empty() {
        let header_end = header_text.len();
        if let Some(id) = header_entity_id {
            boundaries.push(NlEntityBoundary {
                entity_id: id,
                start_byte: 0,
                end_byte: header_end,
            });
        }
        current_offset = header_end;
    }

    for member in members {
        let member_text = match path {
            ChunkPath::Bm25 => member.bm25_text.as_deref().unwrap_or(""),
            ChunkPath::Embedding => member.embedding_text.as_deref().unwrap_or(""),
        };

        if member_text.is_empty() {
            continue;
        }

        if current_offset > 0 {
            current_offset += 2;
        }

        let member_start = current_offset;
        let member_end = current_offset + member_text.len();

        boundaries.push(NlEntityBoundary {
            entity_id: member.entity_id,
            start_byte: member_start,
            end_byte: member_end,
        });

        current_offset = member_end;
    }

    boundaries
}

/// Locate group entities (header + members) by name inside NL text.
///
/// Entity spans from `entity_spans` contain source-code byte offsets, which
/// cannot be used directly to slice into natural-language text. This function
/// computes NL-text byte offsets by searching for entity name occurrences.
///
/// Used by `split_by_members` and by the all-in-one chunking path to provide
/// member boundary data for groups whose members were not converted separately.
pub fn locate_entities_in_nl_text(text: &str, group: &EntityGroup) -> Vec<NlEntityBoundary> {
    use cce_utils::text::split_camel_case;

    let mut boundaries: Vec<NlEntityBoundary> = Vec::new();

    let all_entities: Vec<(EntityId, &str)> = group
        .header
        .as_ref()
        .map(|h| (h.id, h.name.as_str()))
        .into_iter()
        .chain(group.members.iter().map(|m| (m.id, m.name.as_str())))
        .collect();

    let lower_text = text.to_lowercase();

    for (id, name) in &all_entities {
        let lower_name = name.to_lowercase();
        // Try exact match first
        if let Some(pos) = lower_text.find(&lower_name) {
            boundaries.push(NlEntityBoundary {
                entity_id: *id,
                start_byte: pos,
                end_byte: pos + name.len(),
            });
            continue;
        }
        // Try semantic name (camelCase split)
        let semantic = split_camel_case(name);
        let lower_semantic = semantic.to_lowercase();
        if !lower_semantic.is_empty() && lower_semantic != lower_name {
            if let Some(pos) = lower_text.find(&lower_semantic) {
                boundaries.push(NlEntityBoundary {
                    entity_id: *id,
                    start_byte: pos,
                    end_byte: pos + semantic.len(),
                });
                continue;
            }
        }
        // Try just the last part after ::
        if let Some(last) = name.rsplit("::").next() {
            if last != *name {
                let lower_last = last.to_lowercase();
                if let Some(pos) = lower_text.find(&lower_last) {
                    boundaries.push(NlEntityBoundary {
                        entity_id: *id,
                        start_byte: pos,
                        end_byte: pos + last.len(),
                    });
                    continue;
                }
            }
        }
    }

    boundaries.sort_by_key(|b| (b.start_byte, b.entity_id));
    // Deduplicate by position (same-start entities merge)
    boundaries.dedup_by_key(|b| b.start_byte);

    boundaries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_boundary_creation() {
        let boundary = ChunkBoundary::new(0, 100, SplitReason::NotSplit)
            .with_lines(0, 10)
            .with_token_count(50);

        assert_eq!(boundary.start_byte, 0);
        assert_eq!(boundary.end_byte, 100);
        assert_eq!(boundary.byte_len(), 100);
        assert_eq!(boundary.line_count(), 10);
        assert!(boundary.contains_byte(50));
        assert!(!boundary.contains_byte(100));
    }

    #[test]
    fn test_split_reason_display() {
        assert_eq!(
            format!("{}", SplitReason::MemberBoundary),
            "member_boundary"
        );
        assert_eq!(format!("{}", SplitReason::NotSplit), "not_split");
    }

    #[test]
    fn test_compute_nl_entity_boundaries_header_only() {
        use cce_types::entity::EntityId;

        let header_text = "class description";
        let members = vec![];

        let boundaries =
            compute_nl_entity_boundaries(header_text, Some(EntityId(1)), &members, ChunkPath::Bm25);

        assert_eq!(boundaries.len(), 1);
        assert_eq!(boundaries[0].entity_id, EntityId(1));
        assert_eq!(boundaries[0].start_byte, 0);
        assert_eq!(boundaries[0].end_byte, header_text.len());
    }

    #[test]
    fn test_compute_nl_entity_boundaries_header_and_members() {
        use cce_types::entity::EntityId;

        let header_text = "class description";
        let member1 = ConversionResult {
            entity_id: EntityId(2),
            bm25_text: Some("method1 description".to_string()),
            embedding_text: Some("method1 emb".to_string()),
            ..Default::default()
        };
        let member2 = ConversionResult {
            entity_id: EntityId(3),
            bm25_text: Some("method2 description".to_string()),
            embedding_text: Some("method2 emb".to_string()),
            ..Default::default()
        };

        let boundaries = compute_nl_entity_boundaries(
            header_text,
            Some(EntityId(1)),
            &[member1, member2],
            ChunkPath::Bm25,
        );

        assert_eq!(boundaries.len(), 3);
        assert_eq!(boundaries[0].entity_id, EntityId(1));
        assert_eq!(boundaries[0].start_byte, 0);
        let header_end = header_text.len();
        assert_eq!(boundaries[0].end_byte, header_end);
        assert_eq!(boundaries[1].entity_id, EntityId(2));
        assert_eq!(boundaries[1].start_byte, header_end + 2);
        let m1_end = boundaries[1].end_byte;
        assert_eq!(boundaries[2].entity_id, EntityId(3));
        assert_eq!(boundaries[2].start_byte, m1_end + 2);
    }

    #[test]
    fn test_compute_nl_entity_boundaries_no_header() {
        let boundaries = compute_nl_entity_boundaries("", None, &[], ChunkPath::Bm25);
        assert!(boundaries.is_empty());
    }
}
