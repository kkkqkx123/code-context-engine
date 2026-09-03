use std::ops::Range;

use cce_config::modules::ChunkingConfig;
use cce_types::entity::EntityId;
use cce_utils::token_estimation::TokenEstimator;

use crate::grouper::EntityGroup;

use super::boundary::{ChunkBoundary, ChunkSegment, NlEntityBoundary, can_merge, cost};
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
    /// Newline positions.
    Lines,
    /// Blank-line positions.
    Paragraphs,
    /// Hard token/word cut (`split_by_tokens`), the guaranteed terminal.
    Tokens,
}

impl BoundaryLevel {
    /// Next coarser boundary source for the data-driven whole-interval descent.
    fn next(self) -> Self {
        match self {
            Self::NestedGroups => Self::Entities,
            Self::Entities => Self::Sentences,
            Self::Sentences => Self::Lines,
            Self::Lines => Self::Paragraphs,
            Self::Paragraphs => Self::Tokens,
            Self::Tokens => Self::Tokens,
        }
    }

    /// Boundary source that best sub-divides an oversized piece produced by
    /// `self`: sentence-end positions inside entity/nested-group text,
    /// newlines inside sentence/paragraph text, hard token cut for lines.
    fn subdivide(self) -> Self {
        match self {
            Self::NestedGroups | Self::Entities => Self::Sentences,
            Self::Sentences | Self::Paragraphs => Self::Lines,
            Self::Lines | Self::Tokens => Self::Tokens,
        }
    }

    fn from_strategy(strategy: SplitStrategy) -> Self {
        match strategy {
            SplitStrategy::ByNestedGroups => Self::NestedGroups,
            SplitStrategy::ByMembers | SplitStrategy::ByNlEntityBoundaries => Self::Entities,
            SplitStrategy::BySentences => Self::Sentences,
            SplitStrategy::ByLines => Self::Lines,
            SplitStrategy::ByParagraphs => Self::Paragraphs,
            SplitStrategy::ByTokens => Self::Tokens,
        }
    }
}

/// Text splitter
pub struct TextSplitter {
    config: ChunkingConfig,
    estimator: TokenEstimator,
}

impl TextSplitter {
    /// Create new text splitter
    pub fn new(config: ChunkingConfig) -> Self {
        Self {
            config,
            estimator: TokenEstimator::default(),
        }
    }

    pub fn split(
        &self,
        text: &str,
        group: &EntityGroup,
        strategy: SplitStrategy,
        path: ChunkPath,
    ) -> Vec<ChunkSegment> {
        self.split_with_boundaries(text, group, strategy, path, None)
    }

    pub fn split_with_nl_boundaries(
        &self,
        text: &str,
        group: &EntityGroup,
        strategy: SplitStrategy,
        path: ChunkPath,
        nl_boundaries: &[NlEntityBoundary],
    ) -> Vec<ChunkSegment> {
        self.split_with_boundaries(text, group, strategy, path, Some(nl_boundaries))
    }

    fn split_with_boundaries(
        &self,
        text: &str,
        group: &EntityGroup,
        strategy: SplitStrategy,
        path: ChunkPath,
        nl_boundaries: Option<&[NlEntityBoundary]>,
    ) -> Vec<ChunkSegment> {
        let boundaries =
            self.split_range(text, group, strategy, path, nl_boundaries, 0..text.len());
        let segments = self.boundaries_to_segments(boundaries, text);
        self.maybe_merge_segments(segments, path)
    }

