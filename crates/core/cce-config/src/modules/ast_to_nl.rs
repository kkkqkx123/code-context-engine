//! AST to Natural Language conversion configuration
//!
//! This module provides configuration for AST to natural language conversion.

use serde::{Deserialize, Serialize};

use crate::validation::{Validate, ValidationResult};
use cce_text::{Bm25TextCleanerConfig, ChunkUnit};
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

    /// Enable `calls:` line pruning in index-only embedding text.
    ///
    /// When enabled (default), redundant call targets are dropped from the
    /// embedding text after the behavior sidecar is appended. Disable for
    /// A/B comparison against unpruned text; BM25 text is never pruned.
    #[serde(default = "default_true")]
    pub enable_calls_prune: bool,
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
            enable_calls_prune: true,
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

/// The granularity at which domain terminology is injected.
///
/// The chunk is the unit that is actually retrieved, so the chunk is also
/// the unit that carries the expansion of a term: every chunk that mentions
/// a term explains it. The entity scope trades that coverage for a smaller
/// index: the expansion is placed once per entity, so only the chunk that
/// holds the first occurrence explains the term.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GlossaryScope {
    /// One expansion per chunk, at the first occurrence inside the chunk.
    #[default]
    Chunk,
    /// One expansion per entity, at the first occurrence inside the entity.
    Entity,
}

/// Chunking configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkingConfig {
    /// Maximum token count for embedding (default: 512)
    /// Used for LLM embedding size limits
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,
    /// Overlap token count for embedding (default: 50)
    #[serde(default = "default_overlap_tokens")]
    pub overlap_tokens: usize,

    /// Maximum term count for BM25 (default: 150)
    /// Measured by the same tokenizer the keyword index uses, so CJK text is
    /// counted as terms rather than as one whitespace-separated word.
    #[serde(default = "default_max_bm25_terms")]
    pub max_bm25_terms: usize,
    /// Overlap term count for BM25 (default: 5)
    #[serde(default = "default_overlap_bm25_terms")]
    pub overlap_bm25_terms: usize,

    /// Maximum overlap ratio (default: 0.2 = 20%)
    #[serde(default = "default_max_overlap_ratio")]
    pub max_overlap_ratio: f32,
    /// Minimum chunk token count (default: 150)
    #[serde(default = "default_min_chunk_tokens")]
    pub min_chunk_tokens: usize,
    /// Minimum chunk BM25 term count (default: 45)
    #[serde(default = "default_min_chunk_bm25_terms")]
    pub min_chunk_bm25_terms: usize,
    /// Whether to respect entity boundaries when splitting (default: true)
    #[serde(default = "default_respect_boundaries")]
    pub respect_boundaries: bool,
    /// Shared merge ceiling for both intra-group and cross-group merging
    /// (0 = use per-path max limit).
    ///
    /// Guards any merge from combining chunks beyond the path's hard limit
    /// (max_tokens / max_bm25_terms). The BM25 path is always hard-capped at
    /// `max_bm25_terms` regardless of this setting, so it can never exceed
    /// its term limit.
    #[serde(default = "default_cross_group_merge_threshold")]
    pub cross_group_merge_threshold: usize,

    /// The share of the chunk limit reserved for domain terminology
    /// (default: 0.06).
    ///
    /// The reserve is a planning target for the splitter. It is not a
    /// permission to exceed the hard limit: injection is checked against
    /// the absolute final chunk text and stops before the hard limit is
    /// crossed.
    #[serde(default = "default_glossary_reserve_ratio")]
    pub glossary_reserve_ratio: f32,

    /// The maximum number of terminology entries one chunk may carry
    /// (default: 4).
    #[serde(default = "default_glossary_max_entries")]
    pub glossary_max_entries: usize,

    /// The granularity of the terminology injection (default: chunk).
    #[serde(default)]
    pub glossary_scope: GlossaryScope,
}

