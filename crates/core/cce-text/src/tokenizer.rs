use jieba_rs::{Jieba, TokenizeMode};
use std::sync::OnceLock;
use tantivy::tokenizer::{Token, TokenStream, Tokenizer};

use cce_utils::text::split_identifier;

/// Shared Jieba instance.
///
/// `Jieba::new()` eagerly loads the full default dictionary (several MB) and
/// builds a cedar trie, which is far too expensive to repeat per call. All
/// `MixedTokenizer` instances share one lazily-initialized, read-only `Jieba`.
static SHARED_JIEBA: OnceLock<Jieba> = OnceLock::new();

/// A single token produced by [`MixedTokenizer::tokenize_offsets`].
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

#[derive(Clone)]
pub struct MixedTokenizer {
    jieba: &'static Jieba,
}

impl MixedTokenizer {
    pub fn new() -> Self {
        Self {
            jieba: SHARED_JIEBA.get_or_init(Jieba::new),
        }
    }

    /// Tokenize text into words, returning only the word strings.
    /// Used externally for word counting during chunking.
    pub fn tokenize(&self, text: &str) -> Vec<String> {
        self.tokenize_offsets(text)
            .into_iter()
            .map(|td| td.text)
            .collect()
    }

    /// Tokenize text, returning full token metadata including byte offsets.
    ///
    /// This is the canonical public entry for consumers that need span
    /// information (highlighting, benchmarks) and must stay symmetric with the
    /// tantivy `Tokenizer` implementation used during indexing.
    pub fn tokenize_offsets(&self, text: &str) -> Vec<MixedToken> {
        MixedTokenStream::tokenize_text(text, self.jieba)
            .into_iter()
            .map(|td| MixedToken {
                text: td.text,
                offset_from: td.offset_from,
                offset_to: td.offset_to,
                position: td.position,
                position_length: td.position_length,
            })
            .collect()
    }
}

impl Default for MixedTokenizer {
    fn default() -> Self {
        Self::new()
    }
}

impl Tokenizer for MixedTokenizer {
    type TokenStream<'a> = MixedTokenStream<'a>;

    fn token_stream<'a>(&'a mut self, text: &'a str) -> Self::TokenStream<'a> {
        MixedTokenStream::new(text, self.jieba)
    }
}

pub struct MixedTokenStream<'a> {
    tokens: Vec<TokenData>,
    pos: usize,
    token: Token,
    _phantom: std::marker::PhantomData<&'a ()>,
}

struct TokenData {
    text: String,
    offset_from: usize,
    offset_to: usize,
    /// Token position in the stream. Tokens from the same word share the
    /// same position (they are alternatives). Split tokens are marked with
    /// `position_length=0` so they don't participate in phrase queries.
    position: u32,
    /// Span length. `1` for original tokens, `0` for split (auxiliary) tokens.
    position_length: u32,
}

impl<'a> MixedTokenStream<'a> {
    fn new(text: &'a str, jieba: &'static Jieba) -> Self {
        let tokens = Self::tokenize_text(text, jieba);
        Self {
            tokens,
            pos: 0,
            token: Token::default(),
            _phantom: std::marker::PhantomData,
        }
    }

