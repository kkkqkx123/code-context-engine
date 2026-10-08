//! Embedded-engine adapter over the shared segmentation core.
//!
//! The segmentation rules live in [`crate::segment`]; this module owns the
//! tokenizer handle used across the codebase plus the optional engine
//! stream adapter. Consumers that only need word strings or offsets use
//! [`MixedTokenizer::tokenize`] and [`MixedTokenizer::tokenize_offsets`]
//! without the engine dependency.

use jieba_rs::Jieba;

use crate::segment::{MixedToken, segment_text, shared_jieba};

#[derive(Clone)]
pub struct MixedTokenizer {
    jieba: &'static Jieba,
}

impl MixedTokenizer {
    pub fn new() -> Self {
        Self {
            jieba: shared_jieba(),
        }
    }

    /// Tokenize text into words, returning only the word strings.
    /// Used externally for word counting during chunking.
    pub fn tokenize(&self, text: &str) -> Vec<String> {
        segment_text(text, self.jieba)
            .into_iter()
            .map(|token| token.text)
            .collect()
    }

    /// Tokenize text, returning full token metadata including byte offsets.
    ///
    /// This is the canonical public entry for consumers that need span
    /// information (highlighting, benchmarks) and must stay symmetric with the
    /// engine `Tokenizer` implementation used during indexing.
    pub fn tokenize_offsets(&self, text: &str) -> Vec<MixedToken> {
        segment_text(text, self.jieba)
    }
}

impl Default for MixedTokenizer {
    fn default() -> Self {
        Self::new()
    }
}

/// Embedded-engine stream adapter, available with the `tantivy` feature.
#[cfg(feature = "tantivy")]
mod engine {
    use tantivy::tokenizer::{Token, TokenStream, Tokenizer};

    use super::MixedTokenizer;
    use crate::segment::MixedToken;

