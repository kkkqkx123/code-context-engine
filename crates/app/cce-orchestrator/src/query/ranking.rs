//! Ranking module for search results
//!
//! Handles deterministic result ranking and filtering.
//! This module provides pure algorithms without external dependencies.
//!
//! # Architecture
//!
//! The ranking module backs the post-processing stage of the searcher:
//!
//! ```text
//! Ranking Layer (ordering)
//!     │
//!     ├── LlmReranker / PluginReranker
//!     │   └── Reorder candidates via LLM or `Rerank`-capability plugins
//!     │
//!     ├── ScoreSorter (score sorter)
//!     │   └── Sorts by score (descending) with stable ordering
//!     │
//!     └── ThresholdFilter (threshold filter)
//!         └── Filters by minimum score threshold and applies the result limit
//! ```

pub mod common;
pub mod llm_reranker;
pub mod plugin_reranker;
pub mod score_sorter;
pub mod threshold_filter;

pub use llm_reranker::LlmReranker;
pub use plugin_reranker::PluginReranker;
pub use score_sorter::ScoreSorter;
pub use threshold_filter::ThresholdFilter;
