//! Execution strategy types

use super::query_options::{QueryIntent, QueryOptions, SearchSources};
use crate::query::assembly::ExpansionStrategy;
use crate::query::types::search_config::QueryIntentWeightsExt;

/// Internal execution strategy (not exposed to users)
/// Determined automatically from SearchSources
#[derive(Debug, Clone, PartialEq)]
pub enum ExecutionStrategy {
    /// Pure BM25 keyword recall without vector dependency
    Bm25Recall,
    /// Hybrid recall: vector + BM25 parallel paths, fused by weighted normalization
    HybridRecall {
        /// Weight for normalized vector scores
        vector_weight: f32,
        /// Weight for normalized BM25 scores
        bm25_weight: f32,
    },
    /// Pure dense vector recall
    DenseRecall,
    /// Summary-only vector recall (file-level, search summary vectors only)
    SummaryRecall,
    /// Search with relation expansion
    WithRelationExpansion {
        /// Base strategy for the search
        base: Box<ExecutionStrategy>,
        /// Depth for relation traversal
        depth: usize,
    },
    /// Search with SPSR-Graph assembly
    WithAssembly {
        /// Base strategy for the search
        base: Box<ExecutionStrategy>,
        /// Assembly configuration
        depth: usize,
        strategy: ExpansionStrategy,
    },
}

impl ExecutionStrategy {
    /// Determine execution strategy from user options
    pub fn from_sources(
        sources: &SearchSources,
        config: &super::search_config::SearchConfig,
    ) -> Self {
        match (
            sources.vector,
            sources.bm25,
            sources.relation,
            sources.summary,
        ) {
            // BM25 alone -> pure BM25 keyword recall (no vector)
            (false, true, false, _) => ExecutionStrategy::Bm25Recall,

            // Pure vector -> DenseRecall
            (true, false, false, _) => ExecutionStrategy::DenseRecall,

            // Vector + BM25 -> hybrid recall: two-path parallel + weighted fusion
            (true, true, false, _) => ExecutionStrategy::HybridRecall {
                vector_weight: config.bm25.vector_weight,
                bm25_weight: config.bm25.bm25_weight,
            },

            // With relation expansion
            (v, b, true, _) => ExecutionStrategy::WithRelationExpansion {
                base: Box::new(Self::from_sources(
                    &SearchSources {
                        vector: v || b,
                        bm25: false,
                        relation: false,
                        summary: false,
                    },
                    config,
                )),
                depth: config.relation.depth,
            },

            // Summary only -> SummaryRecall (pure summary vector search)
            (false, false, false, true) => ExecutionStrategy::SummaryRecall,

            // Default fallback -> dense recall
            _ => ExecutionStrategy::DenseRecall,
        }
    }

    /// Determine execution strategy with assembly support and query intent resolution.
    ///
    /// When `config.bm25.enable_intent_based_weights` is enabled, this method
    /// resolves the effective `QueryIntent` (via explicit override, defaults to `Hybrid`)
    /// and selects the corresponding weight profile from `config.bm25.intent_weights`.
    pub fn from_options(options: &QueryOptions) -> Self {
        // Resolve fusion weights based on query intent if enabled
        let (resolved_v_weight, resolved_b_weight) =
            if options.config.bm25.enable_intent_based_weights {
                let intent = options.query_intent.unwrap_or(QueryIntent::Hybrid);
                let w = options.config.bm25.intent_weights.for_intent(intent);
                (w.vector_weight, w.bm25_weight)
            } else {
                (
                    options.config.bm25.vector_weight,
                    options.config.bm25.bm25_weight,
                )
            };

        // Create adjusted config with resolved weights for strategy selection
        let mut adjusted_config = options.config.clone();
        adjusted_config.bm25.vector_weight = resolved_v_weight;
        adjusted_config.bm25.bm25_weight = resolved_b_weight;

        let base_strategy = Self::from_sources(&options.sources, &adjusted_config);

        // Check if assembly is enabled
        if options.config.spsr_graph.enable_assembly {
            ExecutionStrategy::WithAssembly {
                base: Box::new(base_strategy),
                depth: options.config.spsr_graph.max_expansion_depth,
                strategy: options.config.spsr_graph.expansion_strategy,
            }
        } else {
            base_strategy
        }
    }

    /// Return a concise label for metrics tracking (not for display).
    ///
    /// Unlike `Display`, this method produces a stable, short identifier
    /// suitable for use as a metric label value.
    pub fn query_type_label(&self) -> &'static str {
        match self {
            ExecutionStrategy::Bm25Recall => "bm25_recall",
            ExecutionStrategy::HybridRecall { .. } => "hybrid_recall",
            ExecutionStrategy::DenseRecall => "dense_recall",
            ExecutionStrategy::SummaryRecall => "summary_recall",
            ExecutionStrategy::WithRelationExpansion { .. } => "with_relation_expansion",
            ExecutionStrategy::WithAssembly { .. } => "with_assembly",
        }
    }
}

