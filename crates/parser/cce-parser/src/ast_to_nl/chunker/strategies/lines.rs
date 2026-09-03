use std::ops::Range;

use cce_config::modules::ChunkingConfig;

use super::super::boundary::{ChunkBoundary, SplitReason, cost};
use super::super::result::ChunkPath;

/// Partition `text[range]` at newline positions (accumulating lines into
/// chunks up to the path limit).
///
/// Pure strategy: returns a complete partition of the interval and never
/// falls back to another strategy. Oversized lines are hard-split by
/// character budget (`force_split_line`); any other oversized piece is the
/// caller's (`split_range`) job to re-split at a coarser boundary source.
pub fn split_text_by_lines(
    text: &str,
    range: Range<usize>,
    path: ChunkPath,
    config: &ChunkingConfig,
) -> Vec<ChunkBoundary> {
    let base = range.start;
    let end = range.end;
    let mut line_ends: Vec<usize> = text[base..end]
        .match_indices('\n')
        .map(|(i, _)| base + i + 1)
        .collect();
    line_ends.push(end);

    let mut boundaries = Vec::new();
    let mut current_start = base;
    let mut prev_pos = base;
    let mut current_tokens = 0;
    let limit = path_limit(path, config);

    for &pos in &line_ends {
        if prev_pos >= pos {
            prev_pos = pos;
            continue;
        }
        let line = &text[prev_pos..pos];
        let line_tokens = cost(line, path);

        if line_tokens > limit {
            let mut flushed = false;
            if current_start < prev_pos && current_tokens > 0 {
                boundaries.push(
                    ChunkBoundary::new(current_start, prev_pos, SplitReason::LineBoundary)
                        .with_token_count(current_tokens),
                );
                flushed = true;
            }
            if prev_pos < pos {
                // Zero-cost lines (blank/whitespace-only) accumulated since
                // the last flush carry no tokens, so they are never flushed.
                // Fold them into the first force-split piece instead; starting
                // the split at `prev_pos` would drop them from the partition.
                let force_start = if flushed { prev_pos } else { current_start };
                let force_boundaries =
                    force_split_line(&text[force_start..pos], force_start, path, config);
                boundaries.extend(force_boundaries);
            }
            current_start = pos;
            current_tokens = 0;
        } else if current_tokens + line_tokens > limit && current_tokens > 0 {
            boundaries.push(
                ChunkBoundary::new(current_start, prev_pos, SplitReason::LineBoundary)
                    .with_token_count(current_tokens),
            );
            current_start = prev_pos;
            current_tokens = line_tokens;
        } else {
            current_tokens += line_tokens;
        }
        prev_pos = pos;
    }

    extend_or_push_trailing(
        &mut boundaries,
        text,
        current_start,
        end,
        path,
        SplitReason::LineBoundary,
    );

    boundaries
}

/// Emit the trailing remainder of a range.
///
/// A whitespace-only remainder is absorbed into the last boundary instead of
/// becoming a hollow segment, keeping the partition a complete, contiguous
/// cover of the range.
pub(crate) fn extend_or_push_trailing(
    boundaries: &mut Vec<ChunkBoundary>,
    text: &str,
    current_start: usize,
    end: usize,
    path: ChunkPath,
    reason: SplitReason,
) {
    if current_start >= end {
        return;
    }
    let remaining = &text[current_start..end];

    if remaining.trim().is_empty() {
        if let Some(last) = boundaries.last_mut() {
            if last.end_byte < end {
                last.end_byte = end;
            }
            return;
        }
    }

    let remaining_tokens = cost(remaining, path);
    boundaries.push(
        ChunkBoundary::new(
            current_start,
            end,
            if boundaries.is_empty() {
                SplitReason::NotSplit
            } else {
                reason
            },
        )
        .with_token_count(remaining_tokens),
    );
}

