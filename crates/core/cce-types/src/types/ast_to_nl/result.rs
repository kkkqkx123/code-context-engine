//! Conversion result types for AST to Natural Language conversion
//!
//! This module defines the result type produced by the AST-to-NL conversion process,
//! including both BM25-optimized and embedding-optimized text outputs.
//!
//! # Cross-Layer Usage
//!
//! `ConversionResult` is a core data exchange format that flows through multiple layers:
//! - **Core layer**: Produced by `ast_to_nl` converter
//! - **Orchestrator layer**: Passed through processing pipeline
//! - **Storage layer**: Converted to `Bm25Document` for indexing
//!
//! # Relationship with Entity
//!
//! `EntityMetadata` (defined in `ast_to_nl::metadata`) is a lightweight summary
//! extracted from `Entity` for use in query result enhancement.

use crate::types::ast_to_nl::EntityMetadata;
use crate::types::position::Span;
use serde::{Deserialize, Serialize};

/// Conversion result from AST to Natural Language
///
/// This struct holds the output of the AST-to-NL conversion process, supporting
/// dual-path indexing (BM25 + Embedding) and entity association tracking.
///
/// # Field Groups
///
/// ## Identity Fields
/// - `entity_id`, `kind`, `name`, `file_path` - Identify the source entity
///
/// ## Output Fields
/// - `bm25_text` - Hybrid enhanced text for keyword search
/// - `embedding_text` - Pure semantic summary for vector search
/// - `keywords` - Extracted keywords for BM25 indexing
///
/// ## Source Tracking Fields
/// - `source_entity_ids` - All entities contributing to this result
/// - `source_span` - Source code location (used to read source file on demand)
/// - `entity_metadata` - Lightweight entity summary for queries
///
/// # Validation
///
/// Use `validate()` to ensure the result is consistent with the output mode:
/// - BM25 mode: `bm25_text` should be present
/// - Embedding mode: `embedding_text` should be present
/// - Both mode: both texts should be present
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversionResult {
    // === Identity Fields ===
    /// Entity ID (references the original entity in ParsedFile)
    pub entity_id: crate::types::entity::EntityId,
    /// Entity kind (function, class, method, etc.)
    pub kind: crate::types::entity::EntityKind,
    /// Original entity name
    pub name: String,
    /// File path (source file where this entity is defined)
    pub file_path: String,

    // === Output Fields ===
    /// BM25 hybrid enhanced text
    /// Format: "Function 'name' that does 'description' with parameters 'params'..."
    /// Contains code symbols for keyword matching
    pub bm25_text: Option<String>,

    /// Embedding pure semantic summary
    /// Format: "Checks if an async operation completes within a time limit..."
    /// Pure natural language without code symbols
    pub embedding_text: Option<String>,

    /// Cached token count for embedding_text (LLM-oriented)
    /// Used for embedding size limits and LLM context window checks
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding_tokens: Option<usize>,

    /// Word count for bm25_text (actual words from tokenization)
    /// Used for BM25 length normalization - BM25 does NOT use LLM tokens
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bm25_word_count: Option<usize>,

    /// Keywords for BM25 indexing
    pub keywords: Vec<String>,

    // === Source Tracking Fields ===
    /// Source entity ID list (supports multi-entity combination)
    /// - Single entity: `vec![entity_id]`
    /// - Class + methods: `vec![class_id, method1_id, method2_id, ...]`
    #[serde(default)]
    pub source_entity_ids: Vec<crate::types::entity::EntityId>,

    /// Source code span (used to read source file on demand during query)
    #[serde(default)]
    pub source_span: Span,

    /// Entity metadata (lightweight summary for query result enhancement)
    /// Defined in `ast_to_nl::metadata` module
    #[serde(default)]
    pub entity_metadata: EntityMetadata,

    /// End line numbers for each sub-entity description in the combined NL text.
    ///
    /// When an entity's NL text is composed of multiple sub-descriptions
    /// (e.g., overview + signature + params + return), this array records
    /// the end line number of each sub-description in the final text.
    ///
    /// This enables the chunker to split at entity boundaries without
    /// breaking the semantic structure of the combined description.
    ///
    /// - Empty array: single description or unable to determine boundaries
    /// - Non-empty array: each element is a line number (0-indexed) marking
    ///   the end of a sub-description in the combined text
    ///
    /// Example:
    /// ```text
    /// Gets the contents of the cell.     <- line 0-1 (end_line: 1)
    ///                                     <- empty line
    /// get_or_try_init function.           <- line 2 (end_line: 2)
    ///                                     <- empty line
    /// Takes f. Returns Result<T, E>.      <- line 3 (end_line: 3)
    /// ```
    /// `entity_end_lines: [1, 2, 3]`
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entity_end_lines: Vec<usize>,

    /// Brief BM25 header text for continuation chunks when a group is split.
    ///
    /// This is a condensed version of the BM25 header that includes only:
    /// - Group name/type (e.g., "once cell inherent_impl")
    /// - Member names without signatures or return values
    ///
    /// Used by the chunker to provide group-level context in continuation
    /// chunks without the redundancy of repeating the full header.
    ///
    /// Example:
    /// "once cell inherent_impl. Methods: new, with_value, is_initialized, initialize."
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bm25_brief_header: Option<String>,

    /// Brief Embedding header text for continuation chunks when a group is split.
    ///
    /// This is a condensed version of the Embedding header that includes only:
    /// - Group name/type (e.g., "once cell inherent_impl")
    /// - Member names without signatures or return values
    ///
    /// Used by the chunker to provide group-level context in continuation
    /// chunks without the redundancy of repeating the full header.
    ///
    /// Example:
    /// "once cell inherent_impl. Methods: new, with_value, is_initialized, initialize."
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding_brief_header: Option<String>,
}

