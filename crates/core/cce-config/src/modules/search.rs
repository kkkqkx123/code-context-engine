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

/// Hybrid fusion algorithm used to combine vector and BM25 recall paths.
///
/// Each variant carries its own parameters so adding an algorithm only
/// requires a new variant plus its merger implementation; the surrounding
/// pipeline (alignment keys, coverage stats, dedup, sorting) is shared.
/// TOML form: `"weighted_min_max"` / `"borda_count"` for
/// the parameter-free variants, `{ rrf = { k = 60 } }` for RRF.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FusionAlgorithm {
    /// Weighted min-max score fusion (the historical default): normalize each
    /// path's per-key scores to [0, 1] and take the weighted linear
    /// combination. Sensitive to per-query score distribution outliers.
    #[default]
    WeightedMinMax,
    /// Reciprocal Rank Fusion: score = w_v/(k+rank_v) + w_b/(k+rank_b), using
    /// ranks only, so the result is robust to raw score distribution skew.
    Rrf { k: u32 },
    /// Weight-aware Borda count: each path awards `(key_count - rank + 1) /
    /// key_count` normalized points per alignment key (rank starts at 1, best
    /// key first) and the fused score is the weighted point sum, on the same
    /// `[0, w_v + w_b]` scale as weighted min-max. Like RRF it ignores raw
    /// score magnitudes, but awards linearly instead of hyperbolically, so
    /// deep ranks keep contributing instead of decaying to zero.
    BordaCount,
}

/// BM25 retrieval configuration (recall only, no fusion semantics).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Bm25RetrievalConfig {
    /// Minimum score threshold for BM25 results
    pub min_score: f32,
    /// BM25 field weights
    pub field_weights: HashMap<String, f32>,
    /// Multi-term query operator (`or`/`and`). Controls how the terms of a
    /// multi-word query are combined; quoted phrases always take precedence.
    /// Defaults to `or`: BM25 is fundamentally a fuzzy/recall-oriented search,
    /// and `and` requires every term to be present, which quickly yields zero
    /// results for natural-language queries. Use `and` only for deliberate
    /// exact-entity lookups (e.g. a known identifier name).
    pub term_operator: TermOperator,
}

/// Hybrid fusion configuration (recall-path combination only).
///
/// Owns the path weights, the algorithm selection, the query-intent weight
/// table, and the fusion runtime switches. Query-intent adaptation only
/// swaps the weight pair; the algorithm is never changed implicitly.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HybridFusionConfig {
    /// Weight assigned to vector path scores in hybrid recall fusion [0.0, 1.0]
    pub vector_weight: f32,
    /// Weight assigned to BM25 path scores in hybrid recall fusion [0.0, 1.0]
    pub bm25_weight: f32,
    /// Hybrid fusion algorithm selection
    pub algorithm: FusionAlgorithm,
    /// Per-intent weight configuration for query-adaptive hybrid fusion
    pub intent_weights: QueryIntentWeights,
    /// Whether to enable query-intent-based dynamic weight selection
    pub enable_intent_based_weights: bool,
    /// Whether to include items that only appear in one path.
    ///
    /// The semantics are asymmetric by design: vector-only keys are always
    /// kept, and this switch only controls whether BM25-only keys join the
    /// fused list. The vector path is treated as the primary semantic recall,
    /// so dropping its single-path hits is never desired; the switch instead
    /// governs the noisy BM25-only tail (e.g. keyword mentions with no
    /// semantic match).
    pub include_single_path: bool,
    /// Minimum fused score threshold. The scale depends on the algorithm:
    /// weighted min-max and borda count yield `[0, w_v + w_b]` (borda normalizes
    /// its per-path points by the path's key count), RRF yields
    /// `(0, (w_v + w_b) / (k + 1)]`.
    pub min_score: f32,
    /// Whether to keep at most one result per physical chunk after fusion.
    ///
    /// Entity-level alignment can surface the same chunk once per contained
    /// entity; enabling this collapses them to the best-scoring entry per
    /// chunk id so multi-entity chunks do not inflate the result list with
    /// identical content.
    pub dedup_by_chunk: bool,
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

