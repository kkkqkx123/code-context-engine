//! Search configuration module
//!
//! Defines serde-compatible configuration types for the search pipeline.
//! These types are user-facing and can be configured via config.toml.
//!
//! The orchestrator's internal `SearchConfig` consumes these types
//! via `From<SearchModuleConfig>` conversion.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::validation::{Validate, ValidationResult};
use cce_types::error::config::ConfigValidationError;

// ============================================================================
// Score normalization strategy (serde-compatible)
// ============================================================================

/// Score normalization strategy
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NormalizationStrategy {
    /// Min-Max normalization to [0, 1]
    #[default]
    MinMax,
    /// Z-score normalization (standardization)
    ZScore,
    /// No normalization (use raw scores)
    None,
}

// ============================================================================
// Query intent weight configuration
// ============================================================================

/// Hybrid fusion weights for a specific query intent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HybridWeightConfig {
    /// Weight assigned to normalized vector scores [0.0, 1.0]
    pub vector_weight: f32,
    /// Weight assigned to normalized BM25 scores [0.0, 1.0]
    pub bm25_weight: f32,
}

impl Default for HybridWeightConfig {
    fn default() -> Self {
        Self {
            vector_weight: 0.5,
            bm25_weight: 0.5,
        }
    }
}

/// Per-intent weight configuration table for hybrid fusion.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct QueryIntentWeights {
    /// Weight for semantic/natural language queries (vector-leaning)
    pub semantic: HybridWeightConfig,
    /// Weight for precise keyword queries (BM25-leaning)
    pub keyword: HybridWeightConfig,
    /// Weight for mixed queries (balanced)
    pub hybrid: HybridWeightConfig,
    /// Weight for entity/code symbol lookups (vector-leaning)
    pub entity: HybridWeightConfig,
}

impl Default for QueryIntentWeights {
    fn default() -> Self {
        Self {
            semantic: HybridWeightConfig {
                vector_weight: 0.8,
                bm25_weight: 0.2,
            },
            keyword: HybridWeightConfig {
                vector_weight: 0.2,
                bm25_weight: 0.8,
            },
            hybrid: HybridWeightConfig {
                vector_weight: 0.5,
                bm25_weight: 0.5,
            },
            entity: HybridWeightConfig {
                vector_weight: 0.7,
                bm25_weight: 0.3,
            },
        }
    }
}

// ============================================================================
// Sub-configuration structs
// ============================================================================

/// Vector retrieval configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VectorRetrievalConfig {
    /// Number of candidates to retrieve
    pub top_k: usize,
    /// Minimum similarity threshold
    pub min_score: f32,
    /// HNSW ef parameter
    pub hnsw_ef: u32,
}

impl Default for VectorRetrievalConfig {
    fn default() -> Self {
        Self {
            top_k: 50,
            min_score: 0.3,
            hnsw_ef: 128,
        }
    }
}

/// BM25 fusion configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Bm25FusionConfig {
    /// Minimum score threshold for BM25 results
    pub min_score: f32,
    /// BM25 field weights
    pub field_weights: HashMap<String, f32>,
    /// Weight assigned to vector path scores in hybrid recall fusion [0.0, 1.0]
    pub vector_weight: f32,
    /// Weight assigned to BM25 path scores in hybrid recall fusion [0.0, 1.0]
    pub bm25_weight: f32,
    /// Per-intent weight configuration for query-adaptive hybrid fusion
    pub intent_weights: QueryIntentWeights,
    /// Whether to enable query-intent-based dynamic weight selection
    pub enable_intent_based_weights: bool,
    /// Multi-term query operator (`or`/`and`). Controls how the terms of a
    /// multi-word query are combined; quoted phrases always take precedence.
    /// Defaults to `or`: BM25 is fundamentally a fuzzy/recall-oriented search,
    /// and `and` requires every term to be present, which quickly yields zero
    /// results for natural-language queries. Use `and` only for deliberate
    /// exact-entity lookups (e.g. a known identifier name).
    pub term_operator: TermOperator,
}

/// Operator used to combine the terms of a multi-word BM25 query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TermOperator {
    /// Match any term (OR semantics). The default and recommended mode:
    /// preserves BM25's fuzzy-search recall for natural-language queries.
    #[default]
    Or,
    /// Match all terms (AND semantics). An opt-in precision mode for exact
    /// entity/identifier lookups only — not intended as a common query mode,
    /// since requiring every term kills fuzzy recall.
    And,
}

