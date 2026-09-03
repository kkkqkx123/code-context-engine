//! Retrieval module for search operations
//!
//! This module provides retrieval implementations organized in layers:
//!
//! # Architecture
//!
//! ```text
//! core/              # Low-level, stateless implementations (direct storage access)
//!     ├── dense.rs      # DenseRetrieval - core vector search (Qdrant)
//!     ├── relation.rs   # RelationRetrieval - core relation queries
//!     └── vector.rs     # FilterOptions and vector types
//!
//! strategies/        # High-level strategy interface (orchestration layer)
//!     ├── bm25.rs       # Bm25Strategy - orchestrates storage Bm25Retrieval + client
//!     ├── dense.rs      # DenseStrategy - orchestrates core + embedder
//!     ├── relation.rs   # RelationStrategy - orchestrates core
//!     └── strategy_enum.rs  # RecallAlgorithm enum and factory
//!
//! post_processing/   # Result processing and enhancement
//!     ├── fusion.rs         # Hybrid recall fusion
//!     ├── glob_filter.rs    # Path-based filtering
//!     └── entity_mapper.rs  # Entity enrichment from SQLite
//! ```
//!
//! # Design Principles
//!
//! - **Symmetric Layering**: Storage read paths live in the storage layer
//!   (`Bm25Retrieval`, `QdrantRetrieval`); the orchestrator only composes
//!   strategies, translates filters and post-processes results
//! - **Pure Recall Paths**: Strategies return raw results without enrichment or fusion.
//!   Post-retrieval processing (SQLite enrichment, BM25 fusion, reranking) happens
//!   in the searcher pipeline.
//! - **Separation of Concerns**: Core layer is stateless and storage-focused;
//!   strategy layer adds orchestration; post-processing is independent.
//! - **Strategy Pattern**: Different recall algorithms can be plugged in via RecallAlgorithm enum
//! - **Composability**: Retrieval results can be enhanced by post-processing modules
//! - **Fusion over Boost**: Hybrid search (vector + BM25) uses weighted normalized
//!   fusion rather than consensus boosting, preserving score magnitude information.

pub mod core;
pub mod post_processing;
pub mod strategies;

// Re-export commonly used types from core for backward compatibility
pub use core::{DenseRetrieval, FilterOptions, RelationOptions, RelationRetrieval};

pub use strategies::{RecallAlgorithm, RetrievalStrategy};

pub use post_processing::{GlobFilter, HybridFusionConfig, fuse_hybrid_results, minmax_normalize};
