//! Conversion options for AST to Natural Language conversion
//!
//! This module defines configuration options for controlling how AST nodes
//! are converted to natural language descriptions.
//!
//! # Output Modes
//!
//! - `OutputMode::Bm25` - Hybrid text for keyword search (preserves code symbols)
//! - `OutputMode::Embedding` - Pure semantic summary (removes code symbols)
//! - `OutputMode::Both` - Dual-path output for hybrid indexing

use cce_types::OutputMode;
use serde::{Deserialize, Serialize};

/// Conversion request options for per-request overrides
#[derive(Debug, Clone, Default)]
pub struct ConversionRequest {
    /// Force specific output mode (overrides global config)
    pub force_mode: Option<OutputMode>,
}

/// Conversion options for controlling AST to Natural Language conversion
///
/// # Field Groups
///
/// ## Mode Selection
/// - `mode` - Output mode (Bm25, Embedding, or Both)
///
/// ## BM25 Options
/// - `include_context` - Include file path and module name
/// - `include_original_names` - Include function/class names
/// - `include_types` - Include type information
/// - `include_keywords` - Include extracted keywords
///
/// ## Embedding Options
/// - `max_summary_words` - Maximum words for semantic summary
///
/// ## Shared Options
/// - `include_docstring` - Include docstring in output
/// - `normalize_types` - Normalize type names
/// - `include_signature` - Include function signature
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversionOptions {
    /// Output mode (Bm25, Embedding, or Both)
    pub mode: OutputMode,

    // === BM25 Options ===
    /// Include context information (file path, module name)
    pub include_context: bool,
    /// Include original function/class names
    pub include_original_names: bool,
    /// Include specific types
    pub include_types: bool,
    /// Include keywords
    pub include_keywords: bool,

    // === Embedding Options ===
    /// Maximum words for semantic summary
    pub max_summary_words: usize,

    // === Shared Options ===
    /// Include docstring
    pub include_docstring: bool,
    /// Normalize types
    pub normalize_types: bool,
    /// Include signature
    pub include_signature: bool,
}

impl Default for ConversionOptions {
    fn default() -> Self {
        Self {
            mode: OutputMode::Bm25,
            include_context: true,
            include_original_names: true,
            include_types: true,
            include_keywords: true,
            max_summary_words: 1024,
            include_docstring: true,
            normalize_types: true,
            include_signature: true,
        }
    }
}

impl ConversionOptions {
    /// Create BM25 mode options
    pub fn bm25() -> Self {
        Self {
            mode: OutputMode::Bm25,
            ..Default::default()
        }
    }

    /// Create Embedding mode options
    pub fn embedding() -> Self {
        Self {
            mode: OutputMode::Embedding,
            ..Default::default()
        }
    }

    /// Create Both mode options
    pub fn both() -> Self {
        Self {
            mode: OutputMode::Both,
            ..Default::default()
        }
    }

    // === Validation Methods ===

    /// Validate the conversion options
    ///
    /// Returns `Ok(())` if the options are valid, or an error message if not.
    ///
    /// # Validation Rules
    ///
    /// - `max_summary_words` must be greater than 0
    /// - At least one output option should be enabled for the selected mode
    pub fn validate(&self) -> Result<(), String> {
        if self.max_summary_words == 0 {
            return Err("max_summary_words must be greater than 0".to_string());
        }

        // Check mode-specific options
        match self.mode {
            OutputMode::Bm25 => {
                if !self.include_context
                    && !self.include_original_names
                    && !self.include_types
                    && !self.include_keywords
                    && !self.include_docstring
                    && !self.include_signature
                {
                    return Err(
                        "BM25 mode requires at least one output option to be enabled".to_string(),
                    );
                }
            }
            OutputMode::Embedding => {
                if !self.include_docstring && !self.include_signature {
                    return Err(
                        "Embedding mode requires at least one output option to be enabled"
                            .to_string(),
                    );
                }
            }
            OutputMode::Both => {
                // Both mode should have reasonable options for both paths
                if !self.include_context && !self.include_original_names && !self.include_docstring
                {
                    return Err(
                        "Both mode requires at least basic output options to be enabled"
                            .to_string(),
                    );
                }
            }
        }

        Ok(())
    }