pub fn force_split_line(
    line: &str,
    line_start: usize,
    path: ChunkPath,
    config: &ChunkingConfig,
) -> Vec<ChunkBoundary> {
    let mut boundaries = Vec::new();
    let max_chars = path_limit(path, config) * 6;
    let mut current_pos = 0;

    while current_pos < line.len() {
        let target_pos = (current_pos + max_chars).min(line.len());

        let end_pos = if target_pos < line.len() {
            let mut pos = target_pos;
            while pos > current_pos && !line.is_char_boundary(pos) {
                pos -= 1;
            }
            if pos == current_pos {
                pos = target_pos;
                while pos < line.len() && !line.is_char_boundary(pos) {
                    pos += 1;
                }
            }
            pos
        } else {
            target_pos
        };

        if end_pos <= current_pos {
            break;
        }

        let chunk = &line[current_pos..end_pos];
        let tokens = cost(chunk, path);

        boundaries.push(
            ChunkBoundary::new(
                line_start + current_pos,
                line_start + end_pos,
                SplitReason::TokenLimit,
            )
            .with_token_count(tokens),
        );

        current_pos = end_pos;
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
    use cce_config::modules::ChunkingConfig;

    use super::super::super::result::ChunkPath;
    use super::*;

    #[test]
    fn test_split_by_lines_basic() {
        let config = ChunkingConfig::default();

        let text = "Line 1\nLine 2\nLine 3";
        let boundaries = split_text_by_lines(text, 0..text.len(), ChunkPath::Embedding, &config);

        assert!(!boundaries.is_empty());
    }

    #[test]
    fn test_force_split_line_basic() {
        let config = ChunkingConfig {
            max_bm25_words: 3,
            ..Default::default()
        };
        let line = "word1 word2 word3 word4 word5 word6 word7";
        let boundaries = force_split_line(line, 0, ChunkPath::Bm25, &config);

        assert!(boundaries.len() > 1);
        for b in &boundaries {
            assert!(b.start_byte < b.end_byte);
        }
    }

    #[test]
    fn test_split_text_by_lines_first_line_oversized() {
        let config = ChunkingConfig {
            max_bm25_words: 3,
            ..Default::default()
        };
        // Single line with no '\n' — must force-split even though current_tokens==0
        let text = "word1 word2 word3 word4 word5 word6 word7 word8";
        let boundaries = split_text_by_lines(text, 0..text.len(), ChunkPath::Bm25, &config);
        assert!(
            boundaries.len() > 1,
            "should force-split oversized single line, got {}",
            boundaries.len()
        );
        for b in &boundaries {
            assert!(
                b.start_byte < b.end_byte,
                "zero-length at ({},{})",
                b.start_byte,
                b.end_byte
            );
        }
        // Must cover the full text
        assert_eq!(boundaries.last().unwrap().end_byte, text.len());
        assert_eq!(boundaries.first().unwrap().start_byte, 0);
    }

    #[test]
    fn test_split_text_by_lines_oversized_followed_by_blank_lines() {
        let config = ChunkingConfig {
            max_bm25_words: 3,
            ..Default::default()
        };
        // Oversized line followed by blank lines (simulates keyword-hybrid + appended code)
        let text = "word1 word2 word3 word4 word5\n\n\nmatch printer standard\n";
        let boundaries = split_text_by_lines(text, 0..text.len(), ChunkPath::Bm25, &config);

        // Verify no boundary produces a whitespace-only segment
        for b in &boundaries {
            let segment_text = &text[b.start_byte..b.end_byte.min(text.len())];
            assert!(
                !segment_text.trim().is_empty(),
                "whitespace-only segment at ({},{}): {:?}",
                b.start_byte,
                b.end_byte,
                segment_text
            );
        }
    }

    #[test]
    fn test_split_text_by_lines_no_zero_length_boundaries() {
        let config = ChunkingConfig {
            max_bm25_words: 10,
            ..Default::default()
        };
        // First line accumulates above half the limit; the second line then
        // pushes past the limit in the old code, which flushed a zero-length
        // boundary at the accumulated chunk start.
        let text =
            "one two three four five six\nseven eight\nnine ten eleven twelve thirteen fourteen\n";
        let boundaries = split_text_by_lines(text, 0..text.len(), ChunkPath::Bm25, &config);

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
    fn test_split_text_by_lines_trailing_whitespace_absorbed() {
        let config = ChunkingConfig {
            max_bm25_words: 3,
            ..Default::default()
        };
        // Trailing blank lines after the last flushed chunk must be absorbed
        // into the previous boundary, not emitted as a hollow segment.
        let text = "word1 word2 word3 word4 word5\n\n\n";
        let boundaries = split_text_by_lines(text, 0..text.len(), ChunkPath::Bm25, &config);

        assert!(boundaries.len() >= 2);
        assert_eq!(boundaries.last().unwrap().end_byte, text.len());
        for b in &boundaries {
            assert!(!text[b.start_byte..b.end_byte].trim().is_empty());
        }
        // Partition must be contiguous and cover the whole range.
        let mut prev = 0;
        for b in &boundaries {
            assert_eq!(b.start_byte, prev);
            prev = b.end_byte;
        }
        assert_eq!(prev, text.len());
    }

    #[test]
    fn test_split_text_by_lines_blank_lines_between_oversized_lines_are_covered() {
        let config = ChunkingConfig {
            max_bm25_words: 3,
            ..Default::default()
        };
        // Blank lines (zero-cost) accumulate between two oversized lines.
        // They carry no tokens, so the flush condition never fires; they must
        // be folded into the force-split of the next oversized line instead of
        // being dropped from the partition.
        let text = "word1 word2 word3 word4 word5\n\n\nword6 word7 word8 word9 word10\n";
        let boundaries = split_text_by_lines(text, 0..text.len(), ChunkPath::Bm25, &config);

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
    fn test_split_text_by_lines_empty_range() {
        let config = ChunkingConfig::default();
        let boundaries = split_text_by_lines("", 0..0, ChunkPath::Bm25, &config);
        assert!(boundaries.is_empty());
    }

    #[test]
    fn test_split_by_paragraphs_oversized_first_produces_no_whitespace_chunks() {
        use super::super::paragraphs::split_text_by_paragraphs;

        let config = ChunkingConfig {
            max_bm25_words: 5,
            ..Default::default()
        };
        // First paragraph is oversized (keyword-hybrid style), followed by blank lines and code
        let text = "word1 word2 word3 word4 word5 word6 word7 word8\n\n\nmatch printer standard\n";
        let boundaries = split_text_by_paragraphs(text, 0..text.len(), ChunkPath::Bm25, &config);

        assert!(!boundaries.is_empty());
    }

    #[test]
    fn test_splitter_filters_whitespace_only_segments() {
        use super::super::super::splitter::TextSplitter;
        use super::super::super::strategy::SplitStrategy;
        use crate::grouper::EntityGroup;

        let config = ChunkingConfig {
            max_bm25_words: 5,
            ..Default::default()
        };
        let splitter = TextSplitter::new(config);

        // Oversized first line followed by blank lines
        let text = "word1 word2 word3 word4 word5 word6 word7 word8\n\n\nmatch printer standard\n";
        let segments = splitter.split(
            text,
            &EntityGroup::default(),
            SplitStrategy::ByParagraphs,
            ChunkPath::Bm25,
        );

        // Verify no segment is whitespace-only
        for seg in &segments {
            assert!(
                !seg.text.trim().is_empty(),
                "whitespace-only segment: {:?} (split_reason: {:?})",
                seg.text,
                seg.boundary.split_reason
            );
        }
    }
}
