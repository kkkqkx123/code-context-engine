//! AST to Natural Language conversion configuration
//!
//! This module provides configuration for AST to natural language conversion.

use serde::{Deserialize, Serialize};

use crate::validation::{Validate, ValidationResult};
use cce_text::Bm25TextCleanerConfig;
use cce_types::OutputMode;
use cce_types::error::config::ConfigValidationError;

// Re-use shared default value functions
use super::defaults::default_true;

/// AST to Natural Language conversion configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AstToNlConfig {
    /// Default output mode
    #[serde(default)]
    pub default_mode: OutputMode,

    /// BM25 generator configuration
    #[serde(default)]
    pub bm25: Bm25GeneratorConfig,

    /// Embedding generator configuration
    #[serde(default)]
    pub embedding: EmbeddingGeneratorConfig,

    /// Chunking configuration
    #[serde(default)]
    pub chunking: ChunkingConfig,

    /// Document-specific chunking configuration (if None, uses chunking)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_chunking: Option<ChunkingConfig>,

    /// BM25 text cleaner configuration
    #[serde(default)]
    pub text_cleaner: Bm25TextCleanerConfig,
}

impl Default for AstToNlConfig {
    fn default() -> Self {
        Self {
            default_mode: OutputMode::Bm25,
            bm25: Bm25GeneratorConfig::default(),
            embedding: EmbeddingGeneratorConfig::default(),
            chunking: ChunkingConfig::default(),
            document_chunking: None,
            text_cleaner: Bm25TextCleanerConfig::default(),
        }
    }
}

impl Validate for AstToNlConfig {
    fn validate_structured(&self) -> ValidationResult {
        let mut errors = Vec::new();

        if let Err(e) = self.bm25.validate_structured() {
            errors.push(e);
        }
        if let Err(e) = self.embedding.validate_structured() {
            errors.push(e);
        }
        if let Err(e) = self.chunking.validate_structured() {
            errors.push(e);
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(ConfigValidationError::multiple(errors))
        }
    }
}

impl AstToNlConfig {
    /// Create config with Both output mode
    pub fn both() -> Self {
        Self {
            default_mode: OutputMode::Both,
            ..Default::default()
        }
    }
}

/// BM25 generator configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bm25GeneratorConfig {
    /// Maximum number of keywords to extract
    #[serde(default = "default_max_keywords")]
    pub max_keywords: usize,
}

impl Default for Bm25GeneratorConfig {
    fn default() -> Self {
        Self {
            max_keywords: default_max_keywords(),
        }
    }
}

impl Validate for Bm25GeneratorConfig {
    fn validate_structured(&self) -> ValidationResult {
        if self.max_keywords == 0 {
            return Err(ConfigValidationError::invalid_field(
                "bm25.max_keywords",
                "must be greater than 0",
            ));
        }
        Ok(())
    }
}

/// Embedding generator configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingGeneratorConfig {
    /// Maximum summary words
    #[serde(default = "default_max_summary_words")]
    pub max_summary_words: usize,

    /// Include docstring in output
    #[serde(default = "default_true")]
    pub include_docstring: bool,
}

impl Default for EmbeddingGeneratorConfig {
    fn default() -> Self {
        Self {
            max_summary_words: default_max_summary_words(),
            include_docstring: true,
        }
    }
}

impl Validate for EmbeddingGeneratorConfig {
    fn validate_structured(&self) -> ValidationResult {
        if self.max_summary_words == 0 {
            return Err(ConfigValidationError::invalid_field(
                "embedding.max_summary_words",
                "must be greater than 0",
            ));
        }
        Ok(())
    }
}

/// Chunking configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkingConfig {
    /// Maximum token count for embedding (default: 512)
    /// Used for LLM embedding size limits
    pub max_tokens: usize,
    /// Overlap token count for embedding (default: 50)
    pub overlap_tokens: usize,

    /// Maximum word count for BM25 (default: 200)
    /// BM25 length normalization should be based on actual word count, not tokens
    #[serde(default = "default_max_bm25_words")]
    pub max_bm25_words: usize,
    /// Overlap word count for BM25 (default: 20)
    #[serde(default = "default_overlap_bm25_words")]
    pub overlap_bm25_words: usize,

    /// Maximum overlap ratio (default: 0.2 = 20%)
    pub max_overlap_ratio: f32,
    /// Minimum chunk token count (default: 250)
    pub min_chunk_tokens: usize,
    /// Minimum chunk BM25 word count (default: 80)
    #[serde(default = "default_min_chunk_bm25_words")]
    pub min_chunk_bm25_words: usize,
    /// Whether to respect entity boundaries when splitting (default: true)
    pub respect_boundaries: bool,
    /// Shared merge ceiling for both intra-group and cross-group merging
    /// (0 = use per-path max limit).
    ///
    /// Guards any merge from combining chunks beyond the path's hard limit
    /// (max_tokens / max_bm25_words). The BM25 path is always hard-capped at
    /// `max_bm25_words` regardless of this setting, so it can never exceed
    /// its word limit.
    #[serde(default = "default_cross_group_merge_threshold")]
    pub cross_group_merge_threshold: usize,
}

impl Default for ChunkingConfig {
    fn default() -> Self {
        Self {
            max_tokens: 512,
            overlap_tokens: 50,
            max_bm25_words: default_max_bm25_words(),
            overlap_bm25_words: default_overlap_bm25_words(),
            max_overlap_ratio: 0.2,
            min_chunk_tokens: 150,
            min_chunk_bm25_words: 80,
            respect_boundaries: true,
            cross_group_merge_threshold: 0,
        }
    }
}

