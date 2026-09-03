//! Embedding-specific text cleaner
//!
//! This module performs only lossless normalization for embedding generation.
//!
//! Rust signatures, lifetimes, pointer/reference markers, generic brackets,
//! and qualified paths are retrieval anchors. Rewriting those tokens as prose
//! creates malformed text and makes implementation strategies indistinguishable.

use cce_utils::text::normalize_whitespace_preserving_newlines;
use cce_utils::token_estimation::{TokenEstimator, estimate_tokens};

/// Find the nearest trim point before `max_byte`.
/// Searches backward for sentence/clause/word boundaries within an 80-char window.
pub(crate) fn find_trim_point(text: &str, max_byte: usize) -> usize {
    let search_start = max_byte.saturating_sub(80);
    let search_area = &text[search_start..max_byte.min(text.len())];

    for pattern in &[". ", ".\n", "。", "；", "; ", ", ", " "] {
        if let Some(pos) = search_area.rfind(pattern) {
            let abs_pos = search_start + pos + pattern.len();
            if abs_pos < text.len() {
                return abs_pos;
            }
        }
    }
    max_byte
}

/// Lightweight text cleaner for embedding path
///
/// Keeps code tokens intact while normalizing whitespace.
#[derive(Default)]
pub struct EmbeddingTextCleaner {
    max_docstring_ratio: Option<f64>,
}

impl EmbeddingTextCleaner {
    /// Create a new embedding text cleaner
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the maximum docstring ratio for trimming.
    /// When enabled, the first sub-description (typically docstring) will be
    /// truncated to stay within this ratio of total tokens.
    pub fn with_docstring_ratio(mut self, max_ratio: f64) -> Self {
        self.max_docstring_ratio = Some(max_ratio);
        self
    }

    /// Clean text for embedding
    ///
    /// Qualified paths and Rust syntax are deliberately preserved. Templates
    /// provide prose explanations; this cleaner must not attempt to infer them
    /// through global string replacement.
    ///
    /// # Example
    ///
    /// ```
    /// use cce_parser::ast_to_nl::embedding::text_cleaner::EmbeddingTextCleaner;
    ///
    /// let cleaner = EmbeddingTextCleaner::new();
    /// let text = "Option<&'a mut T>";
    /// let cleaned = cleaner.clean(text);
    /// assert_eq!(cleaned, "Option<&'a mut T>");
    /// ```
    pub fn clean(&self, text: &str) -> String {
        if text.is_empty() {
            return String::new();
        }

        normalize_whitespace_preserving_newlines(text)
            .trim()
            .to_string()
    }

    /// Clean text with optional docstring trimming based on entity end lines.
    ///
    /// When `entity_end_lines` is non-empty and `max_docstring_ratio` is set,
    /// the first sub-description (typically the docstring) is truncated to
    /// stay within the configured ratio of total tokens.
    pub fn clean_with_boundaries(&self, text: &str, entity_end_lines: &[usize]) -> String {
        let cleaned = self.clean(text);
        if let Some(max_ratio) = self.max_docstring_ratio {
            if !entity_end_lines.is_empty() {
                return self.trim_docstring(&cleaned, max_ratio, entity_end_lines);
            }
        }
        cleaned
    }

    /// Clean type string for embedding
    ///
    /// Normalize a type annotation without changing its Rust spelling.
    pub fn clean_type(type_str: &str) -> String {
        let cleaner = Self::new();
        cleaner.clean(type_str)
    }

    /// Minimum total tokens required to apply docstring trimming.
    /// Short texts (e.g., small class descriptions with a member or two) do not
    /// benefit from docstring compression and could lose essential information.
    const MIN_TRIMMABLE_TOKENS: usize = 200;

    /// Trim docstring to fit within max_ratio of total tokens.
    fn trim_docstring(&self, text: &str, max_ratio: f64, entity_end_lines: &[usize]) -> String {
        let total = estimate_tokens(text);
        // Only trim substantial texts where docstring actually crowds out code.
        if total < Self::MIN_TRIMMABLE_TOKENS {
            return text.to_string();
        }
        let (docstring, code) = self.split_at_docstring_boundary(text, entity_end_lines);
        let doc_tokens = estimate_tokens(docstring);
        if total == 0 || doc_tokens as f64 / total as f64 <= max_ratio {
            return text.to_string();
        }
        let max_doc = (total as f64 * max_ratio) as usize;
        let byte_pos = TokenEstimator::default().find_split_point(docstring, max_doc.max(1));
        let split_point = find_trim_point(docstring, byte_pos);
        let trimmed_doc = &docstring[..split_point];
        let code = code.trim_start();
        if code.is_empty() {
            trimmed_doc.to_string()
        } else {
            format!("{}\n{}", trimmed_doc, code)
        }
    }

