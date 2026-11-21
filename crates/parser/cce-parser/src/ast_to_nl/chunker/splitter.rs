use std::ops::Range;

use cce_config::modules::ChunkBudget;
use cce_config::modules::ChunkingConfig;
use cce_types::ParseError;
use cce_types::entity::{EntityId, EntityKind};
use cce_utils::token_estimation::TokenEstimator;

use crate::grouper::EntityGroup;

use super::boundary::{
    ChunkBoundary, ChunkSegment, NlEntityBoundary, can_merge_with_ceiling, cost,
};
use super::chunk_builder::ChunkBuilder;
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

/// How many times the hard and pre-injection budgets a single-entity BM25
/// whole may exceed while staying whole and silent.
///
/// The keyword index has no neural ceiling, and splitting a function would
/// multiply index entries without shrinking the reported source span. A
/// fragment can also land on a blank-only line with no source coverage, so
/// the splitter keeps such a piece whole and the hard limit truncates an
/// over-bound whole to this extended budget instead. The bound covers the
/// observed long test functions while still capping pathological inputs.
pub(crate) const SINGLE_ENTITY_BM25_TOLERANCE_FACTOR: usize = 3;

/// Whether an entity is a module container.
///
/// Module sections stay independent prose and always subdivide at the body
/// budget.
fn is_module_entity(group: &EntityGroup, id: EntityId) -> bool {
    group
        .header
        .as_ref()
        .is_some_and(|header| header.id == id && header.kind == EntityKind::Module)
        || group
            .members
            .iter()
            .any(|member| member.id == id && member.kind == EntityKind::Module)
}