impl ChunkingConfig {
    /// Check whether `text` exceeds the configured limit for `path`.
    ///
    /// The BM25 path compares actual word count against `max_bm25_words`;
    /// the Embedding path compares estimated tokens (default estimator)
    /// against `max_tokens`. A limit of 0 disables the check.
    pub fn exceeds_limit(&self, text: &str, path: cce_types::ChunkPath) -> bool {
        match path {
            cce_types::ChunkPath::Bm25 => {
                self.max_bm25_words > 0
                    && text.split_whitespace().filter(|w| !w.is_empty()).count()
                        > self.max_bm25_words
            }
            cce_types::ChunkPath::Embedding => {
                self.max_tokens > 0
                    && cce_utils::token_estimation::estimate_tokens(text) > self.max_tokens
            }
        }
    }
}

impl Validate for ChunkingConfig {
    fn validate_structured(&self) -> ValidationResult {
        let mut errors = Vec::new();

        if self.max_tokens == 0 {
            errors.push(ConfigValidationError::invalid_field(
                "chunking.max_tokens",
                "must be greater than 0",
            ));
        }
        if self.overlap_tokens >= self.max_tokens {
            errors.push(ConfigValidationError::invalid_field(
                "chunking.overlap_tokens",
                "must be less than max_tokens",
            ));
        }
        if self.max_bm25_words == 0 {
            errors.push(ConfigValidationError::invalid_field(
                "chunking.max_bm25_words",
                "must be greater than 0",
            ));
        }
        if self.overlap_bm25_words >= self.max_bm25_words {
            errors.push(ConfigValidationError::invalid_field(
                "chunking.overlap_bm25_words",
                "must be less than max_bm25_words",
            ));
        }
        if self.max_overlap_ratio <= 0.0 || self.max_overlap_ratio > 1.0 {
            errors.push(ConfigValidationError::out_of_range(
                "chunking.max_overlap_ratio",
                self.max_overlap_ratio.to_string(),
                "0",
                "1",
            ));
        }
        if self.min_chunk_tokens == 0 {
            errors.push(ConfigValidationError::invalid_field(
                "chunking.min_chunk_tokens",
                "must be greater than 0",
            ));
        }
        if self.min_chunk_bm25_words == 0 {
            errors.push(ConfigValidationError::invalid_field(
                "chunking.min_chunk_bm25_words",
                "must be greater than 0",
            ));
        }
        if self.cross_group_merge_threshold != 0
            && self.cross_group_merge_threshold > self.max_tokens
        {
            errors.push(ConfigValidationError::invalid_field(
                "chunking.cross_group_merge_threshold",
                "must be 0 or no greater than max_tokens",
            ));
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(ConfigValidationError::multiple(errors))
        }
    }
}

fn default_max_keywords() -> usize {
    10
}

fn default_cross_group_merge_threshold() -> usize {
    0
}

fn default_min_chunk_bm25_words() -> usize {
    80
}

fn default_max_summary_words() -> usize {
    1024
}

fn default_max_bm25_words() -> usize {
    150
}

fn default_overlap_bm25_words() -> usize {
    5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AstToNlConfig::default();
        assert_eq!(config.default_mode, OutputMode::Bm25);
        assert_eq!(config.bm25.max_keywords, 10);
        assert_eq!(config.embedding.max_summary_words, 1024);
        assert_eq!(config.chunking.max_tokens, 512);
        assert_eq!(config.chunking.max_bm25_words, 150);
        assert_eq!(config.chunking.overlap_bm25_words, 5);
        assert_eq!(config.chunking.min_chunk_tokens, 150);
        assert_eq!(config.chunking.min_chunk_bm25_words, 80);
    }

    #[test]
    fn test_config_validation() {
        let invalid_config = AstToNlConfig {
            bm25: Bm25GeneratorConfig { max_keywords: 0 },
            ..Default::default()
        };
        assert!(invalid_config.validate_structured().is_err());
    }

    #[test]
    fn test_both_mode_config() {
        let config = AstToNlConfig::both();
        assert_eq!(config.default_mode, OutputMode::Both);
    }

    #[test]
    fn test_chunking_validation() {
        let invalid_chunking = ChunkingConfig {
            max_tokens: 0,
            ..Default::default()
        };
        assert!(invalid_chunking.validate_structured().is_err());

        let invalid_overlap = ChunkingConfig {
            overlap_tokens: 600,
            max_tokens: 512,
            ..Default::default()
        };
        assert!(invalid_overlap.validate_structured().is_err());

        let invalid_bm25_max = ChunkingConfig {
            max_bm25_words: 0,
            ..Default::default()
        };
        assert!(invalid_bm25_max.validate_structured().is_err());

        let invalid_bm25_overlap = ChunkingConfig {
            overlap_bm25_words: 200,
            max_bm25_words: 150,
            ..Default::default()
        };
        assert!(invalid_bm25_overlap.validate_structured().is_err());

        let invalid_bm25_min = ChunkingConfig {
            min_chunk_bm25_words: 0,
            ..Default::default()
        };
        assert!(invalid_bm25_min.validate_structured().is_err());

        let invalid_merge_threshold = ChunkingConfig {
            max_tokens: 512,
            cross_group_merge_threshold: 600,
            ..Default::default()
        };
        assert!(invalid_merge_threshold.validate_structured().is_err());

        let valid_merge_threshold = ChunkingConfig {
            max_tokens: 512,
            cross_group_merge_threshold: 0,
            ..Default::default()
        };
        assert!(valid_merge_threshold.validate_structured().is_ok());

        let valid = ChunkingConfig::default();
        assert!(valid.validate_structured().is_ok());
    }
}