    /// Split `text[range]` by a single data-driven recursion over boundary
    /// sources: the strategy's level is tried first, the recursion descends
    /// to coarser sources when a level has no boundary data, and oversized
    /// pieces are sub-divided recursively. `split_by_tokens` is the
    /// guaranteed-to-succeed terminal.
    pub(crate) fn split_range(
        &self,
        text: &str,
        group: &EntityGroup,
        strategy: SplitStrategy,
        path: ChunkPath,
        nl_boundaries: Option<&[NlEntityBoundary]>,
        range: Range<usize>,
    ) -> Vec<ChunkBoundary> {
        self.split_range_at(
            text,
            group,
            path,
            nl_boundaries,
            range,
            BoundaryLevel::from_strategy(strategy),
            &[],
        )
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
    ) -> Vec<ChunkBoundary> {
        debug_assert!(range.start <= range.end && range.end <= text.len());
        debug_assert!(text.is_char_boundary(range.start));
        debug_assert!(text.is_char_boundary(range.end));

        if level == BoundaryLevel::Tokens {
            let mut boundaries = strategies::tokens::split_by_tokens_in_range(
                text,
                range,
                path,
                &self.config,
                &self.estimator,
            );
            if !inherit_entity_ids.is_empty() {
                for b in &mut boundaries {
                    if b.entity_ids.is_empty() {
                        b.entity_ids = inherit_entity_ids.to_vec();
                    }
                }
            }
            return boundaries;
        }

        let boundaries = self.partition_at(level, text, group, path, nl_boundaries, range.clone());

        // No boundary data at this level → descend to the next coarser
        // boundary source.
        if boundaries.len() < 2 {
            return self.split_range_at(
                text,
                group,
                path,
                nl_boundaries,
                range,
                level.next(),
                inherit_entity_ids,
            );
        }

        let mut result = Vec::with_capacity(boundaries.len());
        for mut b in boundaries {
            debug_assert!(
                b.start_byte >= range.start && b.end_byte <= range.end,
                "strategy boundary escapes its range"
            );
            if b.entity_ids.is_empty() {
                b.entity_ids = inherit_entity_ids.to_vec();
            }
            let piece_ids = b.entity_ids.clone();

            if self
                .config
                .exceeds_limit(&text[b.start_byte..b.end_byte], path)
            {
                result.extend(self.split_range_at(
                    text,
                    group,
                    path,
                    nl_boundaries,
                    b.start_byte..b.end_byte,
                    level.subdivide(),
                    &piece_ids,
                ));
            } else {
                result.push(b);
            }
        }
        result
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
        match level {
            BoundaryLevel::NestedGroups => strategies::nested_groups::split_by_nested_groups(
                text,
                group,
                path,
                &self.config,
                range,
            ),
            BoundaryLevel::Entities => {
                if let Some(nl) = nl_boundaries {
                    strategies::members::split_by_members_with_nl(
                        text,
                        group,
                        path,
                        &self.config,
                        nl,
                        range,
                    )
                } else {
                    strategies::members::split_by_members(text, group, path, &self.config, range)
                }
            }
            BoundaryLevel::Sentences => {
                strategies::sentences::split_text_by_sentences(text, range, path, &self.config)
            }
            BoundaryLevel::Lines => {
                strategies::lines::split_text_by_lines(text, range, path, &self.config)
            }
            BoundaryLevel::Paragraphs => {
                strategies::paragraphs::split_text_by_paragraphs(text, range, path, &self.config)
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
    ) -> Vec<ChunkSegment> {
        let mut segments = Vec::with_capacity(boundaries.len());
        let mut prev_end = 0usize;
        for boundary in boundaries {
            debug_assert!(
                boundary.start_byte == prev_end,
                "boundary gap/overlap: previous end {} vs start {}",
                prev_end,
                boundary.start_byte
            );
            debug_assert!(
                boundary.start_byte < boundary.end_byte && boundary.end_byte <= text.len(),
                "invalid boundary range [{}, {}) for text of length {}",
                boundary.start_byte,
                boundary.end_byte,
                text.len()
            );
            debug_assert!(
                text.is_char_boundary(boundary.start_byte),
                "start byte {} is not a char boundary",
                boundary.start_byte
            );
            debug_assert!(
                text.is_char_boundary(boundary.end_byte),
                "end byte {} is not a char boundary",
                boundary.end_byte
            );
            let segment_text = text[boundary.start_byte..boundary.end_byte].to_string();
            prev_end = boundary.end_byte;
            segments.push(ChunkSegment::new(boundary, segment_text));
        }
        debug_assert!(
            prev_end == text.len(),
            "boundaries do not cover the full text (covered up to {})",
            prev_end
        );
        segments
    }

    /// Merge adjacent small segments whose combined cost fits within the
    /// ceiling for `path`.
    ///
    /// Both merge layers (this intra-splitter pass and the cross-group
    /// pass in `merge::merge_small_chunks_cross_group`) share the same
    /// decision rule (`boundary::can_merge`): a merge happens only when the
    /// leading segment is still below the path's min threshold and the
    /// combined cost stays within the path's merge ceiling. Boundaries are
    /// preserved whenever the leading segment is already large enough —
    /// large segments never absorb neighbors, which keeps entity boundaries
    /// intact unless a chunk is genuinely undersized. A single left-to-right
    /// greedy pass fully determines the result: rejection is monotonic, so
    /// re-scans (backward passes) can never enable a previously rejected
    /// merge.
    fn merge_small_segments(
        &self,
        segments: Vec<ChunkSegment>,
        path: ChunkPath,
    ) -> Vec<ChunkSegment> {
        let mut merged: Vec<ChunkSegment> = Vec::with_capacity(segments.len());
        for segment in segments {
            if let Some(last) = merged.last_mut() {
                if can_merge(&last.text, &segment.text, path, &self.config) {
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
        strategy: SplitStrategy,
        max_tokens: usize,
    ) -> Vec<ChunkBoundary> {
        let config = ChunkingConfig {
            max_tokens,
            ..Default::default()
        };
        let splitter = TextSplitter::new(config);
        splitter.split_range(
            text,
            &EntityGroup::default(),
            strategy,
            ChunkPath::Embedding,
            None,
            0..text.len(),
        )
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
        // A non-char-aligned boundary violates the splitter contract: the
        // invariant is asserted (debug builds panic), never silently repaired.
        assert!(
            std::panic::catch_unwind(|| splitter.boundaries_to_segments(boundaries, text)).is_err()
        );
    }

    #[test]
    fn test_boundaries_to_segments_unicode_only() {
        let config = ChunkingConfig::default();
        let splitter = TextSplitter::new(config);
        let text = "世界你好";
        let boundaries =
            vec![ChunkBoundary::new(0, 12, SplitReason::SentenceBoundary).with_token_count(4)];
        let segments = splitter.boundaries_to_segments(boundaries, text);
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, text);
    }

    #[test]
    fn test_split_range_by_sentences_stops_at_tokens() {
        let text = "A B C D E F G H";
        let boundaries = split_with_limits(text, SplitStrategy::BySentences, 3);
        assert!(boundaries.len() > 1);
        assert_partition(text, &boundaries);
    }

    #[test]
    fn test_split_range_full_chain_from_nested_groups() {
        let text = "no nested groups no members no sentence ends no newlines no paragraphs";
        let boundaries = split_with_limits(text, SplitStrategy::ByNestedGroups, 3);
        assert!(boundaries.len() > 1);
        assert_partition(text, &boundaries);
    }

    #[test]
    fn test_split_range_full_chain_from_members() {
        let text = "no entity spans no sentences no lines no paras";
        let boundaries = split_with_limits(text, SplitStrategy::ByMembers, 3);
        assert!(boundaries.len() > 1);
        assert_partition(text, &boundaries);
    }

    #[test]
    fn test_split_range_from_sentences_to_tokens() {
        let text = "word word word word word word word";
        let boundaries = split_with_limits(text, SplitStrategy::BySentences, 3);
        assert!(boundaries.len() > 1);
        assert_partition(text, &boundaries);
    }

    #[test]
    fn test_split_range_with_text_within_other_limits() {
        let config = ChunkingConfig {
            max_tokens: 512,
            max_bm25_words: 200,
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
            max_bm25_words: 2,
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
            SplitStrategy::ByMembers,
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
            max_bm25_words: 4,
            ..Default::default()
        };
        let splitter = TextSplitter::new(config);
        let text = "one two three four five six seven eight nine ten";
        let boundaries = splitter.split_range(
            text,
            &EntityGroup::default(),
            SplitStrategy::BySentences,
            ChunkPath::Bm25,
            None,
            0..text.len(),
        );
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
    fn test_split_range_oversized_paragraph_subdivides_by_lines() {
        let config = ChunkingConfig {
            max_bm25_words: 3,
            ..Default::default()
        };
        let splitter = TextSplitter::new(config);
        // Oversized paragraphs (bounded by blank lines) with internal newlines:
        // the recursion must subdivide each paragraph at the lines level.
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
                .any(|b| b.split_reason == SplitReason::LineBoundary),
            "oversized paragraph should be subdivided by lines"
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
        let boundaries = splitter.split_range(
            text,
            &EntityGroup::default(),
            SplitStrategy::BySentences,
            ChunkPath::Embedding,
            None,
            0..text.len(),
        );
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
            let mut cut = len;
            while !s.is_char_boundary(cut) {
                cut -= 1;
            }
            s.truncate(cut);
            s
        }
    }

    #[test]
    fn test_split_range_random_text_invariants() {
        let strategies = [
            SplitStrategy::ByNestedGroups,
            SplitStrategy::ByMembers,
            SplitStrategy::BySentences,
            SplitStrategy::ByLines,
            SplitStrategy::ByParagraphs,
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
                    max_bm25_words: limit,
                    ..Default::default()
                },
                ChunkPath::Embedding => ChunkingConfig {
                    max_tokens: limit,
                    ..Default::default()
                },
            };
            let strategy = strategies[rng.range(strategies.len())];
            let splitter = TextSplitter::new(cfg);
            let boundaries = splitter.split_range(
                &text,
                &EntityGroup::default(),
                strategy,
                path,
                None,
                0..text.len(),
            );

            // Invariants: monotonic + contiguous, non-zero, full coverage,
            // char-aligned, and every piece within the path limit.
            assert!(!boundaries.is_empty(), "round {round}: empty partition");
            let mut prev = 0;
            for b in &boundaries {
                assert_eq!(
                    b.start_byte, prev,
                    "round {round}: strategy={strategy:?} path={path:?} partition gap/overlap"
                );
                assert!(b.start_byte < b.end_byte, "round {round}: zero-length");
                assert!(text.is_char_boundary(b.start_byte));
                assert!(text.is_char_boundary(b.end_byte));
                let piece = &text[b.start_byte..b.end_byte];
                if !text.trim().is_empty() && !piece.trim().is_empty() {
                    let piece_cost = cost(piece, path);
                    assert!(
                        piece_cost <= limit,
                        "round {round}: strategy={strategy:?} path={path:?} text={text:?} piece of {piece_cost} exceeds limit {limit}: {:?}",
                        piece
                    );
                }
                prev = b.end_byte;
            }
            assert_eq!(prev, text.len(), "round {round}: coverage");
        }
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
            max_bm25_words: 512,
            min_chunk_bm25_words: 60,
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
            max_bm25_words: 512,
            min_chunk_bm25_words: 60,
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
            max_bm25_words: 512,
            min_chunk_bm25_words: 60,
            ..Default::default()
        };
        let splitter = TextSplitter::new(config);

        // [40w, 200w, 30w]: the leading 40w chunk is below min and absorbs the
        // 200w neighbor; the trailing 30w fragment's left neighbor is now
        // above min, so it stays separate (unified "prev below min" rule).
        let segments = vec![
            segment(&words(40), 0, 50),
            segment(&words(200), 50, 250),
            segment(&words(30), 250, 300),
        ];

        let merged = splitter.merge_small_segments(segments, ChunkPath::Bm25);

        assert_eq!(merged.len(), 2);
        assert_eq!(cost(&merged[0].text, ChunkPath::Bm25), 240);
        assert_eq!(cost(&merged[1].text, ChunkPath::Bm25), 30);
    }

    #[test]
    fn test_merge_small_chunks_cascading() {
        let config = ChunkingConfig {
            max_bm25_words: 512,
            min_chunk_bm25_words: 60,
            ..Default::default()
        };
        let splitter = TextSplitter::new(config);

        // 30+30 reaches exactly the min threshold, so the chain stops there.
        let segments = vec![
            segment(&words(30), 0, 50),
            segment(&words(30), 50, 100),
            segment(&words(30), 100, 150),
        ];

        let merged = splitter.merge_small_segments(segments, ChunkPath::Bm25);

        assert_eq!(merged.len(), 2);
        assert_eq!(cost(&merged[0].text, ChunkPath::Bm25), 60);
        assert_eq!(cost(&merged[1].text, ChunkPath::Bm25), 30);
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
    fn test_merge_small_chunks_embedding_trailing() {
        let config = ChunkingConfig {
            max_tokens: 512,
            min_chunk_tokens: 50,
            ..Default::default()
        };
        let splitter = TextSplitter::new(config);
        // The leading chunk is already above min, so the trailing small
        // fragment is left alone (unified "prev below min" rule).
        let segments = vec![segment(&words(100), 0, 150), segment(&words(20), 150, 180)];
        let merged = splitter.merge_small_segments(segments, ChunkPath::Embedding);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn test_merge_small_chunks_respect_boundaries_disabled() {
        let config = ChunkingConfig {
            max_bm25_words: 2,
            min_chunk_bm25_words: 3,
            respect_boundaries: false,
            ..Default::default()
        };
        let splitter = TextSplitter::new(config);
        let text = "one two. three four. five six. seven eight.";
        let segments = splitter.split(
            text,
            &EntityGroup::default(),
            SplitStrategy::BySentences,
            ChunkPath::Bm25,
        );
        assert!(segments.len() >= 3);
    }

    #[test]
    fn test_merge_small_chunks_applied_in_split() {
        let config = ChunkingConfig {
            max_bm25_words: 512,
            ..Default::default()
        };
        let splitter = TextSplitter::new(config);
        let text = "one two. three four. five six. seven eight.";
        let segments = splitter.split(
            text,
            &EntityGroup::default(),
            SplitStrategy::BySentences,
            ChunkPath::Bm25,
        );
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, text);
        assert_eq!(segments[0].boundary.token_count, 8);
    }

    #[test]
    fn test_merge_small_chunks_trailing() {
        let config = ChunkingConfig {
            max_bm25_words: 512,
            min_chunk_bm25_words: 60,
            ..Default::default()
        };
        let splitter = TextSplitter::new(config);

        // Leading chunk (150w) is above min, so the trailing small fragment
        // is not absorbed.
        let segments = vec![segment(&words(150), 0, 200), segment(&words(40), 200, 250)];

        let merged = splitter.merge_small_segments(segments, ChunkPath::Bm25);

        assert_eq!(merged.len(), 2);
        assert_eq!(cost(&merged[0].text, ChunkPath::Bm25), 150);
        assert_eq!(cost(&merged[1].text, ChunkPath::Bm25), 40);
    }

    #[test]
    fn test_merge_small_chunks_rescues_undersized_leading_chunk() {
        let config = ChunkingConfig {
            max_bm25_words: 512,
            min_chunk_bm25_words: 60,
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
