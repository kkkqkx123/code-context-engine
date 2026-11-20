//! Retrieval strategy definitions
//!
//! This module defines the recall algorithm selection and enum-based
//! retrieval strategies with static dispatch.

use crate::query::error::Result;
use crate::query::filter::QueryFilter;
use crate::query::types::{QueryOptions, SearchResult};

use super::bm25::Bm25Strategy;
use super::dense::DenseStrategy;

/// Raw retrieval results with dense-path timing.
pub struct RetrievalOutcome {
    pub results: Result<Vec<SearchResult>>,
    pub embedding_ms: u64,
    pub embedding_provider_ms: u64,
    pub embedding_first_ms: u64,
    pub embedding_overhead_ms: u64,
    pub embedding_cache_hits: u64,
    pub vector_search_ms: u64,
}

/// Supported recall algorithms.
///
/// Each variant maps to a concrete retrieval strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecallAlgorithm {
    /// Pure dense vector semantic search
    Dense,
    /// Pure BM25 keyword-based search
    Bm25,
}

impl RecallAlgorithm {
    /// Create a retrieval strategy for this algorithm
    pub fn create_strategy(self, searcher: &crate::query::Searcher) -> RetrievalStrategy {
        match self {
            Self::Dense => RetrievalStrategy::Dense(DenseStrategy::new(searcher)),
            Self::Bm25 => {
                // Create BM25 strategy with native project_id filtering in the index
                let bm25_strategy =
                    Bm25Strategy::new(crate::query::Searcher::extract_fulltext_store(searcher));
                RetrievalStrategy::Bm25(bm25_strategy)
            }
        }
    }
}

impl std::fmt::Display for RecallAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Dense => write!(f, "dense"),
            Self::Bm25 => write!(f, "bm25"),
        }
    }
}

/// Retrieval strategy enum (static dispatch)
pub enum RetrievalStrategy {
    Dense(DenseStrategy),
    Bm25(Bm25Strategy),
}

impl RetrievalStrategy {
    /// Execute retrieval, returning raw results and dense-path timing.
    pub async fn retrieve(
        &self,
        options: &QueryOptions,
        query_filter: &QueryFilter,
    ) -> RetrievalOutcome {
        match self {
            Self::Dense(s) => {
                let outcome = s.retrieve(options, query_filter).await;
                RetrievalOutcome {
                    results: outcome.results,
                    embedding_ms: outcome.embedding_ms,
                    embedding_provider_ms: outcome.embedding_provider_ms,
                    embedding_first_ms: outcome.embedding_first_ms,
                    embedding_overhead_ms: outcome.embedding_overhead_ms,
                    embedding_cache_hits: outcome.embedding_cache_hits,
                    vector_search_ms: outcome.vector_search_ms,
                }
            }
            Self::Bm25(s) => RetrievalOutcome {
                results: s.retrieve(options, query_filter).await,
                embedding_ms: 0,
                embedding_provider_ms: 0,
                embedding_first_ms: 0,
                embedding_overhead_ms: 0,
                embedding_cache_hits: 0,
                vector_search_ms: 0,
            },
        }
    }

    /// Strategy name (for logging and monitoring)
    pub fn name(&self) -> &str {
        match self {
            Self::Dense(_) => "dense",
            Self::Bm25(_) => "bm25",
        }
    }
}
