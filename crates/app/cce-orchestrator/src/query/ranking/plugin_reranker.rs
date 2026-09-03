//! Plugin-based reranker for search results
//!
//! Applies `Rerank`-capability plugins (Lua / native) to search results.
//! Plugins execute synchronously on a blocking thread (`spawn_blocking`);
//! the plugin adapters impose their own hard timeout. On failure the
//! original result order is kept (same semantics as the LLM reranker).
//! Whether reranking runs at all is gated by `QueryOptions::enable_rerank`
//! in the searcher, mirroring the LLM reranker.

use cce_plugin::CodePlugin;
use cce_types::RerankCandidate;

use super::common::{build_candidates, merge_rerank_results, select_candidate_count};
use crate::query::error::Result;
use crate::query::types::{SearchConfig, SearchResult};

/// Reranker backed by `Rerank`-capability plugins.
#[derive(Clone)]
pub struct PluginReranker {
    plugins: Vec<std::sync::Arc<dyn CodePlugin>>,
}

impl PluginReranker {
    /// Create a reranker from matched plugins.
    pub fn new(plugins: Vec<std::sync::Arc<dyn CodePlugin>>) -> Self {
        Self { plugins }
    }

    /// Whether any plugins are available.
    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }

    /// Apply plugin reranking to search results.
    ///
    /// The first plugin returning a non-empty `RerankResult` wins.
    /// Mirrors [`super::LlmReranker`]'s candidate selection and merging.
    pub async fn rerank(
        &self,
        results: Vec<SearchResult>,
        query: &str,
        config: &SearchConfig,
    ) -> Result<Vec<SearchResult>> {
        if self.plugins.is_empty() {
            return Ok(results);
        }
        if results.is_empty() {
            return Ok(results);
        }

        let candidate_count = select_candidate_count(
            &results,
            config.rerank.max_candidates,
            config.rerank.min_candidates,
            config.rerank.score_drop_threshold,
            config.rerank.min_score,
            config.rerank.drop_detection_start,
        );
        if candidate_count == 0 {
            tracing::trace!("No candidates meet the minimum score threshold for plugin reranking");
            return Ok(results);
        }

        let candidates: Vec<RerankCandidate> = build_candidates(&results, candidate_count);

        let plugins = self.plugins.clone();
        let query_owned = query.to_string();
        let candidates_for_blocking = candidates.clone();

        // Plugin execution is synchronous blocking; run it off the async
        // runtime. The Lua/native adapters impose a hard per-call timeout.
        let rerank_result = tokio::task::spawn_blocking(move || {
            for plugin in plugins {
                match plugin.rerank(&query_owned, candidates_for_blocking.clone()) {
                    Ok(Some(rr)) if !rr.reranked_candidates.is_empty() => return Some(rr),
                    Ok(_) => {}
                    Err(e) => {
                        tracing::warn!(
                            plugin = %plugin.metadata().id,
                            error = %e,
                            "Rerank plugin failed; trying next plugin"
                        );
                    }
                }
            }
            None
        })
        .await
        .unwrap_or(None);

        match rerank_result {
            Some(rr) => {
                let merged = merge_rerank_results(results, rr);
                tracing::trace!("Plugin reranking completed: {} candidates", merged.len());
                Ok(merged)
            }
            None => {
                tracing::warn!("Plugin reranking produced no result; keeping original order");
                Ok(results)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_plugin::{PluginError, PluginMetadata};
    use cce_types::RerankedCandidate;

    type RerankFn = fn(
        &str,
        Vec<cce_types::RerankCandidate>,
    ) -> std::result::Result<Option<cce_types::RerankResult>, PluginError>;

    /// Configurable `CodePlugin` test double for the `Rerank` capability.
    struct RerankMockPlugin {
        meta: PluginMetadata,
        rerank: RerankFn,
    }

    impl RerankMockPlugin {
        fn new(id: &str, rerank: RerankFn) -> Self {
            Self {
                meta: PluginMetadata {
                    id: id.to_string(),
                    name: id.to_string(),
                    version: "0.1.0".to_string(),
                    priority: 0,
                    capabilities: Vec::new(),
                    capability_priorities: std::collections::HashMap::new(),
                    description: None,
                },
                rerank,
            }
        }
    }

    impl CodePlugin for RerankMockPlugin {
        fn metadata(&self) -> &PluginMetadata {
            &self.meta
        }
        fn supports_rerank(&self) -> bool {
            true
        }
        fn rerank(
            &self,
            query: &str,
            candidates: Vec<cce_types::RerankCandidate>,
        ) -> std::result::Result<Option<cce_types::RerankResult>, PluginError> {
            (self.rerank)(query, candidates)
        }
    }

    fn make_result(id: &str, score: f32) -> SearchResult {
        SearchResult {
            id: id.to_string(),
            score,
            original_score: score,
            vector_score: score,
            kind: "function".to_string(),
            ..Default::default()
        }
    }

    /// Reverse the candidate order and assign descending final scores.
    fn reverse_rerank(
        _query: &str,
        candidates: Vec<cce_types::RerankCandidate>,
    ) -> std::result::Result<Option<cce_types::RerankResult>, PluginError> {
        let reranked: Vec<RerankedCandidate> = candidates
            .iter()
            .rev()
            .enumerate()
            .map(|(i, c)| RerankedCandidate {
                id: c.id.clone(),
                rerank_score: 1.0 - i as f32 * 0.2,
                initial_score: c.initial_score,
                final_score: 1.0 - i as f32 * 0.2,
                rank_change: i as i32,
                reasoning: None,
            })
            .collect();
        Ok(Some(cce_types::RerankResult::new(reranked)))
    }

    fn decline_rerank(
        _query: &str,
        _candidates: Vec<cce_types::RerankCandidate>,
    ) -> std::result::Result<Option<cce_types::RerankResult>, PluginError> {
        Ok(None)
    }

    fn failing_rerank(
        _query: &str,
        _candidates: Vec<cce_types::RerankCandidate>,
    ) -> std::result::Result<Option<cce_types::RerankResult>, PluginError> {
        Err(PluginError::ExecutionFailed("broken".to_string()))
    }

    #[tokio::test]
    async fn test_rerank_plugin_reorders_results() {
        let reranker = PluginReranker::new(vec![std::sync::Arc::new(RerankMockPlugin::new(
            "rerank",
            reverse_rerank,
        ))]);
        let results = vec![make_result("a", 0.5), make_result("b", 0.4)];

        let out = reranker
            .rerank(results, "query", &SearchConfig::default())
            .await
            .expect("rerank must succeed");

        assert_eq!(out.len(), 2);
        assert_eq!(
            out[0].id, "b",
            "plugin reranking must reorder by final score"
        );
        assert!(
            out[0].metadata.contains_key("rerank_score"),
            "rerank score metadata must be recorded"
        );
        assert!(
            out[0].metadata.contains_key("rank_change"),
            "rank change metadata must be recorded"
        );
    }

    #[tokio::test]
    async fn test_rerank_decline_keeps_original_order() {
        let reranker = PluginReranker::new(vec![std::sync::Arc::new(RerankMockPlugin::new(
            "decline",
            decline_rerank,
        ))]);
        let results = vec![make_result("a", 0.5), make_result("b", 0.4)];

        let out = reranker
            .rerank(results, "query", &SearchConfig::default())
            .await
            .expect("rerank must succeed");
        let ids: Vec<&str> = out.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b"], "declined rerank keeps original order");
    }

    #[tokio::test]
    async fn test_rerank_multiple_plugins_first_non_empty_wins() {
        let reranker = PluginReranker::new(vec![
            std::sync::Arc::new(RerankMockPlugin::new("decline", decline_rerank)),
            std::sync::Arc::new(RerankMockPlugin::new("rerank", reverse_rerank)),
        ]);
        let results = vec![make_result("a", 0.5), make_result("b", 0.4)];

        let out = reranker
            .rerank(results, "query", &SearchConfig::default())
            .await
            .expect("rerank must succeed");
        assert_eq!(out[0].id, "b", "second plugin's rerank must take effect");
    }

    #[tokio::test]
    async fn test_rerank_plugin_error_keeps_original() {
        let reranker = PluginReranker::new(vec![std::sync::Arc::new(RerankMockPlugin::new(
            "broken",
            failing_rerank,
        ))]);
        let results = vec![make_result("a", 0.5), make_result("b", 0.4)];

        let out = reranker
            .rerank(results, "query", &SearchConfig::default())
            .await
            .expect("failed rerank must not propagate");
        let ids: Vec<&str> = out.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b"], "failed rerank keeps original order");
    }

    #[tokio::test]
    async fn test_rerank_empty_results_and_plugins() {
        let empty = PluginReranker::new(vec![]);
        let out = empty
            .rerank(vec![], "query", &SearchConfig::default())
            .await
            .expect("empty rerank must succeed");
        assert!(out.is_empty());

        let reranker = PluginReranker::new(vec![std::sync::Arc::new(RerankMockPlugin::new(
            "rerank",
            reverse_rerank,
        ))]);
        let out = reranker
            .rerank(vec![], "query", &SearchConfig::default())
            .await
            .expect("empty results must pass through");
        assert!(out.is_empty());
    }
}
