//! Shared annotation formatting for BM25 and Embedding templates
//!
//! Provides a single source of truth for formatting annotation/decorator metadata
//! as natural language text, eliminating duplication between template implementations.
//!
//! # Usage
//!
//! Both BM25 and Embedding templates should call [`format_annotations`] instead of
//! directly accessing `entity.metadata.get("annotations")` and formatting inline.

use cce_types::OutputMode;

/// Format annotations metadata into natural language description
///
/// Returns `None` when annotations are absent or empty, allowing callers to
/// skip the annotation section entirely in generated text.
///
/// # Formatting Differences
///
/// - **Embedding**: Natural language sentence (`"With annotations: derive(Debug, Clone)."`)
/// - **BM25 / Both**: Keyword-oriented (`"annotations derive(Debug, Clone)"`) for
///   full-text search matching without natural language overhead.
pub fn format_annotations(annotations: &str, mode: OutputMode) -> Option<String> {
    if annotations.is_empty() {
        return None;
    }
    match mode {
        OutputMode::Embedding => Some(format!("With annotations: {}.", annotations)),
        _ => Some(format!("annotations {}", annotations)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_empty_returns_none() {
        assert!(format_annotations("", OutputMode::Embedding).is_none());
        assert!(format_annotations("", OutputMode::Bm25).is_none());
        assert!(format_annotations("", OutputMode::Both).is_none());
    }

    #[test]
    fn test_format_embedding_style() {
        let result = format_annotations("derive(Debug)", OutputMode::Embedding);
        assert_eq!(result, Some("With annotations: derive(Debug).".to_string()));
    }

    #[test]
    fn test_format_bm25_style() {
        let result = format_annotations("derive(Debug)", OutputMode::Bm25);
        assert_eq!(result, Some("annotations derive(Debug)".to_string()));
    }

    #[test]
    fn test_format_both_style() {
        let result = format_annotations("derive(Debug)", OutputMode::Both);
        assert_eq!(result, Some("annotations derive(Debug)".to_string()));
    }
}
