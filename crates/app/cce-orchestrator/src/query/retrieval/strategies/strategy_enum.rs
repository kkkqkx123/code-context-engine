//! Retrieval strategy definitions
//!
//! This module defines the recall algorithm selection and enum-based
//! retrieval strategies with static dispatch.

use crate::query::error::Result;
use crate::query::filter::QueryFilter;
use crate::query::types::{QueryOptions, SearchResult};

use super::bm25::Bm25Strategy;
use super::dense::DenseStrategy;
use super::relation::RelationStrategy;
use super::summary::SummaryStrategy;

/// Supported recall algorithms.
///
/// Each variant maps to a concrete retrieval strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecallAlgorithm {
    /// Pure dense vector semantic search
    Dense,
    /// Pure BM25 keyword-based search
    Bm25,
    /// Relation and call chain queries
    Relation,
    /// Summary-level vector search (file-level, not chunks)
    Summary,
}

impl RecallAlgorithm {
    /// Create a retrieval strategy for this algorithm
    pub fn create_strategy(self, searcher: &crate::query::Searcher) -> RetrievalStrategy {
        match self {
            Self::Dense => RetrievalStrategy::Dense(DenseStrategy::new(searcher)),
            Self::Bm25 => {
                // Create BM25 strategy with native project_id filtering in the index
                let bm25_strategy =
                    Bm25Strategy::new(crate::query::Searcher::extract_bm25_client(searcher));
                RetrievalStrategy::Bm25(bm25_strategy)
            }
            Self::Relation => RetrievalStrategy::Relation(RelationStrategy::new(searcher)),
            Self::Summary => RetrievalStrategy::Summary(SummaryStrategy::new(searcher)),
        }
    }
}

impl std::fmt::Display for RecallAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Dense => write!(f, "dense"),
            Self::Bm25 => write!(f, "bm25"),
            Self::Relation => write!(f, "relation"),
            Self::Summary => write!(f, "summary"),
        }
    }
}

/// Retrieval strategy enum (static dispatch)
pub enum RetrievalStrategy {
    Dense(DenseStrategy),
    Bm25(Bm25Strategy),
    Relation(RelationStrategy),
    Summary(SummaryStrategy),
}

impl RetrievalStrategy {
    /// Execute retrieval, returning raw results (before enrichment and post-processing)
    pub async fn retrieve(
        &self,
        options: &QueryOptions,
        query_filter: &QueryFilter,
    ) -> Result<Vec<SearchResult>> {
        match self {
            Self::Dense(s) => s.retrieve(options, query_filter).await,
            Self::Bm25(s) => s.retrieve(options, query_filter).await,
            Self::Relation(s) => s.retrieve(options, query_filter).await,
            Self::Summary(s) => s.retrieve(options, query_filter).await,
        }
    }

    /// Strategy name (for logging and monitoring)
    pub fn name(&self) -> &str {
        match self {
            Self::Dense(_) => "dense",
            Self::Bm25(_) => "bm25",
            Self::Relation(_) => "relation",
            Self::Summary(_) => "summary",
        }
    }
}
