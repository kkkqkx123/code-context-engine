//! Relation info enricher for assembly stage
//!
//! Provides optional enrichment of search results with call chain metadata.
//! This is executed during the assembly stage and does not affect ranking.
//! Unlike RelationScoreBoost which modifies scores, this enricher only
//! attaches relation information (callers/callees) to results.

use std::sync::Arc;

use crate::query::error::Result;
use crate::query::relation_searcher::RelationSearcher;
use crate::query::types::{CallInfo, Relations, SearchConfig, SearchResult};
use cce_types::EntityId;

/// Relation info enricher
///
/// Enriches search results with call chain metadata (callers/callees).
/// This is an optional step that runs during the assembly stage.
/// Reuses RelationSearcher for consistent behavior and reduced code duplication.
///
/// The enricher carries a staleness gate. When the relation segment
/// is stale (update in progress or failed), `enhance` returns the results
/// untouched, i.e. search falls back to pure vector/BM25 results.
pub struct RelationInfoEnricher {
    relation_searcher: Arc<RelationSearcher>,
    /// When false, enrichment is skipped and results pass through untouched.
    enabled: bool,
}

impl RelationInfoEnricher {
    /// Create a new relation info enricher from a RelationSearcher
    pub fn new(relation_searcher: Arc<RelationSearcher>) -> Self {
        Self {
            relation_searcher,
            enabled: true,
        }
    }

    /// Disable enrichment (stale relation segment): results pass through
    /// untouched, equivalent to a pure vector/BM25 fallback.
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Whether enrichment is currently active.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Enhance search results with relation information
    ///
    /// Only enhances the top N results (configured by relation_top_n).
    /// Each enhancement query has a timeout (configured by relation_timeout_ms).
    pub async fn enhance(
        &self,
        results: Vec<SearchResult>,
        config: &SearchConfig,
    ) -> Result<Vec<SearchResult>> {
        if !self.enabled {
            return Ok(results);
        }
        let mut results = results;
        let top_n = config.relation.top_n.min(results.len());

        // Process top N results concurrently
        let futures: Vec<_> = results[..top_n]
            .iter_mut()
            .map(|result| self.enhance_single(result, config))
            .collect();

        // Wait for all enhancements to complete
        for future in futures {
            let _ = future.await; // Ignore individual errors
        }

        Ok(results)
    }

    /// Enhance a single search result
    async fn enhance_single(&self, result: &mut SearchResult, config: &SearchConfig) -> Result<()> {
        // Get entity ID from result
        let entity_id = match result.entity_ids.first().copied() {
            Some(id) => id,
            None => return Ok(()), // Skip if no entity ID
        };

        // Query with timeout
        let timeout = std::time::Duration::from_millis(config.relation.timeout_ms);
        let depth = config.relation.depth;

        let (callers, callees) = tokio::time::timeout(timeout, async {
            let callers = self.query_callers(entity_id, depth);
            let callees = self.query_callees(entity_id, depth);
            (callers, callees)
        })
        .await
        .unwrap_or_else(|_| (Vec::new(), Vec::new()))
            as (Vec<CallInfo>, Vec<CallInfo>);

        // Attach relations to result
        result.relations = Some(Relations { callers, callees });

        Ok(())
    }

    /// Query callers (functions that call this entity)
    fn query_callers(&self, entity_id: EntityId, depth: usize) -> Vec<CallInfo> {
        use crate::query::relation_searcher::RelationQueryOptions;

        let options = RelationQueryOptions::new().with_max_depth(depth);

        self.relation_searcher
            .query_backward(entity_id, &options)
            .map(|nodes| {
                nodes
                    .into_iter()
                    .map(|node| CallInfo {
                        id: node.function_id,
                        name: node.function_name,
                        file: node.file_path,
                        line: node.call_line.map(|l| l as u32),
                    })
                    .take(5) // Limit to top 5 callers
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Query callees (functions called by this entity)
    fn query_callees(&self, entity_id: EntityId, depth: usize) -> Vec<CallInfo> {
        use crate::query::relation_searcher::RelationQueryOptions;

        let options = RelationQueryOptions::new().with_max_depth(depth);

        self.relation_searcher
            .query_forward(entity_id, &options)
            .map(|nodes| {
                nodes
                    .into_iter()
                    .map(|node| CallInfo {
                        id: node.function_id,
                        name: node.function_name,
                        file: node.file_path,
                        line: node.call_line.map(|l| l as u32),
                    })
                    .take(5) // Limit to top 5 callees
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::types::SearchConfig;

    #[tokio::test]
    async fn stale_enricher_falls_back_to_pure_results() {
        let searcher = Arc::new(RelationSearcher::new(Arc::new(
            cce_relation::CallChainQuery::new(),
        )));
        let result = SearchResult {
            id: "chunk-1".to_string(),
            entity_ids: vec![EntityId(7)],
            ..Default::default()
        };
        let results = vec![result.clone()];
        let config = SearchConfig::default();

        // Enabled: enrichment attaches relation metadata (empty graph here, but
        // the call still produces a Relations payload).
        let enabled = RelationInfoEnricher::new(searcher.clone());
        let enriched = enabled
            .enhance(results.clone(), &config)
            .await
            .expect("enrichment should not fail");
        assert!(
            enriched[0].relations.is_some(),
            "enabled enricher must attach relation metadata"
        );

        // Disabled (stale relation segment): results pass through untouched,
        // i.e. the search falls back to pure vector/BM25 results.
        let stale = RelationInfoEnricher::new(searcher).with_enabled(false);
        assert!(!stale.is_enabled());
        let passed = stale
            .enhance(results.clone(), &config)
            .await
            .expect("stale enricher should not fail");
        assert_eq!(passed.len(), 1);
        assert!(passed[0].relations.is_none());
        assert_eq!(passed[0].id, result.id);
    }
}