impl Default for Bm25FusionConfig {
    fn default() -> Self {
        Self {
            min_score: 0.1,
            vector_weight: 0.5,
            bm25_weight: 0.5,
            intent_weights: QueryIntentWeights::default(),
            enable_intent_based_weights: true,
            term_operator: TermOperator::default(),
            field_weights: {
                let mut w = HashMap::new();
                // title=2.0 chosen from bm25_parameter_sweep benchmark:
                // title_w=2 beats 4 and 6 consistently across all fixtures
                // (see docs/archive/bm25-parameter-tuning.md)
                w.insert("title".to_string(), 2.0);
                w.insert("content".to_string(), 1.0);
                w.insert("keywords".to_string(), 2.0);
                w
            },
        }
    }
}

/// Result filtering configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ResultFilterConfig {
    /// Maximum number of results to return
    pub limit: usize,
    /// Minimum score threshold for final results
    pub min_score: f32,
    /// Maximum results per file (diversity control)
    pub max_per_file: usize,
}

impl Default for ResultFilterConfig {
    fn default() -> Self {
        Self {
            limit: 10,
            min_score: 0.25,
            max_per_file: 3,
        }
    }
}

/// Relation-based score boost configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RelationBoostConfig {
    /// Number of top results to enhance with relation data
    pub top_n: usize,
    /// Query depth for relation traversal
    pub depth: usize,
    /// Timeout in milliseconds for relation queries
    pub timeout_ms: u64,
    /// Base boost factor for directly related entities (1 hop)
    pub boost_factor: f32,
    /// Maximum number of hops in relation graph traversal
    pub max_hops: usize,
    /// Whether to include callees (forward expansion)
    pub include_callees: bool,
    /// Whether to include callers (backward expansion)
    ///
    /// When enabled, low-level utility functions can receive score boosts
    /// from their high-level callers, improving recall for "who uses X"
    /// style queries at the cost of additional graph traversal.
    pub include_callers: bool,
    /// Whether to enable relation boost by default for hybrid/hierarchical queries
    ///
    /// When true, `hybrid` and `hierarchical` query types automatically
    /// include relation-based score boosting without requiring the client to
    /// explicitly set `query_type: "semantic_with_relations"`.
    pub enabled_by_default: bool,
}

impl Default for RelationBoostConfig {
    fn default() -> Self {
        Self {
            top_n: 5,
            depth: 2,
            timeout_ms: 500,
            boost_factor: 1.15,
            max_hops: 2,
            include_callees: true,
            include_callers: false,
            enabled_by_default: false,
        }
    }
}

/// Summary-based score boost configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SummaryBoostConfig {
    /// Enable summary-based file pre-filtering
    pub enable_pre_filter: bool,
    /// Enable summary-based score boosting
    pub enable_boost: bool,
    /// Number of files to retrieve from summary index
    pub top_k: usize,
    /// Minimum similarity threshold for summary matching
    pub min_score: f32,
    /// Boost factor for results in matching files
    pub boost_factor: f32,
}

impl Default for SummaryBoostConfig {
    fn default() -> Self {
        Self {
            enable_pre_filter: false,
            enable_boost: false,
            top_k: 20,
            min_score: 0.4,
            boost_factor: 1.2,
        }
    }
}

/// Score normalization configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ScoreNormalizationConfig {
    /// Whether to enable pre-boost score normalization
    pub enable: bool,
    /// Normalization strategy
    pub strategy: NormalizationStrategy,
}

impl Default for ScoreNormalizationConfig {
    fn default() -> Self {
        Self {
            enable: false,
            strategy: NormalizationStrategy::MinMax,
        }
    }
}

/// Boost aggregation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BoostAggregationConfig {
    /// Whether unified boost aggregation is enabled
    pub enabled: bool,
    /// Maximum total addition across all sources (e.g., 0.5 = 50% max boost)
    pub max_addition: f32,
    /// Default per-source addition cap (overridden by source-specific caps)
    pub max_source_boost: f32,
    /// Maximum addition for summary relevance boost
    pub summary_max: f32,
    /// Maximum addition for relation graph boost
    pub relation_max: f32,
}