    /// Check if this options is for BM25 mode
    pub fn is_bm25_mode(&self) -> bool {
        matches!(self.mode, OutputMode::Bm25)
    }

    /// Check if this options is for Embedding mode
    pub fn is_embedding_mode(&self) -> bool {
        matches!(self.mode, OutputMode::Embedding)
    }

    /// Check if this options is for Both mode
    pub fn is_both_mode(&self) -> bool {
        matches!(self.mode, OutputMode::Both)
    }

    /// Check if BM25 output is needed
    pub fn needs_bm25_output(&self) -> bool {
        matches!(self.mode, OutputMode::Bm25 | OutputMode::Both)
    }

    /// Check if Embedding output is needed
    pub fn needs_embedding_output(&self) -> bool {
        matches!(self.mode, OutputMode::Embedding | OutputMode::Both)
    }

    // === Builder Methods ===

    /// Set the output mode
    pub fn with_mode(mut self, mode: OutputMode) -> Self {
        self.mode = mode;
        self
    }

    /// Set include_context
    pub fn with_context(mut self, include: bool) -> Self {
        self.include_context = include;
        self
    }

    /// Set include_original_names
    pub fn with_original_names(mut self, include: bool) -> Self {
        self.include_original_names = include;
        self
    }

    /// Set max_summary_words
    pub fn with_max_summary_words(mut self, words: usize) -> Self {
        self.max_summary_words = words;
        self
    }

    /// Set include_docstring
    pub fn with_docstring(mut self, include: bool) -> Self {
        self.include_docstring = include;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_options() {
        let opts = ConversionOptions::default();
        assert!(matches!(opts.mode, OutputMode::Bm25));
        assert!(opts.include_context);
        assert_eq!(opts.max_summary_words, 1024);
    }

    #[test]
    fn test_bm25_mode() {
        let opts = ConversionOptions::bm25();
        assert!(matches!(opts.mode, OutputMode::Bm25));
    }

    #[test]
    fn test_embedding_mode() {
        let opts = ConversionOptions::embedding();
        assert!(matches!(opts.mode, OutputMode::Embedding));
    }

    #[test]
    fn test_validate_default_options() {
        let opts = ConversionOptions::default();
        assert!(opts.validate().is_ok());
    }

    #[test]
    fn test_validate_zero_max_summary_words() {
        let opts = ConversionOptions::embedding().with_max_summary_words(0);
        assert!(opts.validate().is_err());
    }

    #[test]
    fn test_mode_check_methods() {
        let bm25_opts = ConversionOptions::bm25();
        assert!(bm25_opts.is_bm25_mode());
        assert!(!bm25_opts.is_embedding_mode());
        assert!(!bm25_opts.is_both_mode());
        assert!(bm25_opts.needs_bm25_output());
        assert!(!bm25_opts.needs_embedding_output());

        let embedding_opts = ConversionOptions::embedding();
        assert!(!embedding_opts.is_bm25_mode());
        assert!(embedding_opts.is_embedding_mode());
        assert!(!embedding_opts.is_both_mode());
        assert!(!embedding_opts.needs_bm25_output());
        assert!(embedding_opts.needs_embedding_output());

        let both_opts = ConversionOptions::both();
        assert!(!both_opts.is_bm25_mode());
        assert!(!both_opts.is_embedding_mode());
        assert!(both_opts.is_both_mode());
        assert!(both_opts.needs_bm25_output());
        assert!(both_opts.needs_embedding_output());
    }

    #[test]
    fn test_builder_methods() {
        let opts = ConversionOptions::default()
            .with_mode(OutputMode::Embedding)
            .with_context(false)
            .with_max_summary_words(50)
            .with_docstring(false);

        assert!(matches!(opts.mode, OutputMode::Embedding));
        assert!(!opts.include_context);
        assert_eq!(opts.max_summary_words, 50);
        assert!(!opts.include_docstring);
    }
}