/// Whether two entity sets hold a direct call edge in either direction.
///
/// Adjacent cross-entity fragments whose entities call each other keep
/// the caller context the query needs together (for example a binder
/// method next to the body parser it feeds). The edge comes from the
/// call-path metadata the grouper already wrote, so the splitter only
/// reads. Name comparison uses the last path segment case-insensitively
/// so qualified targets still match their short entity names.
fn entities_call_related(group: &EntityGroup, first: &[EntityId], second: &[EntityId]) -> bool {
    fn names_of(group: &EntityGroup, ids: &[EntityId]) -> Vec<String> {
        let mut out = Vec::new();
        for id in ids {
            let name = group
                .header
                .as_ref()
                .filter(|h| h.id == *id)
                .map(|h| h.name.as_str())
                .or_else(|| {
                    group
                        .members
                        .iter()
                        .find(|m| m.id == *id)
                        .map(|m| m.name.as_str())
                });
            if let Some(name) = name {
                let short = name
                    .rsplit(['.', ':', '/', '#'])
                    .next()
                    .unwrap_or(name)
                    .trim()
                    .to_lowercase();
                if !short.is_empty() {
                    out.push(short);
                }
            }
        }
        out
    }
    fn targets_of(group: &EntityGroup, id: EntityId) -> Vec<String> {
        let raw = group
            .header
            .as_ref()
            .filter(|h| h.id == id)
            .and_then(|h| h.metadata.get(cce_types::entity::meta_keys::CALL_PATHS))
            .or_else(|| {
                group
                    .members
                    .iter()
                    .find(|m| m.id == id)
                    .and_then(|m| m.metadata.get(cce_types::entity::meta_keys::CALL_PATHS))
            });
        let Some(raw) = raw else { return Vec::new() };
        raw.split(',')
            .map(|part| {
                part.rsplit(['.', ':', '/', '#'])
                    .next()
                    .unwrap_or(part)
                    .trim()
                    .to_lowercase()
            })
            .filter(|s| !s.is_empty())
            .collect()
    }
    let first_names = names_of(group, first);
    let second_names = names_of(group, second);
    for id in first {
        let targets = targets_of(group, *id);
        if targets.iter().any(|t| second_names.contains(t)) {
            return true;
        }
    }
    for id in second {
        let targets = targets_of(group, *id);
        if targets.iter().any(|t| first_names.contains(t)) {
            return true;
        }
    }
    false
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
        Ok(self.maybe_merge_segments(group, segments, path))
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
            // Over-body pieces subdivide, except a single-entity BM25 piece
            // stays whole: splitting it would multiply index entries while
            // the reported source span stays whole, and a fragment can land
            // on a blank-only line with no source coverage. The hard limit
            // truncates such a whole to a bounded extended budget instead.
            if budget.body_limit == 0 || piece_cost <= budget.body_limit {
                return Ok(vec![boundary]);
            }
            let atomic = path == ChunkPath::Bm25
                && piece_ids.len() == 1
                && piece_ids
                    .first()
                    .is_some_and(|id| !is_module_entity(group, *id));
            if atomic {
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
            let piece_cost = self.config.cost(&text[b.start_byte..b.end_byte], path);
            let atomic = path == ChunkPath::Bm25
                && piece_ids.len() == 1
                && piece_ids
                    .first()
                    .is_some_and(|id| !is_module_entity(group, *id));
            if budget.body_limit > 0 && piece_cost > budget.body_limit && !atomic {
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
        group: &EntityGroup,
        segments: Vec<ChunkSegment>,
        path: ChunkPath,
    ) -> Vec<ChunkSegment> {
        if self.config.respect_boundaries {
            self.merge_small_segments(group, segments, path)
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
    /// ceiling for `path` (pre-injection for same-entity fragments).
    ///
    /// Both merge layers (this intra-splitter pass and the cross-group
    /// pass in `merge::merge_small_chunks_cross_group`) share the same
    /// limits from `boundary::merge_limits`: a cross-entity merge happens
    /// when either side is still below the path's min threshold and the
    /// combined cost stays within the path's merge ceiling. Fragments of
    /// one entity are instead packed up to the pre-injection limit, so a long method
    /// does not stay fragmented at the min threshold. A single
    /// left-to-right greedy pass fully determines the result.
    ///
    /// Test boundary: a production segment must never absorb (or be
    /// absorbed by) a test segment. Merging them would attribute test
    /// status to production content and hide it from test-filtered
    /// retrieval, the same invariant enforced by `TestInfo::can_merge` in
    /// the grouper small-fragment merger and the cross-group chunk merge.
    /// Fragments of one entity always share one marker, so the check
    /// never blocks the same-unit packing path.
    fn merge_small_segments(
        &self,
        group: &EntityGroup,
        segments: Vec<ChunkSegment>,
        path: ChunkPath,
    ) -> Vec<ChunkSegment> {
        let (min_threshold, ceiling) = self.merge_ceiling(path);
        let budget = self.budget(path);
        let same_unit_ceiling = budget.pre_injection_limit.max(ceiling);
        // Single-entity BM25 fragments may reunite up to the bounded whole
        // (three times the hard limit): a doc-only slice plus its code-only
        // sibling cover the same source span, and keeping them apart forces
        // semantic queries to choose one side. Multi-entity packs stay at
        // the pre-injection ceiling so unrelated members do not merge.
        let single_unit_ceiling = if budget.hard_limit == 0 {
            usize::MAX
        } else {
            budget
                .hard_limit
                .saturating_mul(SINGLE_ENTITY_BM25_TOLERANCE_FACTOR)
        };
        let mut merged: Vec<ChunkSegment> = Vec::with_capacity(segments.len());
        for segment in segments {
            if let Some(last) = merged.last_mut() {
                // Fragments of one entity are packed back together up to the
                // pre-injection limit; pieces from different entities only merge
                // when one of them is still undersized.
                let same_unit = !last.boundary.entity_ids.is_empty()
                    && last.boundary.entity_ids == segment.boundary.entity_ids;
                let last_test = ChunkBuilder::fragment_test_info(group, &last.boundary.entity_ids);
                let next_test =
                    ChunkBuilder::fragment_test_info(group, &segment.boundary.entity_ids);
                if !cce_types::TestInfo::can_merge(&last_test, &next_test) {
                    merged.push(segment);
                    continue;
                }
                let effective_ceiling = if same_unit {
                    let single_entity = last.boundary.entity_ids.len() == 1
                        && segment.boundary.entity_ids.len() == 1;
                    if path == ChunkPath::Bm25 && single_entity {
                        single_unit_ceiling
                    } else {
                        same_unit_ceiling
                    }
                } else if path == ChunkPath::Bm25
                    && entities_call_related(
                        group,
                        &last.boundary.entity_ids,
                        &segment.boundary.entity_ids,
                    )
                {
                    // Call-related neighbours keep caller context together
                    // up to the pre-injection ceiling so a binder method is
                    // not split from the body parser it feeds. Unrelated
                    // members stay at the merge ceiling.
                    same_unit_ceiling
                } else {
                    ceiling
                };
                let mergeable = can_merge_with_ceiling(
                    &last.text,
                    &segment.text,
                    path,
                    min_threshold,
                    effective_ceiling,
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
mod tests;
