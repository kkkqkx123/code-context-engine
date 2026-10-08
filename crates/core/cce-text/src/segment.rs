//! Pure segmentation core shared by every fulltext branch.
//!
//! This module owns the identifier splitting plus CJK dictionary
//! segmentation rules and exposes them without any search-engine types,
//! so remote branches can reuse the exact token stream without pulling
//! the embedded engine dependency.

use jieba_rs::{Jieba, TokenizeMode};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

use cce_utils::text::split_identifier;

/// Upper bound for tokenized input length, in characters.
///
/// Inputs beyond this limit are truncated to the prefix before
/// segmentation. Leading terms dominate BM25 scoring, so the prefix keeps
/// retrieval quality while pathological blobs stay bounded. Truncations are
/// counted in [`truncated_input_count`].
pub const MAX_TOKENIZE_CHARS: usize = 32_768;

/// Maximum consecutive CJK characters sent to the dictionary segmenter in a
/// single call. Longer runs are split into character-aligned windows so one
/// huge run cannot spike indexing latency. The window is orders of magnitude
/// larger than common words, keeping boundary effects negligible.
const MAX_CJK_RUN_CHARS: usize = 4096;

/// Number of inputs truncated by the length guard since process start.
static TRUNCATED_INPUTS: AtomicU64 = AtomicU64::new(0);

/// Number of inputs truncated by [`MAX_TOKENIZE_CHARS`] so far.
pub fn truncated_input_count() -> u64 {
    TRUNCATED_INPUTS.load(Ordering::Relaxed)
}

/// Truncate over-long inputs to the [`MAX_TOKENIZE_CHARS`] prefix,
/// preserving a character boundary. Short inputs pass through untouched.
fn cap_text(text: &str) -> &str {
    match text.char_indices().nth(MAX_TOKENIZE_CHARS) {
        Some((byte_idx, _)) => {
            TRUNCATED_INPUTS.fetch_add(1, Ordering::Relaxed);
            &text[..byte_idx]
        }
        None => text,
    }
}

/// Shared Jieba instance.
///
/// `Jieba::new()` eagerly loads the full default dictionary (several MB) and
/// builds a cedar trie, which is far too expensive to repeat per call. All
/// tokenizer instances share one lazily-initialized, read-only `Jieba`.
static SHARED_JIEBA: OnceLock<Jieba> = OnceLock::new();

/// Access the shared dictionary segmenter.
pub fn shared_jieba() -> &'static Jieba {
    SHARED_JIEBA.get_or_init(Jieba::new)
}

/// A single token produced by [`segment_text`].
///
/// Exposes the full token metadata (text, byte offsets, position) so that
/// downstream consumers (highlighting, benchmarks) can reconstruct token
/// spans without re-implementing the tokenization rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MixedToken {
    /// Token text (lowercased).
    pub text: String,
    /// Byte offset of the token start within the input text.
    pub offset_from: usize,
    /// Byte offset of the token end (exclusive) within the input text.
    pub offset_to: usize,
    /// Token position. Tokens sharing the same source word share a position.
    pub position: u32,
    /// Span length: `1` for original tokens, `0` for split (auxiliary) tokens.
    pub position_length: u32,
}

/// Segment text with the shared rules, returning full token metadata.
///
/// This is the canonical entry for every consumer that does not need the
/// embedded engine stream type. The engine adapter converts these tokens
/// without re-implementing the rules.
pub fn segment_text(text: &str, jieba: &Jieba) -> Vec<MixedToken> {
    let capped = cap_text(text);
    let mut result = Vec::new();
    let mut current_position: u32 = 0;
    let mut i = 0;

    while i < capped.len() {
        // The byte index `i` is always advanced by `len_utf8()`, so it
        // stays on a char boundary; still guard against an unexpected
        // non-boundary by skipping the byte defensively.
        let Some(c) = capped[i..].chars().next() else {
            i += 1;
            continue;
        };
        let char_len = c.len_utf8();

        if is_cjk(c) {
            let cjk_start = i;
            let cjk_end;
            i += char_len;
            loop {
                if i >= capped.len() {
                    cjk_end = i;
                    break;
                }
                let Some(nc) = capped[i..].chars().next() else {
                    i += 1;
                    cjk_end = i;
                    break;
                };
                if is_cjk(nc) {
                    i += nc.len_utf8();
                } else {
                    cjk_end = i;
                    break;
                }
            }

            let cjk_text = &capped[cjk_start..cjk_end];
            let char_offsets = calc_char_offsets(cjk_text);
            let total_chars = char_offsets.len().saturating_sub(1);
            let mut window_char_start = 0usize;
            while window_char_start < total_chars {
                let window_char_end = (window_char_start + MAX_CJK_RUN_CHARS).min(total_chars);
                let window =
                    &cjk_text[char_offsets[window_char_start]..char_offsets[window_char_end]];
                let window_base = cjk_start + char_offsets[window_char_start];
                push_cjk_window(
                    &mut result,
                    &mut current_position,
                    jieba,
                    window_base,
                    window,
                );
                window_char_start = window_char_end;
            }
        } else if c.is_whitespace() {
            i += char_len;
        } else {
            let word_start = i;
            let word_end;
            i += char_len;
            loop {
                if i >= capped.len() {
                    word_end = i;
                    break;
                }
                let Some(nc) = capped[i..].chars().next() else {
                    i += 1;
                    word_end = i;
                    break;
                };
                if is_cjk(nc) || nc.is_whitespace() {
                    word_end = i;
                    break;
                }
                i += nc.len_utf8();
            }

            let word_text = &capped[word_start..word_end];
            let Some(trimmed_start_byte) = word_text
                .char_indices()
                .find(|(_, ch)| ch.is_alphanumeric())
                .map(|(pos, _)| pos)
            else {
                continue;
            };
            let trimmed_end_byte = word_text
                .char_indices()
                .rfind(|(_, ch)| ch.is_alphanumeric())
                .map(|(pos, ch)| pos + ch.len_utf8())
                .unwrap_or(word_text.len());

            let trimmed = &word_text[trimmed_start_byte..trimmed_end_byte];
            let original_lower = trimmed.to_lowercase();

            // Output the original token (lowercased) at the current position
            result.push(MixedToken {
                text: original_lower.clone(),
                offset_from: word_start + trimmed_start_byte,
                offset_to: word_start + trimmed_end_byte,
                position: current_position,
                position_length: 1,
            });

            // Output split tokens at the same position (auxiliary, position_length=0)
            let split_words = split_identifier(trimmed);
            for word in split_words {
                if word != original_lower {
                    result.push(MixedToken {
                        text: word,
                        offset_from: word_start + trimmed_start_byte,
                        offset_to: word_start + trimmed_end_byte,
                        position: current_position,
                        position_length: 0,
                    });
                }
            }

            current_position += 1;
        }
    }

    result
}

