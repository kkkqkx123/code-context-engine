//! Token estimation for mixed-language text
//!
//! Thin forwarding over the shared llm-suite implementation so batch budgets
//! and provider fallbacks agree on one counting rule. CCE-specific helpers
//! (budget truncation) stay here.

pub use llm_token::estimation::{SYMBOL_FACTOR, TokenEstimator, estimate_tokens};

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
        assert!((2..=4).contains(&tokens), "tokens: {}", tokens);
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
        let text = "a".repeat(2000);
        let result = truncate_to_token_budget(&text, 50);
        assert!(result.truncated);
        assert!(TokenEstimator::default().estimate_text(&result.text) <= 50);
    }
}
