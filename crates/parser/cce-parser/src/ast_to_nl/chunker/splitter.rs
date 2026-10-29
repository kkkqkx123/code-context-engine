use std::ops::Range;

use cce_config::modules::ChunkBudget;
use cce_config::modules::ChunkingConfig;
use cce_types::ParseError;
use cce_types::entity::EntityId;
use cce_utils::token_estimation::TokenEstimator;

use crate::grouper::EntityGroup;

use super::boundary::{
    ChunkBoundary, ChunkSegment, NlEntityBoundary, can_merge_with_ceiling, cost,
};
use super::result::ChunkPath;
use super::strategies;
use super::strategy::SplitStrategy;

/// Boundary-source level of the data-driven split recursion.
///
/// Finest first. `split_range` descends one level at a time while a level
/// has no boundary data in the interval, and sub-divides oversized pieces at
/// the level that best expresses the piece's internal structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum BoundaryLevel {
    /// Nested-group spans.
    NestedGroups,
    /// Entity spans (member boundaries).
    Entities,
    /// Sentence-end positions.
    Sentences,
    /// Blank-line positions.
    Paragraphs,
    /// Hard token/word cut (`split_by_tokens`), the guaranteed terminal.
    Tokens,
}

impl BoundaryLevel {
    /// Next coarser boundary source for the data-driven whole-interval descent.
    fn next(self, path: ChunkPath) -> Self {
        if path == ChunkPath::Bm25 {
            return match self {
                Self::NestedGroups => Self::Entities,
                Self::Entities | Self::Sentences => Self::Paragraphs,
                Self::Paragraphs | Self::Tokens => Self::Tokens,
            };
        }
        match self {
            Self::NestedGroups => Self::Entities,
            // An entity is the finest unit that may own a chunk, so an
            // oversized entity re-splits at the sentence ends inside it and
            // only an oversized sentence falls through to the hard cut.
            Self::Entities => Self::Sentences,
            Self::Sentences | Self::Paragraphs => Self::Tokens,
            Self::Tokens => Self::Tokens,
        }
    }

    /// Boundary source that best sub-divides an oversized piece produced by
    /// `self`: sentence ends inside entity/nested-group text on the embedding
    /// path and paragraph ends on the BM25 path, the hard token cut for
    /// everything finer.
    fn subdivide(self, path: ChunkPath) -> Self {
        if path == ChunkPath::Bm25 {
            return match self {
                Self::NestedGroups | Self::Entities => Self::Paragraphs,
                Self::Sentences | Self::Paragraphs | Self::Tokens => Self::Tokens,
            };
        }
        match self {
            Self::NestedGroups | Self::Entities => Self::Sentences,
            Self::Sentences | Self::Paragraphs | Self::Tokens => Self::Tokens,
        }
    }

    fn from_strategy(strategy: SplitStrategy) -> Self {
        match strategy {
            SplitStrategy::ByNestedGroups => Self::NestedGroups,
            SplitStrategy::ByNlEntityBoundaries => Self::Entities,
            SplitStrategy::ByParagraphs => Self::Paragraphs,
        }
    }
}

/// Text splitter
pub struct TextSplitter {
    config: ChunkingConfig,
    estimator: TokenEstimator,
    emb_budget: ChunkBudget,
    bm25_budget: ChunkBudget,
}

impl TextSplitter {
    /// Create new text splitter
    pub fn new(config: ChunkingConfig) -> Self {
        let emb_budget = config.budget(cce_types::ChunkPath::Embedding, false, 0);
        let bm25_budget = config.budget(cce_types::ChunkPath::Bm25, false, 0);
        Self {
            config,
            estimator: TokenEstimator::default(),
            emb_budget,
            bm25_budget,
        }
    }

    pub fn with_budgets(
        config: ChunkingConfig,
        emb_budget: ChunkBudget,
        bm25_budget: ChunkBudget,
    ) -> Self {
        Self {
            config,
            estimator: TokenEstimator::default(),
            emb_budget,
            bm25_budget,
        }
    }

    fn budget(&self, path: ChunkPath) -> ChunkBudget {
        match path {
            ChunkPath::Embedding => self.emb_budget,
            ChunkPath::Bm25 => self.bm25_budget,
        }
    }

    fn effective_config(&self, path: ChunkPath) -> ChunkingConfig {
        let mut c = self.config.clone();
        let b = self.budget(path);
        match path {
            ChunkPath::Embedding => {
                c.max_tokens = b.body_limit;
                c.min_chunk_tokens = b.min_limit;
                c.cross_group_merge_threshold = b.merge_limit;
            }
            ChunkPath::Bm25 => {
                c.max_bm25_terms = b.body_limit;
                c.min_chunk_bm25_terms = b.min_limit;
            }
        }
        c
    }

    fn merge_ceiling(&self, path: ChunkPath) -> (usize, usize) {
        let budget = self.budget(path);
        (budget.min_limit, budget.merge_limit)
    }

    pub fn split(
        &self,
        text: &str,
        group: &EntityGroup,
        strategy: SplitStrategy,
        path: ChunkPath,
    ) -> Result<Vec<ChunkSegment>, ParseError> {
        self.split_with_boundaries(text, group, strategy, path, None)
    }

    pub fn split_with_nl_boundaries(
        &self,
        text: &str,
        group: &EntityGroup,
        strategy: SplitStrategy,
        path: ChunkPath,
        nl_boundaries: &[NlEntityBoundary],
    ) -> Result<Vec<ChunkSegment>, ParseError> {
        self.split_with_boundaries(text, group, strategy, path, Some(nl_boundaries))
    }