fn is_cjk(c: char) -> bool {
    matches!(c,
        '\u{4E00}'..='\u{9FFF}' |
        '\u{3400}'..='\u{4DBF}' |
        '\u{20000}'..='\u{2A6DF}' |
        '\u{2A700}'..='\u{2B73F}' |
        '\u{2B740}'..='\u{2B81F}' |
        '\u{2B820}'..='\u{2CEAF}' |
        '\u{F900}'..='\u{FAFF}' |
        '\u{2F800}'..='\u{2FA1F}' |
        // Japanese hiragana
        '\u{3040}'..='\u{309F}' |
        // Japanese katakana
        '\u{30A0}'..='\u{30FF}' |
        '\u{31F0}'..='\u{31FF}' |
        '\u{FF66}'..='\u{FF9D}' |
        // Korean Hangul syllables, Jamo, and compatibility Jamo
        '\u{AC00}'..='\u{D7AF}' |
        '\u{1100}'..='\u{11FF}' |
        '\u{3130}'..='\u{318F}'
    )
}

fn calc_char_offsets(text: &str) -> Vec<usize> {
    let mut offsets = Vec::with_capacity(text.chars().count() + 1);
    offsets.push(0);
    for (byte_index, _) in text.char_indices().skip(1) {
        offsets.push(byte_index);
    }
    offsets.push(text.len());
    offsets
}

/// Segment one character-aligned CJK window through the dictionary and
/// append the resulting tokens with absolute byte offsets.
fn push_cjk_window(
    result: &mut Vec<MixedToken>,
    current_position: &mut u32,
    jieba: &Jieba,
    window_base: usize,
    window: &str,
) {
    let char_offsets = calc_char_offsets(window);
    let jieba_tokens = jieba.tokenize(window, TokenizeMode::Search, true);
    for jt in jieba_tokens {
        let byte_start = char_offsets.get(jt.start).copied().unwrap_or(0);
        let byte_end = char_offsets.get(jt.end).copied().unwrap_or(window.len());
        result.push(MixedToken {
            text: jt.word.to_string(),
            offset_from: window_base + byte_start,
            offset_to: window_base + byte_end,
            position: *current_position,
            position_length: 1,
        });
        *current_position += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(tokens: &[MixedToken]) -> Vec<&str> {
        tokens.iter().map(|t| t.text.as_str()).collect()
    }

    #[test]
    fn chinese_segments_through_dictionary() {
        let tokens = segment_text("计算总价", shared_jieba());
        let words = texts(&tokens);
        assert!(words.contains(&"计算"));
        assert!(words.contains(&"总价"));
    }

    #[test]
    fn identifier_splits_share_position() {
        let tokens = segment_text("get_or_init", shared_jieba());
        assert!(texts(&tokens).contains(&"get_or_init"));
        let positions: std::collections::HashSet<u32> = tokens.iter().map(|t| t.position).collect();
        assert_eq!(positions.len(), 1);
        let original = tokens
            .iter()
            .find(|t| t.text == "get_or_init")
            .expect("original");
        assert_eq!(original.position_length, 1);
    }

    #[test]
    fn long_input_truncates_to_prefix() {
        let long = "数据库连接池".repeat(8000);
        assert!(long.chars().count() > MAX_TOKENIZE_CHARS);
        let before = truncated_input_count();
        let words = segment_text(&long, shared_jieba());
        assert!(truncated_input_count() > before);
        let prefix: String = long.chars().take(MAX_TOKENIZE_CHARS).collect();
        assert_eq!(words, segment_text(&prefix, shared_jieba()));
    }
}