impl Default for ConversionResult {
    fn default() -> Self {
        use crate::types::entity::{EntityId, EntityKind};
        Self {
            entity_id: EntityId(0),
            kind: EntityKind::Function,
            name: String::new(),
            file_path: String::new(),
            bm25_text: None,
            embedding_text: None,
            embedding_tokens: None,
            bm25_word_count: None,
            keywords: Vec::new(),
            source_entity_ids: Vec::new(),
            source_span: Span::default(),
            entity_metadata: EntityMetadata::default(),
            entity_end_lines: Vec::new(),
            bm25_brief_header: None,
            embedding_brief_header: None,
        }
    }
}

impl ConversionResult {
    /// Create a new conversion result with both outputs
    pub fn new(
        entity_id: crate::types::entity::EntityId,
        kind: crate::types::entity::EntityKind,
        name: String,
        file_path: String,
        bm25_text: String,
        embedding_text: String,
        keywords: Vec<String>,
    ) -> Self {
        use cce_utils::token_estimation::estimate_tokens;

        let embedding_tokens = estimate_tokens(&embedding_text);
        let bm25_word_count = Self::count_words(&bm25_text);

        Self {
            entity_id,
            kind,
            name,
            file_path,
            bm25_text: Some(bm25_text),
            embedding_text: Some(embedding_text),
            embedding_tokens: Some(embedding_tokens),
            bm25_word_count: Some(bm25_word_count),
            keywords,
            source_entity_ids: vec![entity_id],
            source_span: Span::default(),
            entity_metadata: EntityMetadata::default(),
            entity_end_lines: Vec::new(),
            bm25_brief_header: None,
            embedding_brief_header: None,
        }
    }

    /// Create a BM25-only result
    pub fn bm25_only(
        entity_id: crate::types::entity::EntityId,
        kind: crate::types::entity::EntityKind,
        name: String,
        file_path: String,
        bm25_text: String,
        keywords: Vec<String>,
    ) -> Self {
        let bm25_word_count = Self::count_words(&bm25_text);

        Self {
            entity_id,
            kind,
            name,
            file_path,
            bm25_text: Some(bm25_text),
            embedding_text: None,
            embedding_tokens: None,
            bm25_word_count: Some(bm25_word_count),
            keywords,
            source_entity_ids: vec![entity_id],
            source_span: Span::default(),
            entity_metadata: EntityMetadata::default(),
            entity_end_lines: Vec::new(),
            bm25_brief_header: None,
            embedding_brief_header: None,
        }
    }

    /// Create an Embedding-only result
    pub fn embedding_only(
        entity_id: crate::types::entity::EntityId,
        kind: crate::types::entity::EntityKind,
        name: String,
        file_path: String,
        embedding_text: String,
    ) -> Self {
        use cce_utils::token_estimation::estimate_tokens;

        let embedding_tokens = estimate_tokens(&embedding_text);

        Self {
            entity_id,
            kind,
            name,
            file_path,
            bm25_text: None,
            embedding_text: Some(embedding_text),
            embedding_tokens: Some(embedding_tokens),
            bm25_word_count: None,
            keywords: Vec::new(),
            source_entity_ids: vec![entity_id],
            source_span: Span::default(),
            entity_metadata: EntityMetadata::default(),
            entity_end_lines: Vec::new(),
            bm25_brief_header: None,
            embedding_brief_header: None,
        }
    }