impl Default for BoostAggregationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_addition: 0.5,
            max_source_boost: 0.3,
            summary_max: 0.15,
            relation_max: 0.15,
        }
    }
}

// ============================================================================
// Score fusion strategy (serde-compatible)
// ============================================================================

/// Score fusion strategy for combining initial and rerank scores.
///
/// Serializes/deserializes as a simple string for TOML backward compatibility.
/// - `"rerank_only"` → `RerankOnly`
/// - `"linear_weighted"` → `LinearWeighted { alpha: 0.7 }`
/// - `"multiplicative"` → `Multiplicative`
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScoreFusionStrategy {
    /// Use only rerank scores
    RerankOnly,
    /// Linear weighted fusion: final = alpha * rerank + (1 - alpha) * initial
    LinearWeighted { alpha: f32 },
    /// Multiplicative fusion: final = rerank * initial
    Multiplicative,
}

impl Serialize for ScoreFusionStrategy {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            ScoreFusionStrategy::RerankOnly => serializer.serialize_str("rerank_only"),
            ScoreFusionStrategy::LinearWeighted { .. } => {
                serializer.serialize_str("linear_weighted")
            }
            ScoreFusionStrategy::Multiplicative => serializer.serialize_str("multiplicative"),
        }
    }
}

impl<'de> Deserialize<'de> for ScoreFusionStrategy {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de;
        struct ScoreFusionVisitor;
        impl<'de> de::Visitor<'de> for ScoreFusionVisitor {
            type Value = ScoreFusionStrategy;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a score fusion strategy string: \"rerank_only\", \"linear_weighted\", or \"multiplicative\"")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                match v {
                    "rerank_only" => Ok(ScoreFusionStrategy::RerankOnly),
                    "linear_weighted" => Ok(ScoreFusionStrategy::LinearWeighted { alpha: 0.7 }),
                    "multiplicative" => Ok(ScoreFusionStrategy::Multiplicative),
                    _ => Err(de::Error::unknown_variant(
                        v,
                        &["rerank_only", "linear_weighted", "multiplicative"],
                    )),
                }
            }
        }
        deserializer.deserialize_str(ScoreFusionVisitor)
    }
}

impl ScoreFusionStrategy {
    /// Calculate the final score from rerank and initial scores.
    pub fn calculate(&self, rerank_score: f32, initial_score: f32, _rank: usize) -> f32 {
        match self {
            ScoreFusionStrategy::RerankOnly => rerank_score,
            ScoreFusionStrategy::LinearWeighted { alpha } => {
                alpha * rerank_score + (1.0 - alpha) * initial_score
            }
            ScoreFusionStrategy::Multiplicative => rerank_score * initial_score,
        }
    }
}

// ============================================================================
// Expansion strategy for SPSR-Graph
// ============================================================================

/// Expansion strategy for call chain traversal during SPSR-Graph assembly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExpansionStrategy {
    /// No expansion
    None,
    /// Forward only (get callees)
    #[default]
    ForwardOnly,
    /// Backward only (get callers)
    BackwardOnly,
    /// Bidirectional expansion
    Bidirectional,
}

impl std::fmt::Display for ExpansionStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => write!(f, "none"),
            Self::ForwardOnly => write!(f, "forward_only"),
            Self::BackwardOnly => write!(f, "backward_only"),
            Self::Bidirectional => write!(f, "bidirectional"),
        }
    }
}

// ============================================================================
// Deduplication strategy for SPSR-Graph
// ============================================================================

/// Deduplication strategy for assembled results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DedupStrategy {
    /// No deduplication
    None,
    /// Deduplicate by entity_id
    #[default]
    ByEntityId,
    /// Deduplicate by content hash
    ByContentHash,
}

// ============================================================================
// SPSR-Graph assembly configuration
// ============================================================================

