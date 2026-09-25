//! Token estimation for mixed-language text
//!
//! Provides fast token count estimation supporting ASCII, CJK, and other Unicode.
//! Used across the codebase for LLM request sizing, chunking, and content compression.

/// Tokens per ASCII symbol (punctuation) character (default: 0.5)
///
/// Symbols like `->`, `(`, `{` are typically tokenized separately from
/// identifiers. Counting them at the latin factor would underestimate
/// code-heavy text; 0.5 per symbol approximates standalone punctuation tokens.
/// Tokens per ASCII punctuation character. Code-dense text (long identifiers,
/// heavy punctuation) tokenizes at roughly 2 chars/token on provider
/// tokenizers; punctuation counted at 1.0 keeps estimates on the high side
/// instead of systematically underestimating and sending over-limit requests.
pub const SYMBOL_FACTOR: f32 = 1.0;

/// Token estimator for mixed-language text
#[derive(Debug, Clone, Copy)]
pub struct TokenEstimator {
    /// Tokens per CJK character (default: 1.0)
    cjk_factor: f32,
    /// Tokens per Latin character (default: 0.25, ~4 chars per token)
    latin_factor: f32,
}

impl Default for TokenEstimator {
    fn default() -> Self {
        Self {
            cjk_factor: 1.0,
            latin_factor: 0.25,
        }
    }
}

impl TokenEstimator {
    /// Create a new token estimator with custom factors
    ///
    /// # Arguments
    ///
    /// * `cjk_factor` - Tokens per CJK character (default: 1.0)
    /// * `latin_factor` - Tokens per Latin character (default: 0.25, ~4 chars per token)
    ///
    /// # Example
    ///
    /// ```
    /// use cce_core::utils::token_estimation::TokenEstimator;
    ///
    /// // Create estimator for models with different tokenization behavior
    /// let estimator = TokenEstimator::new(0.8, 0.3);
    /// ```
    pub const fn new(cjk_factor: f32, latin_factor: f32) -> Self {
        Self {
            cjk_factor,
            latin_factor,
        }
    }

    /// Estimate token count for text using default factors
    ///
    /// This is a convenience static method that uses default tokenization factors.
    /// For custom factors, create an instance with `TokenEstimator::new()`.
    ///
    /// # Example
    ///
    /// ```
    /// use cce_core::utils::token_estimation::TokenEstimator;
    ///
    /// let tokens = TokenEstimator::estimate("Hello world");
    /// ```
    pub fn estimate(text: &str) -> usize {
        DEFAULT_ESTIMATOR.estimate_with_config(text)
    }

    /// Estimate token count for text with this estimator's configuration
    ///
    /// Uses fast path for ASCII-only text, otherwise processes character by character.
    pub fn estimate_with_config(&self, text: &str) -> usize {
        let bytes = text.as_bytes();

        // Fast path: ASCII-only text
        if text.is_ascii() {
            let ascii_ws =
                count_byte(bytes, b' ') + count_byte(bytes, b'\t') + count_byte(bytes, b'\n');
            let non_ws = text.len() - ascii_ws;
            let symbols = bytes.iter().filter(|&&b| b.is_ascii_punctuation()).count();
            let letters = non_ws - symbols;
            let tokens = letters as f32 * self.latin_factor
                + symbols as f32 * SYMBOL_FACTOR
                + ascii_ws as f32 * 0.5;
            return tokens.round() as usize;
        }

        let mut count = 0.0f32;

        // Count ASCII whitespace quickly
        let ascii_ws =
            count_byte(bytes, b' ') + count_byte(bytes, b'\t') + count_byte(bytes, b'\n');
        count += ascii_ws as f32 * 0.5;

        // Process each character
        for ch in text.chars() {
            if ch.is_ascii_whitespace() || ch.is_ascii_control() {
                continue;
            }

            if Self::is_cjk(ch) {
                count += self.cjk_factor;
            } else if ch.is_ascii_punctuation() {
                // Symbols are tokenized separately from identifiers
                count += SYMBOL_FACTOR;
            } else if ch.is_ascii() {
                count += self.latin_factor;
            } else {
                // Other Unicode (emoji, symbols, etc.) - count as 1 token
                count += 1.0;
            }
        }

        count.round() as usize
    }