    fn tokenize_text(text: &str, jieba: &Jieba) -> Vec<TokenData> {
        let mut result = Vec::new();
        let mut current_position: u32 = 0;
        let mut i = 0;

        while i < text.len() {
            // The byte index `i` is always advanced by `len_utf8()`, so it
            // stays on a char boundary; still guard against an unexpected
            // non-boundary by skipping the byte defensively.
            let Some(c) = text[i..].chars().next() else {
                i += 1;
                continue;
            };
            let char_len = c.len_utf8();

            if Self::is_cjk(c) {
                let cjk_start = i;
                let cjk_end;
                i += char_len;
                loop {
                    if i >= text.len() {
                        cjk_end = i;
                        break;
                    }
                    let Some(nc) = text[i..].chars().next() else {
                        i += 1;
                        cjk_end = i;
                        break;
                    };
                    if Self::is_cjk(nc) {
                        i += nc.len_utf8();
                    } else {
                        cjk_end = i;
                        break;
                    }
                }

                let cjk_text = &text[cjk_start..cjk_end];
                let char_offsets = Self::calc_char_offsets(cjk_text);
                let jieba_tokens = jieba.tokenize(cjk_text, TokenizeMode::Search, true);
                for jt in jieba_tokens {
                    let byte_start = char_offsets.get(jt.start).copied().unwrap_or(0);
                    let byte_end = char_offsets.get(jt.end).copied().unwrap_or(cjk_text.len());
                    result.push(TokenData {
                        text: jt.word.to_string(),
                        offset_from: cjk_start + byte_start,
                        offset_to: cjk_start + byte_end,
                        position: current_position,
                        position_length: 1,
                    });
                    current_position += 1;
                }
            } else if c.is_whitespace() {
                i += char_len;
            } else {
                let word_start = i;
                let word_end;
                i += char_len;
                loop {
                    if i >= text.len() {
                        word_end = i;
                        break;
                    }
                    let Some(nc) = text[i..].chars().next() else {
                        i += 1;
                        word_end = i;
                        break;
                    };
                    if Self::is_cjk(nc) || nc.is_whitespace() {
                        word_end = i;
                        break;
                    }
                    i += nc.len_utf8();
                }

                let word_text = &text[word_start..word_end];
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
                result.push(TokenData {
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
                        result.push(TokenData {
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
}

impl TokenStream for MixedTokenStream<'_> {
    fn advance(&mut self) -> bool {
        if self.pos < self.tokens.len() {
            let data = &self.tokens[self.pos];
            self.token = Token {
                offset_from: data.offset_from,
                offset_to: data.offset_to,
                position: data.position as usize,
                position_length: data.position_length as usize,
                text: data.text.clone(),
            };
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn token(&self) -> &Token {
        &self.token
    }

    fn token_mut(&mut self) -> &mut Token {
        &mut self.token
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect_tokens(text: &str) -> Vec<String> {
        let mut tokenizer = MixedTokenizer::default();
        let mut stream = tokenizer.token_stream(text);
        let mut tokens = Vec::new();
        let mut collect = |token: &Token| tokens.push(token.text.clone());
        stream.process(&mut collect);
        tokens
    }

    fn collect_tokens_with_positions(text: &str) -> Vec<(String, usize, usize)> {
        let mut tokenizer = MixedTokenizer::default();
        let mut stream = tokenizer.token_stream(text);
        let mut tokens = Vec::new();
        let mut collect = |token: &Token| {
            tokens.push((token.text.clone(), token.position, token.position_length))
        };
        stream.process(&mut collect);
        tokens
    }

    #[test]
    fn test_chinese_tokenization() {
        let tokens = collect_tokens("计算总价");
        assert!(tokens.contains(&"计算".to_string()));
        assert!(tokens.contains(&"总价".to_string()));
    }

    #[test]
    fn test_english_tokenization() {
        let tokens = collect_tokens("calculate total price");
        assert!(tokens.contains(&"calculate".to_string()));
        assert!(tokens.contains(&"total".to_string()));
        assert!(tokens.contains(&"price".to_string()));
    }

    #[test]
    fn test_mixed_tokenization() {
        let tokens = collect_tokens("计算total price");
        assert!(tokens.contains(&"计算".to_string()));
        assert!(tokens.contains(&"total".to_string()));
        assert!(tokens.contains(&"price".to_string()));
    }

    #[test]
    fn test_snake_case_split() {
        let tokens = collect_tokens("get_or_init");
        assert!(tokens.contains(&"get_or_init".to_string()));
        assert!(tokens.contains(&"get".to_string()));
        assert!(tokens.contains(&"or".to_string()));
        assert!(tokens.contains(&"init".to_string()));
    }

    #[test]
    fn test_camel_case_split() {
        let tokens = collect_tokens("calculateTotal");
        assert!(tokens.contains(&"calculatetotal".to_string()));
        assert!(tokens.contains(&"calculate".to_string()));
        assert!(tokens.contains(&"total".to_string()));
    }

    #[test]
    fn test_path_split() {
        let tokens = collect_tokens("std::path::Path");
        assert!(tokens.contains(&"std::path::path".to_string()));
        assert!(tokens.contains(&"std".to_string()));
        assert!(tokens.contains(&"path".to_string()));
    }

    #[test]
    fn test_kebab_case_split() {
        let tokens = collect_tokens("utf-8");
        assert!(tokens.contains(&"utf-8".to_string()));
        assert!(tokens.contains(&"utf".to_string()));
    }

    #[test]
    fn test_split_tokens_share_position() {
        let tokens = collect_tokens_with_positions("get_or_init");
        // All tokens from "get_or_init" should share the same position
        let positions: Vec<usize> = tokens.iter().map(|(_, pos, _)| *pos).collect();
        let unique_positions: Vec<usize> = positions
            .into_iter()
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        assert_eq!(unique_positions.len(), 1);
    }

    #[test]
    fn test_original_has_full_span() {
        let tokens = collect_tokens_with_positions("get_or_init");
        // The original token should have position_length=1
        let original = tokens
            .iter()
            .find(|(text, _, pl)| text == "get_or_init" && *pl == 1);
        assert!(original.is_some());
        // Split tokens should have position_length=0
        let splits: Vec<_> = tokens
            .iter()
            .filter(|(text, _, pl)| text != "get_or_init" && *pl == 0)
            .collect();
        assert_eq!(splits.len(), 3); // get, or, init
    }

    #[test]
    fn test_byte_offsets() {
        let mut tokenizer = MixedTokenizer::default();
        let mut stream = tokenizer.token_stream("hello world");
        let mut tokens = Vec::new();
        let mut collect = |token: &Token| {
            tokens.push((token.text.clone(), token.offset_from, token.offset_to));
        };
        stream.process(&mut collect);
        // The first token is "hello" (original)
        assert_eq!(tokens[0].0, "hello");
        assert_eq!(tokens[0].1, 0);
        assert_eq!(tokens[0].2, 5);
        // Skip split tokens, find "world" (original)
        let world = tokens
            .iter()
            .find(|(t, _, _)| t == "world")
            .expect("world token");
        assert_eq!(world.1, 6);
        assert_eq!(world.2, 11);
    }

    #[test]
    fn test_chinese_byte_offsets() {
        let mut tokenizer = MixedTokenizer::default();
        let mut stream = tokenizer.token_stream("计算总价");
        let mut tokens = Vec::new();
        let mut collect = |token: &Token| {
            tokens.push((token.text.clone(), token.offset_from, token.offset_to));
        };
        stream.process(&mut collect);
        assert_eq!(tokens[0].0, "计算");
        assert_eq!(tokens[1].0, "总价");
        assert!(tokens[0].1 < tokens[0].2);
        assert!(tokens[1].1 < tokens[1].2);
    }

    #[test]
    fn test_case_lowered() {
        let tokens = collect_tokens("Hello World");
        assert!(tokens.contains(&"hello".to_string()));
        assert!(tokens.contains(&"world".to_string()));
    }

    #[test]
    fn test_empty_input() {
        let tokens = collect_tokens("");
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_single_char_tokens_preserved() {
        let tokens = collect_tokens("a b cd ef");
        assert!(tokens.contains(&"a".to_string()));
        assert!(tokens.contains(&"b".to_string()));
        assert!(tokens.contains(&"cd".to_string()));
        assert!(tokens.contains(&"ef".to_string()));
    }

    #[test]
    fn test_qualified_path_dual_form() {
        let tokens = collect_tokens("OnceCell::get_or_init");
        // Original form (lowercased)
        assert!(tokens.contains(&"oncecell::get_or_init".to_string()));
        // Split forms
        assert!(tokens.contains(&"once".to_string()));
        assert!(tokens.contains(&"cell".to_string()));
        assert!(tokens.contains(&"get".to_string()));
        assert!(tokens.contains(&"or".to_string()));
        assert!(tokens.contains(&"init".to_string()));
    }
}
