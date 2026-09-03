//! BM25-specific text cleaner
//!
//! This module provides text cleaning functionality specifically for BM25 indexing.
//! It removes redundant words and formatting that don't contribute
//! to search relevance, while preserving the essential information for code search.

use cce_utils::text::{normalize_whitespace, remove_quotes};
use regex::Regex;
use serde::{Deserialize, Serialize};

/// Configuration for BM25 text cleaner
///
/// Allows customization of lexical cleaning rules for BM25 text.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Bm25TextCleanerConfig {
    /// Redundant word patterns to remove (exact phrase matches, case-insensitive)
    pub redundant_patterns: Vec<String>,
}

impl Default for Bm25TextCleanerConfig {
    fn default() -> Self {
        Self {
            redundant_patterns: vec![
                // Natural language redundant phrases (safe to remove)
                "that does the".into(),
                "that does".into(),
                "that is".into(),
                "with parameters".into(),
                "that returns".into(),
                "defined in file".into(),
                "defined in".into(),
                "within module".into(),
                "in file".into(),
                "of class".into(),
                "of type".into(),
                "as method of".into(),
                // Code-specific redundant words (safe to remove)
                "normalized".into(),
                "keywords".into(),
            ],
        }
    }
}

/// BM25 text cleaner
///
/// Optimizes text for BM25 search by removing redundant words,
/// quotes, and formatting markers that don't contribute to search relevance.
pub struct Bm25TextCleaner {
    redundant_regex: Option<Regex>,
}

impl Bm25TextCleaner {
    /// Create a new BM25 text cleaner with default configuration
    pub fn new() -> Self {
        Self::with_config(Bm25TextCleanerConfig::default())
    }

    /// Create a BM25 text cleaner with custom configuration
    pub fn with_config(config: Bm25TextCleanerConfig) -> Self {
        let redundant_regex = Self::build_redundant_regex(&config);
        Self { redundant_regex }
    }

    /// Build regex pattern for redundant words
    fn build_redundant_regex(config: &Bm25TextCleanerConfig) -> Option<Regex> {
        if config.redundant_patterns.is_empty() {
            return None;
        }

        let patterns: Vec<&str> = config
            .redundant_patterns
            .iter()
            .map(|s| s.as_str())
            .collect();
        let regex_str = format!(r"(?i)\b({})\b", patterns.join("|"));
        Regex::new(&regex_str).ok()
    }

    /// Clean text for BM25 indexing
    ///
    /// Symbol splitting, camelCase decomposition, and lowercasing are now
    /// handled by the tokenizer (`MixedTokenizer`). This cleaner focuses on
    /// removing low-value textual noise that the tokenizer cannot detect:
    ///
    /// 1. Remove quotes (single/double/backtick)
    /// 2. Normalize whitespace
    /// 3. Remove redundant natural-language phrases
    /// 4. Final whitespace normalization and empty check
    pub fn clean(&self, text: &str) -> String {
        if text.is_empty() {
            return String::new();
        }

        let mut result = text.to_string();

        result = remove_quotes(&result);
        result = normalize_whitespace(&result);
        result = self.remove_redundant_words(&result);
        result = self.clean_up_empty(&result);

        result
    }

    /// Remove redundant words using regex whole-word matching
    ///
    /// Uses case-insensitive regex to match whole words only.
    fn remove_redundant_words(&self, text: &str) -> String {
        if let Some(ref re) = self.redundant_regex {
            re.replace_all(text, " ").to_string()
        } else {
            text.to_string()
        }
    }

    /// Clean up empty or whitespace-only text
    fn clean_up_empty(&self, text: &str) -> String {
        let normalized = normalize_whitespace(text);
        if normalized.trim().is_empty() {
            String::new()
        } else {
            normalized
        }
    }
}

