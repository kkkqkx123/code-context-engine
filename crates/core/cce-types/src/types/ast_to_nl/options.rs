//! Output mode for AST to Natural Language conversion
//!
//! This module defines the output mode enum that controls the format
//! of generated natural language text. This type is shared across
//! multiple layers (config, ast_to_nl, types) and is kept in the
//! types layer for cross-layer access.
//!
//! # Output Modes
//!
//! - `OutputMode::Bm25` - Hybrid text for keyword search (preserves code symbols)
//! - `OutputMode::Embedding` - Pure semantic summary (removes code symbols)
//! - `OutputMode::Both` - Dual-path output for hybrid indexing

use serde::{Deserialize, Serialize};

/// Output mode for AST to NL conversion
///
/// Determines the format of the generated natural language text.
///
/// # Cross-Layer Usage
///
/// This enum is used across multiple layers:
/// - **Config layer**: Parsed from environment variables and config files
/// - **Core layer**: Used by `ast_to_nl` module to select conversion strategy
/// - **Types layer**: Used in validation and result structures
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputMode {
    /// BM25 hybrid enhanced text
    /// Preserves key entities (function names, types, file paths) for keyword matching
    #[default]
    Bm25,

    /// Embedding pure semantic summary
    /// Removes all code symbols, fully natural language for semantic search
    Embedding,

    /// Both outputs (for dual-path indexing)
    /// Generates both BM25 and Embedding texts
    Both,
}

impl OutputMode {
    /// Check if this mode produces BM25 output
    pub fn produces_bm25(&self) -> bool {
        matches!(self, OutputMode::Bm25 | OutputMode::Both)
    }

    /// Check if this mode produces Embedding output
    pub fn produces_embedding(&self) -> bool {
        matches!(self, OutputMode::Embedding | OutputMode::Both)
    }

    /// Get a description of the mode
    pub fn description(&self) -> &'static str {
        match self {
            OutputMode::Bm25 => "BM25 hybrid text with code symbols for keyword search",
            OutputMode::Embedding => "Pure semantic summary without code symbols for vector search",
            OutputMode::Both => "Dual-path output for both keyword and vector search",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_mode() {
        let mode = OutputMode::default();
        assert!(matches!(mode, OutputMode::Bm25));
    }

    #[test]
    fn test_produces_bm25() {
        assert!(OutputMode::Bm25.produces_bm25());
        assert!(!OutputMode::Embedding.produces_bm25());
        assert!(OutputMode::Both.produces_bm25());
    }

    #[test]
    fn test_produces_embedding() {
        assert!(!OutputMode::Bm25.produces_embedding());
        assert!(OutputMode::Embedding.produces_embedding());
        assert!(OutputMode::Both.produces_embedding());
    }

    #[test]
    fn test_description() {
        assert!(OutputMode::Bm25.description().contains("keyword"));
        assert!(OutputMode::Embedding.description().contains("vector"));
        assert!(OutputMode::Both.description().contains("Dual"));
    }

    #[test]
    fn test_serde_roundtrip() {
        let mode = OutputMode::Both;
        let json = serde_json::to_string(&mode).expect("Failed to serialize");
        let parsed: OutputMode = serde_json::from_str(&json).expect("Failed to deserialize");
        assert_eq!(mode, parsed);
    }
}
