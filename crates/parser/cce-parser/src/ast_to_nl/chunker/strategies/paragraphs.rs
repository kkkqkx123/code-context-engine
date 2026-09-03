use std::ops::Range;

use cce_config::modules::ChunkingConfig;

use super::super::boundary::{ChunkBoundary, SplitReason, cost};
use super::super::result::ChunkPath;
use super::lines::extend_or_push_trailing;

/// Partition `text[range]` at blank-line positions (accumulating paragraphs
/// into chunks up to the path limit).
///
/// Pure strategy: returns a complete partition of the interval and never
/// falls back to another strategy. An oversized paragraph is emitted as a
/// single piece; re-splitting it is the caller's (`split_range`) job.
pub fn split_text_by_paragraphs(
    text: &str,
    range: Range<usize>,
    path: ChunkPath,
    config: &ChunkingConfig,
) -> Vec<ChunkBoundary> {
    let base = range.start;
    let end = range.end;
    let paragraph_ends: Vec<usize> = text[base..end]
        .match_indices("\n\n")
        .map(|(i, _)| base + i + 2)
        .collect();

    let mut boundaries = Vec::new();
    let mut current_start = base;
    let mut prev_pos = base;
    let mut current_tokens = 0;
    let limit = path_limit(path, config);

    for &pos in &paragraph_ends {
        let paragraph = &text[prev_pos..pos];
        let para_tokens = cost(paragraph, path);

        if para_tokens > limit {
            let mut flushed = false;
            if current_start < prev_pos && current_tokens > 0 {
                boundaries.push(
                    ChunkBoundary::new(current_start, prev_pos, SplitReason::ParagraphBoundary)
                        .with_token_count(current_tokens),
                );
                flushed = true;
            }
            // Zero-cost paragraphs (blank lines) accumulated since the last
            // flush carry no tokens, so they are never flushed. Fold them
            // into the oversized paragraph piece; starting it at `prev_pos`
            // would drop them from the partition.
            let para_start = if flushed { prev_pos } else { current_start };
            boundaries.push(
                ChunkBoundary::new(para_start, pos, SplitReason::ParagraphBoundary)
                    .with_token_count(cost(&text[para_start..pos], path)),
            );
            current_start = pos;
            current_tokens = 0;
        } else if current_tokens + para_tokens > limit && current_tokens > 0 {
            boundaries.push(
                ChunkBoundary::new(current_start, prev_pos, SplitReason::ParagraphBoundary)
                    .with_token_count(current_tokens),
            );
            current_start = prev_pos;
            current_tokens = para_tokens;
        } else {
            current_tokens += para_tokens;
        }
        prev_pos = pos;
    }

    extend_or_push_trailing(
        &mut boundaries,
        text,
        current_start,
        end,
        path,
        SplitReason::ParagraphBoundary,
    );

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
    use cce_config::modules::ChunkingConfig;

    use super::super::super::result::ChunkPath;
    use super::*;

    #[test]
    fn test_split_by_paragraphs_basic() {
        let config = ChunkingConfig {
            max_tokens: 10,
            ..Default::default()
        };

        let text =
            "First paragraph content.\n\nSecond paragraph content.\n\nThird paragraph content.";
        let boundaries =
            split_text_by_paragraphs(text, 0..text.len(), ChunkPath::Embedding, &config);

        assert!(!boundaries.is_empty());
        assert!(boundaries.len() > 1 || boundaries[0].split_reason == SplitReason::NotSplit);
    }

    #[test]
    fn test_split_by_paragraphs_no_zero_length_boundaries() {
        let config = ChunkingConfig {
            max_bm25_words: 5,
            ..Default::default()
        };

        // First paragraph fits, second pushes past the limit, third is oversized.
        let text = "alpha beta gamma\n\ndelta epsilon zeta\n\necho foxtrot golf hotel india juliet kilo lima mike november oscar papa quebec romeo sierra tango";
        let boundaries = split_text_by_paragraphs(text, 0..text.len(), ChunkPath::Bm25, &config);

        assert!(boundaries.len() > 1);
        for b in &boundaries {
            assert!(
                b.start_byte < b.end_byte,
                "zero-length boundary at ({},{})",
                b.start_byte,
                b.end_byte
            );
        }
        assert_eq!(boundaries.first().unwrap().start_byte, 0);
        assert_eq!(boundaries.last().unwrap().end_byte, text.len());
    }

    #[test]
    fn test_split_by_paragraphs_blank_lines_between_oversized_paragraphs_are_covered() {
        let config = ChunkingConfig {
            max_bm25_words: 3,
            ..Default::default()
        };
        // Oversized paragraphs separated by "\n\n" separators and a blank
        // gap; the zero-cost gap must be folded into the oversized piece,
        // not dropped from the partition.
        let text = "word1 word2 word3 word4 word5\n\n\n\nword6 word7 word8 word9 word10\n\n";
        let boundaries = split_text_by_paragraphs(text, 0..text.len(), ChunkPath::Bm25, &config);

        assert!(!boundaries.is_empty());
        let mut prev = 0;
        for b in &boundaries {
            assert_eq!(b.start_byte, prev, "partition gap/overlap");
            assert!(b.start_byte < b.end_byte, "zero-length");
            prev = b.end_byte;
        }
        assert_eq!(prev, text.len(), "coverage");
    }

    #[test]
    fn test_split_by_paragraphs_partition_is_contiguous() {
        let config = ChunkingConfig {
            max_bm25_words: 5,
            ..Default::default()
        };
        let text = "alpha beta\n\n\n\ngamma delta epsilon zeta\n\ntail";
        let boundaries = split_text_by_paragraphs(text, 0..text.len(), ChunkPath::Bm25, &config);

        assert!(!boundaries.is_empty());
        let mut prev = 0;
        for b in &boundaries {
            assert_eq!(b.start_byte, prev, "partition gap/overlap");
            assert!(b.start_byte < b.end_byte);
            prev = b.end_byte;
        }
        assert_eq!(prev, text.len());
    }
}
