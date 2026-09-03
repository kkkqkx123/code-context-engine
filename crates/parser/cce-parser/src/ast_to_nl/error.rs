//! Error types for AST to Natural Language conversion
//!
//! This module provides detailed error types for the AstToNl module,
//! including context information for better debugging and error handling.

use cce_types::entity::{EntityId, EntityKind};
use thiserror::Error;

/// Conversion context for error reporting
///
/// Provides detailed information about the context in which an error occurred.
#[derive(Debug, Clone)]
pub struct ConversionContext {
    /// File path where the conversion occurred
    pub file_path: String,
    /// Entity ID that caused the error
    pub entity_id: EntityId,
    /// Entity name that caused the error
    pub entity_name: String,
    /// Entity kind that caused the error
    pub entity_kind: EntityKind,
}

impl ConversionContext {
    /// Create a new conversion context
    pub fn new(
        file_path: String,
        entity_id: EntityId,
        entity_name: String,
        entity_kind: EntityKind,
    ) -> Self {
        Self {
            file_path,
            entity_id,
            entity_name,
            entity_kind,
        }
    }
}

impl std::fmt::Display for ConversionContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} in {} ({} {})",
            self.entity_kind, self.file_path, self.entity_id, self.entity_name
        )
    }
}

/// Error types for AST to Natural Language conversion
#[derive(Error, Debug)]
pub enum AstToNlError {
    /// Failed to convert entity to natural language
    #[error("Failed to convert entity '{name}': {reason}")]
    ConversionFailed { name: String, reason: String },

    /// Failed to generate BM25 text for entity
    #[error("Failed to generate BM25 text for entity '{name}': {reason}")]
    Bm25GenerationFailed { name: String, reason: String },

    /// Failed to generate embedding text for entity
    #[error("Failed to generate embedding text for entity '{name}': {reason}")]
    EmbeddingGenerationFailed { name: String, reason: String },

    /// Failed to infer intent from function name
    #[error("Failed to infer intent from function name '{name}': {reason}")]
    IntentInferenceFailed { name: String, reason: String },

    /// Failed to extract keywords from entity
    #[error("Failed to extract keywords from entity '{name}': {reason}")]
    KeywordExtractionFailed { name: String, reason: String },

    /// Failed to normalize name
    #[error("Failed to normalize name '{name}': {reason}")]
    NameNormalizationFailed { name: String, reason: String },

    /// Failed to clean docstring
    #[error("Failed to clean docstring for entity '{name}': {reason}")]
    DocstringCleaningFailed { name: String, reason: String },

    /// Template rendering failed
    #[error("Template rendering failed for template '{template_name}': {reason}")]
    TemplateRenderingFailed {
        template_name: String,
        reason: String,
    },

    /// Entity group conversion failed
    #[error("Failed to convert entity group '{group_type}': {reason}")]
    EntityGroupConversionFailed { group_type: String, reason: String },

    /// Missing header in entity group
    #[error("Missing header in entity group for pattern '{pattern}': {reason}")]
    MissingHeader { pattern: String, reason: String },

    /// Failed to generate description for pattern
    #[error("Failed to generate description for pattern '{pattern}': {reason}")]
    DescriptionGenerationFailed { pattern: String, reason: String },

    /// General error with context
    #[error("AstToNl error: {message} (context: {context})")]
    WithContext {
        message: String,
        context: ConversionContext,
    },
}

impl AstToNlError {
    /// Create an error with conversion context
    pub fn with_context(message: String, context: ConversionContext) -> Self {
        Self::WithContext { message, context }
    }

    /// Create a conversion failed error
    pub fn conversion_failed(name: &str, reason: &str) -> Self {
        Self::ConversionFailed {
            name: name.to_string(),
            reason: reason.to_string(),
        }
    }

    /// Create a BM25 generation failed error
    pub fn bm25_generation_failed(name: &str, reason: &str) -> Self {
        Self::Bm25GenerationFailed {
            name: name.to_string(),
            reason: reason.to_string(),
        }
    }

    /// Create an embedding generation failed error
    pub fn embedding_generation_failed(name: &str, reason: &str) -> Self {
        Self::EmbeddingGenerationFailed {
            name: name.to_string(),
            reason: reason.to_string(),
        }
    }

    /// Create an intent inference failed error
    pub fn intent_inference_failed(name: &str, reason: &str) -> Self {
        Self::IntentInferenceFailed {
            name: name.to_string(),
            reason: reason.to_string(),
        }
    }

    /// Create a missing header error
    pub fn missing_header(pattern: &str, reason: &str) -> Self {
        Self::MissingHeader {
            pattern: pattern.to_string(),
            reason: reason.to_string(),
        }
    }

    /// Create a description generation failed error
    pub fn description_generation_failed(pattern: &str, reason: &str) -> Self {
        Self::DescriptionGenerationFailed {
            pattern: pattern.to_string(),
            reason: reason.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conversion_context_display() {
        let context = ConversionContext::new(
            "src/test.rs".to_string(),
            EntityId(1),
            "test_func".to_string(),
            EntityKind::Function,
        );

        let display = format!("{}", context);
        assert!(display.contains("function"));
        assert!(display.contains("src/test.rs"));
        assert!(display.contains("test_func"));
    }

    #[test]
    fn test_error_creation() {
        let error = AstToNlError::conversion_failed("test_func", "Invalid parameter type");
        assert!(error.to_string().contains("test_func"));
        assert!(error.to_string().contains("Invalid parameter type"));
    }

    #[test]
    fn test_error_with_context() {
        let context = ConversionContext::new(
            "src/test.rs".to_string(),
            EntityId(1),
            "test_func".to_string(),
            EntityKind::Function,
        );

        let error = AstToNlError::with_context("Conversion failed".to_string(), context);

        assert!(error.to_string().contains("Conversion failed"));
        assert!(error.to_string().contains("function"));
        assert!(error.to_string().contains("test_func"));
    }

    #[test]
    fn test_missing_header_error() {
        let error = AstToNlError::missing_header("Builder", "No class entity found");
        assert!(error.to_string().contains("Builder"));
        assert!(error.to_string().contains("No class entity found"));
    }

    #[test]
    fn test_description_generation_failed_error() {
        let error = AstToNlError::description_generation_failed(
            "Strategy",
            "Failed to infer strategy intent",
        );
        assert!(error.to_string().contains("Strategy"));
        assert!(
            error
                .to_string()
                .contains("Failed to infer strategy intent")
        );
    }
}