    pub struct MixedTokenStream<'a> {
        tokens: Vec<MixedToken>,
        pos: usize,
        token: Token,
        _phantom: std::marker::PhantomData<&'a ()>,
    }

    impl<'a> MixedTokenStream<'a> {
        fn from_tokens(tokens: Vec<MixedToken>) -> Self {
            Self {
                tokens,
                pos: 0,
                token: Token::default(),
                _phantom: std::marker::PhantomData,
            }
        }
    }

    impl Tokenizer for MixedTokenizer {
        type TokenStream<'a> = MixedTokenStream<'a>;

        fn token_stream<'a>(&'a mut self, text: &'a str) -> Self::TokenStream<'a> {
            MixedTokenStream::from_tokens(self.tokenize_offsets(text))
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
}

#[cfg(feature = "tantivy")]
pub use engine::MixedTokenStream;

#[cfg(test)]
mod tests {
    use super::*;

    fn collect_texts(text: &str) -> Vec<String> {
        MixedTokenizer::default().tokenize(text)
    }

    #[test]
    fn test_chinese_tokenization() {
        let tokens = collect_texts("计算总价");
        assert!(tokens.contains(&"计算".to_string()));
        assert!(tokens.contains(&"总价".to_string()));
    }

    #[test]
    fn test_english_tokenization() {
        let tokens = collect_texts("calculate total price");
        assert!(tokens.contains(&"calculate".to_string()));
        assert!(tokens.contains(&"total".to_string()));
        assert!(tokens.contains(&"price".to_string()));
    }

    #[test]
    fn test_mixed_tokenization() {
        let tokens = collect_texts("计算total price");
        assert!(tokens.contains(&"计算".to_string()));
        assert!(tokens.contains(&"total".to_string()));
        assert!(tokens.contains(&"price".to_string()));
    }

    #[test]
    fn test_snake_case_split() {
        let tokens = collect_texts("get_or_init");
        assert!(tokens.contains(&"get_or_init".to_string()));
        assert!(tokens.contains(&"get".to_string()));
        assert!(tokens.contains(&"or".to_string()));
        assert!(tokens.contains(&"init".to_string()));
    }

    #[test]
    fn test_camel_case_split() {
        let tokens = collect_texts("calculateTotal");
        assert!(tokens.contains(&"calculatetotal".to_string()));
        assert!(tokens.contains(&"calculate".to_string()));
        assert!(tokens.contains(&"total".to_string()));
    }

    #[test]
    fn test_path_split() {
        let tokens = collect_texts("std::path::Path");
        assert!(tokens.contains(&"std::path::path".to_string()));
        assert!(tokens.contains(&"std".to_string()));
        assert!(tokens.contains(&"path".to_string()));
    }

    #[test]
    fn test_kebab_case_split() {
        let tokens = collect_texts("utf-8");
        assert!(tokens.contains(&"utf-8".to_string()));
        assert!(tokens.contains(&"utf".to_string()));
    }

    #[test]
    fn test_tokenize_matches_offsets_texts() {
        let text = "计算total price get_or_init 数据库连接";
        let tokenizer = MixedTokenizer::default();
        let words = tokenizer.tokenize(text);
        let offsets: Vec<String> = tokenizer
            .tokenize_offsets(text)
            .into_iter()
            .map(|t| t.text)
            .collect();
        assert_eq!(words, offsets);
    }

    #[test]
    fn test_case_lowered() {
        let tokens = collect_texts("Hello World");
        assert!(tokens.contains(&"hello".to_string()));
        assert!(tokens.contains(&"world".to_string()));
    }

    #[test]
    fn test_empty_input() {
        assert!(collect_texts("").is_empty());
    }

    #[test]
    fn test_single_char_tokens_preserved() {
        let tokens = collect_texts("a b cd ef");
        assert!(tokens.contains(&"a".to_string()));
        assert!(tokens.contains(&"b".to_string()));
        assert!(tokens.contains(&"cd".to_string()));
        assert!(tokens.contains(&"ef".to_string()));
    }

    #[test]
    fn test_qualified_path_dual_form() {
        let tokens = collect_texts("OnceCell::get_or_init");
        assert!(tokens.contains(&"oncecell::get_or_init".to_string()));
        assert!(tokens.contains(&"once".to_string()));
        assert!(tokens.contains(&"cell".to_string()));
        assert!(tokens.contains(&"get".to_string()));
        assert!(tokens.contains(&"or".to_string()));
        assert!(tokens.contains(&"init".to_string()));
    }

    #[cfg(feature = "tantivy")]
    mod engine_tests {
        use tantivy::tokenizer::{Token, TokenStream, Tokenizer};

        use super::MixedTokenizer;

        fn collect_stream(text: &str) -> Vec<Token> {
            let mut tokenizer = MixedTokenizer::default();
            let mut stream = tokenizer.token_stream(text);
            let mut tokens = Vec::new();
            let mut collect = |token: &Token| tokens.push(token.clone());
            stream.process(&mut collect);
            tokens
        }

        #[test]
        fn stream_matches_pure_segmentation() {
            let tokenizer = MixedTokenizer::default();
            let expected = tokenizer.tokenize_offsets("计算total price");
            let streamed = collect_stream("计算total price");
            let texts: Vec<String> = streamed.iter().map(|t| t.text.clone()).collect();
            let plain: Vec<String> = expected.iter().map(|t| t.text.clone()).collect();
            assert_eq!(texts, plain);
        }

        #[test]
        fn split_tokens_share_position() {
            let tokens = collect_stream("get_or_init");
            let positions: std::collections::HashSet<usize> =
                tokens.iter().map(|t| t.position).collect();
            assert_eq!(positions.len(), 1);
            let original = tokens
                .iter()
                .find(|t| t.text == "get_or_init")
                .expect("original");
            assert_eq!(original.position_length, 1);
        }

        #[test]
        fn byte_offsets_match_source_spans() {
            let tokens = collect_stream("hello world");
            assert_eq!(tokens[0].text, "hello");
            assert_eq!((tokens[0].offset_from, tokens[0].offset_to), (0, 5));
            let world = tokens
                .iter()
                .find(|t| t.text == "world")
                .expect("world token");
            assert_eq!((world.offset_from, world.offset_to), (6, 11));
        }
    }
}