/// SPSR-Graph assembly configuration.
///
/// Controls how search results are assembled into structure-preserving
/// code graphs with call-chain context.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SPSRGraphConfig {
    /// Enable SPSR-Graph assembly
    pub enable_assembly: bool,
    /// Expansion strategy
    pub expansion_strategy: ExpansionStrategy,
    /// Maximum expansion depth
    pub max_expansion_depth: usize,
    /// Maximum expanded nodes per result
    pub max_expanded_nodes: usize,
    /// Maximum assembled content in tokens (using TokenEstimator)
    pub max_assembled_length: usize,
    /// Include file boundary markers
    pub include_file_markers: bool,
    /// Include relation markers
    pub include_relation_markers: bool,
    /// Deduplication strategy
    pub dedup_strategy: DedupStrategy,
    /// Number of top results to assemble
    pub assembly_top_n: usize,
    /// Enable adjacent segment merging
    pub enable_segment_merge: bool,
    /// Maximum gap between segments to merge (in lines)
    pub segment_merge_gap: u32,
    /// Enable file coverage threshold check
    pub enable_file_coverage_threshold: bool,
    /// File coverage threshold (0.0-1.0), return whole file if exceeded
    pub file_coverage_threshold: f32,
}

impl Default for SPSRGraphConfig {
    fn default() -> Self {
        Self {
            enable_assembly: false,
            expansion_strategy: ExpansionStrategy::ForwardOnly,
            max_expansion_depth: 2,
            max_expanded_nodes: 5,
            max_assembled_length: 2500,
            include_file_markers: true,
            include_relation_markers: true,
            dedup_strategy: DedupStrategy::ByEntityId,
            assembly_top_n: 3,
            enable_segment_merge: true,
            segment_merge_gap: 2,
            enable_file_coverage_threshold: true,
            file_coverage_threshold: 0.6,
        }
    }
}

// ============================================================================
// SPSR-Graph config builder & utility methods
// ============================================================================

impl SPSRGraphConfig {
    /// Creates a new `SPSRGraphConfig` with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable or disable SPSR-Graph assembly (builder pattern).
    pub fn enable(mut self, enabled: bool) -> Self {
        self.enable_assembly = enabled;
        self
    }

    /// Set the expansion strategy (builder pattern).
    pub fn with_expansion_strategy(mut self, strategy: ExpansionStrategy) -> Self {
        self.expansion_strategy = strategy;
        self
    }

    /// Set the maximum expansion depth (builder pattern).
    pub fn with_max_depth(mut self, depth: usize) -> Self {
        self.max_expansion_depth = depth;
        self
    }

    /// Set the maximum expanded nodes per result (builder pattern).
    pub fn with_max_nodes(mut self, nodes: usize) -> Self {
        self.max_expanded_nodes = nodes;
        self
    }

    /// Set the maximum assembled content length in tokens (builder pattern).
    pub fn with_max_length(mut self, length: usize) -> Self {
        self.max_assembled_length = length;
        self
    }

    /// Returns the maximum assembled content length in tokens.
    pub fn get_max_length(&self) -> usize {
        self.max_assembled_length
    }

    /// Check whether the given token count is within the configured limit.
    pub fn check_content_limit(&self, token_count: usize) -> bool {
        token_count <= self.max_assembled_length
    }

    /// Estimate the token count of a text string using `TokenEstimator`.
    pub fn estimate_content_tokens(&self, text: &str) -> usize {
        use cce_utils::token_estimation::TokenEstimator;
        TokenEstimator::estimate(text)
    }
}

impl Validate for SPSRGraphConfig {
    fn validate_structured(&self) -> ValidationResult {
        let mut errors = Vec::new();

        if self.max_expansion_depth == 0 {
            errors.push(ConfigValidationError::invalid_field(
                "max_expansion_depth",
                "must be greater than 0",
            ));
        }
        if self.max_expanded_nodes == 0 {
            errors.push(ConfigValidationError::invalid_field(
                "max_expanded_nodes",
                "must be greater than 0",
            ));
        }
        if self.max_assembled_length == 0 {
            errors.push(ConfigValidationError::invalid_field(
                "max_assembled_length",
                "must be greater than 0",
            ));
        }
        if self.file_coverage_threshold <= 0.0 || self.file_coverage_threshold > 1.0 {
            errors.push(ConfigValidationError::out_of_range(
                "file_coverage_threshold",
                self.file_coverage_threshold.to_string(),
                "0.0",
                "1.0",
            ));
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(ConfigValidationError::multiple(errors))
        }
    }
}

impl SPSRGraphConfig {
    /// Create a conservative SPSR-Graph configuration with shallow expansion.
    pub fn conservative() -> Self {
        Self {
            enable_assembly: true,
            max_expansion_depth: 1,
            max_expanded_nodes: 3,
            max_assembled_length: 1500,
            ..Self::default()
        }
    }

