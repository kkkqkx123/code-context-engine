//! Ranking module for search results
//!
//! Handles deterministic result ranking, diversity control, and filtering.
//! This module provides pure algorithms without external dependencies.
//!
//! # Architecture
//!
//! The ranking module applies final transformations to search results:
//!
//! ```text
//! Ranking Layer (ordering)
//!     │
//!     ├── ScoreSorter (score sorter)
//!     │   └── Sorts by score (descending) with stable ordering
//!     │
//!     ├── DiversityControl (diversity control)
//!     │   └── Limits results per file to ensure diversity
//!     │
//!     ├── CandidateSelection (candidate selector)
//!     │   └── Selects top-N candidates based on limits
//!     │
//!     └── ThresholdFilter (threshold filter)
//!         └── Filters by minimum score threshold
//! ```
//!
//! # Usage Pattern
//!
//! Ranking operations are typically applied directly in the coordinator:
//!
//! ```ignore
//! // Sort by score
//! results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
//!
//! // Apply diversity control
//! results = diversity_control.apply(results, max_per_file);
//!
//! // Select top-N
//! results = candidate_selection.select(results, top_k);
//!
//! // Filter by threshold
//! results = threshold_filter.apply(results, min_score);
//! ```

pub mod candidate_selection;
pub mod common;
pub mod diversity_control;
pub mod llm_reranker;
pub mod plugin_reranker;
pub mod score_sorter;
pub mod threshold_filter;

pub use candidate_selection::CandidateSelection;
pub use diversity_control::DiversityControl;
pub use llm_reranker::LlmReranker;
pub use plugin_reranker::PluginReranker;
pub use score_sorter::ScoreSorter;
pub use threshold_filter::ThresholdFilter;
