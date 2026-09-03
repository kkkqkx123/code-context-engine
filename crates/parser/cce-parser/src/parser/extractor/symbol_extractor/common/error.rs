//! Error types for symbol extraction
//!
//! Provides detailed error information for extraction failures.

use thiserror::Error;

/// Extraction error types
#[derive(Error, Debug, Clone)]
pub enum ExtractionError {
    /// Failed to parse source code
    #[error("Parse error: {message}")]
    ParseError {
        /// Error message
        message: String,
    },

    /// Failed to extract node text
    #[error("Failed to extract text from node at {start_line}:{start_col}")]
    NodeTextError {
        /// Start line
        start_line: usize,
        /// Start column
        start_col: usize,
    },

    /// Invalid import statement structure
    #[error("Invalid import statement: {reason}")]
    InvalidImport {
        /// Reason for invalidity
        reason: String,
    },

    /// Invalid export statement structure
    #[error("Invalid export statement: {reason}")]
    InvalidExport {
        /// Reason for invalidity
        reason: String,
    },

    /// Missing required field
    #[error("Missing required field '{field}' in {context}")]
    MissingField {
        /// Field name
        field: String,
        /// Context (e.g., "import statement")
        context: String,
    },

    /// Unsupported language feature
    #[error("Unsupported feature: {feature} in language {language}")]
    UnsupportedFeature {
        /// Feature name
        feature: String,
        /// Language name
        language: String,
    },

    /// Plugin-backed extraction failed
    #[error("Plugin extraction error: {message}")]
    Plugin {
        /// Plugin error message
        message: String,
    },
}

impl ExtractionError {
    /// Create a plugin extraction error.
    pub fn plugin_error(e: impl std::fmt::Display) -> Self {
        Self::Plugin {
            message: e.to_string(),
        }
    }

    /// Create a parse error
    pub fn parse(message: impl Into<String>) -> Self {
        Self::ParseError {
            message: message.into(),
        }
    }

    /// Create a node text error
    pub fn node_text(line: usize, col: usize) -> Self {
        Self::NodeTextError {
            start_line: line,
            start_col: col,
        }
    }

    /// Create an invalid import error
    pub fn invalid_import(reason: impl Into<String>) -> Self {
        Self::InvalidImport {
            reason: reason.into(),
        }
    }

    /// Create an invalid export error
    pub fn invalid_export(reason: impl Into<String>) -> Self {
        Self::InvalidExport {
            reason: reason.into(),
        }
    }

    /// Create a missing field error
    pub fn missing_field(field: impl Into<String>, context: impl Into<String>) -> Self {
        Self::MissingField {
            field: field.into(),
            context: context.into(),
        }
    }

    /// Create an unsupported feature error
    pub fn unsupported(feature: impl Into<String>, language: impl Into<String>) -> Self {
        Self::UnsupportedFeature {
            feature: feature.into(),
            language: language.into(),
        }
    }
}

/// Result type for extraction operations
pub type ExtractionResult<T> = Result<T, ExtractionError>;