    /// Estimate tokens for text (instance method)
    ///
    /// Convenience method that delegates to estimate_with_config.
    pub fn estimate_text(&self, text: &str) -> usize {
        self.estimate_with_config(text)
    }

    /// Check if character is CJK (Chinese, Japanese, Korean)
    #[inline]
    fn is_cjk(ch: char) -> bool {
        let code = ch as u32;
        // CJK Unified Ideographs
        (0x4E00..=0x9FFF).contains(&code)
            || (0x3400..=0x4DBF).contains(&code)
            || (0x20000..=0x2A6DF).contains(&code)
            // Hiragana & Katakana
            || (0x3040..=0x309F).contains(&code)
            || (0x30A0..=0x30FF).contains(&code)
            // Hangul (Korean)
            || (0xAC00..=0xD7AF).contains(&code)
            || (0x1100..=0x11FF).contains(&code)
            || (0x3130..=0x318F).contains(&code)
    }

    /// Find split point that fits within token limit
    ///
    /// Returns the byte offset where to split.
    pub fn find_split_point(&self, text: &str, max_tokens: usize) -> usize {
        let mut bytes = 0;
        let mut tokens = 0.0f32;
        let mut last_break = 0; // byte index of last safe split (newline/space)

        for ch in text.chars() {
            let ch_bytes = ch.len_utf8();
            let ch_tokens = if TokenEstimator::is_cjk(ch) {
                self.cjk_factor
            } else if ch.is_ascii_punctuation() {
                SYMBOL_FACTOR
            } else if ch.is_ascii() && !ch.is_ascii_whitespace() && !ch.is_ascii_control() {
                self.latin_factor
            } else if ch.is_ascii_whitespace() {
                0.5
            } else {
                1.0
            };

            if tokens + ch_tokens > max_tokens as f32 {
                // If we have a previous safe break, split there; otherwise split at current byte offset
                return if last_break > 0 { last_break } else { bytes };
            }

            tokens += ch_tokens;
            bytes += ch_bytes;

            // Record safe split positions (after newline or space)
            if ch == '\n' || ch == ' ' {
                last_break = bytes;
            }
        }

        text.len()
    }
}

/// Count occurrences of a byte in a slice
#[inline]
fn count_byte(bytes: &[u8], target: u8) -> usize {
    bytes.iter().filter(|&&b| b == target).count()
}

/// Global default estimator
static DEFAULT_ESTIMATOR: TokenEstimator = TokenEstimator {
    cjk_factor: 1.0,
    latin_factor: 0.25,
};

/// Estimate tokens using default estimator
///
/// This is the primary function for token estimation.
pub fn estimate_tokens(text: &str) -> usize {
    TokenEstimator::estimate(text)
}

/// Proportional cut applied per truncation round (80% = 4/5 of current length)
const TRUNCATE_RATIO_NUM: usize = 4;
const TRUNCATE_RATIO_DEN: usize = 5;

/// Never truncate below this byte length; shorter text that still fails
/// should surface as an error rather than be shaved to nothing.
const MIN_TRUNCATED_LEN: usize = 2_000;

/// Outcome of [`truncate_to_token_budget`].
#[derive(Debug, Clone)]
pub struct TruncationResult {
    /// The (possibly shortened) text
    pub text: String,
    /// Whether any content was removed
    pub truncated: bool,
    /// Byte length before truncation
    pub original_len: usize,
    /// Byte length after truncation
    pub final_len: usize,
    /// Estimated tokens before truncation
    pub original_estimate: usize,
}

