use std::ops::Range;

use cce_config::modules::ChunkingConfig;

use super::super::boundary::{ChunkBoundary, SplitReason, cost};
use super::super::result::ChunkPath;
use super::lines::extend_or_push_trailing;

/// Partition `text[range]` at sentence boundaries (accumulating sentences
/// into chunks up to the path limit).
///
/// Pure strategy: returns a complete partition of the interval and never
/// falls back to another strategy. An oversized accumulated piece is emitted
/// as-is; re-splitting it is the caller's (`split_range`) job.
pub fn split_text_by_sentences(
    text: &str,
    range: Range<usize>,
    path: ChunkPath,
    config: &ChunkingConfig,
) -> Vec<ChunkBoundary> {
    let base = range.start;
    let end = range.end;
    let mut boundaries = Vec::new();
    let mut chunk_start = base;
    let mut chunk_tokens = 0usize;
    let mut prev_sentence_end = base;
    let limit = path_limit(path, config);

    for &pos in &find_sentence_boundaries(&text[base..end]) {
        let abs = base + pos;
        let sentence = &text[prev_sentence_end..abs];
        let sentence_tokens = cost(sentence, path);

        if chunk_tokens + sentence_tokens > limit && chunk_tokens > 0 {
            boundaries.push(
                ChunkBoundary::new(
                    chunk_start,
                    prev_sentence_end,
                    SplitReason::SentenceBoundary,
                )
                .with_token_count(chunk_tokens),
            );
            chunk_start = prev_sentence_end;
            chunk_tokens = 0;
        }

        chunk_tokens += sentence_tokens;
        prev_sentence_end = abs;
    }

    extend_or_push_trailing(
        &mut boundaries,
        text,
        chunk_start,
        end,
        path,
        SplitReason::SentenceBoundary,
    );

    boundaries
}

pub fn find_sentence_boundaries(text: &str) -> Vec<usize> {
    let mut boundaries = Vec::new();
    let sentence_endings = ['.', '!', '?', '。', '！', '？', '\n'];

    for (i, ch) in text.char_indices() {
        if sentence_endings.contains(&ch) {
            let next_pos = i + ch.len_utf8();
            if next_pos < text.len() {
                boundaries.push(next_pos);
            }
        }
    }

    if boundaries.last() != Some(&text.len()) {
        boundaries.push(text.len());
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
    fn test_split_by_sentences_basic() {
        let config = ChunkingConfig::default();

        let text = "First sentence. Second sentence. Third sentence.";
        let boundaries =
            split_text_by_sentences(text, 0..text.len(), ChunkPath::Embedding, &config);

        assert!(!boundaries.is_empty());
        assert!(
            boundaries[0].split_reason == SplitReason::SentenceBoundary
                || boundaries[0].split_reason == SplitReason::NotSplit
        );
    }

    #[test]
    fn test_find_sentence_boundaries_basic() {
        let text = "First. Second! Third? Final.";
        let boundaries = find_sentence_boundaries(text);

        assert!(!boundaries.is_empty());
        assert_eq!(*boundaries.last().unwrap(), text.len());
    }

    #[test]
    fn test_find_sentence_boundaries_unicode() {
        let text = "你好。世界！";
        let boundaries = find_sentence_boundaries(text);

        assert!(!boundaries.is_empty());
        assert_eq!(*boundaries.last().unwrap(), text.len());
    }

    #[test]
    fn test_split_by_sentences_oversized_piece_is_contiguous() {
        let config = ChunkingConfig {
            max_tokens: 10,
            ..Default::default()
        };
        // Text with '\n' to trigger multiple-line splitting path.
        let text = "Short.\nA very long line that goes on and on without sentence\nboundaries to trigger forced line splitting here indeed.";
        let boundaries =
            split_text_by_sentences(text, 0..text.len(), ChunkPath::Embedding, &config);

        assert!(
            boundaries.len() > 1,
            "should have >1 boundaries, got {}",
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
        assert_eq!(boundaries.last().unwrap().end_byte, text.len());
        // Partition must be contiguous and cover the whole range.
        let mut prev = 0;
        for b in &boundaries {
            assert_eq!(b.start_byte, prev);
            prev = b.end_byte;
        }
        assert_eq!(prev, text.len());
    }

    #[test]
    fn test_split_by_sentences_empty_text() {
        let config = ChunkingConfig::default();
        let boundaries = split_text_by_sentences("", 0..0, ChunkPath::Bm25, &config);
        assert!(boundaries.is_empty());
    }
}
