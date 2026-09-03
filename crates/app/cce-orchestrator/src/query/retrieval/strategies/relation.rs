//! Relation retrieval strategy
//!
//! Orchestrates relation search by providing access to the core RelationRetrieval
//! implementation within the strategy pattern framework.
//!
//! This strategy is suitable for:
//! - Call chain and dependency queries
//! - Function caller/callee traversal
//! - Relation graph exploration

use std::sync::Arc;

use crate::query::error::{QueryError, Result};
use crate::query::filter::QueryFilter;
use crate::query::retrieval::core::relation::RelationRetrieval;
use crate::query::types::{QueryOptions, SearchResult};

/// Relation retrieval strategy — call chain and dependency queries
///
/// Provides access to relation search capabilities within the strategy pattern.
/// While relation queries don't map directly to SearchResult format
/// (they operate on entity relationships rather than content search),
/// this strategy layer maintains consistency with other retrieval strategies.
#[derive(Clone)]
pub struct RelationStrategy {
    relation_retrieval: RelationRetrieval,
    searcher: Arc<crate::query::Searcher>,
}

impl RelationStrategy {
    pub fn new(searcher: &crate::query::Searcher) -> Self {
        Self {
            relation_retrieval: RelationRetrieval::new(),
            searcher: Arc::new(searcher.clone()),
        }
    }

    /// Get the underlying relation retrieval for direct access to relation queries
    pub fn relation_retrieval(&self) -> &RelationRetrieval {
        &self.relation_retrieval
    }

    /// Get access to searcher for relation-aware processing
    pub fn searcher(&self) -> &crate::query::Searcher {
        &self.searcher
    }

    /// Execute relation retrieval (placeholder for trait compatibility)
    ///
    /// Note: Relation queries are not directly compatible with SearchResult format.
    /// Use relation_retrieval() for direct access to relation search methods.
    pub async fn retrieve(
        &self,
        _options: &QueryOptions,
        _query_filter: &QueryFilter,
    ) -> Result<Vec<SearchResult>> {
        // Relation searches are handled separately through relation_searcher
        // This method exists for trait compatibility but is not typically used
        // in the standard retrieval pipeline
        Err(QueryError::Config(
            "Relation strategy does not support generic retrieve interface".to_string(),
        ))
    }
}