impl std::fmt::Display for ExecutionStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecutionStrategy::Bm25Recall => write!(f, "bm25_recall"),
            ExecutionStrategy::HybridRecall {
                vector_weight,
                bm25_weight,
            } => {
                write!(f, "hybrid_recall(v={}, b={})", vector_weight, bm25_weight)
            }
            ExecutionStrategy::DenseRecall => write!(f, "dense_recall"),
            ExecutionStrategy::SummaryRecall => write!(f, "summary_recall"),
            ExecutionStrategy::WithRelationExpansion { base, depth } => {
                write!(f, "with_relation(depth={}, base={})", depth, base)
            }
            ExecutionStrategy::WithAssembly {
                base,
                depth,
                strategy,
            } => {
                write!(
                    f,
                    "with_assembly(depth={}, strategy={}, base={})",
                    depth, strategy, base
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::query_options::QueryIntent;
    use super::*;

    /// Helper to create a minimal SearchConfig for testing
    fn test_config() -> super::super::search_config::SearchConfig {
        super::super::search_config::SearchConfig::default()
    }

    #[test]
    fn test_execution_strategy_from_sources() {
        let config = test_config();

        // Pure vector -> DenseRecall
        let sources = SearchSources::none().with_vector();
        let strategy = ExecutionStrategy::from_sources(&sources, &config);
        assert_eq!(strategy, ExecutionStrategy::DenseRecall);

        // Pure BM25 -> now maps to Bm25Recall
        let sources = SearchSources::none().with_bm25();
        let strategy = ExecutionStrategy::from_sources(&sources, &config);
        assert_eq!(strategy, ExecutionStrategy::Bm25Recall);

        // Vector + BM25 -> HybridRecall
        let sources = SearchSources::default();
        let strategy = ExecutionStrategy::from_sources(&sources, &config);
        assert_eq!(
            strategy,
            ExecutionStrategy::HybridRecall {
                vector_weight: 0.5,
                bm25_weight: 0.5,
            }
        );

        // Summary only -> SummaryRecall
        let sources = SearchSources::none().with_summary();
        let strategy = ExecutionStrategy::from_sources(&sources, &config);
        assert_eq!(strategy, ExecutionStrategy::SummaryRecall);

        // With relation
        let sources = SearchSources::none().with_vector().with_relation();
        let strategy = ExecutionStrategy::from_sources(&sources, &config);
        matches!(strategy, ExecutionStrategy::WithRelationExpansion { .. });
    }

    #[test]
    fn test_query_options_execution_strategy() {
        use super::super::query_options::QueryConfigBuilder;

        let options = QueryConfigBuilder::new(1)
            .build("test query")
            .with_sources(SearchSources::none().with_bm25());
        let strategy = options.execution_strategy();
        assert_eq!(strategy, ExecutionStrategy::Bm25Recall);
    }

    #[test]
    fn test_execution_strategy_with_assembly() {
        use super::super::query_options::QueryConfigBuilder;

        let options = QueryConfigBuilder::new(1)
            .with_assembly(3)
            .assembly_strategy(ExpansionStrategy::Bidirectional)
            .build("test");

        let strategy = options.execution_strategy();
        matches!(strategy, ExecutionStrategy::WithAssembly { depth, strategy: ExpansionStrategy::Bidirectional, .. } if depth == 3);
    }

    // ========================================================================
    // Query Intent based weight resolution tests
    // ========================================================================

    #[test]
    fn test_from_options_semantic_intent_uses_vector_weight() {
        use super::super::query_options::QueryConfigBuilder;

        // Semantic query: "how does the authentication flow work"
        let options = QueryConfigBuilder::new(1)
            .with_query_intent(Some(QueryIntent::Semantic))
            .build("how does the authentication flow work");

        let strategy = options.execution_strategy();
        match strategy {
            ExecutionStrategy::HybridRecall {
                vector_weight,
                bm25_weight,
            } => {
                assert!((vector_weight - 0.8).abs() < 0.001);
                assert!((bm25_weight - 0.2).abs() < 0.001);
            }
            other => panic!("Expected HybridRecall, got {:?}", other),
        }
    }

    #[test]
    fn test_from_options_keyword_intent_uses_bm25_weight() {
        use super::super::query_options::QueryConfigBuilder;

        // Keyword query: "fn parse_query terms"
        let options = QueryConfigBuilder::new(1)
            .with_query_intent(Some(QueryIntent::Keyword))
            .build("fn parse_query terms");

        let strategy = options.execution_strategy();
        match strategy {
            ExecutionStrategy::HybridRecall {
                vector_weight,
                bm25_weight,
            } => {
                assert!((vector_weight - 0.2).abs() < 0.001);
                assert!((bm25_weight - 0.8).abs() < 0.001);
            }
            other => panic!("Expected HybridRecall, got {:?}", other),
        }
    }

    #[test]
    fn test_from_options_with_intent_disabled_uses_static_weights() {
        use super::super::query_options::QueryConfigBuilder;
        use super::super::search_config::SearchConfig;

        // Disable intent-based weights, should use static 0.5/0.5
        let mut config = SearchConfig::default();
        config.bm25.enable_intent_based_weights = false;
        config.bm25.vector_weight = 0.6;
        config.bm25.bm25_weight = 0.4;

        let options = QueryConfigBuilder::new(1)
            .build("semantic query that should be ignored")
            .with_config(config)
            .with_query_intent(Some(QueryIntent::Semantic));

        let strategy = options.execution_strategy();
        match strategy {
            ExecutionStrategy::HybridRecall {
                vector_weight,
                bm25_weight,
            } => {
                assert!((vector_weight - 0.6).abs() < 0.001);
                assert!((bm25_weight - 0.4).abs() < 0.001);
            }
            other => panic!("Expected HybridRecall, got {:?}", other),
        }
    }

    #[test]
    fn test_from_options_entity_intent_resolves_correctly() {
        use super::super::query_options::QueryConfigBuilder;

        // Entity query should use vector-leaning weights
        let options = QueryConfigBuilder::new(1)
            .with_query_intent(Some(QueryIntent::Entity))
            .build("ConfigBuilder");

        let strategy = options.execution_strategy();
        match strategy {
            ExecutionStrategy::HybridRecall {
                vector_weight,
                bm25_weight,
            } => {
                assert!((vector_weight - 0.7).abs() < 0.001);
                assert!((bm25_weight - 0.3).abs() < 0.001);
            }
            other => panic!("Expected HybridRecall, got {:?}", other),
        }
    }
}