    fn split_with_boundaries(
        &self,
        text: &str,
        group: &EntityGroup,
        strategy: SplitStrategy,
        path: ChunkPath,
        nl_boundaries: Option<&[NlEntityBoundary]>,
    ) -> Result<Vec<ChunkSegment>, ParseError> {
        let boundaries =
            self.try_split_range(text, group, strategy, path, nl_boundaries, 0..text.len())?;
        let segments = self.boundaries_to_segments(boundaries, text)?;
        Ok(self.maybe_merge_segments(segments, path))
    }

    /// Split `text[range]` by a single data-driven recursion over boundary
    /// sources: the strategy's level is tried first, the recursion descends
    /// to coarser sources when a level has no boundary data, and oversized
    /// pieces are sub-divided recursively. `split_by_tokens` is the
    /// guaranteed-to-succeed terminal.
    pub(crate) fn try_split_range(
        &self,
        text: &str,
        group: &EntityGroup,
        strategy: SplitStrategy,
        path: ChunkPath,
        nl_boundaries: Option<&[NlEntityBoundary]>,
        range: Range<usize>,
    ) -> Result<Vec<ChunkBoundary>, ParseError> {
        validate_range(text, &range)?;
        let boundaries = self.split_range_at(
            text,
            group,
            path,
            nl_boundaries,
            range.clone(),
            BoundaryLevel::from_strategy(strategy),
            &[],
        )?;
        validate_partition(&boundaries, text, range)?;
        Ok(boundaries)
    }

    /// Test-only convenience wrapper around [`Self::try_split_range`].
    #[cfg(test)]
    fn split_range(
        &self,
        text: &str,
        group: &EntityGroup,
        strategy: SplitStrategy,
        path: ChunkPath,
        nl_boundaries: Option<&[NlEntityBoundary]>,
        range: Range<usize>,
    ) -> Vec<ChunkBoundary> {
        self.try_split_range(text, group, strategy, path, nl_boundaries, range)
            .expect("test splitter input must be valid")
    }

    #[allow(clippy::too_many_arguments)]
    fn split_range_at(
        &self,
        text: &str,
        group: &EntityGroup,
        path: ChunkPath,
        nl_boundaries: Option<&[NlEntityBoundary]>,
        range: Range<usize>,
        level: BoundaryLevel,
        inherit_entity_ids: &[EntityId],
    ) -> Result<Vec<ChunkBoundary>, ParseError> {
        if range.start > range.end
            || range.end > text.len()
            || !text.is_char_boundary(range.start)
            || !text.is_char_boundary(range.end)
        {
            return Err(ParseError::code_splitting(format!(
                "splitter received an invalid range [{}, {}) for text of length {}",
                range.start,
                range.end,
                text.len()
            )));
        }

        if level == BoundaryLevel::Tokens {
            let cfg = self.effective_config(path);
            let mut boundaries = strategies::tokens::split_by_tokens_in_range(
                text,
                range,
                path,
                &cfg,
                &self.estimator,
            );
            if !inherit_entity_ids.is_empty() {
                for b in &mut boundaries {
                    if b.entity_ids.is_empty() {
                        b.entity_ids = inherit_entity_ids.to_vec();
                    }
                }
            }
            return Ok(boundaries);
        }

        let boundaries = self.partition_at(level, text, group, path, nl_boundaries, range.clone());

        // No boundary data at this level → descend to the next coarser
        // boundary source. A single boundary is data, not absence: it carries
        // the entity ids of the whole range and must survive when the piece
        // fits. Only an oversized single piece needs a deeper level.
        if boundaries.is_empty() {
            return self.split_range_at(
                text,
                group,
                path,
                nl_boundaries,
                range,
                level.next(path),
                inherit_entity_ids,
            );
        }
        if boundaries.len() == 1 {
            let Some(mut boundary) = boundaries.into_iter().next() else {
                return Ok(Vec::new());
            };
            validate_boundary(&boundary, &range, text)?;
            if boundary.start_byte != range.start || boundary.end_byte != range.end {
                return Err(ParseError::code_splitting(format!(
                    "single boundary [{}, {}) does not cover the split range [{}, {})",
                    boundary.start_byte, boundary.end_byte, range.start, range.end
                )));
            }
            if boundary.entity_ids.is_empty() {
                boundary.entity_ids = inherit_entity_ids.to_vec();
            }
            let piece_ids = boundary.entity_ids.clone();
            let budget = self.budget(path);
            let piece_cost = self
                .config
                .cost(&text[boundary.start_byte..boundary.end_byte], path);
            // A single piece that fits the pre-injection budget is kept whole:
            // it absorbs decoration without crossing the hard limit, and
            // splitting it only strands a tiny tail fragment next to a full
            // block. The output stays one segment, so no continuation
            // identity is appended on top of it.
            if budget.body_limit == 0 || piece_cost <= budget.pre_injection_limit {
                return Ok(vec![boundary]);
            }
            return self.split_range_at(
                text,
                group,
                path,
                nl_boundaries,
                boundary.start_byte..boundary.end_byte,
                level.subdivide(path),
                &piece_ids,
            );
        }

        let budget = self.budget(path);
        let mut result = Vec::with_capacity(boundaries.len());
        for mut b in boundaries {
            validate_boundary(&b, &range, text)?;
            if b.entity_ids.is_empty() {
                b.entity_ids = inherit_entity_ids.to_vec();
            }
            let piece_ids = b.entity_ids.clone();

            if budget.body_limit > 0
                && self.config.cost(&text[b.start_byte..b.end_byte], path) > budget.body_limit
            {
                result.extend(self.split_range_at(
                    text,
                    group,
                    path,
                    nl_boundaries,
                    b.start_byte..b.end_byte,
                    level.subdivide(path),
                    &piece_ids,
                )?);
            } else {
                result.push(b);
            }
        }
        Ok(result)
    }