    /// Set source entity IDs
    pub fn with_source_entity_ids(
        mut self,
        entity_ids: Vec<crate::types::entity::EntityId>,
    ) -> Self {
        self.source_entity_ids = entity_ids;
        self
    }

    /// Set source span
    pub fn with_source_span(mut self, span: Span) -> Self {
        self.source_span = span;
        self
    }

    /// Set entity metadata
    pub fn with_entity_metadata(mut self, metadata: EntityMetadata) -> Self {
        self.entity_metadata = metadata;
        self
    }

    /// Append index-only context text to both output paths.
    ///
    /// Append raw code context text (like Control Flow or Behavior fragments)
    /// without any symbol transformation.
    ///
    /// This method is used for code snippets extracted from source, which should
    /// preserve their original operators and symbols (e.g., &, *, &&) to maintain
    /// code correctness. These symbols are critical to understanding code logic
    /// and should not be converted to natural language.
    ///
    /// The text is appended as-is to both BM25 and embedding outputs.
    /// Metrics (word count and token count) are updated accordingly.
    pub fn append_index_context_raw(&mut self, extra_text: &str) {
        self.append_index_context_raw_separate(extra_text, extra_text);
    }

    /// Append code context text with separate content for BM25 and embedding.
    ///
    /// BM25 text uses double-newline separators for clear token boundaries.
    /// Embedding text uses single-newline separator for readability while
    /// keeping the section logically connected.
    pub fn append_index_context_raw_separate(&mut self, bm25_extra: &str, emb_extra: &str) {
        use cce_utils::token_estimation::estimate_tokens;

        let bm25_extra = bm25_extra.trim();
        if !bm25_extra.is_empty() {
            if let Some(text) = self.bm25_text.as_mut() {
                if !text.is_empty() {
                    text.push_str("\n\n");
                }
                text.push_str(bm25_extra);
                self.bm25_word_count = Some(Self::count_words(text));
            }
        }

        let emb_extra = emb_extra.trim();
        if !emb_extra.is_empty() {
            if let Some(text) = self.embedding_text.as_mut() {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(emb_extra);
                self.embedding_tokens = Some(estimate_tokens(text));
            }
        }
    }

    // === Helper Methods ===

    /// Count words in text using simple whitespace-based tokenization
    /// This provides actual word count for BM25 length normalization
    fn count_words(text: &str) -> usize {
        text.split_whitespace()
            .filter(|word| !word.is_empty())
            .count()
    }

    // === Validation Methods ===

    /// Validate the conversion result for a given output mode
    ///
    /// Returns `Ok(())` if the result is valid, or an error message if not.
    ///
    /// # Validation Rules
    ///
    /// - `OutputMode::Bm25`: `bm25_text` must be present and non-empty
    /// - `OutputMode::Embedding`: `embedding_text` must be present and non-empty
    /// - `OutputMode::Both`: both texts must be present and non-empty
    ///
    /// # Example
    ///
    /// ```ignore
    /// use cce_core::types::ast_to_nl::result::ConversionResult;
    /// use cce_core::types::OutputMode;
    /// use cce_core::types::entity::{EntityId, EntityKind};
    ///
    /// // let result = ConversionResult::bm25_only(
    /// //     EntityId(1),
    /// //     EntityKind::Function,
    /// //     "test".to_string(),
    /// //     "test.rs".to_string(),
    /// //     "test content".to_string(),
    /// //     vec![]
    /// // );
    /// // assert!(result.validate(OutputMode::Bm25).is_ok());
    /// // assert!(result.validate(OutputMode::Embedding).is_err());
    /// ```
    pub fn validate(&self, mode: super::OutputMode) -> Result<(), String> {
        match mode {
            super::OutputMode::Bm25 => {
                if self.bm25_text.is_none() || self.bm25_text.as_ref().is_none_or(|s| s.is_empty())
                {
                    return Err(format!(
                        "BM25 mode requires bm25_text to be present and non-empty for entity '{}' ({:?})",
                        self.name, self.kind
                    ));
                }
            }
            super::OutputMode::Embedding => {
                if self.embedding_text.is_none()
                    || self.embedding_text.as_ref().is_none_or(|s| s.is_empty())
                {
                    return Err(format!(
                        "Embedding mode requires embedding_text to be present and non-empty for entity '{}' ({:?})",
                        self.name, self.kind
                    ));
                }
            }
            super::OutputMode::Both => {
                let bm25_empty = self.bm25_text.is_none()
                    || self.bm25_text.as_ref().is_none_or(|s| s.is_empty());
                let embedding_empty = self.embedding_text.is_none()
                    || self.embedding_text.as_ref().is_none_or(|s| s.is_empty());

                if bm25_empty || embedding_empty {
                    return Err(format!(
                        "Both mode requires both bm25_text and embedding_text to be present and non-empty for entity '{}' ({:?})",
                        self.name, self.kind
                    ));
                }
            }
        }
        Ok(())
    }

