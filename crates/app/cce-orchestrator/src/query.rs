//! Unified query module
//!
//! Provides a unified interface for all search operations.
//!
//! # Architecture
//!
//! The query module follows a layered architecture with clear separation of concerns:
//!
//! ```text
//! QueryCoordinator (unified entry point: capabilities, cache, retry queue)
//!     │
//!     ├── Searcher (core search engine, one pipeline per ExecutionStrategy)
//!     │   ├── Retrieval Strategies (retrieval/strategies: Dense, Bm25, Summary)
//!     │   │   └── core/ = stateless storage access (Qdrant), strategies = orchestration
//!     │   │
//!     │   ├── Hybrid Fusion (retrieval/post_processing/fusion)
//!     │   │   └── entity-level alignment, weighted normalized score fusion
//!     │   │
//!     │   ├── Glob Filter + Score Normalization + Enrichment
//!     │   │   └── path filtering, uniform score scale, SQLite chunk enrichment
//!     │   │
//!     │   ├── Boost Layer (boost: additive score boosting)
//!     │   │   └── SummaryBoost (dense + hybrid paths; skipped in SummaryRecall)
//!     │   │
//!     │   └── Post-processing (ranking: rerank → sort → ResultFilter → threshold)
//!     │       ├── LlmReranker / PluginReranker
//!     │       ├── ScoreSorter
//!     │       └── ThresholdFilter (min_score + limit)
//!     │
//!     └── RelationSearcher (standalone relation queries, not semantic-scored)
//!         ├── Call chain queries / path finding / inheritance
//!         └── GraphService (ego graph, shortest path, components, export)
//!
//! Dormant: assembly/ (SPSR-Graph) is disconnected from the online pipeline;
//! it is kept only for the offline assembly-review example in cce-e2e-tests.
//!
//! Tools Module (code analysis tools, outside the query pipeline)
//!     ├── SymbolLookup (symbol lookup: find references, goto definition)
//!     ├── AstDiagnosis (AST diagnosis)
//!     └── Compression (code compression)
//! ```
//!
//! # Key Components
//!
//! - **QueryCoordinator**: unified entry point coordinating all query operations, with caching, capability checks and retry-queue fault tolerance
//! - **Searcher**: core search engine executing the per-strategy pipeline (retrieval → fusion → filter → normalize → boost → enrich → post-process)
//! - **Retrieval Strategies**: pluggable recall strategies (Dense / Bm25 / Summary) with static dispatch
//! - **Boost / Ranking**: additive score boosting and deterministic rerank/sort/threshold stages
//!
//! # Usage Example
//!
//! ```ignore
//! use code_context_engine::orchestrator::query::{QueryCoordinator, Searcher};
//!
//! // Create a Searcher via its builder
//! let searcher = Searcher::builder(qdrant, embedder, bm25, scope)
//!     .with_sqlite(sqlite)
//!     .with_rerank(rerank_handler)
//!     .build();
//!
//! // Create a QueryCoordinator
//! let coordinator = QueryCoordinator::new(
//!     Arc::new(searcher),
//!     Arc::new(relation_searcher),
//!     project_id,
//! );
//!
//! // Run a search
//! let options = QueryConfigBuilder::default()
//!     .build()?;
//! let result = coordinator.search(&options).await?;
//! ```

// Core types (organized in types/ directory)
pub mod types;

// Error types
pub mod error;

// Query cache
pub mod cache;

// Index capabilities
pub mod capabilities;

// Unified searcher
pub mod searcher;

// Relation searcher
pub mod relation_searcher;

// Query coordinator (unified entry point)
pub mod coordinator;

// Fusion and Ranking module (deprecated, use boost + ranking instead)
// pub mod fusion_ranking;

// Boost module — additive score boosting from multiple sources
pub mod boost;

// Ranking module
pub mod ranking;

// SPSR-Graph assembly
pub mod assembly;

// Independent graph retrieval path
pub mod graph;

// Retrieval layer (separated from storage)
pub mod retrieval;

// Retry queue for fault tolerance
pub mod retry_queue;

// Query filter for version-aware filtering
pub mod filter;

// Embedding memoization wrapper shared by all searcher consumers
pub mod cached_embedder;

// Re-export main types
pub use types::{
    AggregatedQueryOptions, ExcludableContentType, ExecutionStrategy, QueryConfigBuilder,
    QueryOptions, QueryResult, SearchConfig, SearchResult, SearchSources, SubQuery,
};

// Re-export cache types
pub use cache::{CacheConfig, CacheKey, QueryCache};
pub use cached_embedder::CachedEmbedder;

// Re-export capabilities
pub use capabilities::IndexCapabilities;

// Re-export error types
pub use error::{QueryError, Result};

// Re-export searcher
pub use searcher::{Searcher, SearcherBuilder};

// Re-export relation searcher
pub use relation_searcher::{PathQueryOptions, RelationQueryOptions, RelationSearcher};

// Re-export graph path
pub use graph::{Confidence, GraphDirection, GraphEdge, GraphNode, GraphService, SubGraph};

// Re-export query coordinator
pub use coordinator::QueryCoordinator;

// Re-export boost module components
pub use boost::{BoostAggregationConfig, BoostContribution, SummaryBoost, apply_boosts};
pub use boost::{NormalizationStrategy, normalize_scores};

// Re-export ranking module components (includes LlmReranker)
pub use ranking::{LlmReranker, ScoreSorter, ThresholdFilter};
pub use retrieval::{FilterOptions, GlobFilter};

// Re-export query filter
pub use filter::QueryFilter;