    /// Partition `text[range]` at `level`'s boundary source (pure strategies).
    fn partition_at(
        &self,
        level: BoundaryLevel,
        text: &str,
        group: &EntityGroup,
        path: ChunkPath,
        nl_boundaries: Option<&[NlEntityBoundary]>,
        range: Range<usize>,
    ) -> Vec<ChunkBoundary> {
        let cfg = self.effective_config(path);
        match level {
            BoundaryLevel::NestedGroups => {
                strategies::nested_groups::split_by_nested_groups(text, group, path, &cfg, range)
            }
            BoundaryLevel::Entities => {
                // Entity boundaries come from the conversion layer, which
                // knows which text each entity contributed. Without those
                // intervals the level has no data and the recursion descends
                // to a coarser source.
                match nl_boundaries {
                    Some(nl) => strategies::members::split_by_members_with_nl(
                        text, group, path, &cfg, nl, range,
                    ),
                    None => Vec::new(),
                }
            }
            BoundaryLevel::Sentences => {
                strategies::sentences::split_text_by_sentences(text, range, path, &cfg)
            }
            BoundaryLevel::Paragraphs => {
                strategies::paragraphs::split_text_by_paragraphs(text, range, path, &cfg)
            }
            BoundaryLevel::Tokens => unreachable!("tokens is the terminal, not a partition level"),
        }
    }

    fn maybe_merge_segments(
        &self,
        segments: Vec<ChunkSegment>,
        path: ChunkPath,
    ) -> Vec<ChunkSegment> {
        if self.config.respect_boundaries {
            self.merge_small_segments(segments, path)
        } else {
            segments
        }
    }

    /// Convert a complete, contiguous boundary partition into segments.
    ///
    /// Invariants (monotonic + contiguous, non-zero length, full coverage,
    /// char-aligned) are asserted rather than silently repaired: a violation
    /// is a strategy bug and must never silently drop content.
    fn boundaries_to_segments(
        &self,
        boundaries: Vec<ChunkBoundary>,
        text: &str,
    ) -> Result<Vec<ChunkSegment>, ParseError> {
        let mut segments = Vec::with_capacity(boundaries.len());
        let mut prev_end = 0usize;
        for boundary in boundaries {
            if boundary.start_byte != prev_end {
                return Err(ParseError::code_splitting(format!(
                    "boundary gap/overlap: previous end {} vs start {}",
                    prev_end, boundary.start_byte
                )));
            }
            if boundary.start_byte >= boundary.end_byte
                || boundary.end_byte > text.len()
                || !text.is_char_boundary(boundary.start_byte)
                || !text.is_char_boundary(boundary.end_byte)
            {
                return Err(ParseError::code_splitting(format!(
                    "invalid boundary range [{}, {}) for text of length {}",
                    boundary.start_byte,
                    boundary.end_byte,
                    text.len()
                )));
            }
            let segment_text = text[boundary.start_byte..boundary.end_byte].to_string();
            prev_end = boundary.end_byte;
            segments.push(ChunkSegment::new(boundary, segment_text));
        }
        if prev_end != text.len() {
            return Err(ParseError::code_splitting(format!(
                "boundaries do not cover the full text (covered up to {}, length {})",
                prev_end,
                text.len()
            )));
        }
        Ok(segments)
    }

    /// Merge adjacent small segments whose combined cost fits within the
    /// ceiling for `path`.
    ///
    /// Both merge layers (this intra-splitter pass and the cross-group
    /// pass in `merge::merge_small_chunks_cross_group`) share the same
    /// limits from `boundary::merge_limits`: a cross-entity merge happens
    /// when either side is still below the path's min threshold and the
    /// combined cost stays within the path's merge ceiling. Fragments of
    /// one entity are instead packed up to the ceiling, so a long method
    /// does not stay fragmented at the min threshold. A single
    /// left-to-right greedy pass fully determines the result.
    fn merge_small_segments(
        &self,
        segments: Vec<ChunkSegment>,
        path: ChunkPath,
    ) -> Vec<ChunkSegment> {
        let (min_threshold, ceiling) = self.merge_ceiling(path);
        let mut merged: Vec<ChunkSegment> = Vec::with_capacity(segments.len());
        for segment in segments {
            if let Some(last) = merged.last_mut() {
                // Fragments of one entity are packed back together up to the
                // ceiling; pieces from different entities only merge when one
                // of them is still undersized.
                let same_unit = !last.boundary.entity_ids.is_empty()
                    && last.boundary.entity_ids == segment.boundary.entity_ids;
                let mergeable = can_merge_with_ceiling(
                    &last.text,
                    &segment.text,
                    path,
                    min_threshold,
                    ceiling,
                    same_unit,
                );
                if mergeable {
                    last.text.push_str(&segment.text);
                    last.boundary.end_byte = segment.boundary.end_byte;
                    last.boundary.end_line = segment.boundary.end_line;
                    last.boundary.token_count = cost(&last.text, path);
                    last.boundary.entity_ids.extend(segment.boundary.entity_ids);
                    continue;
                }
            }
            merged.push(segment);
        }
        merged
    }
}

/// Validate that `range` is a char-aligned sub-range of `text`.
pub(crate) fn validate_range(text: &str, range: &Range<usize>) -> Result<(), ParseError> {
    if range.start > range.end
        || range.end > text.len()
        || !text.is_char_boundary(range.start)
        || !text.is_char_boundary(range.end)
    {
        return Err(ParseError::code_splitting(format!(
            "invalid split range [{}, {}) for text of length {}",
            range.start,
            range.end,
            text.len()
        )));
    }
    Ok(())
}