    /// Check if this result has BM25 output
    pub fn has_bm25_output(&self) -> bool {
        self.bm25_text.as_ref().is_some_and(|s| !s.is_empty())
    }

    /// Check if this result has Embedding output
    pub fn has_embedding_output(&self) -> bool {
        self.embedding_text.as_ref().is_some_and(|s| !s.is_empty())
    }

    /// Check if this result has any output
    pub fn has_any_output(&self) -> bool {
        self.has_bm25_output() || self.has_embedding_output()
    }

    /// Get the effective description (bm25_text preferred, fallback to embedding_text)
    ///
    /// This method provides the same logic as the deprecated `description` field
    /// for migration purposes.
    pub fn effective_description(&self) -> Option<&str> {
        self.bm25_text
            .as_deref()
            .filter(|s| !s.is_empty())
            .or_else(|| self.embedding_text.as_deref().filter(|s| !s.is_empty()))
    }
}

#[cfg(test)]
mod tests {
    use super::super::OutputMode;
    use super::*;
    use crate::types::entity::{EntityId, EntityKind};

    #[test]
    fn test_conversion_result_default() {
        let result = ConversionResult::default();
        assert!(result.bm25_text.is_none());
        assert!(result.embedding_text.is_none());
        assert!(result.embedding_tokens.is_none());
        assert!(result.bm25_word_count.is_none());
    }

    #[test]
    fn test_bm25_only() {
        let result = ConversionResult::bm25_only(
            EntityId(1),
            EntityKind::Function,
            "test".to_string(),
            "test.rs".to_string(),
            "Test description".to_string(),
            vec!["test".to_string()],
        );
        assert!(result.bm25_text.is_some());
        assert!(result.embedding_text.is_none());
        assert_eq!(result.bm25_text.as_deref(), Some("Test description"));
        // BM25 should have word count (not tokens)
        assert!(result.bm25_word_count.is_some());
        // Embedding tokens should be None for BM25-only
        assert!(result.embedding_tokens.is_none());
    }

    #[test]
    fn test_validate_bm25_mode() {
        let valid_result = ConversionResult::bm25_only(
            EntityId(1),
            EntityKind::Function,
            "test".to_string(),
            "test.rs".to_string(),
            "Test description".to_string(),
            vec!["test".to_string()],
        );
        assert!(valid_result.validate(OutputMode::Bm25).is_ok());
        assert!(valid_result.validate(OutputMode::Embedding).is_err());

        let invalid_result = ConversionResult::default();
        assert!(invalid_result.validate(OutputMode::Bm25).is_err());
    }

    #[test]
    fn test_validate_embedding_mode() {
        let valid_result = ConversionResult::embedding_only(
            EntityId(1),
            EntityKind::Function,
            "test".to_string(),
            "test.rs".to_string(),
            "Test semantic summary".to_string(),
        );
        assert!(valid_result.validate(OutputMode::Embedding).is_ok());
        assert!(valid_result.validate(OutputMode::Bm25).is_err());
    }

    #[test]
    fn test_validate_both_mode() {
        let valid_result = ConversionResult::new(
            EntityId(1),
            EntityKind::Function,
            "test".to_string(),
            "test.rs".to_string(),
            "BM25 text".to_string(),
            "Embedding text".to_string(),
            vec!["test".to_string()],
        );
        assert!(valid_result.validate(OutputMode::Both).is_ok());

        let partial_result = ConversionResult::bm25_only(
            EntityId(1),
            EntityKind::Function,
            "test".to_string(),
            "test.rs".to_string(),
            "BM25 text".to_string(),
            vec!["test".to_string()],
        );
        assert!(partial_result.validate(OutputMode::Both).is_err());
    }