    /// Create an aggressive SPSR-Graph configuration with deep expansion.
    pub fn aggressive() -> Self {
        Self {
            enable_assembly: true,
            expansion_strategy: ExpansionStrategy::Bidirectional,
            max_expansion_depth: 3,
            max_expanded_nodes: 10,
            max_assembled_length: 5000,
            ..Self::default()
        }
    }
}

// ============================================================================
// Top-level search module configuration
// ============================================================================

/// Search configuration module.
///
/// Aggregates all sub-configurations for the search pipeline.
/// Placed under `[search]` in config.toml.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SearchModuleConfig {
    /// Vector retrieval configuration
    #[serde(default)]
    pub vector: VectorRetrievalConfig,
    /// BM25 fusion configuration
    #[serde(default)]
    pub bm25: Bm25FusionConfig,
    /// Result filtering configuration
    #[serde(default)]
    pub result: ResultFilterConfig,
    /// Relation-based score boost configuration
    #[serde(default)]
    pub relation: RelationBoostConfig,
    /// Summary-based score boost configuration
    #[serde(default)]
    pub summary: SummaryBoostConfig,
    /// Score normalization configuration
    #[serde(default)]
    pub score: ScoreNormalizationConfig,
    /// Unified boost aggregation configuration
    #[serde(default)]
    pub boost: BoostAggregationConfig,
    /// SPSR-Graph assembly configuration
    #[serde(default)]
    pub spsr_graph: SPSRGraphConfig,
    /// Query-side plugin hooks configuration (`QueryRewrite` / `Fusion` /
    /// `ResultFilter`).
    #[serde(default)]
    pub plugin: PluginSearchConfig,
}

/// Query-side plugin hook toggles.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginSearchConfig {
    /// Whether `QueryRewrite` plugins run before recall.
    #[serde(default)]
    pub rewrite_enabled: bool,
    /// Whether `Fusion` plugins can override fusion weights.
    #[serde(default)]
    pub fusion_enabled: bool,
    /// Whether `ResultFilter` plugins run after rerank.
    #[serde(default)]
    pub filter_enabled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vector_retrieval_config_default() {
        let config = VectorRetrievalConfig::default();
        assert_eq!(config.top_k, 50);
        assert_eq!(config.min_score, 0.3);
        assert_eq!(config.hnsw_ef, 128);
    }

    #[test]
    fn test_result_filter_config_default() {
        let config = ResultFilterConfig::default();
        assert_eq!(config.limit, 10);
        assert_eq!(config.min_score, 0.25);
        assert_eq!(config.max_per_file, 3);
    }

    #[test]
    fn test_query_intent_weights_default() {
        let weights = QueryIntentWeights::default();
        assert!((weights.semantic.vector_weight - 0.8).abs() < 0.001);
        assert!((weights.semantic.bm25_weight - 0.2).abs() < 0.001);
        assert!((weights.keyword.vector_weight - 0.2).abs() < 0.001);
        assert!((weights.keyword.bm25_weight - 0.8).abs() < 0.001);
    }

    #[test]
    fn test_search_module_config_default() {
        let config = SearchModuleConfig::default();
        assert_eq!(config.vector.top_k, 50);
        assert_eq!(config.result.limit, 10);
        assert!(config.boost.enabled);
        assert!((config.boost.max_addition - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_toml_roundtrip() {
        let config = SearchModuleConfig::default();
        let toml_str = toml::to_string(&config).expect("serialize");
        let deserialized: SearchModuleConfig = toml::from_str(&toml_str).expect("deserialize");
        assert_eq!(deserialized.vector.top_k, 50);
        assert_eq!(deserialized.boost.max_addition, 0.5);
    }

    #[test]
    fn test_normalization_strategy_serde() {
        let json = serde_json::to_string(&NormalizationStrategy::MinMax).unwrap();
        assert_eq!(json, "\"min_max\"");
        let deser: NormalizationStrategy = serde_json::from_str(&json).unwrap();
        assert_eq!(deser, NormalizationStrategy::MinMax);

        let json = serde_json::to_string(&NormalizationStrategy::ZScore).unwrap();
        assert_eq!(json, "\"z_score\"");
        let deser: NormalizationStrategy = serde_json::from_str(&json).unwrap();
        assert_eq!(deser, NormalizationStrategy::ZScore);
    }
}