/// Proportionally truncate text until its token estimate fits `max_tokens`.
///
/// The estimator can underestimate code-dense text, so instead of trusting a
/// single absolute split the loop cuts the text to 80% of its current length
/// (aligned to the last line boundary at or before the target) and re-estimates,
/// repeating until the estimate fits or the minimum length floor is reached.
/// A final `find_split_point` pass guarantees an absolute cut when the floor is
/// hit while the estimate is still over budget.
pub fn truncate_to_token_budget(text: &str, max_tokens: usize) -> TruncationResult {
    let estimator = TokenEstimator::default();
    let original_len = text.len();
    let original_estimate = estimator.estimate_text(text);

    if original_estimate <= max_tokens {
        return TruncationResult {
            text: text.to_string(),
            truncated: false,
            original_len,
            final_len: original_len,
            original_estimate,
        };
    }

    let mut current = text.to_string();
    while estimator.estimate_text(&current) > max_tokens {
        let mut target = current.len() * TRUNCATE_RATIO_NUM / TRUNCATE_RATIO_DEN;
        while !current.is_char_boundary(target) {
            target -= 1;
        }
        if target < MIN_TRUNCATED_LEN {
            break;
        }
        let cut = current[..target]
            .rfind('\n')
            .map(|p| p + 1)
            .unwrap_or(target);
        tracing::info!(
            from_bytes = current.len(),
            to_bytes = cut,
            estimate = estimator.estimate_text(&current),
            max_tokens,
            "Text over token budget, truncating proportionally at line boundary"
        );
        current = current[..cut].trim_end().to_string();
    }

    if estimator.estimate_text(&current) > max_tokens {
        let split = estimator.find_split_point(&current, max_tokens);
        tracing::info!(
            from_bytes = current.len(),
            to_bytes = split,
            max_tokens,
            "Proportional truncation hit floor, applying absolute split"
        );
        current = current[..split].to_string();
    }

    let final_len = current.len();
    TruncationResult {
        text: current,
        truncated: final_len < original_len,
        original_len,
        final_len,
        original_estimate,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_text() {
        assert_eq!(estimate_tokens(""), 0);
    }

    #[test]
    fn test_ascii_text() {
        let text = "Hello World";
        let tokens = estimate_tokens(text);
        // "Hello World" = 11 chars, ~3 tokens
        assert!((2..=4).contains(&tokens), "tokens: {}", tokens);
    }

    #[test]
    fn test_cjk_text() {
        let text = "Hello world.";
        let tokens = estimate_tokens(text);
        // 4 CJK chars = ~4 tokens
        assert!((3..=5).contains(&tokens), "tokens: {}", tokens);
    }

    #[test]
    fn test_mixed_text() {
        let text = "Hello World";
        let tokens = estimate_tokens(text);
        // "Hello" ~1-2 tokens + "World" ~1-2 tokens = ~2-4 tokens
        assert!((2..=6).contains(&tokens), "tokens: {}", tokens);
    }

    #[test]
    fn test_whitespace() {
        let text = "Hello   World";
        let tokens = estimate_tokens(text);
        assert!(tokens > 0);
    }

    #[test]
    fn test_long_ascii() {
        let text = "The quick brown fox jumps over the lazy dog";
        let tokens = estimate_tokens(text);
        // ~43 chars, ~11 tokens
        assert!((8..=15).contains(&tokens), "tokens: {}", tokens);
    }

    #[test]
    fn test_japanese() {
        let text = "Hello World";
        let tokens = estimate_tokens(text);
        // ASCII text: 11 chars / 4 = 2.75 -> rounded to 3 tokens
        assert_eq!(tokens, 3, "tokens: {}", tokens);
    }

    #[test]
    fn test_korean() {
        let text = "hello";
        let tokens = estimate_tokens(text);
        // ASCII text: 5 chars * 0.25 = 1.25 -> rounded to 1 token
        assert_eq!(tokens, 1, "tokens: {}", tokens);
    }

    #[test]
    fn test_custom_estimator() {
        // Test with custom factors for different tokenization behavior
        let estimator = TokenEstimator::new(0.8, 0.3);

        let text = "Hello";
        let tokens = estimator.estimate_with_config(text);
        // 5 ASCII chars * 0.3 = 1.5 -> rounded to 2 tokens
        assert_eq!(tokens, 2, "tokens: {}", tokens);

        let text = "How are you?";
        let tokens = estimator.estimate_with_config(text);
        // 9 letters * 0.3 = 2.7, symbol '?' * 1.0 = 1.0, 3 spaces * 0.5 = 1.5
        // total 5.2 -> 5 tokens
        assert_eq!(tokens, 5, "tokens: {}", tokens);
    }

    #[test]
    fn test_custom_estimator_ascii_fast_path() {
        // Test that ASCII fast path respects latin_factor
        let estimator = TokenEstimator::new(1.0, 0.2); // 5 chars per token

        let text = "ABCDEFGHIJ"; // 10 chars
        let tokens = estimator.estimate_with_config(text);
        // 10 chars / 5 = 2 tokens
        assert_eq!(tokens, 2, "tokens: {}", tokens);
    }

    #[test]
    fn test_default_estimator_consistency() {
        // Ensure default estimator matches estimate_tokens function
        let text = "Hello World 123";
        let estimator = TokenEstimator::default();
        assert_eq!(estimator.estimate_text(text), estimate_tokens(text));
    }

    #[test]
    fn test_ascii_symbols_counted_separately() {
        let text = "fn foo() -> u32";
        let tokens = estimate_tokens(text);
        // letters: "fnfoou32" = 8 * 0.25 = 2.0
        // symbols: "()->" = 4 * 1.0 = 4.0
        // whitespace: 3 * 0.5 = 1.5
        // total 7.5 -> 8
        assert_eq!(tokens, 8, "tokens: {}", tokens);
    }

    #[test]
    fn test_symbol_estimation_matches_find_split_point() {
        // Symbol-heavy text must be estimated consistently by the splitter
        let text = "map(|x| x + 1) -> Result<(), Error>";
        let estimator = TokenEstimator::default();
        let total = estimator.estimate_text(text);
        let split = estimator.find_split_point(text, total);
        // Split point must never exceed the requested budget (float accumulation
        // may stop a few bytes early at the exact boundary, never late).
        assert!(
            estimator.estimate_text(&text[..split]) <= total,
            "split text ({}) must fit within budget {}",
            estimator.estimate_text(&text[..split]),
            total
        );
        assert!(split > 0, "split point must be non-empty");
    }

    #[test]
    fn test_find_split_point() {
        let text = "This is a test string that is longer than the limit.";
        let split = TokenEstimator::default().find_split_point(text, 5);
        assert!(split < text.len());
        assert!(split > 0);
    }

    #[test]
    fn test_truncate_noop_within_budget() {
        let text = "short text";
        let result = truncate_to_token_budget(text, 8192);
        assert!(!result.truncated);
        assert_eq!(result.text, text);
        assert_eq!(result.original_len, result.final_len);
    }

    #[test]
    fn test_truncate_over_budget_fits_after_loop() {
        // Many lines so proportional line-boundary cuts converge above the floor.
        let text = "fn example() {\n    let x = compute(a, b, c);\n}\n".repeat(3000);
        let max_tokens = 7200;
        let result = truncate_to_token_budget(&text, max_tokens);
        assert!(result.truncated);
        assert!(result.final_len < result.original_len);
        assert!(
            TokenEstimator::default().estimate_text(&result.text) <= max_tokens,
            "truncated text must fit budget"
        );
        assert!(result.text.is_char_boundary(result.text.len()));
    }

    #[test]
    fn test_truncate_absolute_split_when_below_floor() {
        // A single long token-less run with no line boundaries and no spaces:
        // the proportional loop cannot cut above the floor, so the absolute
        // split must still bring the estimate within budget.
        let text = "a".repeat(2000); // ~500 tokens at 0.25/char
        let result = truncate_to_token_budget(&text, 50);
        assert!(result.truncated);
        assert!(TokenEstimator::default().estimate_text(&result.text) <= 50);
    }
}