impl Default for ChunkingConfig {
    fn default() -> Self {
        Self {
            max_tokens: default_max_tokens(),
            overlap_tokens: default_overlap_tokens(),
            max_bm25_terms: default_max_bm25_terms(),
            overlap_bm25_terms: default_overlap_bm25_terms(),
            max_overlap_ratio: default_max_overlap_ratio(),
            min_chunk_tokens: default_min_chunk_tokens(),
            min_chunk_bm25_terms: default_min_chunk_bm25_terms(),
            respect_boundaries: default_respect_boundaries(),
            cross_group_merge_threshold: default_cross_group_merge_threshold(),
            glossary_reserve_ratio: default_glossary_reserve_ratio(),
            glossary_max_entries: default_glossary_max_entries(),
            glossary_scope: GlossaryScope::default(),
        }
    }
}

/// The hard limit and the derived room for one path.
///
/// The hard limit is the only threshold a final chunk is checked against.
/// The reserve is a planning target: it keeps room for terminology
/// expansions, but it is never a permission to exceed the hard limit. The
/// body limit additionally subtracts the decoration that will be appended
/// after splitting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkBudget {
    /// The unit all values are measured in.
    pub unit: ChunkUnit,
    /// The configured hard limit; zero disables the limit.
    pub hard_limit: usize,
    /// The room planned for terminology expansion.
    pub reserve: usize,
    /// The decoration appended after splitting, in the same unit.
    pub decoration_cost: usize,
    /// The limit a chunk must satisfy before terminology injection.
    pub pre_injection_limit: usize,
    /// The limit the body splitter works with.
    pub body_limit: usize,
    /// The undersized-chunk trigger for merging.
    pub min_limit: usize,
    /// The largest combined cost a merge may produce.
    pub merge_limit: usize,
}

impl ChunkBudget {
    /// Whether `text` fits the hard limit.
    pub fn fits_hard(&self, text: &str) -> bool {
        self.unit.fits(text, self.hard_limit)
    }

    /// Whether `text` fits the pre-injection limit.
    pub fn fits_pre_injection(&self, text: &str) -> bool {
        self.unit.fits(text, self.pre_injection_limit)
    }

    /// Whether `text` fits the body limit.
    pub fn fits_body(&self, text: &str) -> bool {
        self.unit.fits(text, self.body_limit)
    }
}

impl ChunkingConfig {
    /// The unit the path is measured in.
    pub fn unit(&self, path: cce_types::ChunkPath) -> ChunkUnit {
        match path {
            cce_types::ChunkPath::Bm25 => ChunkUnit::Terms,
            cce_types::ChunkPath::Embedding => ChunkUnit::Tokens,
        }
    }

    /// The configured hard limit for `path`.
    pub fn limit(&self, path: cce_types::ChunkPath) -> usize {
        match path {
            cce_types::ChunkPath::Bm25 => self.max_bm25_terms,
            cce_types::ChunkPath::Embedding => self.max_tokens,
        }
    }

    /// Measure `text` in the path's unit.
    pub fn cost(&self, text: &str, path: cce_types::ChunkPath) -> usize {
        self.unit(path).cost(text)
    }

    /// The room one chunk reserves for domain terminology, in the unit of
    /// `path`.
    pub fn glossary_reserve(&self, path: cce_types::ChunkPath) -> usize {
        let limit = self.limit(path);
        if limit == 0 || self.glossary_reserve_ratio <= 0.0 {
            return 0;
        }
        (((limit as f32) * self.glossary_reserve_ratio).ceil() as usize)
            .min(limit.saturating_sub(1))
    }

    /// The limit a chunk must satisfy before terminology injection.
    pub fn pre_injection_limit(&self, path: cce_types::ChunkPath, has_vocabulary: bool) -> usize {
        let limit = self.limit(path);
        if limit == 0 {
            return usize::MAX;
        }
        let reserve = if has_vocabulary {
            self.glossary_reserve(path)
        } else {
            0
        };
        limit.saturating_sub(reserve)
    }