    /// Split text at the first entity_end_line boundary.
    /// Returns (docstring_part, code_part).
    fn split_at_docstring_boundary<'a>(
        &self,
        text: &'a str,
        entity_end_lines: &[usize],
    ) -> (&'a str, &'a str) {
        if let Some(&first_boundary) = entity_end_lines.first() {
            let lines: Vec<&str> = text.lines().collect();
            let boundary_idx = first_boundary.min(lines.len().saturating_sub(1));
            let doc_part = lines[..=boundary_idx].join("\n");
            if let Some(pos) = text.find(&doc_part) {
                let end = pos + doc_part.len();
                return (&text[..end], &text[end..]);
            }
        }
        ("", text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_empty() {
        let cleaner = EmbeddingTextCleaner::new();
        assert_eq!(cleaner.clean(""), "");
    }

    #[test]
    fn test_clean_simple_type() {
        let cleaner = EmbeddingTextCleaner::new();
        assert_eq!(cleaner.clean("i32"), "i32");
        assert_eq!(cleaner.clean("String"), "String");
    }

    #[test]
    fn test_preserves_reference_syntax() {
        let cleaner = EmbeddingTextCleaner::new();

        assert_eq!(cleaner.clean("&T"), "&T");
        assert_eq!(cleaner.clean("&mut str"), "&mut str");
    }

    #[test]
    fn test_preserves_generics_and_paths() {
        let cleaner = EmbeddingTextCleaner::new();

        assert_eq!(cleaner.clean("Vec<Option<T>>"), "Vec<Option<T>>");
        assert_eq!(
            cleaner.clean("critical_section::with"),
            "critical_section::with"
        );
    }

    #[test]
    fn test_preserves_complex_types() {
        let cleaner = EmbeddingTextCleaner::new();

        assert_eq!(cleaner.clean("Option<&mut T>"), "Option<&mut T>");
        assert_eq!(cleaner.clean("Result<&str, Error>"), "Result<&str, Error>");
        assert_eq!(cleaner.clean("Arc<Mutex<T>>"), "Arc<Mutex<T>>");
    }

    #[test]
    fn test_preserves_pointers_and_lifetimes() {
        let cleaner = EmbeddingTextCleaner::new();

        assert_eq!(cleaner.clean("*const T"), "*const T");
        assert_eq!(cleaner.clean("&'a T"), "&'a T");
    }

    #[test]
    fn test_clean_type_function() {
        assert_eq!(
            EmbeddingTextCleaner::clean_type("Option<&mut T>"),
            "Option<&mut T>"
        );
        assert_eq!(
            EmbeddingTextCleaner::clean_type("Vec<String>"),
            "Vec<String>"
        );
    }

    #[test]
    fn test_preserves_semantic_types() {
        let cleaner = EmbeddingTextCleaner::new();

        // Rust tokens remain available as exact retrieval anchors.
        let cleaned = cleaner.clean("Result<Option<Vec<String>>, Error>");
        assert!(cleaned.contains("Result"));
        assert!(cleaned.contains("Option"));
        assert!(cleaned.contains("Vec"));
        assert!(cleaned.contains("String"));
        assert!(cleaned.contains("Error"));
    }

    #[test]
    fn test_mut_preserved_in_identifier() {
        let cleaner = EmbeddingTextCleaner::new();

        // Identifiers containing mut must NOT be stripped
        assert_eq!(cleaner.clean("get mut"), "get mut");
        assert_eq!(cleaner.clean("force mut"), "force mut");
        assert_eq!(cleaner.clean("force mut function"), "force mut function");
    }

    #[test]
    fn test_whitespace_normalization() {
        let cleaner = EmbeddingTextCleaner::new();

        assert_eq!(cleaner.clean("Vec<  T  >"), "Vec< T >");
        assert_eq!(cleaner.clean("&  mut  T"), "& mut T");
    }
}
