//! Unified query module
//!
//! Provides a unified interface for all search operations.
//!
//! # Architecture
//!
//! The query module follows a layered architecture with clear separation of concerns:
//!
//! ```text
//! QueryCoordinator (unified entry point)
//!     │
//!     ├── Searcher (core search engine)
//!     │   ├── Retrieval Layer (base retrieval)
//!     │   │   ├── VectorRetrieval (vector search)
//!     │   │   ├── Bm25Retrieval (BM25 search)
//!     │   │   └── Strategies (retrieval strategies: Dense, DenseSparse)
//!     │   │
//!     │   ├── Boost Layer (unified boosting)
//!     │   │   ├── SummaryBoost (summary relevance boost)
//!     │   │   └── RelationBoost (relation graph boost)
//!     │   │
//!     │   ├── Ranking Layer (ordering)
//!     │   │   ├── LlmReranker (LLM re-ranking)
//!     │   │   ├── ScoreSorter (score ordering)
//!     │   │   ├── DiversityControl (diversity control)
//!     │   │   ├── CandidateSelection (candidate selection)
//!     │   │   └── ThresholdFilter (threshold filtering)
//!     │   │
//!     │   └── Post-Processing Layer (post-processing)
//!     │       ├── ScoreSorter (score ordering)
//!     │       ├── DiversityControl (diversity control)
//!     │       ├── CandidateSelection (candidate selection)
//!     │       └── ThresholdFilter (threshold filtering)
//!     │
//!     ├── AssemblyHandler (SPSR-Graph assembly)
//!     │   └── SPSRGraphAssembler
//!     │       ├── SemanticUnitExtractor (semantic unit extraction)
//!     │       ├── RelationSearcher (relation expansion)
//!     │       ├── SegmentAggregator (segment aggregation)
//!     │       └── StructureConcatenator (structure concatenation)
//!     │
//!     └── RelationSearcher (standalone relation queries)
//!         ├── Call chain queries
//!         ├── Path finding
//!         └── Inheritance queries
//!
//! Tools Module (code analysis tools)
//!     ├── SymbolLookup (symbol lookup: find references, goto definition)
//!     ├── AstDiagnosis (AST diagnosis)
//!     └── Compression (code compression)
//! ```
//!
//! # Key Components
//!
//! - **QueryCoordinator**: unified entry point coordinating all query operations, with caching and capability checks
//! - **Searcher**: core search engine orchestrating retrieval, enhancement and post-processing through a pipeline
//! - **SearchPipeline**: search pipeline executing retrieval, enhancement and post-processing in order
//! - **Retrieval Strategies**: pluggable retrieval strategies supporting multiple search modes
//! - **Enhancement**: optional result enhancers that can be enabled/disabled independently
//! - **Assembly**: assembles search results into an SPSR-Graph structure, preserving code structure and semantics
//!
//! # Usage Example
//!
//! ```ignore
//! use code_context_engine::orchestrator::query::{QueryCoordinator, Searcher};
//!
//! // Create a Searcher via its builder
//! let searcher = Searcher::builder(qdrant, embedder, bm25, project_group_id)
//!     .with_sqlite(sqlite)
//!     .with_assembler(assembler)
//!     .with_rerank(rerank_handler)
//!     .build();
//!
//! // Create a QueryCoordinator
//! let coordinator = QueryCoordinator::new(
//!     Arc::new(searcher),
//!     Arc::new(relation_searcher)
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

// Retrieval layer (separated from storage)
pub mod retrieval;

// Relation enrichment bridge
pub mod relation_bridge;

// Retry queue for fault tolerance
pub mod retry_queue;

// Query filter for version-aware filtering
pub mod filter;

// Embedding memoization wrapper shared by all searcher consumers
pub mod cached_embedder;

// Re-export main types
pub use types::{
    AggregatedQueryOptions, CallInfo, ExcludableContentType, ExecutionStrategy, QueryConfigBuilder,
    QueryOptions, QueryResult, Relations, SearchConfig, SearchResult, SearchSources, SubQuery,
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

// Re-export query coordinator
pub use coordinator::QueryCoordinator;

// Re-export boost module components
pub use boost::{
    BoostAggregationConfig, BoostContribution, RelationBoost, SummaryBoost, apply_boosts,
};
pub use boost::{NormalizationStrategy, normalize_scores};

// Re-export ranking module components (includes LlmReranker)
pub use ranking::{
    CandidateSelection, DiversityControl, LlmReranker, ScoreSorter, ThresholdFilter,
};
pub use retrieval::{FilterOptions, GlobFilter};

// Re-export query filter
pub use filter::QueryFilter;
