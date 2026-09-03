use std::ops::Range;

use cce_config::modules::ChunkingConfig;
use cce_utils::token_estimation::TokenEstimator;

use super::super::boundary::{ChunkBoundary, SplitReason, cost};
use super::super::result::ChunkPath;

pub fn split_by_tokens(
    text: &str,
    path: ChunkPath,
    config: &ChunkingConfig,
    estimator: &TokenEstimator,
) -> Vec<ChunkBoundary> {
    split_by_tokens_in_range(text, 0..text.len(), path, config, estimator)
}

/// Hard token/word-level split of `text[range]`.
///
/// The guaranteed-to-succeed terminal of the data-driven recursion: every
/// non-empty range can be cut at character granularity.
pub fn split_by_tokens_in_range(
    text: &str,
    range: Range<usize>,
    path: ChunkPath,
    config: &ChunkingConfig,
    estimator: &TokenEstimator,
) -> Vec<ChunkBoundary> {
    let mut boundaries = Vec::new();
    let mut current_pos = range.start;

    while current_pos < range.end {
        let target = match path {
            ChunkPath::Bm25 => {
                find_bm25_split_point(text, current_pos, config.max_bm25_words).min(range.end)
            }
            ChunkPath::Embedding => {
                let relative =
                    estimator.find_split_point(&text[current_pos..range.end], config.max_tokens);
                (current_pos + relative).min(range.end)
            }
        };
        let safe_end = find_good_split_point(text, current_pos, target).min(range.end);

        if safe_end <= current_pos {
            // Invalid configuration (for example, a zero token limit) must not
            // make the splitter drop the remainder of valid UTF-8 input.
            let next_char_end = text[current_pos..range.end]
                .chars()
                .next()
                .map(|ch| current_pos + ch.len_utf8())
                .unwrap_or(range.end);
            if next_char_end <= current_pos {
                break;
            }
            boundaries.push(
                ChunkBoundary::new(current_pos, next_char_end, SplitReason::HardLimit)
                    .with_token_count(cost(&text[current_pos..next_char_end], path)),
            );
            current_pos = next_char_end;
            continue;
        }

        let chunk_tokens = cost(&text[current_pos..safe_end], path);

        boundaries.push(
            ChunkBoundary::new(current_pos, safe_end, SplitReason::HardLimit)
                .with_token_count(chunk_tokens),
        );

        current_pos = safe_end;
    }

    boundaries
}

fn find_good_split_point(text: &str, start: usize, target: usize) -> usize {
    debug_assert!(text.is_char_boundary(start));

    let target = floor_char_boundary(text, target.clamp(start, text.len()));
    if target >= text.len() {
        return text.len();
    }

    let search_window = target.saturating_sub(start).max(50);
    let window_start = floor_char_boundary(text, target.saturating_sub(search_window));

    // Look for code fence boundary (preferred: never split inside a code block)
    if let Some(fence_pos) = find_preceding_line_start(text, window_start, target, "```") {
        return fence_pos;
    }

    // Iterate by Unicode scalar values. A byte-by-byte loop cannot safely use
    // its index as the start of a `str` slice for non-ASCII source text.
    let preferred_start = window_start;
    let mut sentence_boundary = None;
    let mut whitespace_boundary = None;
    let mut newline_boundary = None;
    for (relative, ch) in text[start..target].char_indices() {
        let end = start + relative + ch.len_utf8();
        if ch.is_whitespace() {
            whitespace_boundary = Some(end);
        }
        if ch == '\n' && end >= preferred_start {
            newline_boundary = Some(end);
        }
        // `end` already points past the char, so the boundary is `end` itself.
        // Adding `ch.len_utf8()` again would overshoot by the char width and
        // land mid-char for multi-byte sentence endings (`。` is 3 bytes),
        // producing a non-char-boundary split point or one beyond `target`.
        if end >= preferred_start && matches!(ch, '.' | '!' | '?' | '。' | '！' | '？') {
            sentence_boundary = Some(end);
        }
    }

    // Prefer sentence boundary, then newline, then whitespace — but avoid
    // splitting after incomplete expression markers
    let candidate = sentence_boundary
        .or(newline_boundary)
        .or(whitespace_boundary)
        .unwrap_or(target);

    avoid_incomplete_expression(text, start, candidate)
}

