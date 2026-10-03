//! Query types module
//!
//! This module provides type definitions for query operations, organized by functionality.

// Core query option types
pub mod execution_strategy;
pub mod intent_detector;
pub mod query_options;
pub mod search_config;

// Result types
pub mod content_reference;
pub mod query_result;
pub mod search_result;

pub use content_reference::{
    ContentState, DowngradeReason, file_level_reference, reference_content,
};
pub use execution_strategy::ExecutionStrategy;
pub use query_options::{
    ExcludableContentType, QueryConfigBuilder, QueryIntent, QueryOptions, SearchSources,
};
pub use query_result::{AggregatedQueryOptions, QueryResult, SubQuery};
pub use search_config::{
    Bm25RetrievalConfig, HybridFusionConfig, HybridWeightConfig, QueryIntentWeights, RerankConfig,
    ResultFilterConfig, ScoreNormalizationConfig, SearchConfig, SummaryBoostConfig,
    VectorRetrievalConfig,
};
pub use search_result::{BoostStats, SearchResult};
