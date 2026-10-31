//! Search configuration types
//!
//! SearchConfig is composed of multiple sub-configuration structs,
//! each responsible for a specific phase of the search pipeline.
//! This modular design simplifies parameter passing to individual
//! pipeline stages and makes defaults more maintainable.
//!
//! The runtime types here can be constructed from serde-compatible
//! config file types via `From<cce_config::modules::search::SearchModuleConfig>`.

/// Re-export reusable configuration types from the search module.
///
/// These are the serde-compatible, data-only types that map 1:1
/// between config.toml and runtime usage.
pub use cce_config::modules::rerank::RerankConfig;
pub use cce_config::modules::search::{
    Bm25RetrievalConfig, HybridFusionConfig, HybridWeightConfig, PluginSearchConfig,
    QueryIntentWeights, RelationAnnotationConfig, RerankFusionStrategy, ResultFilterConfig,
    ScoreNormalizationConfig, VectorRetrievalConfig,
};

// ============================================================================
// Extension trait for cce_core's QueryIntentWeights
// ============================================================================

/// Extension trait adding orchestrator-specific methods to `QueryIntentWeights`.
pub trait QueryIntentWeightsExt {
    /// Get weight config for a specific query intent.
    fn for_intent(
        &self,
        intent: crate::query::types::query_options::QueryIntent,
    ) -> &HybridWeightConfig;
}

impl QueryIntentWeightsExt for QueryIntentWeights {
    fn for_intent(
        &self,
        intent: crate::query::types::query_options::QueryIntent,
    ) -> &HybridWeightConfig {
        match intent {
            crate::query::types::query_options::QueryIntent::Semantic => &self.semantic,
            crate::query::types::query_options::QueryIntent::Keyword => &self.keyword,
            crate::query::types::query_options::QueryIntent::Hybrid => &self.hybrid,
            crate::query::types::query_options::QueryIntent::Entity => &self.entity,
        }
    }
}

// ============================================================================
// Top-level SearchConfig
// ============================================================================

/// Search configuration composed of sub-configurations for each pipeline stage.
///
/// # Organization
///
/// | Field | Purpose |
/// |-------|---------|
/// | `vector` | Vector retrieval parameters (top_k, min_score, hnsw_ef) |
/// | `bm25` | BM25 retrieval parameters (min_score, field_weights, term_operator) |
/// | `fusion` | Hybrid fusion parameters (weights, algorithm, intent weights) |
/// | `result` | Result filtering (limit, min_score, max_per_file) |
/// | `rerank` | LLM reranking (enable, model, candidates, temperature) |
/// | `score` | Score normalization (enable, strategy) |
#[derive(Debug, Clone)]
pub struct SearchConfig {
    /// Vector retrieval configuration
    pub vector: VectorRetrievalConfig,
    /// BM25 retrieval configuration
    pub bm25: Bm25RetrievalConfig,
    /// Hybrid fusion configuration
    pub fusion: HybridFusionConfig,
    /// Result filtering configuration
    pub result: ResultFilterConfig,
    /// LLM reranking configuration
    pub rerank: RerankConfig,
    /// Score normalization configuration
    pub score: ScoreNormalizationConfig,
    /// Query-side plugin hooks configuration
    pub plugin: PluginSearchConfig,
    /// Structure-preserving annotation of the final top-N results
    pub annotation: RelationAnnotationConfig,
    /// Overall search pipeline timeout in milliseconds
    pub timeout_ms: u64,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            vector: VectorRetrievalConfig::default(),
            bm25: Bm25RetrievalConfig::default(),
            fusion: HybridFusionConfig::default(),
            result: ResultFilterConfig::default(),
            rerank: RerankConfig::default(),
            score: ScoreNormalizationConfig::default(),
            plugin: PluginSearchConfig::default(),
            annotation: RelationAnnotationConfig::default(),
            timeout_ms: 30_000,
        }
    }
}

impl From<cce_config::modules::search::SearchModuleConfig> for SearchConfig {
    fn from(cfg: cce_config::modules::search::SearchModuleConfig) -> Self {
        Self {
            vector: cfg.vector,
            bm25: cfg.bm25,
            fusion: cfg.fusion,
            result: cfg.result,
            // `[search.rerank]` was removed: rerank runtime parameters come
            // exclusively from the top-level `[rerank]` section (merged per
            // request), so this slot keeps the defaults here.
            rerank: RerankConfig::default(),
            score: cfg.score,
            plugin: cfg.plugin,
            annotation: cfg.annotation,
            timeout_ms: cfg.timeout_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hybrid_weight_config_default() {
        let weights = HybridWeightConfig {
            vector_weight: 0.5,
            bm25_weight: 0.5,
        };
        let sum = weights.vector_weight + weights.bm25_weight;
        assert!((sum - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_query_intent_weights_default() {
        let weights = QueryIntentWeights::default();
        assert!((weights.semantic.vector_weight - 0.8).abs() < 0.001);
        assert!((weights.semantic.bm25_weight - 0.2).abs() < 0.001);
        assert!((weights.keyword.vector_weight - 0.2).abs() < 0.001);
        assert!((weights.keyword.bm25_weight - 0.8).abs() < 0.001);
        assert!((weights.entity.vector_weight - 0.7).abs() < 0.001);
        assert!((weights.entity.bm25_weight - 0.3).abs() < 0.001);
    }

    #[test]
    fn test_query_intent_weights_for_intent() {
        let weights = QueryIntentWeights::default();
        use crate::query::types::query_options::QueryIntent;
        use crate::query::types::search_config::QueryIntentWeightsExt;

        assert!((weights.for_intent(QueryIntent::Semantic).vector_weight - 0.8).abs() < 0.001);
        assert!((weights.for_intent(QueryIntent::Keyword).bm25_weight - 0.8).abs() < 0.001);
        assert!((weights.for_intent(QueryIntent::Hybrid).vector_weight - 0.5).abs() < 0.001);
        assert!((weights.for_intent(QueryIntent::Entity).vector_weight - 0.7).abs() < 0.001);
    }

    #[test]
    fn test_bm25_fusion_config_default() {
        let retrieval = Bm25RetrievalConfig::default();
        assert!((retrieval.min_score - 0.1).abs() < f32::EPSILON);
        let fusion = HybridFusionConfig::default();
        assert!(fusion.enable_intent_based_weights);
        assert!((fusion.vector_weight - 0.5).abs() < f32::EPSILON);
        assert!((fusion.bm25_weight - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_search_config_default() {
        let config = SearchConfig::default();
        assert_eq!(config.vector.top_k, 50);
        assert_eq!(config.vector.min_score, 0.3);
        assert_eq!(config.result.limit, 10);
        assert_eq!(config.result.min_score, 0.25);
    }

    #[test]
    fn test_search_config_with_overrides() {
        let mut config = SearchConfig::default();
        config.vector.top_k = 30;
        config.result.limit = 5;
    }

    #[test]
    fn test_search_config_from_search_module_config() {
        use cce_config::modules::search::SearchModuleConfig;
        let module_cfg = SearchModuleConfig::default();
        let config = SearchConfig::from(module_cfg);
        assert_eq!(config.vector.top_k, 50);
        assert_eq!(config.result.limit, 10);
        assert!(!config.rerank.enabled);
    }
}