/// Find the start of a line containing `marker` within [start, end].
/// Returns the byte position at the beginning of that line.
fn find_preceding_line_start(text: &str, start: usize, end: usize, marker: &str) -> Option<usize> {
    let search_region = &text[start..end];
    let rel = search_region.rfind(marker)?;
    let abs = start + rel;
    // Walk back to the beginning of the line containing the marker
    let line_start = text[start..abs]
        .rfind('\n')
        .map(|rel_nl| start + rel_nl + 1)
        .unwrap_or(start);
    Some(floor_char_boundary(text, line_start))
}

/// Avoid splitting immediately after expression-continuation markers.
///
/// If the split point falls after `=`, `->`, `=>`, `,`, `(`, `{`, `[`, `.`,
/// walk back to the previous whitespace boundary to prevent truncated output
/// like `let matcher =`.
fn avoid_incomplete_expression(text: &str, chunk_start: usize, split: usize) -> usize {
    if split <= chunk_start || split >= text.len() {
        return split;
    }

    // Check the last non-whitespace char before split
    let prefix = &text[chunk_start..split];
    let last_non_ws = prefix.trim_end().chars().next_back();

    let is_incomplete = match last_non_ws {
        Some('=' | '{' | '(' | '[' | ',' | '.' | ':') => true,
        Some('-') | Some('>') => {
            // Check for -> or =>
            prefix.trim_end().ends_with("->") || prefix.trim_end().ends_with("=>")
        }
        _ => false,
    };

    if is_incomplete {
        // Walk back to previous newline or double-newline
        if let Some(rel) = prefix.rfind("\n\n") {
            let candidate = chunk_start + rel + 2;
            if candidate > chunk_start {
                return candidate;
            }
        }
        if let Some(rel) = prefix.rfind('\n') {
            let candidate = chunk_start + rel + 1;
            if candidate > chunk_start {
                return candidate;
            }
        }
    }

    split
}

fn find_bm25_split_point(text: &str, start: usize, max_words: usize) -> usize {
    if max_words == 0 {
        return text.len();
    }

    let mut words = 0;
    let mut in_word = false;
    for (relative, ch) in text[start..].char_indices() {
        if ch.is_whitespace() {
            if in_word {
                words += 1;
                if words >= max_words {
                    return start + relative + ch.len_utf8();
                }
                in_word = false;
            }
        } else {
            in_word = true;
        }
    }

    text.len()
}

fn floor_char_boundary(text: &str, mut position: usize) -> usize {
    while position > 0 && !text.is_char_boundary(position) {
        position -= 1;
    }
    position
}

#[cfg(test)]
mod tests {
    use cce_config::modules::ChunkingConfig;

    use super::super::super::result::ChunkPath;
    use super::*;

    #[test]
    fn test_split_by_tokens_basic() {
        let config = ChunkingConfig {
            max_tokens: 5,
            ..Default::default()
        };

        let text = "A B C D E F G H I J K L M N O P Q R S T U V W X Y Z";
        let estimator = TokenEstimator::default();
        let boundaries = split_by_tokens(text, ChunkPath::Embedding, &config, &estimator);

        assert!(boundaries.len() > 1);
        for b in &boundaries {
            assert!(b.end_byte <= text.len());
            assert!(b.start_byte < b.end_byte);
        }
    }

