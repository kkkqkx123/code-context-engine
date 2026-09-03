//! Helper functions for template text processing
//!
//! Provides shared utilities for:
//! - Keyword extraction from identifiers
//! - Text combination and deduplication

use std::collections::HashSet;

/// Template helper functions
///
/// Provides common text processing utilities used by both BM25 and Embedding templates.
pub struct TemplateHelpers;

impl TemplateHelpers {
    /// Build a signature string from structured parameters and return type.
    ///
    /// Produces output like `"(x: i32, y: String) -> bool"` from the
    /// already-parsed parameter and return-type fields, avoiding reliance
    /// on language-specific signature syntax.
    pub fn build_signature_from_fields<S1: AsRef<str>, S2: AsRef<str>>(
        params: &[(S1, Option<S2>)],
        return_type: Option<&str>,
    ) -> String {
        let param_str: Vec<String> = params
            .iter()
            .map(|(name, ty)| match ty {
                Some(ty) if !ty.as_ref().is_empty() => {
                    format!("{}: {}", name.as_ref(), ty.as_ref())
                }
                _ => name.as_ref().to_string(),
            })
            .collect();
        let params_part = format!("({})", param_str.join(", "));
        match return_type.filter(|r| !r.is_empty()) {
            Some(ret) => format!("{} -> {}", params_part, ret),
            None => params_part,
        }
    }

    /// Extract keywords from an identifier
    ///
    /// Splits camelCase, snake_case, kebab-case, and `::` paths into individual words.
    /// Correctly handles consecutive uppercase letters (acronyms):
    /// - `XMLParser` → `["xml", "parser"]`
    /// - `INCOMPLETE` → `["incomplete"]`
    /// - `STATE_MASK` → `["state", "mask"]`
    /// - `core::fmt::Debug` → `["core", "fmt", "debug"]`
    ///
    /// # Examples
    ///
    /// ```
    /// use cce_parser::ast_to_nl::TemplateHelpers;
    ///
    /// let keywords = TemplateHelpers::extract_keywords("UserBuilder");
    /// assert_eq!(keywords, vec!["user", "builder"]);
    ///
    /// let keywords = TemplateHelpers::extract_keywords("calculate_total_price");
    /// assert_eq!(keywords, vec!["calculate", "total", "price"]);
    ///
    /// let keywords = TemplateHelpers::extract_keywords("INCOMPLETE");
    /// assert_eq!(keywords, vec!["incomplete"]);
    ///
    /// let keywords = TemplateHelpers::extract_keywords("core::fmt::Debug");
    /// assert_eq!(keywords, vec!["core", "fmt", "debug"]);
    /// ```
    pub fn extract_keywords(name: &str) -> Vec<String> {
        if name.is_empty() {
            return vec![];
        }

        // Step 1: Replace all separators with spaces
        // This handles snake_case, kebab-case, `::` paths, and spaced names uniformly
        let normalized: String = name.replace(['_', '-'], " ").replace("::", " ");

        // Step 2: Insert spaces at camelCase boundaries
        // Correctly handles consecutive uppercase letters (acronyms):
        // - "getUser" → space before "U" (lowercase before uppercase)
        // - "XMLParser" → space before "P" (end of uppercase sequence followed by lowercase)
        // - "INCOMPLETE" → no space inserted (all uppercase, no boundary trigger)
        let mut with_boundaries = String::new();
        let chars: Vec<char> = normalized.chars().collect();

        for i in 0..chars.len() {
            let c = chars[i];

            if i > 0 {
                let prev = chars[i - 1];
                let next = chars.get(i + 1).copied();
                let insert_space = if c.is_uppercase() && prev != ' ' {
                    prev.is_lowercase()
                        || (prev.is_uppercase() && next.is_some_and(|n| n.is_lowercase()))
                        || prev.is_ascii_digit()
                } else if c.is_ascii_digit() && prev.is_alphabetic() {
                    true
                } else {
                    c.is_alphabetic() && prev.is_ascii_digit()
                };

                if insert_space && !with_boundaries.ends_with(' ') {
                    with_boundaries.push(' ');
                }
            }

            with_boundaries.push(c);
        }

        // Step 3: Split on whitespace and normalize to lowercase
        with_boundaries
            .split_whitespace()
            .map(|s| s.to_lowercase())
            .collect()
    }

    /// Join text parts into a single string preserving all words (no dedup).
    ///
    /// Unlike `combine_text`, this preserves term frequency which is critical
    /// for BM25 ranking signal.
    pub fn join_parts(parts: &[&str]) -> String {
        let mut result = Vec::new();
        for part in parts {
            for word in part.split_whitespace() {
                let normalized = word.to_lowercase();
                if !normalized.is_empty() {
                    result.push(normalized);
                }
            }
        }
        result.join(" ")
    }