    #[test]
    fn test_has_output_methods() {
        let bm25_result = ConversionResult::bm25_only(
            EntityId(1),
            EntityKind::Function,
            "test".to_string(),
            "test.rs".to_string(),
            "BM25 text".to_string(),
            vec!["test".to_string()],
        );
        assert!(bm25_result.has_bm25_output());
        assert!(!bm25_result.has_embedding_output());
        assert!(bm25_result.has_any_output());

        let empty_result = ConversionResult::default();
        assert!(!empty_result.has_bm25_output());
        assert!(!empty_result.has_embedding_output());
        assert!(!empty_result.has_any_output());
    }

    #[test]
    fn test_effective_description() {
        let bm25_result = ConversionResult::bm25_only(
            EntityId(1),
            EntityKind::Function,
            "test".to_string(),
            "test.rs".to_string(),
            "BM25 text".to_string(),
            vec!["test".to_string()],
        );
        assert_eq!(bm25_result.effective_description(), Some("BM25 text"));

        let embedding_result = ConversionResult::embedding_only(
            EntityId(1),
            EntityKind::Function,
            "test".to_string(),
            "test.rs".to_string(),
            "Embedding text".to_string(),
        );
        assert_eq!(
            embedding_result.effective_description(),
            Some("Embedding text")
        );

        let empty_result = ConversionResult::default();
        assert_eq!(empty_result.effective_description(), None);
    }

    #[test]
    fn test_token_caching() {
        use cce_utils::token_estimation::estimate_tokens;

        // Test bm25_only - should only have word count, no tokens
        let bm25_text = "This is a test description for BM25";
        let result = ConversionResult::bm25_only(
            EntityId(1),
            EntityKind::Function,
            "test".to_string(),
            "test.rs".to_string(),
            bm25_text.to_string(),
            vec!["test".to_string()],
        );
        // BM25 uses word count, not tokens
        let expected_words = bm25_text.split_whitespace().count();
        assert_eq!(result.bm25_word_count, Some(expected_words));
        assert!(result.embedding_tokens.is_none());

        // Test embedding_only - should only have tokens, no word count
        let embedding_text = "This is a semantic summary for embedding";
        let result = ConversionResult::embedding_only(
            EntityId(1),
            EntityKind::Function,
            "test".to_string(),
            "test.rs".to_string(),
            embedding_text.to_string(),
        );
        let expected_tokens = estimate_tokens(embedding_text);
        assert_eq!(result.embedding_tokens, Some(expected_tokens));
        assert!(result.bm25_word_count.is_none());

        // Test new() with both texts
        let result = ConversionResult::new(
            EntityId(1),
            EntityKind::Function,
            "test".to_string(),
            "test.rs".to_string(),
            bm25_text.to_string(),
            embedding_text.to_string(),
            vec!["test".to_string()],
        );
        // BM25 path: word count only
        assert_eq!(result.bm25_word_count, Some(expected_words));
        // Embedding path: token count only
        assert_eq!(
            result.embedding_tokens,
            Some(estimate_tokens(embedding_text))
        );
    }

    #[test]
    fn test_append_index_context_raw_preserves_code_symbols() {
        use cce_utils::token_estimation::estimate_tokens;

        let mut result = ConversionResult::new(
            EntityId(1),
            EntityKind::Function,
            "test".to_string(),
            "test.rs".to_string(),
            "BM25 base".to_string(),
            "Embedding base".to_string(),
            vec!["test".to_string()],
        );

        // Code fragment with operators that should NOT be transformed
        let code_fragment =
            "Behavior:\nlet slot: *mut T = self.value.get();\nif (state & MASK) != 0 { }";
        result.append_index_context_raw(code_fragment);

        let bm25_text = result.bm25_text.as_ref().expect("bm25 text should exist");
        let embedding_text = result
            .embedding_text
            .as_ref()
            .expect("embedding text should exist");

        // Verify code symbols are preserved (not transformed)
        assert!(bm25_text.contains("*mut T"), "BM25 should preserve *mut");
        assert!(
            bm25_text.contains("& MASK"),
            "BM25 should preserve & operator"
        );

        assert!(
            embedding_text.contains("*mut T"),
            "Embedding should preserve *mut (not convert to 'mutable pointer to')"
        );
        assert!(
            embedding_text.contains("& MASK"),
            "Embedding should preserve & (not convert to 'reference to')"
        );

        // Verify metrics are updated
        assert_eq!(
            result.bm25_word_count,
            Some(bm25_text.split_whitespace().count())
        );
        assert_eq!(
            result.embedding_tokens,
            Some(estimate_tokens(embedding_text))
        );
    }
}
