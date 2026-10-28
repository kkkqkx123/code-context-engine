//! Budget units shared by every chunking path.
//!
//! A chunk limit is only meaningful together with the unit it is measured
//! in. The embedding path counts estimated tokens; the keyword path counts
//! the terms the index tokenizer will produce. Both the splitter and the
//! injection must use the same unit as the limit they are checked against,
//! otherwise a text can pass one check and fail the other.

use cce_utils::token_estimation::estimate_tokens;

use crate::segment::{segment_text, shared_jieba};

/// The unit a chunk limit is expressed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkUnit {
    /// Estimated tokens for the embedding provider.
    Tokens,
    /// Terms produced by the keyword index tokenizer.
    Terms,
}

impl ChunkUnit {
    /// The cost of `text` in this unit.
    pub fn cost(self, text: &str) -> usize {
        match self {
            Self::Tokens => estimate_tokens(text),
            // One position per indexed term: a source word and its
            // identifier splits share a position, so one word costs one no
            // matter how many forms the index keeps for it. CJK dictionary
            // segments each own a position and keep their granularity.
            Self::Terms => position_count(text),
        }
    }

    /// Whether `text` fits a limit expressed in this unit.
    ///
    /// A limit of zero disables the check.
    pub fn fits(self, text: &str, limit: usize) -> bool {
        limit == 0 || self.cost(text) <= limit
    }
}

/// The longest prefix of `text` whose cost does not exceed `limit`.
///
/// The returned slice is always aligned to a character boundary. An empty
/// prefix is returned when even the first character exceeds the limit; the
/// caller decides whether that is a configuration error.
pub fn truncate_to_cost(text: &str, unit: ChunkUnit, limit: usize) -> &str {
    if limit == 0 || unit.cost(text) <= limit {
        return text;
    }

    let total_chars = text.chars().count();
    // Binary search for a fitting prefix. The cost of a prefix is not
    // guaranteed to be strictly monotonic for every tokenizer boundary, so
    // the invariant is only "`lo` fits", never "`mid + 1` does not". The
    // forward scan below recovers the longest run that actually fits.
    let mut lo = 0usize;
    let mut hi = total_chars;
    while lo < hi {
        let mid = lo + (hi - lo).div_ceil(2);
        let end = byte_at_char(text, mid);
        if unit.cost(&text[..end]) <= limit {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    let mut best = byte_at_char(text, lo);
    while best < text.len() {
        let next = text[best..]
            .chars()
            .next()
            .map(|character| best + character.len_utf8())
            .unwrap_or(text.len());
        if unit.cost(&text[..next]) > limit {
            break;
        }
        best = next;
    }
    &text[..best]
}

/// Number of indexed positions in `text`.
///
/// The original word and its auxiliary splits share one position
/// (`position_length == 0` marks the auxiliaries), so counting positions
/// keeps one source word at cost one while CJK dictionary segments each
/// keep their own.
fn position_count(text: &str) -> usize {
    segment_text(text, shared_jieba())
        .iter()
        .filter(|token| token.position_length == 1)
        .count()
}

fn byte_at_char(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map(|(index, _)| index)
        .unwrap_or(text.len())
}

/// The start of the longest suffix of `text` whose cost does not exceed
/// `limit`.
///
/// Used to carry an overlap of at most `limit` units from the end of the
/// previous piece. The returned index is always a character boundary. When
/// `limit` is zero the suffix is empty.
pub fn suffix_start_for_cost(text: &str, unit: ChunkUnit, limit: usize) -> usize {
    if limit == 0 {
        return text.len();
    }
    let mut best = text.len();
    for (index, _) in text.char_indices().rev() {
        if unit.cost(&text[index..]) > limit {
            break;
        }
        best = index;
    }
    best
}

/// Split `text` into char-aligned ranges that each fit `limit`.
///
/// The terminal budget primitive shared by every chunking path: each
/// returned range is non-empty, contiguous, fully covers `text`, stays on
/// character boundaries and costs at most `limit`. Callers only choose
/// where to prefer boundaries inside the guaranteed-to-fit prefix; the
/// budget enforcement itself lives here. A single character that still
/// exceeds the limit signals an unsatisfiable configuration.
pub fn split_to_fit(text: &str, unit: ChunkUnit, limit: usize) -> Vec<(usize, usize)> {
    if text.is_empty() {
        return Vec::new();
    }
    if limit == 0 {
        return vec![(0, text.len())];
    }
    let mut ranges = Vec::new();
    let mut start = 0usize;
    while start < text.len() {
        let remaining = &text[start..];
        let prefix = truncate_to_cost(remaining, unit, limit);
        let mut end = start + prefix.len();
        if end <= start {
            end = remaining
                .chars()
                .next()
                .map(|character| start + character.len_utf8())
                .unwrap_or(text.len());
        }
        ranges.push((start, end));
        if end >= text.len() {
            break;
        }
        start = end;
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terms_count_positions_not_split_forms() {
        assert_eq!(ChunkUnit::Terms.cost("get_or_init"), 1);
        assert_eq!(ChunkUnit::Terms.cost("is_hidden"), 1);
        assert_eq!(ChunkUnit::Terms.cost("hello world"), 2);
    }

    #[test]
    fn token_and_term_units_are_distinct() {
        let text = "mov_read_tfra parses the track fragment box";
        assert!(ChunkUnit::Tokens.cost(text) > 0);
        assert!(ChunkUnit::Terms.cost(text) > 0);
    }

    #[test]
    fn cjk_is_not_one_term() {
        let text = "媒体服务器轨道片段";
        assert!(ChunkUnit::Terms.cost(text) > 1);
    }

    #[test]
    fn truncate_never_splits_a_character() {
        let text = "媒体服务器";
        let truncated = truncate_to_cost(text, ChunkUnit::Tokens, 2);
        assert!(text.starts_with(truncated));
        assert!(text.is_char_boundary(truncated.len()));
    }

    #[test]
    fn zero_limit_is_unbounded() {
        let text = "anything";
        assert!(ChunkUnit::Tokens.fits(text, 0));
        assert_eq!(truncate_to_cost(text, ChunkUnit::Tokens, 0), text);
    }

    #[test]
    fn truncated_prefix_fits_and_cannot_grow_by_one_char() {
        let text = "媒体服务器 parse_stts track fragment box 轨道片段 run";
        for unit in [ChunkUnit::Tokens, ChunkUnit::Terms] {
            for limit in 1..unit.cost(text) {
                let prefix = truncate_to_cost(text, unit, limit);
                assert!(text.is_char_boundary(prefix.len()));
                assert!(unit.cost(prefix) <= limit);
                if prefix.len() < text.len() {
                    let next = prefix.len()
                        + text[prefix.len()..]
                            .chars()
                            .next()
                            .map(|character| character.len_utf8())
                            .unwrap_or(0);
                    assert!(
                        unit.cost(&text[..next]) > limit,
                        "prefix for limit {limit} is not maximal"
                    );
                }
            }
        }
    }
}