/// Validate that `boundaries` form a complete, contiguous, char-aligned
/// partition of `text[range]`.
///
/// The splitter's contract is that no source byte is dropped or duplicated.
/// This check lives in the splitter (not only in tests) because assembly
/// slices the returned boundaries directly and a gap would silently lose
/// content.
pub(crate) fn validate_partition(
    boundaries: &[ChunkBoundary],
    text: &str,
    range: Range<usize>,
) -> Result<(), ParseError> {
    validate_range(text, &range)?;

    let mut prev_end = range.start;
    for boundary in boundaries {
        validate_boundary(boundary, &range, text)?;
        if boundary.start_byte != prev_end {
            return Err(ParseError::code_splitting(format!(
                "split partition gap/overlap: previous end {} vs start {}",
                prev_end, boundary.start_byte
            )));
        }
        prev_end = boundary.end_byte;
    }
    if prev_end != range.end {
        return Err(ParseError::code_splitting(format!(
            "split partition does not cover the range (covered up to {}, range end {})",
            prev_end, range.end
        )));
    }
    Ok(())
}

fn validate_boundary(
    boundary: &ChunkBoundary,
    range: &Range<usize>,
    text: &str,
) -> Result<(), ParseError> {
    if boundary.start_byte < range.start
        || boundary.end_byte > range.end
        || boundary.start_byte >= boundary.end_byte
        || boundary.end_byte > text.len()
        || !text.is_char_boundary(boundary.start_byte)
        || !text.is_char_boundary(boundary.end_byte)
    {
        return Err(ParseError::code_splitting(format!(
            "strategy boundary [{}, {}) escapes [{}, {}) or is not char-aligned",
            boundary.start_byte, boundary.end_byte, range.start, range.end
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use cce_config::modules::ChunkingConfig;
    use cce_types::entity::EntityId;

    use crate::grouper::types::EntityGroup;

    use super::super::boundary::{ChunkBoundary, SplitReason};
    use super::super::result::ChunkPath;
    use super::super::strategy::SplitStrategy;
    use super::*;

    fn split_with_limits(
        text: &str,
        level: BoundaryLevel,
        max_tokens: usize,
    ) -> Vec<ChunkBoundary> {
        let config = ChunkingConfig {
            max_tokens,
            ..Default::default()
        };
        let splitter = TextSplitter::new(config);
        splitter
            .split_range_at(
                text,
                &EntityGroup::default(),
                ChunkPath::Embedding,
                None,
                0..text.len(),
                level,
                &[],
            )
            .expect("test splitter input must be valid")
    }

    fn split_at_level(
        text: &str,
        level: BoundaryLevel,
        path: ChunkPath,
        config: ChunkingConfig,
    ) -> Vec<ChunkSegment> {
        let splitter = TextSplitter::new(config);
        let boundaries = splitter
            .split_range_at(
                text,
                &EntityGroup::default(),
                path,
                None,
                0..text.len(),
                level,
                &[],
            )
            .expect("test splitter input must be valid");
        let segments = splitter
            .boundaries_to_segments(boundaries, text)
            .expect("test partition must be valid");
        splitter.maybe_merge_segments(segments, path)
    }

    fn assert_partition(text: &str, boundaries: &[ChunkBoundary]) {
        assert!(!boundaries.is_empty(), "no boundaries produced");
        assert_eq!(boundaries.first().unwrap().start_byte, 0);
        assert_eq!(boundaries.last().unwrap().end_byte, text.len());
        let mut prev = 0;
        for b in boundaries {
            assert_eq!(b.start_byte, prev, "partition gap/overlap");
            assert!(b.start_byte < b.end_byte, "zero-length boundary");
            assert!(text.is_char_boundary(b.start_byte));
            assert!(text.is_char_boundary(b.end_byte));
            prev = b.end_byte;
        }
        assert_eq!(prev, text.len());
    }

    #[test]
    fn test_boundaries_to_segments_utf8_mid_char() {
        let config = ChunkingConfig::default();
        let splitter = TextSplitter::new(config);
        let text = "Hello 世界 World";
        let boundaries =
            vec![ChunkBoundary::new(0, 7, SplitReason::SentenceBoundary).with_token_count(2)];
        // A non-char-aligned boundary violates the splitter contract: it is
        // reported as an error in every build profile, never silently repaired.
        assert!(splitter.boundaries_to_segments(boundaries, text).is_err());
    }

    #[test]
    fn test_boundaries_to_segments_unicode_only() {
        let config = ChunkingConfig::default();
        let splitter = TextSplitter::new(config);
        let text = "世界你好";
        let boundaries =
            vec![ChunkBoundary::new(0, 12, SplitReason::SentenceBoundary).with_token_count(4)];
        let segments = splitter
            .boundaries_to_segments(boundaries, text)
            .expect("test partition must be valid");
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, text);
    }

    #[test]
    fn test_split_range_by_sentences_stops_at_tokens() {
        let text = "A B C D E F G H";
        let boundaries = split_with_limits(text, BoundaryLevel::Sentences, 3);
        assert!(boundaries.len() > 1);
        assert_partition(text, &boundaries);
    }

    #[test]
    fn test_split_range_full_chain_from_nested_groups() {
        let text = "no nested groups no members no sentence ends no newlines no paragraphs";
        let boundaries = split_with_limits(text, BoundaryLevel::NestedGroups, 3);
        assert!(boundaries.len() > 1);
        assert_partition(text, &boundaries);
    }

    #[test]
    fn test_split_range_full_chain_from_members() {
        let text = "no entity spans no sentences no lines no paras";
        let boundaries = split_with_limits(text, BoundaryLevel::Entities, 3);
        assert!(boundaries.len() > 1);
        assert_partition(text, &boundaries);
    }

    #[test]
    fn test_split_range_from_sentences_to_tokens() {
        let text = "word word word word word word word";
        let boundaries = split_with_limits(text, BoundaryLevel::Sentences, 3);
        assert!(boundaries.len() > 1);
        assert_partition(text, &boundaries);
    }

    #[test]
    fn test_split_range_with_text_within_other_limits() {
        let config = ChunkingConfig {
            max_tokens: 512,
            max_bm25_terms: 200,
            ..Default::default()
        };
        let splitter = TextSplitter::new(config);
        let text = "A B C D E F G H I J";
        let boundaries = splitter.split_range(
            text,
            &EntityGroup::default(),
            SplitStrategy::ByParagraphs,
            ChunkPath::Bm25,
            None,
            0..text.len(),
        );
        assert_eq!(boundaries.len(), 1);
        assert_eq!(boundaries[0].start_byte, 0);
        assert_eq!(boundaries[0].end_byte, text.len());
    }

    #[test]
    fn test_split_range_no_boundaries_with_nl_entity_boundaries() {
        let config = ChunkingConfig {
            max_tokens: 3,
            ..Default::default()
        };
        let splitter = TextSplitter::new(config);
        let text = "aaa bbb ccc ddd eee fff";
        let boundaries = splitter.split_range(
            text,
            &EntityGroup::default(),
            SplitStrategy::ByNlEntityBoundaries,
            ChunkPath::Embedding,
            Some(&[]),
            0..text.len(),
        );
        assert!(boundaries.len() > 1);
        assert_partition(text, &boundaries);
    }

    #[test]
    fn test_split_range_entity_boundaries_attributed_through_recursion() {
        let config = ChunkingConfig {
            max_bm25_terms: 2,
            ..Default::default()
        };
        let splitter = TextSplitter::new(config);
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
        let boundaries = splitter.split_range(
            text,
            &EntityGroup::default(),
            SplitStrategy::ByNlEntityBoundaries,
            ChunkPath::Bm25,
            Some(&nlb),
            0..text.len(),
        );
        assert!(boundaries.len() > 1);
        assert_partition(text, &boundaries);
        // Pieces overlapping an entity span inherit that entity's id through
        // the recursion (id 1 lives in the [0, 14) region).
        for b in boundaries.iter().filter(|b| b.end_byte <= 14) {
            assert!(
                b.entity_ids.contains(&EntityId(1)),
                "piece [{}, {}) lost inherited entity id",
                b.start_byte,
                b.end_byte
            );
        }
    }

    #[test]
    fn test_split_range_single_paragraph_text_by_paragraphs() {
        let config = ChunkingConfig {
            max_tokens: 512,
            ..Default::default()
        };
        let splitter = TextSplitter::new(config);
        let text = "Short text.";
        let boundaries = splitter.split_range(
            text,
            &EntityGroup::default(),
            SplitStrategy::ByParagraphs,
            ChunkPath::Embedding,
            None,
            0..text.len(),
        );
        assert_eq!(boundaries.len(), 1);
        assert_eq!(boundaries[0].start_byte, 0);
        assert_eq!(boundaries[0].end_byte, text.len());
    }

    #[test]
    fn test_split_range_empty_text() {
        let config = ChunkingConfig::default();
        let splitter = TextSplitter::new(config);
        let boundaries = splitter.split_range(
            "",
            &EntityGroup::default(),
            SplitStrategy::ByParagraphs,
            ChunkPath::Embedding,
            None,
            0..0,
        );
        assert!(boundaries.is_empty());
    }

    #[test]
    fn test_split_range_oversized_pieces_all_within_limit() {
        let config = ChunkingConfig {
            max_bm25_terms: 4,
            ..Default::default()
        };
        let splitter = TextSplitter::new(config);
        let text = "one two three four five six seven eight nine ten";
        let boundaries = splitter
            .split_range_at(
                text,
                &EntityGroup::default(),
                ChunkPath::Bm25,
                None,
                0..text.len(),
                BoundaryLevel::Sentences,
                &[],
            )
            .expect("test splitter input must be valid");
        assert!(boundaries.len() >= 2);
        assert_partition(text, &boundaries);
        for b in &boundaries {
            assert!(
                text[b.start_byte..b.end_byte].split_whitespace().count() <= 4,
                "piece exceeds limit"
            );
        }
    }

    #[test]
    fn test_split_range_oversized_paragraph_subdivides_by_hard_cut_on_bm25() {
        let config = ChunkingConfig {
            max_bm25_terms: 3,
            ..Default::default()
        };
        let splitter = TextSplitter::new(config);
        // The BM25 path subdivides oversized paragraphs at line boundaries
        // before the hard token cut so method signatures stay intact; only
        // line-oversize pieces use the hard cut.
        let text = "word1 word2 word3 word4 word5\nword6 word7\n\nword8 word9 word10 word11 word12\nword13 word14";
        let boundaries = splitter.split_range(
            text,
            &EntityGroup::default(),
            SplitStrategy::ByParagraphs,
            ChunkPath::Bm25,
            None,
            0..text.len(),
        );
        assert!(boundaries.len() >= 4);
        assert_partition(text, &boundaries);
        assert!(
            boundaries
                .iter()
                .all(|b| b.split_reason != SplitReason::SentenceBoundary),
            "BM25 must not subdivide into sentences"
        );
        for b in &boundaries {
            let words = text[b.start_byte..b.end_byte].split_whitespace().count();
            assert!(words <= 3, "piece of {} words exceeds limit", words);
        }
    }

    #[test]
    fn test_split_range_unicode_text_no_zero_length() {
        let config = ChunkingConfig {
            max_tokens: 2,
            ..Default::default()
        };
        let splitter = TextSplitter::new(config);
        let text = "这是一段用于测试分块器不变量保证的中文文本，包含一些标点符号。以及换行\n测试。";
        let boundaries = splitter
            .split_range_at(
                text,
                &EntityGroup::default(),
                ChunkPath::Embedding,
                None,
                0..text.len(),
                BoundaryLevel::Sentences,
                &[],
            )
            .expect("test splitter input must be valid");
        assert!(boundaries.len() >= 2);
        assert_partition(text, &boundaries);
    }

    /// Deterministic pseudo-random text generator over a mixed alphabet
    /// (ASCII words, no-punctuation long lines, blank lines, CJK, pure
    /// whitespace) used to exercise the split invariants.
    struct Lcg(u64);

    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            self.0 >> 33
        }

        fn range(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }

        fn gen_text(&mut self, max_len: usize) -> String {
            let alphabets: &[&str] = &[
                "word1 word2 word3 word4 word5 word6 word7 word8 word9 word10 ",
                "a",
                "b ",
                "\n",
                "\n\n",
                "x.y z! q? r。 s。 ",
                "句段一。句段二。句段三。",
                "   ",
                "\t",
                "no_punctuation_long_line_without_any_spaces_abcdefghijklmnopqrstuvwxyz ",
                "λ=1, δ=2; Ω! ",
            ];
            let len = self.range(max_len) + 1;
            let mut s = String::new();
            while s.len() < len {
                s.push_str(alphabets[self.range(alphabets.len())]);
            }
            cce_utils::truncate_string_to_byte_len(&mut s, len);
            s
        }
    }

    #[test]
    fn test_split_range_random_text_invariants() {
        let levels = [
            BoundaryLevel::NestedGroups,
            BoundaryLevel::Entities,
            BoundaryLevel::Sentences,
            BoundaryLevel::Paragraphs,
            BoundaryLevel::Tokens,
        ];
        let mut rng = Lcg(0x5eed_c0de_cafe_f00d);
        for round in 0..64 {
            let text = rng.gen_text(600);
            let path = if round % 2 == 0 {
                ChunkPath::Bm25
            } else {
                ChunkPath::Embedding
            };
            let limit = 3 + rng.range(10);
            let cfg = match path {
                ChunkPath::Bm25 => ChunkingConfig {
                    max_bm25_terms: limit,
                    ..Default::default()
                },
                ChunkPath::Embedding => ChunkingConfig {
                    max_tokens: limit,
                    ..Default::default()
                },
            };
            let level = levels[rng.range(levels.len())];
            let splitter = TextSplitter::new(cfg);
            let boundaries = splitter
                .split_range_at(
                    &text,
                    &EntityGroup::default(),
                    path,
                    None,
                    0..text.len(),
                    level,
                    &[],
                )
                .expect("test splitter input must be valid");

            // Invariants: monotonic + contiguous, non-zero, full coverage,
            // char-aligned, and every piece within the path limit.
            assert!(!boundaries.is_empty(), "round {round}: empty partition");
            let mut prev = 0;
            for b in &boundaries {
                assert_eq!(
                    b.start_byte, prev,
                    "round {round}: level={level:?} path={path:?} partition gap/overlap"
                );
                assert!(b.start_byte < b.end_byte, "round {round}: zero-length");
                assert!(text.is_char_boundary(b.start_byte));
                assert!(text.is_char_boundary(b.end_byte));
                let piece = &text[b.start_byte..b.end_byte];
                if !text.trim().is_empty() && !piece.trim().is_empty() {
                    let piece_cost = cost(piece, path);
                    assert!(
                        piece_cost <= limit,
                        "round {round}: level={level:?} path={path:?} text={text:?} piece of {piece_cost} exceeds limit {limit}: {:?}",
                        piece
                    );
                }
                prev = b.end_byte;
            }
            assert_eq!(prev, text.len(), "round {round}: coverage");
        }
    }

    #[test]
    fn test_single_piece_between_body_and_pre_injection_kept_whole() {
        let config = ChunkingConfig {
            max_bm25_terms: 150,
            ..Default::default()
        };
        // A real group with vocabulary and decoration shrinks the body limit
        // below the pre-injection limit; reproduce that shape directly.
        let mut bm25_budget = config.budget(ChunkPath::Bm25, false, 0);
        bm25_budget.body_limit = 50;
        let emb_budget = config.budget(ChunkPath::Embedding, false, 0);
        let splitter = TextSplitter::with_budgets(config, emb_budget, bm25_budget);
        let text = words(140);
        let boundaries = splitter
            .split_range_at(
                &text,
                &EntityGroup::default(),
                ChunkPath::Bm25,
                None,
                0..text.len(),
                BoundaryLevel::Paragraphs,
                &[],
            )
            .expect("test splitter input must be valid");
        // 50 < 140 <= 150: above the body limit but within the pre-injection
        // budget, so the single entity piece is kept whole instead of being
        // cut into a full block plus a stranded tail.
        assert_eq!(boundaries.len(), 1, "over-body piece should be kept whole");
        assert_eq!(boundaries[0].start_byte, 0);
        assert_eq!(boundaries[0].end_byte, text.len());
    }

    #[test]
    fn test_single_piece_over_pre_injection_subdivided() {
        let config = ChunkingConfig {
            max_bm25_terms: 150,
            ..Default::default()
        };
        let mut bm25_budget = config.budget(ChunkPath::Bm25, false, 0);
        bm25_budget.body_limit = 50;
        let emb_budget = config.budget(ChunkPath::Embedding, false, 0);
        let splitter = TextSplitter::with_budgets(config, emb_budget, bm25_budget);
        let text = words(200);
        let boundaries = splitter
            .split_range_at(
                &text,
                &EntityGroup::default(),
                ChunkPath::Bm25,
                None,
                0..text.len(),
                BoundaryLevel::Paragraphs,
                &[],
            )
            .expect("test splitter input must be valid");
        // 200 > 150: beyond the pre-injection limit, subdivision still runs
        // and every piece is cut to the body limit.
        assert!(boundaries.len() >= 2, "over-pre piece must be subdivided");
        assert_partition(&text, &boundaries);
        for b in &boundaries {
            assert!(
                text[b.start_byte..b.end_byte].split_whitespace().count() <= 50,
                "piece exceeds the body limit"
            );
        }
    }

    #[test]
    fn test_entity_subdivision_routes_bm25_to_paragraphs_embedding_to_sentences() {
        let text = "alpha beta gamma. delta epsilon zeta.\n\neta theta iota kappa.";
        let nlb = vec![NlEntityBoundary {
            entity_id: EntityId(1),
            start_byte: 0,
            end_byte: text.len(),
        }];

        let bm25_config = ChunkingConfig {
            max_bm25_terms: 5,
            ..Default::default()
        };
        let bm25_splitter = TextSplitter::new(bm25_config);
        let boundaries = bm25_splitter
            .split_range_at(
                text,
                &EntityGroup::default(),
                ChunkPath::Bm25,
                Some(&nlb),
                0..text.len(),
                BoundaryLevel::Entities,
                &[],
            )
            .expect("test splitter input must be valid");
        assert!(boundaries.len() >= 2);
        assert_partition(text, &boundaries);
        for b in &boundaries {
            assert_ne!(
                b.split_reason,
                SplitReason::SentenceBoundary,
                "BM25 subdivision must skip the sentence level"
            );
        }

        let emb_config = ChunkingConfig {
            max_tokens: 10,
            ..Default::default()
        };
        let emb_splitter = TextSplitter::new(emb_config);
        let boundaries = emb_splitter
            .split_range_at(
                text,
                &EntityGroup::default(),
                ChunkPath::Embedding,
                Some(&nlb),
                0..text.len(),
                BoundaryLevel::Entities,
                &[],
            )
            .expect("test splitter input must be valid");
        assert!(boundaries.len() >= 2);
        assert_partition(text, &boundaries);
        assert!(
            boundaries
                .iter()
                .any(|b| b.split_reason == SplitReason::SentenceBoundary),
            "embedding subdivision must still use sentence boundaries"
        );
    }

    fn words(n: usize) -> String {
        let mut s = (0..n)
            .map(|i| format!("w{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        s.push(' ');
        s
    }

    fn segment(text: &str, start: usize, end: usize) -> ChunkSegment {
        ChunkSegment::new(
            ChunkBoundary::new(start, end, SplitReason::MemberBoundary),
            text.to_string(),
        )
    }

    #[test]
    fn test_merge_small_chunks_both_below_threshold() {
        let config = ChunkingConfig {
            max_bm25_terms: 512,
            min_chunk_bm25_terms: 60,
            ..Default::default()
        };
        let splitter = TextSplitter::new(config);

        let segments = vec![segment(&words(30), 0, 60), segment(&words(30), 60, 120)];

        let merged = splitter.merge_small_segments(segments, ChunkPath::Bm25);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].boundary.token_count, 60);
    }

    #[test]
    fn test_merge_small_chunks_one_above_threshold() {
        let config = ChunkingConfig {
            max_bm25_terms: 512,
            min_chunk_bm25_terms: 60,
            ..Default::default()
        };
        let splitter = TextSplitter::new(config);

        // Segment with 50 words (below 60 threshold) should be merged
        // into the adjacent 150-word chunk.
        let segments = vec![segment(&words(50), 0, 100), segment(&words(150), 100, 300)];

        let merged = splitter.merge_small_segments(segments, ChunkPath::Bm25);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].boundary.token_count, 200);
    }

    #[test]
    fn test_merge_small_chunks_assimilates_small_fragments() {
        let config = ChunkingConfig {
            max_bm25_terms: 512,
            min_chunk_bm25_terms: 60,
            ..Default::default()
        };
        let splitter = TextSplitter::new(config);

        // [40w, 200w, 30w]: the leading 40w chunk is below min and absorbs the
        // 200w neighbor; the trailing 30w fragment is below min so it is
        // absorbed by its large left neighbor (bidirectional merge).
        let segments = vec![
            segment(&words(40), 0, 50),
            segment(&words(200), 50, 250),
            segment(&words(30), 250, 300),
        ];

        let merged = splitter.merge_small_segments(segments, ChunkPath::Bm25);

        assert_eq!(merged.len(), 1);
        assert_eq!(cost(&merged[0].text, ChunkPath::Bm25), 270);
    }

    #[test]
    fn test_merge_small_chunks_cascading() {
        let config = ChunkingConfig {
            max_bm25_terms: 512,
            min_chunk_bm25_terms: 60,
            ..Default::default()
        };
        let splitter = TextSplitter::new(config);

        // Bidirectional merge absorbs trailing small fragments as well,
        // so all three undersized pieces collapse into one chunk.
        let segments = vec![
            segment(&words(30), 0, 50),
            segment(&words(30), 50, 100),
            segment(&words(30), 100, 150),
        ];

        let merged = splitter.merge_small_segments(segments, ChunkPath::Bm25);

        assert_eq!(merged.len(), 1);
        assert_eq!(cost(&merged[0].text, ChunkPath::Bm25), 90);
    }

    #[test]
    fn test_merge_small_chunks_embedding_below_threshold() {
        let config = ChunkingConfig {
            max_tokens: 512,
            min_chunk_tokens: 10,
            ..Default::default()
        };
        let splitter = TextSplitter::new(config);
        let segments = vec![segment(&words(5), 0, 10), segment(&words(5), 10, 20)];
        let merged = splitter.merge_small_segments(segments, ChunkPath::Embedding);
        assert_eq!(merged.len(), 1);
    }

    #[test]
    fn test_merge_small_chunks_embedding_one_above() {
        let config = ChunkingConfig {
            max_tokens: 512,
            min_chunk_tokens: 50,
            ..Default::default()
        };
        let splitter = TextSplitter::new(config);
        let segments = vec![segment(&words(20), 0, 30), segment(&words(100), 30, 200)];
        let merged = splitter.merge_small_segments(segments, ChunkPath::Embedding);
        assert_eq!(merged.len(), 1);
    }

    #[test]
    fn test_same_unit_fragments_pack_to_the_ceiling() {
        let config = ChunkingConfig {
            max_tokens: 300,
            min_chunk_tokens: 100,
            glossary_reserve_ratio: 0.0,
            ..Default::default()
        };
        let splitter = TextSplitter::new(config);
        // Six fragments of one entity, each far above the min threshold:
        // they still pack together because they are parts of one unit.
        let segments: Vec<ChunkSegment> = (0..6)
            .map(|i| {
                ChunkSegment::new(
                    ChunkBoundary::new(i * 60, (i + 1) * 60, SplitReason::MemberBoundary)
                        .with_entity_ids(vec![EntityId(1)]),
                    words(60),
                )
            })
            .collect();

        let merged = splitter.merge_small_segments(segments, ChunkPath::Embedding);

        let piece = cost(&words(60), ChunkPath::Embedding);
        assert_eq!(merged.len(), 2);
        assert!(
            cost(&merged[0].text, ChunkPath::Embedding) > piece * 3,
            "one entity's fragments must pack together, got {} pieces",
            merged.len()
        );
        for segment in &merged {
            assert!(
                cost(&segment.text, ChunkPath::Embedding) <= 300,
                "a packed fragment must stay within the ceiling"
            );
        }
    }

    #[test]
    fn test_same_unit_fragments_never_exceed_the_ceiling() {
        let config = ChunkingConfig {
            max_tokens: 300,
            min_chunk_tokens: 100,
            glossary_reserve_ratio: 0.0,
            ..Default::default()
        };
        let splitter = TextSplitter::new(config);
        // Fragments of one entity still stop at the ceiling: merging them
        // would produce a chunk the path's limit rejects.
        let segments = vec![
            ChunkSegment::new(
                ChunkBoundary::new(0, 170, SplitReason::MemberBoundary)
                    .with_entity_ids(vec![EntityId(1)]),
                words(170),
            ),
            ChunkSegment::new(
                ChunkBoundary::new(170, 340, SplitReason::MemberBoundary)
                    .with_entity_ids(vec![EntityId(1)]),
                words(170),
            ),
        ];

        let merged = splitter.merge_small_segments(segments, ChunkPath::Embedding);

        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn test_merge_small_chunks_embedding_trailing() {
        let config = ChunkingConfig {
            max_tokens: 512,
            min_chunk_tokens: 50,
            ..Default::default()
        };
        let splitter = TextSplitter::new(config);
        // Bidirectional merge: the trailing small fragment is absorbed by
        // its large left neighbor.
        let segments = vec![segment(&words(100), 0, 150), segment(&words(20), 150, 180)];
        let merged = splitter.merge_small_segments(segments, ChunkPath::Embedding);
        assert_eq!(merged.len(), 1);
    }

    #[test]
    fn test_merge_small_chunks_respect_boundaries_disabled() {
        let config = ChunkingConfig {
            max_bm25_terms: 2,
            min_chunk_bm25_terms: 3,
            respect_boundaries: false,
            ..Default::default()
        };
        let text = "one two. three four. five six. seven eight.";
        let segments = split_at_level(text, BoundaryLevel::Sentences, ChunkPath::Bm25, config);
        assert!(segments.len() >= 3);
    }

    #[test]
    fn test_merge_small_chunks_applied_in_split() {
        let config = ChunkingConfig {
            max_bm25_terms: 512,
            ..Default::default()
        };
        let text = "one two. three four. five six. seven eight.";
        let segments = split_at_level(text, BoundaryLevel::Sentences, ChunkPath::Bm25, config);
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, text);
        assert_eq!(segments[0].boundary.token_count, 8);
    }

    #[test]
    fn test_merge_small_chunks_trailing() {
        let config = ChunkingConfig {
            max_bm25_terms: 512,
            min_chunk_bm25_terms: 60,
            ..Default::default()
        };
        let splitter = TextSplitter::new(config);

        // Bidirectional merge: a large leading chunk absorbs the trailing
        // small fragment when the combined cost fits the ceiling.
        let segments = vec![segment(&words(150), 0, 200), segment(&words(40), 200, 250)];

        let merged = splitter.merge_small_segments(segments, ChunkPath::Bm25);

        assert_eq!(merged.len(), 1);
        assert_eq!(cost(&merged[0].text, ChunkPath::Bm25), 190);
    }

    #[test]
    fn test_merge_small_chunks_rescues_undersized_leading_chunk() {
        let config = ChunkingConfig {
            max_bm25_terms: 512,
            min_chunk_bm25_terms: 60,
            ..Default::default()
        };
        let splitter = TextSplitter::new(config);

        // The leading chunk is below min, so it absorbs the (larger) neighbor:
        // a merge rescues the undersized chunk rather than being driven by
        // the neighbor's size.
        let segments = vec![segment(&words(40), 0, 50), segment(&words(200), 50, 250)];

        let merged = splitter.merge_small_segments(segments, ChunkPath::Bm25);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].boundary.token_count, 240);
    }
}