impl Default for Bm25RetrievalConfig {
    fn default() -> Self {
        Self {
            min_score: 0.1,
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

impl Default for HybridFusionConfig {
    fn default() -> Self {
        Self {
            vector_weight: 0.5,
            bm25_weight: 0.5,
            algorithm: FusionAlgorithm::default(),
            intent_weights: QueryIntentWeights::default(),
            enable_intent_based_weights: true,
            include_single_path: true,
            min_score: 0.0,
            dedup_by_chunk: true,
        }
    }
}

fn validate_weight(field: &str, value: f32, errors: &mut Vec<ConfigValidationError>) {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        errors.push(ConfigValidationError::invalid_field(
            field,
            "must be finite and within [0.0, 1.0]",
        ));
    }
}

/// Require a vector/bm25 weight pair to sum to 1.
///
/// The fused score, the single-path score, and therefore `min_score` are all
/// interpreted on the `[0, 1]` scale only when the weights are complementary;
/// a pair summing to something else silently rescales every threshold.
fn validate_weight_pair_sum(
    field: &str,
    vector: f32,
    bm25: f32,
    errors: &mut Vec<ConfigValidationError>,
) {
    if vector.is_finite() && bm25.is_finite() && (vector + bm25 - 1.0).abs() > 1e-4 {
        errors.push(ConfigValidationError::invalid_field(
            field,
            "vector_weight + bm25_weight must sum to 1.0",
        ));
    }
}

impl Validate for HybridFusionConfig {
    fn validate_structured(&self) -> ValidationResult {
        let mut errors = Vec::new();

        validate_weight("vector_weight", self.vector_weight, &mut errors);
        validate_weight("bm25_weight", self.bm25_weight, &mut errors);
        validate_weight_pair_sum(
            "vector_weight",
            self.vector_weight,
            self.bm25_weight,
            &mut errors,
        );
        for (name, w) in [
            ("intent_weights.semantic", &self.intent_weights.semantic),
            ("intent_weights.keyword", &self.intent_weights.keyword),
            ("intent_weights.hybrid", &self.intent_weights.hybrid),
            ("intent_weights.entity", &self.intent_weights.entity),
        ] {
            validate_weight(
                &format!("{name}.vector_weight"),
                w.vector_weight,
                &mut errors,
            );
            validate_weight(&format!("{name}.bm25_weight"), w.bm25_weight, &mut errors);
            validate_weight_pair_sum(name, w.vector_weight, w.bm25_weight, &mut errors);
        }
        if !self.min_score.is_finite() || self.min_score < 0.0 {
            errors.push(ConfigValidationError::invalid_field(
                "min_score",
                "must be finite and >= 0.0 (interpreted on the selected algorithm scale)",
            ));
        }
        if let FusionAlgorithm::Rrf { k } = self.algorithm {
            if k == 0 {
                errors.push(ConfigValidationError::invalid_field(
                    "algorithm.rrf.k",
                    "must be greater than 0",
                ));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(ConfigValidationError::multiple(errors))
        }
    }
}

impl Validate for Bm25RetrievalConfig {
    fn validate_structured(&self) -> ValidationResult {
        let mut errors = Vec::new();

        if !self.min_score.is_finite() || self.min_score < 0.0 {
            errors.push(ConfigValidationError::invalid_field(
                "min_score",
                "must be finite and >= 0.0",
            ));
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(ConfigValidationError::multiple(errors))
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
    /// Maximum token budget for a single result body. A body over this budget
    /// is replaced by a file-and-range reference instead of being returned in
    /// full.
    pub max_content_tokens: usize,
}

impl Default for ResultFilterConfig {
    fn default() -> Self {
        Self {
            limit: 10,
            min_score: 0.25,
            max_content_tokens: 2000,
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
    /// Whether to enable score normalization before thresholding.
    ///
    /// Enabled by default so the final `result.min_score` threshold observes a
    /// uniform score scale across all recall strategies (bm25 raw scores are
    /// unbounded without it).
    pub enable: bool,
    /// Normalization strategy
    pub strategy: NormalizationStrategy,
}

impl Default for ScoreNormalizationConfig {
    fn default() -> Self {
        Self {
            enable: true,
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
    /// Default per-source addition cap for sources without an explicit entry
    /// in `source_caps`
    pub max_source_boost: f32,
    /// Per-source addition caps by source name (e.g. `summary`). Adding a new
    /// boost source only requires a new entry here, no code change. Sources
    /// without an entry fall back to `max_source_boost`.
    pub source_caps: HashMap<String, f32>,
}

impl Default for BoostAggregationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_addition: 0.5,
            max_source_boost: 0.3,
            source_caps: HashMap::from([("summary".to_string(), 0.15)]),
        }
    }
}

impl BoostAggregationConfig {
    /// Addition cap for one boost source: the explicit entry when present,
    /// otherwise the default per-source cap.
    pub fn cap_for(&self, source: &str) -> f32 {
        self.source_caps
            .get(source)
            .copied()
            .unwrap_or(self.max_source_boost)
    }
}

impl Validate for BoostAggregationConfig {
    fn validate_structured(&self) -> ValidationResult {
        let mut errors = Vec::new();

        for (field, value) in [
            ("max_addition", self.max_addition),
            ("max_source_boost", self.max_source_boost),
        ] {
            if !value.is_finite() || value < 0.0 {
                errors.push(ConfigValidationError::invalid_field(
                    field,
                    "must be finite and >= 0.0",
                ));
            }
        }
        let mut names: Vec<&String> = self.source_caps.keys().collect();
        names.sort();
        for name in names {
            let value = self.source_caps[name];
            if !value.is_finite() || value < 0.0 {
                errors.push(ConfigValidationError::invalid_field(
                    format!("source_caps[{name}]"),
                    "must be finite and >= 0.0",
                ));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(ConfigValidationError::multiple(errors))
        }
    }
}

// ============================================================================
// Score fusion strategy (serde-compatible)
// ============================================================================

fn default_fusion_alpha() -> f32 {
    0.7
}

fn default_fusion_rrf_k() -> f32 {
    60.0
}

/// Score fusion strategy for combining initial and rerank scores.
///
/// Unit variants keep the plain-string TOML form (`"rerank_only"`,
/// `"multiplicative"`); parameterized variants use single-key maps
/// (`{ linear_weighted = { alpha = 0.8 } }`,
/// `{ reciprocal_rank_fusion = { k = 60.0 } }`). The bare strings
/// `"linear_weighted"` and `"rrf"` / `"reciprocal_rank_fusion"` resolve to
/// the defaults so existing configuration files keep parsing.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScoreFusionStrategy {
    /// Use only rerank scores
    RerankOnly,
    /// Linear weighted fusion: final = alpha * rerank + (1 - alpha) * initial
    LinearWeighted { alpha: f32 },
    /// Multiplicative fusion: final = rerank * initial
    Multiplicative,
    /// Rank fusion of the rerank order and the initial order:
    /// final = 1/(k+rerank_rank+1) + 1/(k+initial_rank+1), with 0-based
    /// ranks. Ignores raw score magnitudes, so it stays robust when rerank
    /// and initial scores live on incompatible scales.
    ReciprocalRankFusion { k: f32 },
}

#[derive(Deserialize)]
struct LinearWeightedParams {
    #[serde(default = "default_fusion_alpha")]
    alpha: f32,
}

#[derive(Deserialize)]
struct RankFusionParams {
    #[serde(default = "default_fusion_rrf_k")]
    k: f32,
}

impl<'de> Deserialize<'de> for ScoreFusionStrategy {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de;
        struct StrategyVisitor;
        impl<'de> de::Visitor<'de> for StrategyVisitor {
            type Value = ScoreFusionStrategy;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a score fusion strategy string (\"rerank_only\", \"linear_weighted\", \"multiplicative\", \"reciprocal_rank_fusion\"/\"rrf\") or a single-key map such as {\"linear_weighted\": {\"alpha\": 0.7}}")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                match v {
                    "rerank_only" => Ok(ScoreFusionStrategy::RerankOnly),
                    "linear_weighted" => Ok(ScoreFusionStrategy::LinearWeighted {
                        alpha: default_fusion_alpha(),
                    }),
                    "multiplicative" => Ok(ScoreFusionStrategy::Multiplicative),
                    "reciprocal_rank_fusion" | "rrf" => {
                        Ok(ScoreFusionStrategy::ReciprocalRankFusion {
                            k: default_fusion_rrf_k(),
                        })
                    }
                    _ => Err(de::Error::unknown_variant(
                        v,
                        &[
                            "rerank_only",
                            "linear_weighted",
                            "multiplicative",
                            "reciprocal_rank_fusion",
                            "rrf",
                        ],
                    )),
                }
            }
            fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let name: Option<String> = map.next_key()?;
                let Some(name) = name else {
                    return Err(de::Error::invalid_length(0, &"a single-key strategy map"));
                };
                let strategy = match name.as_str() {
                    "rerank_only" => {
                        let _: de::IgnoredAny = map.next_value()?;
                        ScoreFusionStrategy::RerankOnly
                    }
                    "linear_weighted" => {
                        let params: LinearWeightedParams = map.next_value()?;
                        ScoreFusionStrategy::LinearWeighted {
                            alpha: params.alpha,
                        }
                    }
                    "multiplicative" => {
                        let _: de::IgnoredAny = map.next_value()?;
                        ScoreFusionStrategy::Multiplicative
                    }
                    "reciprocal_rank_fusion" | "rrf" => {
                        let params: RankFusionParams = map.next_value()?;
                        ScoreFusionStrategy::ReciprocalRankFusion { k: params.k }
                    }
                    other => {
                        return Err(de::Error::unknown_variant(
                            other,
                            &[
                                "rerank_only",
                                "linear_weighted",
                                "multiplicative",
                                "reciprocal_rank_fusion",
                                "rrf",
                            ],
                        ));
                    }
                };
                if map.next_key::<de::IgnoredAny>()?.is_some() {
                    return Err(de::Error::invalid_length(2, &"a single-key strategy map"));
                }
                Ok(strategy)
            }
        }
        deserializer.deserialize_any(StrategyVisitor)
    }
}

impl ScoreFusionStrategy {
    /// Calculate the final score from rerank and initial scores.
    ///
    /// Both ranks are 0-based positions: `rerank_rank` in the reranked order,
    /// `initial_rank` in the pre-rerank order. Only the rank-fusion variant
    /// reads them; score-based variants ignore both ranks.
    pub fn calculate(
        &self,
        rerank_score: f32,
        initial_score: f32,
        rerank_rank: usize,
        initial_rank: usize,
    ) -> f32 {
        match self {
            ScoreFusionStrategy::RerankOnly => rerank_score,
            ScoreFusionStrategy::LinearWeighted { alpha } => {
                alpha * rerank_score + (1.0 - alpha) * initial_score
            }
            ScoreFusionStrategy::Multiplicative => rerank_score * initial_score,
            ScoreFusionStrategy::ReciprocalRankFusion { k } => {
                let denom_rerank = *k + rerank_rank as f32 + 1.0;
                let denom_initial = *k + initial_rank as f32 + 1.0;
                if !denom_rerank.is_finite()
                    || !denom_initial.is_finite()
                    || denom_rerank <= 0.0
                    || denom_initial <= 0.0
                {
                    rerank_score
                } else {
                    1.0 / denom_rerank + 1.0 / denom_initial
                }
            }
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
    /// Maximum assembled content per result in tokens (using TokenEstimator).
    /// A single oversized body is downgraded to a path-and-range reference;
    /// the batch total is bounded by `assembly_top_n` times this value.
    pub max_assembled_length: usize,
    /// Include file boundary markers
    pub include_file_markers: bool,
    /// Deduplication strategy
    pub dedup_strategy: DedupStrategy,
    /// Number of top results to assemble
    pub assembly_top_n: usize,
    /// Enable adjacent segment merging
    pub enable_segment_merge: bool,
    /// Maximum gap between segments to merge (in lines)
    pub segment_merge_gap: u32,
    /// Enable relation expansion: attach pre-resolved call-graph neighbours
    /// (callees/callers) supplied by the caller to each assembled result.
    pub expansion_enabled: bool,
    /// Maximum number of expansion units attached to one result
    /// (shared across both directions; forward units are taken first).
    pub max_expanded_units: usize,
    /// Include caller-side (backward) expansion units.
    pub expansion_include_callers: bool,
    /// Also attach non-call edges (inheritance, implementation, imports).
    /// Off by default: structural edges fan out and drown the budget.
    pub allow_structural_edges: bool,
    /// Drop standard-library targets during expansion (defense in depth; the
    /// caller filters first so dropped units never occupy budget).
    pub filter_stdlib: bool,
    /// Drop external targets without workspace source during expansion.
    pub filter_external: bool,
    /// Workspace root for file existence checks. Relative segment paths
    /// resolve against it; absolute paths are checked directly. Missing files
    /// are downgraded to references. `None` skips the check entirely.
    ///
    /// Runtime-only: derived from the project registry, never read from or
    /// written to the config file.
    #[serde(skip)]
    pub workspace_root: Option<std::path::PathBuf>,
}

impl Default for SPSRGraphConfig {
    fn default() -> Self {
        Self {
            enable_assembly: false,
            max_assembled_length: 8000,
            include_file_markers: true,
            dedup_strategy: DedupStrategy::ByEntityId,
            assembly_top_n: 3,
            enable_segment_merge: true,
            segment_merge_gap: 2,
            expansion_enabled: false,
            max_expanded_units: 4,
            expansion_include_callers: true,
            allow_structural_edges: false,
            filter_stdlib: true,
            filter_external: true,
            workspace_root: None,
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

    /// Set the maximum assembled content length in tokens (builder pattern).
    pub fn with_max_length(mut self, length: usize) -> Self {
        self.max_assembled_length = length;
        self
    }

    /// Enable or disable relation expansion (builder pattern).
    pub fn with_expansion(mut self, enabled: bool) -> Self {
        self.expansion_enabled = enabled;
        self
    }

    /// Set the maximum expansion units per result (builder pattern).
    pub fn with_max_expanded_units(mut self, count: usize) -> Self {
        self.max_expanded_units = count;
        self
    }

    /// Include or exclude caller-side expansion units (builder pattern).
    pub fn with_caller_expansion(mut self, include: bool) -> Self {
        self.expansion_include_callers = include;
        self
    }

    /// Allow or forbid non-call (structural) expansion edges (builder pattern).
    pub fn with_structural_edges(mut self, allow: bool) -> Self {
        self.allow_structural_edges = allow;
        self
    }

    /// Enable or disable standard-library filtering during expansion (builder pattern).
    pub fn with_stdlib_filter(mut self, filter: bool) -> Self {
        self.filter_stdlib = filter;
        self
    }

    /// Enable or disable external-target filtering during expansion (builder pattern).
    pub fn with_external_filter(mut self, filter: bool) -> Self {
        self.filter_external = filter;
        self
    }

    /// Set the workspace root for file existence checks (builder pattern).
    pub fn with_workspace_root(mut self, root: impl Into<std::path::PathBuf>) -> Self {
        self.workspace_root = Some(root.into());
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

        if self.max_assembled_length == 0 {
            errors.push(ConfigValidationError::invalid_field(
                "max_assembled_length",
                "must be greater than 0",
            ));
        }
        if self.expansion_enabled && self.max_expanded_units == 0 {
            errors.push(ConfigValidationError::invalid_field(
                "max_expanded_units",
                "must be greater than 0 when expansion is enabled",
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
    /// Create a conservative SPSR-Graph configuration with shallow assembly.
    pub fn conservative() -> Self {
        Self {
            enable_assembly: true,
            max_assembled_length: 4000,
            ..Self::default()
        }
    }

    /// Create an aggressive SPSR-Graph configuration with deep assembly.
    pub fn aggressive() -> Self {
        Self {
            enable_assembly: true,
            max_assembled_length: 16000,
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
    /// BM25 retrieval configuration (recall only)
    #[serde(default)]
    pub bm25: Bm25RetrievalConfig,
    /// Hybrid fusion configuration (vector + BM25 combination)
    #[serde(default)]
    pub fusion: HybridFusionConfig,
    /// Result filtering configuration
    #[serde(default)]
    pub result: ResultFilterConfig,
    /// Summary-based score boost configuration
    #[serde(default)]
    pub summary: SummaryBoostConfig,
    /// Score normalization configuration
    #[serde(default)]
    pub score: ScoreNormalizationConfig,
    /// Unified boost aggregation configuration
    #[serde(default)]
    pub boost: BoostAggregationConfig,
    /// Query-side plugin hooks configuration (`QueryRewrite` / `Fusion` /
    /// `ResultFilter`).
    #[serde(default)]
    pub plugin: PluginSearchConfig,
    /// Structure-preserving assembly of the final top-N results.
    #[serde(default)]
    pub assembly: SPSRGraphConfig,
    /// Overall search pipeline timeout in milliseconds.
    ///
    /// Bounds the end-to-end search operation at the coordinator level,
    /// covering retrieval, fusion, enrichment, reranking, and assembly.
    /// Defaults to 30 seconds.
    #[serde(default = "default_search_timeout_ms")]
    pub timeout_ms: u64,
}

fn default_search_timeout_ms() -> u64 {
    30_000
}

impl Validate for SearchModuleConfig {
    fn validate_structured(&self) -> ValidationResult {
        let mut errors = Vec::new();

        if let Err(e) = self.bm25.validate_structured() {
            errors.push(e);
        }
        if let Err(e) = self.fusion.validate_structured() {
            errors.push(e);
        }
        if let Err(e) = self.boost.validate_structured() {
            errors.push(e);
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(ConfigValidationError::multiple(errors))
        }
    }
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
        assert_eq!(config.fusion.algorithm, FusionAlgorithm::WeightedMinMax);
        assert!((config.boost.cap_for("summary") - 0.15).abs() < f32::EPSILON);
        assert!((config.boost.cap_for("unknown_source") - 0.3).abs() < f32::EPSILON);
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

    #[test]
    fn test_fusion_algorithm_serde() {
        let json = serde_json::to_string(&FusionAlgorithm::WeightedMinMax).unwrap();
        assert_eq!(json, "\"weighted_min_max\"");
        let deser: FusionAlgorithm = serde_json::from_str(&json).unwrap();
        assert_eq!(deser, FusionAlgorithm::WeightedMinMax);

        let rrf = FusionAlgorithm::Rrf { k: 30 };
        let json = serde_json::to_string(&rrf).unwrap();
        let deser: FusionAlgorithm = serde_json::from_str(&json).unwrap();
        assert_eq!(deser, rrf);
    }

    #[test]
    fn test_score_fusion_strategy_serde_roundtrip() {
        for strategy in [
            ScoreFusionStrategy::RerankOnly,
            ScoreFusionStrategy::LinearWeighted { alpha: 0.8 },
            ScoreFusionStrategy::Multiplicative,
            ScoreFusionStrategy::ReciprocalRankFusion { k: 30.0 },
        ] {
            let json = serde_json::to_string(&strategy).expect("serialize");
            let deser: ScoreFusionStrategy = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(deser, strategy);
        }
    }

    #[test]
    fn test_score_fusion_strategy_legacy_strings() {
        let deser: ScoreFusionStrategy =
            serde_json::from_str("\"linear_weighted\"").expect("deserialize");
        assert_eq!(deser, ScoreFusionStrategy::LinearWeighted { alpha: 0.7 });
        let deser: ScoreFusionStrategy = serde_json::from_str("\"rrf\"").expect("deserialize");
        assert_eq!(deser, ScoreFusionStrategy::ReciprocalRankFusion { k: 60.0 });
    }

    #[test]
    fn test_score_fusion_rrf_uses_both_ranks() {
        let strategy = ScoreFusionStrategy::ReciprocalRankFusion { k: 60.0 };
        let top_both = strategy.calculate(0.9, 0.8, 0, 0);
        let top_rerank_only = strategy.calculate(0.9, 0.8, 0, 9);
        let deep_both = strategy.calculate(0.9, 0.8, 9, 9);
        assert!((top_both - (1.0 / 61.0 + 1.0 / 61.0)).abs() < 1e-6);
        assert!(top_rerank_only < top_both);
        assert!(deep_both < top_rerank_only);
    }

    #[test]
    fn test_hybrid_fusion_config_validation() {
        let mut config = HybridFusionConfig::default();
        assert!(config.validate_structured().is_ok());

        config.vector_weight = 1.7;
        assert!(config.validate_structured().is_err());
        config.vector_weight = 0.5;

        config.min_score = -0.1;
        assert!(config.validate_structured().is_err());
        config.min_score = 0.0;

        config.algorithm = FusionAlgorithm::Rrf { k: 0 };
        assert!(config.validate_structured().is_err());
        config.algorithm = FusionAlgorithm::default();

        config.bm25_weight = 0.6;
        assert!(config.validate_structured().is_err());
        config.bm25_weight = 0.5;

        config.intent_weights.semantic = HybridWeightConfig {
            vector_weight: 0.9,
            bm25_weight: 0.2,
        };
        assert!(config.validate_structured().is_err());
    }
}