    /// The budget for one path, given the decoration that will be appended
    /// to a chunk after splitting.
    pub fn budget(
        &self,
        path: cce_types::ChunkPath,
        has_vocabulary: bool,
        decoration_cost: usize,
    ) -> ChunkBudget {
        let limit = self.limit(path);
        let reserve = if has_vocabulary {
            self.glossary_reserve(path)
        } else {
            0
        };
        if limit == 0 {
            return ChunkBudget {
                unit: self.unit(path),
                hard_limit: 0,
                reserve: 0,
                decoration_cost: 0,
                pre_injection_limit: usize::MAX,
                body_limit: usize::MAX,
                min_limit: 0,
                merge_limit: usize::MAX,
            };
        }
        let pre_injection_limit = limit.saturating_sub(reserve);
        let body_limit = pre_injection_limit.saturating_sub(decoration_cost).max(1);
        let min_limit = match path {
            cce_types::ChunkPath::Bm25 => self.min_chunk_bm25_terms,
            cce_types::ChunkPath::Embedding => self.min_chunk_tokens,
        };
        let merge_limit = match path {
            cce_types::ChunkPath::Bm25 => body_limit,
            cce_types::ChunkPath::Embedding => {
                if self.cross_group_merge_threshold == 0 {
                    body_limit
                } else {
                    self.cross_group_merge_threshold.min(body_limit)
                }
            }
        };
        ChunkBudget {
            unit: self.unit(path),
            hard_limit: limit,
            reserve,
            decoration_cost,
            pre_injection_limit,
            body_limit,
            min_limit,
            merge_limit,
        }
    }

    /// The merge ceiling for one path.
    ///
    /// The ceiling is the pre-injection limit, additionally capped by the
    /// custom cross-group threshold on the embedding path. It is the single
    /// semantic source for every merge decision; derived configs carry the
    /// capped value in their thresholds so boundary checks observe it
    /// without recomputing the reserve.
    pub fn merge_limit(&self, path: cce_types::ChunkPath, has_vocabulary: bool) -> usize {
        let pre = self.pre_injection_limit(path, has_vocabulary);
        match path {
            cce_types::ChunkPath::Bm25 => pre,
            cce_types::ChunkPath::Embedding => {
                if self.cross_group_merge_threshold == 0 {
                    pre
                } else if pre == usize::MAX {
                    self.cross_group_merge_threshold
                } else {
                    self.cross_group_merge_threshold.min(pre)
                }
            }
        }
    }