    #[test]
    fn test_split_by_tokens_handles_unicode_at_byte_limit() {
        let config = ChunkingConfig {
            max_tokens: 2,
            ..Default::default()
        };
        let estimator = TokenEstimator::default();
        let text = "aaaaaaaδbbbbbbb 世界 🌍";

        let boundaries = split_by_tokens(text, ChunkPath::Embedding, &config, &estimator);

        assert!(!boundaries.is_empty());
        assert_eq!(
            boundaries.first().map(|boundary| boundary.start_byte),
            Some(0)
        );
        assert_eq!(
            boundaries.last().map(|boundary| boundary.end_byte),
            Some(text.len())
        );
        for boundary in boundaries {
            assert!(text.is_char_boundary(boundary.start_byte));
            assert!(text.is_char_boundary(boundary.end_byte));
            assert!(boundary.start_byte < boundary.end_byte);
        }
    }

    #[test]
    fn test_split_by_tokens_bm25_path() {
        let config = ChunkingConfig {
            max_bm25_words: 3,
            ..Default::default()
        };
        let estimator = TokenEstimator::default();
        let text = "one two three four five six seven eight nine ten";
        let boundaries = split_by_tokens(text, ChunkPath::Bm25, &config, &estimator);

        assert!(boundaries.len() > 1);
        assert_eq!(boundaries.first().unwrap().start_byte, 0);
        assert_eq!(boundaries.last().unwrap().end_byte, text.len());
        for b in &boundaries {
            assert!(text.is_char_boundary(b.start_byte));
            assert!(text.is_char_boundary(b.end_byte));
            assert!(b.start_byte < b.end_byte);
        }
    }

    #[test]
    fn test_split_by_tokens_zero_bm25_limit() {
        let config = ChunkingConfig {
            max_bm25_words: 0,
            ..Default::default()
        };
        let estimator = TokenEstimator::default();
        let text = "some words here";
        let boundaries = split_by_tokens(text, ChunkPath::Bm25, &config, &estimator);
        assert_eq!(boundaries.len(), 1);
        assert_eq!(boundaries[0].start_byte, 0);
        assert_eq!(boundaries[0].end_byte, text.len());
    }

    #[test]
    fn test_split_by_tokens_zero_embedding_limit() {
        let config = ChunkingConfig {
            max_tokens: 0,
            ..Default::default()
        };
        let estimator = TokenEstimator::default();
        let text = "text that should not be dropped";
        let boundaries = split_by_tokens(text, ChunkPath::Embedding, &config, &estimator);
        assert!(!boundaries.is_empty());
        assert_eq!(boundaries.last().unwrap().end_byte, text.len());
        // Safety path must emit at least one boundary covering the full text
        let full: String = boundaries
            .iter()
            .map(|b| &text[b.start_byte..b.end_byte])
            .collect();
        assert_eq!(full, text);
    }

    #[test]
    fn test_split_by_tokens_empty_text() {
        let config = ChunkingConfig::default();
        let estimator = TokenEstimator::default();
        let boundaries = split_by_tokens("", ChunkPath::Embedding, &config, &estimator);
        assert!(boundaries.is_empty());
    }

    #[test]
    fn test_split_by_tokens_shorter_than_limit() {
        let config = ChunkingConfig {
            max_tokens: 100,
            ..Default::default()
        };
        let estimator = TokenEstimator::default();
        let text = "short text";
        let boundaries = split_by_tokens(text, ChunkPath::Embedding, &config, &estimator);
        assert_eq!(boundaries.len(), 1);
        assert_eq!(boundaries[0].split_reason, SplitReason::HardLimit);
        assert_eq!(boundaries[0].start_byte, 0);
        assert_eq!(boundaries[0].end_byte, text.len());
    }

    #[test]
    fn test_find_good_split_point_does_not_slice_inside_unicode() {
        let text = format!("{}δ trailing text", "a".repeat(1812));
        let split = find_good_split_point(&text, 0, 1813);

        assert!(text.is_char_boundary(split));
        assert!(split > 0);
    }
}