impl Default for Bm25TextCleaner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_removes_quotes() {
        let cleaner = Bm25TextCleaner::new();
        let text = "Function 'test'";
        let cleaned = cleaner.clean(text);
        assert!(!cleaned.contains('\''));
        assert!(cleaned.contains("test"));
    }

    #[test]
    fn test_clean_removes_redundant_words() {
        let cleaner = Bm25TextCleaner::new();
        let text = "Function test that does something with parameters x that returns y";
        let cleaned = cleaner.clean(text);
        assert!(!cleaned.contains("that does"));
        assert!(!cleaned.contains("with parameters"));
        assert!(!cleaned.contains("that returns"));
        assert!(cleaned.contains("test"));
        assert!(cleaned.contains("something"));
        assert!(cleaned.contains("x"));
        assert!(cleaned.contains("y"));
    }

    #[test]
    fn test_clean_normalizes_whitespace() {
        let cleaner = Bm25TextCleaner::new();
        let text = "Function   test  that  does  something";
        let cleaned = cleaner.clean(text);
        assert_eq!(cleaned, "Function test something");
    }

    #[test]
    fn test_clean_empty() {
        let cleaner = Bm25TextCleaner::new();
        assert_eq!(cleaner.clean(""), "");
    }

    #[test]
    fn test_clean_complex_example() {
        let cleaner = Bm25TextCleaner::new();
        let text = "Function 'calculate_total' (normalized: 'calculate total') that does 'Calculates the total price' with parameters 'price: f64, quantity: i32' that returns 'f64', defined in file 'calculator.rs' within module 'math'. Keywords: calculate, total, price.";
        let cleaned = cleaner.clean(text);

        assert!(!cleaned.contains('\''));
        assert!(!cleaned.contains("that does"));
        assert!(!cleaned.contains("with parameters"));
        assert!(!cleaned.contains("that returns"));
        assert!(!cleaned.contains("defined in file"));
        assert!(!cleaned.contains("within module"));

        // Quotes removed, identifiers preserved as-is
        assert!(cleaned.contains("calculate_total"));
        assert!(cleaned.contains("price"));
        assert!(cleaned.contains("f64"));
        assert!(cleaned.contains("quantity"));
        assert!(cleaned.contains("i32"));
        assert!(cleaned.contains("calculator.rs"));
        assert!(cleaned.contains("math"));
    }

    #[test]
    fn test_regex_whole_word_matching() {
        let cleaner = Bm25TextCleaner::new();

        let text = "withinmodule";
        let cleaned = cleaner.clean(text);
        assert!(cleaned.contains("withinmodule"));

        let text = "within module";
        let cleaned = cleaner.clean(text);
        assert!(!cleaned.contains("within"));
    }

    #[test]
    fn test_case_insensitive_redundant_words() {
        let cleaner = Bm25TextCleaner::new();

        let text = "Function THAT DOES something";
        let cleaned = cleaner.clean(text);
        assert!(!cleaned.contains("that does"));
        assert!(cleaned.contains("something"));
    }

    #[test]
    fn test_empty_after_cleaning() {
        let cleaner = Bm25TextCleaner::new();

        let text = "that does that is";
        let cleaned = cleaner.clean(text);
        assert_eq!(cleaned, "");
    }

    #[test]
    fn test_complex_bm25_scenario() {
        let cleaner = Bm25TextCleaner::new();

        let text =
            "Function 'read_data' that returns Vec<T> with parameters Array<T> and Promise<T>";
        let cleaned = cleaner.clean(text);

        assert!(!cleaned.contains("'"));

        assert!(cleaned.contains("Vec<T>"));
        assert!(cleaned.contains("Array<T>"));
        assert!(cleaned.contains("Promise<T>"));

        assert!(cleaned.contains("read_data"));

        assert!(!cleaned.contains("that returns"));
        assert!(!cleaned.contains("with parameters"));
    }

    #[test]
    fn test_code_semantics_preserved() {
        let cleaner = Bm25TextCleaner::new();

        let text = "vector of int";
        let cleaned = cleaner.clean(text);
        assert!(cleaned.contains("vector"));
        assert!(cleaned.contains("int"));

        let text = "result in chunk";
        let cleaned = cleaner.clean(text);
        assert!(cleaned.contains("result"));
        assert!(cleaned.contains("chunk"));

        let text = "cast as u32";
        let cleaned = cleaner.clean(text);
        assert!(cleaned.contains("cast"));
        assert!(cleaned.contains("u32"));
    }

    #[test]
    fn test_preserve_code_syntax() {
        let cleaner = Bm25TextCleaner::new();

        // Code syntax is preserved — tokenization is the tokenizer's job
        let text = "Option<&mut T>";
        let cleaned = cleaner.clean(text);
        assert!(cleaned.contains("Option<&mut T>"));

        let text = "std::path::Path";
        let cleaned = cleaner.clean(text);
        assert!(cleaned.contains("std::path::Path"));

        let text = "fn calculate(x: i32) -> f64";
        let cleaned = cleaner.clean(text);
        assert!(cleaned.contains("fn"));
        assert!(cleaned.contains("calculate"));
        assert!(cleaned.contains("i32"));
        assert!(cleaned.contains("f64"));
    }
}