    /// Raw reserve before clamping, used for validation.
    fn raw_glossary_reserve(&self, limit: usize) -> usize {
        if limit == 0 || self.glossary_reserve_ratio <= 0.0 {
            return 0;
        }
        ((limit as f32) * self.glossary_reserve_ratio).ceil() as usize
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
        if self.max_bm25_terms == 0 {
            errors.push(ConfigValidationError::invalid_field(
                "chunking.max_bm25_terms",
                "must be greater than 0",
            ));
        }
        if self.overlap_bm25_terms >= self.max_bm25_terms {
            errors.push(ConfigValidationError::invalid_field(
                "chunking.overlap_bm25_terms",
                "must be less than max_bm25_terms",
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
        if self.min_chunk_bm25_terms == 0 {
            errors.push(ConfigValidationError::invalid_field(
                "chunking.min_chunk_bm25_terms",
                "must be greater than 0",
            ));
        }
        if self.min_chunk_tokens > self.max_tokens {
            errors.push(ConfigValidationError::invalid_field(
                "chunking.min_chunk_tokens",
                "must be no greater than max_tokens",
            ));
        }
        if self.min_chunk_bm25_terms > self.max_bm25_terms {
            errors.push(ConfigValidationError::invalid_field(
                "chunking.min_chunk_bm25_terms",
                "must be no greater than max_bm25_terms",
            ));
        }
        if self.glossary_reserve_ratio > 0.0 {
            for (limit, field) in [
                (self.max_tokens, "chunking.max_tokens"),
                (self.max_bm25_terms, "chunking.max_bm25_terms"),
            ] {
                let raw = self.raw_glossary_reserve(limit);
                if limit > 0 && raw >= limit {
                    errors.push(ConfigValidationError::invalid_field(
                        field,
                        "must leave room for the glossary reserve",
                    ));
                }
            }
        }
        if self.cross_group_merge_threshold != 0
            && self.cross_group_merge_threshold > self.max_tokens
        {
            errors.push(ConfigValidationError::invalid_field(
                "chunking.cross_group_merge_threshold",
                "must be 0 or no greater than max_tokens",
            ));
        }
        if self.glossary_reserve_ratio < 0.0 || self.glossary_reserve_ratio >= 1.0 {
            errors.push(ConfigValidationError::out_of_range(
                "chunking.glossary_reserve_ratio",
                self.glossary_reserve_ratio.to_string(),
                "0",
                "1",
            ));
        }
        if self.glossary_max_entries == 0 {
            errors.push(ConfigValidationError::invalid_field(
                "chunking.glossary_max_entries",
                "must be greater than 0",
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

fn default_max_tokens() -> usize {
    512
}

fn default_overlap_tokens() -> usize {
    50
}

fn default_max_overlap_ratio() -> f32 {
    0.2
}

fn default_min_chunk_tokens() -> usize {
    150
}

fn default_respect_boundaries() -> bool {
    true
}

fn default_min_chunk_bm25_terms() -> usize {
    45
}

fn default_max_summary_words() -> usize {
    1024
}

fn default_max_bm25_terms() -> usize {
    150
}

fn default_overlap_bm25_terms() -> usize {
    5
}

fn default_glossary_reserve_ratio() -> f32 {
    0.06
}

fn default_glossary_max_entries() -> usize {
    4
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
        assert_eq!(config.chunking.max_bm25_terms, 150);
        assert_eq!(config.chunking.overlap_bm25_terms, 5);
        assert_eq!(config.chunking.min_chunk_tokens, 150);
        assert_eq!(config.chunking.min_chunk_bm25_terms, 45);
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
            max_bm25_terms: 0,
            ..Default::default()
        };
        assert!(invalid_bm25_max.validate_structured().is_err());

        let invalid_bm25_overlap = ChunkingConfig {
            overlap_bm25_terms: 200,
            max_bm25_terms: 150,
            ..Default::default()
        };
        assert!(invalid_bm25_overlap.validate_structured().is_err());

        let invalid_bm25_min = ChunkingConfig {
            min_chunk_bm25_terms: 0,
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

    #[test]
    fn test_partial_chunking_table_uses_documented_defaults() {
        // A chunking table that only overrides one field must load, and every
        // omitted field must take the value its documentation states.
        let chunking: ChunkingConfig =
            toml::from_str("max_bm25_terms = 200").expect("partial chunking table must parse");
        assert_eq!(chunking.max_bm25_terms, 200);
        assert_eq!(chunking.max_tokens, 512);
        assert_eq!(chunking.overlap_tokens, 50);
        assert_eq!(chunking.overlap_bm25_terms, 5);
        assert_eq!(chunking.max_overlap_ratio, 0.2);
        assert_eq!(chunking.min_chunk_tokens, 150);
        assert_eq!(chunking.min_chunk_bm25_terms, 45);
        assert!(chunking.respect_boundaries);
        assert_eq!(chunking.cross_group_merge_threshold, 0);
    }

    #[test]
    fn test_documented_defaults_match_implementation() {
        let defaults = ChunkingConfig::default();
        assert_eq!(defaults.max_tokens, 512);
        assert_eq!(defaults.overlap_tokens, 50);
        assert_eq!(defaults.max_bm25_terms, 150);
        assert_eq!(defaults.overlap_bm25_terms, 5);
        assert_eq!(defaults.max_overlap_ratio, 0.2);
        assert_eq!(defaults.min_chunk_tokens, 150);
        assert_eq!(defaults.min_chunk_bm25_terms, 45);
        assert!(defaults.respect_boundaries);
    }
}