    /// Combine text parts into a single string with deduplication
    ///
    /// Joins parts with spaces, removing duplicates (case-insensitive).
    /// Kept for keyword extraction where dedup is desirable.
    pub fn combine_text(parts: &[&str]) -> String {
        if parts.is_empty() {
            return String::new();
        }

        let mut seen = HashSet::new();
        let mut result = Vec::new();

        for part in parts {
            for word in part.split_whitespace() {
                let normalized = word.to_lowercase();
                if !normalized.is_empty() && seen.insert(normalized.clone()) {
                    result.push(normalized);
                }
            }
        }

        result.join(" ")
    }

    /// Extract keywords from an identifier and combine them into a deduplicated string
    ///
    /// This is a convenience function that chains `extract_keywords` and `combine_text`.
    ///
    /// # Examples
    ///
    /// ```
    /// use cce_parser::ast_to_nl::TemplateHelpers;
    ///
    /// let text = TemplateHelpers::extract_and_combine("UserBuilder");
    /// assert_eq!(text, "user builder");
    /// ```
    pub fn extract_and_combine(name: &str) -> String {
        let keywords = Self::extract_keywords(name);
        let parts: Vec<&str> = keywords.iter().map(|s| s.as_str()).collect();
        Self::combine_text(&parts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_keywords_camel_case() {
        let keywords = TemplateHelpers::extract_keywords("UserBuilder");
        assert_eq!(keywords, vec!["user", "builder"]);

        let keywords = TemplateHelpers::extract_keywords("calculateTotalPrice");
        assert_eq!(keywords, vec!["calculate", "total", "price"]);
    }

    #[test]
    fn test_extract_keywords_snake_case() {
        let keywords = TemplateHelpers::extract_keywords("user_builder");
        assert_eq!(keywords, vec!["user", "builder"]);

        let keywords = TemplateHelpers::extract_keywords("calculate_total_price");
        assert_eq!(keywords, vec!["calculate", "total", "price"]);
    }

    #[test]
    fn test_extract_keywords_mixed() {
        let keywords = TemplateHelpers::extract_keywords("createUserAccount");
        assert_eq!(keywords, vec!["create", "user", "account"]);
    }

    #[test]
    fn test_extract_keywords_all_uppercase() {
        // All-uppercase identifiers must not be split letter-by-letter
        let keywords = TemplateHelpers::extract_keywords("INCOMPLETE");
        assert_eq!(keywords, vec!["incomplete"]);

        let keywords = TemplateHelpers::extract_keywords("STATE_MASK");
        assert_eq!(keywords, vec!["state", "mask"]);

        let keywords = TemplateHelpers::extract_keywords("COMPLETE_PTR");
        assert_eq!(keywords, vec!["complete", "ptr"]);
    }

    #[test]
    fn test_extract_keywords_mixed_with_acronyms() {
        // Mixed-case with acronyms must split correctly
        let keywords = TemplateHelpers::extract_keywords("XMLParser");
        assert_eq!(keywords, vec!["xml", "parser"]);

        let keywords = TemplateHelpers::extract_keywords("parseHTML");
        assert_eq!(keywords, vec!["parse", "html"]);

        let keywords = TemplateHelpers::extract_keywords("getUserID");
        assert_eq!(keywords, vec!["get", "user", "id"]);
    }

    #[test]
    fn test_extract_keywords_kebab_case() {
        let keywords = TemplateHelpers::extract_keywords("my-variable-name");
        assert_eq!(keywords, vec!["my", "variable", "name"]);

        let keywords = TemplateHelpers::extract_keywords("with-value");
        assert_eq!(keywords, vec!["with", "value"]);
    }

    #[test]
    fn test_extract_keywords_empty() {
        let keywords = TemplateHelpers::extract_keywords("");
        assert!(keywords.is_empty());
    }

    #[test]
    fn test_combine_text() {
        let text = TemplateHelpers::combine_text(&["User", "user", "Builder", "builder"]);
        assert_eq!(text, "user builder");

        let text = TemplateHelpers::combine_text(&["hello", "world"]);
        assert_eq!(text, "hello world");
    }

    #[test]
    fn test_combine_text_empty() {
        let text = TemplateHelpers::combine_text(&[]);
        assert_eq!(text, "");

        let text = TemplateHelpers::combine_text(&["", "test", ""]);
        assert_eq!(text, "test");
    }

    #[test]
    fn test_combine_text_dedup() {
        // Identical parts should be deduplicated
        let text = TemplateHelpers::combine_text(&["hello", "hello"]);
        assert_eq!(text, "hello");

        // Different parts with overlapping words should be deduplicated at word level
        let text = TemplateHelpers::combine_text(&["user builder", "user data"]);
        assert_eq!(text, "user builder data");

        let text = TemplateHelpers::combine_text(&["a", "b", "a", "c"]);
        assert_eq!(text, "a b c");
    }

    #[test]
    fn test_extract_and_combine() {
        let text = TemplateHelpers::extract_and_combine("UserBuilder");
        assert!(text.contains("user"));
        assert!(text.contains("builder"));
    }
}
