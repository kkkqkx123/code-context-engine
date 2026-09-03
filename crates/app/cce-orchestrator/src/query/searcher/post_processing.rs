//! Unified searcher implementation
//!
//! Provides a unified search interface that combines vector retrieval,
//! BM25 enhancement, and result ranking into a cohesive search flow.
//!
//! # Architecture
//!
//! The searcher delegates to specialized components:
//! - ResultProcessor: Ranking, filtering, and threshold application
//! - AssemblyHandler: SPSR-Graph assembly operations

use crate::query::error::QueryError;
use crate::query::error::Result;
use crate::query::filter::QueryFilter;
use crate::query::types::{QueryOptions, SearchResult};

use super::searcher_core::Searcher;

impl Searcher {
    /// Apply post-processing pipeline (rerank, sort, filter)
    ///
    /// Shared across all execution strategies:
    /// - `DenseRecall`: applies all post-processing steps
    /// - `Bm25Recall`: applies all post-processing steps
    /// - `HybridRecall`: applies all post-processing steps (after fusion)
    pub(crate) async fn post_process_results(
        &self,
        results: Vec<SearchResult>,
        options: &QueryOptions,
    ) -> Result<Vec<SearchResult>> {
        tracing::trace!("Applying post-processing");

        // Step 1: Reranking (plugin + LLM per the configured execution order).
        // The config layer decides whether the rerank handler/plugins exist;
        // the per-request `enable_rerank` override can force it on/off.
        let reranked_results = if options.enable_rerank.unwrap_or(true) {
            match options.config.rerank.order {
                cce_config::modules::rerank::RerankOrder::PluginOnly => {
                    self.plugin_reranker
                        .rerank(results, &options.query, &options.config)
                        .await?
                }
                cce_config::modules::rerank::RerankOrder::LlmOnly => {
                    self.reranker
                        .rerank(results, &options.query, &options.config)
                        .await?
                }
                cce_config::modules::rerank::RerankOrder::PluginThenLlm => {
                    let after_plugin = self
                        .plugin_reranker
                        .rerank(results, &options.query, &options.config)
                        .await?;
                    self.reranker
                        .rerank(after_plugin, &options.query, &options.config)
                        .await?
                }
            }
        } else {
            tracing::trace!("Reranking disabled by request-level override");
            results
        };

        // Step 2: Score sorting
        let sorted_results = self.score_sorter.sort(reranked_results);

        // Result filtering (ResultFilter capability), which runs after
        // reranking + sorting and before the threshold filter so plugins can
        // remove/boost/annotate candidates.
        let sorted_results = if options.config.plugin.filter_enabled {
            self.apply_result_filter(&sorted_results, options).await
        } else {
            sorted_results
        };

        // Step 3: Threshold filtering
        let final_results = self
            .threshold_filter
            .apply(sorted_results, &options.config)?;

        Ok(final_results)
    }

    /// Apply the `QueryRewrite` capability chain: each plugin rewrites the
    /// query (previous output is the next input); failures keep the last
    /// successful query. Every plugin's `expansion_terms` are accumulated and
    /// appended to the final rewritten query as OR-joined keywords so they
    /// all participate in recall.
    pub(crate) async fn apply_query_rewrite(
        &self,
        mut options: QueryOptions,
    ) -> Result<QueryOptions> {
        let Some(registry) = &self.plugin_registry else {
            return Ok(options);
        };
        let original = options.query.clone();
        options.query = rewrite_query_via_plugins(registry, &options.query).await;
        tracing::trace!(
            original = %original,
            rewritten = %options.query,
            "Query rewritten by plugins"
        );
        Ok(options)
    }

    /// Apply the `Fusion` capability weight override.
    ///
    /// Plugins are queried by priority (descending); the **first** plugin
    /// returning a non-`None` weight set takes effect (override tier).
    /// Provided weights are validated to `[0, 1]`; invalid fields keep the
    /// configured default.
    pub(crate) async fn apply_fusion_override(
        &self,
        options: &QueryOptions,
        config: crate::query::retrieval::HybridFusionConfig,
        vector_count: usize,
        bm25_count: usize,
    ) -> crate::query::retrieval::HybridFusionConfig {
        let Some(registry) = &self.plugin_registry else {
            return config;
        };
        let results =
            query_fusion_weights_from_plugins(registry, &options.query, vector_count, bm25_count)
                .await;
        merge_fusion_weights_override(config, results)
    }

    /// Apply the `ResultFilter` capability chain after reranking.
    pub(crate) async fn apply_result_filter(
        &self,
        results: &[SearchResult],
        options: &QueryOptions,
    ) -> Vec<SearchResult> {
        let Some(registry) = &self.plugin_registry else {
            return results.to_vec();
        };
        apply_result_filter_chain(registry, results, &options.query).await
    }

    /// Derive the epoch view of the active publication.
    ///
    /// Reads the active manifest (own epoch + inheritance link) and its
    /// generation overrides; recomputed per request by design.
    pub(crate) fn load_query_filter(&self, project_id: i64) -> Result<QueryFilter> {
        let sqlite = self
            .sqlite
            .as_ref()
            .ok_or_else(|| QueryError::config("SQLite database not available"))?;

        // read-only connection — the active manifest is queried on
        // every search request and must not contend with the write lock.
        let conn = sqlite
            .read_connection()
            .map_err(|e| QueryError::storage(&format!("Failed to get SQLite connection: {e}")))?;
        crate::query::filter::load_active_query_filter(&conn, project_id)
    }
}

pub(crate) fn merge_fusion_weights_override(
    mut config: crate::query::retrieval::HybridFusionConfig,
    results: impl IntoIterator<Item = Option<cce_types::plugin::FusionWeights>>,
) -> crate::query::retrieval::HybridFusionConfig {
    for result in results {
        let Some(weights) = result else {
            continue;
        };
        if let Some(w) = weights.vector_weight {
            if (0.0..=1.0).contains(&w) {
                config.vector_weight = w;
            }
        }
        if let Some(w) = weights.bm25_weight {
            if (0.0..=1.0).contains(&w) {
                config.bm25_weight = w;
            }
        }
        if let Some(min_score) = weights.min_score {
            config.min_score = min_score;
        }
        break;
    }
    config
}

/// Run the `QueryRewrite` capability chain over a registry.
///
/// Each plugin rewrites the query produced by the previous plugin (failures
/// keep the last successful query); every plugin's `expansion_terms` are
/// accumulated (deduplicated) and OR-joined into a trailing `(t1 OR t2)`.
pub(crate) async fn rewrite_query_via_plugins(
    registry: &cce_plugin::PluginRegistry,
    query: &str,
) -> String {
    let rewritters = registry.get_plugins(cce_plugin::PluginCapability::QueryRewrite, None, None);
    if rewritters.is_empty() {
        return query.to_string();
    }
    let mut current = query.to_string();
    let mut expansions: Vec<String> = Vec::new();
    for plugin in rewritters {
        let plugin_id = plugin.metadata().id.clone();
        let plugin = plugin.clone();
        let query = current.clone();
        let started = std::time::Instant::now();
        let log_query = query.clone();
        let result = tokio::task::spawn_blocking(move || plugin.rewrite_query(&query)).await;
        match result {
            Ok(Ok(Some(rw))) => {
                tracing::debug!(
                    plugin = %plugin_id,
                    input = %log_query,
                    output = %rw.rewritten_query,
                    expansion_terms = rw.expansion_terms.len(),
                    elapsed_ms = started.elapsed().as_millis() as u64,
                    "QueryRewrite chain step applied"
                );
                if !rw.rewritten_query.trim().is_empty() {
                    current = rw.rewritten_query;
                }
                if !rw.expansion_terms.is_empty() {
                    for term in rw.expansion_terms {
                        if !term.trim().is_empty() && !expansions.contains(&term) {
                            expansions.push(term);
                        }
                    }
                }
            }
            Ok(Ok(None)) => {}
            Ok(Err(e)) => {
                tracing::warn!(
                    plugin = %plugin_id,
                    error = %e,
                    "rewrite_query failed, keeping previous query"
                );
            }
            Err(e) => {
                tracing::warn!(
                    plugin = %plugin_id,
                    error = %e,
                    "rewrite_query panicked, keeping previous query"
                );
            }
        }
    }
    if expansions.is_empty() {
        current
    } else {
        let mut q = current;
        q.push_str(&format!(" ({})", expansions.join(" OR ")));
        q
    }
}

/// Query the `Fusion` capability plugins of a registry.
///
/// Returns one entry per matched plugin (in priority order): `Some(weights)`
/// for a successful override, `None` for decline / failure / panic.
pub(crate) async fn query_fusion_weights_from_plugins(
    registry: &cce_plugin::PluginRegistry,
    query: &str,
    vector_count: usize,
    bm25_count: usize,
) -> Vec<Option<cce_types::plugin::FusionWeights>> {
    let plugins = registry.get_plugins(cce_plugin::PluginCapability::Fusion, None, None);
    let mut results: Vec<Option<cce_types::plugin::FusionWeights>> = Vec::new();
    for plugin in plugins {
        let plugin_id = plugin.metadata().id.clone();
        let plugin = plugin.clone();
        let query = query.to_string();
        let result = tokio::task::spawn_blocking(move || {
            plugin.fusion_weights(&query, vector_count, bm25_count)
        })
        .await;
        match result {
            Ok(Ok(Some(weights))) => results.push(Some(weights)),
            Ok(Ok(None)) => results.push(None),
            Ok(Err(e)) => {
                tracing::warn!(
                    plugin = %plugin_id,
                    error = %e,
                    "fusion_weights failed, keeping default weights"
                );
                results.push(None);
            }
            Err(e) => {
                tracing::warn!(
                    plugin = %plugin_id,
                    error = %e,
                    "fusion_weights panicked, keeping default weights"
                );
                results.push(None);
            }
        }
    }
    results
}

/// Apply the `ResultFilter` capability chain over a registry.
///
/// Each plugin receives the current candidate list; its entries remove or
/// boost results by id. Failures keep the current list.
pub(crate) async fn apply_result_filter_chain(
    registry: &cce_plugin::PluginRegistry,
    results: &[SearchResult],
    query: &str,
) -> Vec<SearchResult> {
    let plugins = registry.get_plugins(cce_plugin::PluginCapability::ResultFilter, None, None);
    if plugins.is_empty() {
        return results.to_vec();
    }
    let mut current: Vec<SearchResult> = results.to_vec();
    for plugin in plugins {
        let plugin_id = plugin.metadata().id.clone();
        let plugin = plugin.clone();
        let query = query.to_string();
        let candidates: Vec<cce_types::RerankCandidate> = current
            .iter()
            .map(|r| cce_types::RerankCandidate {
                id: r.id.clone(),
                content: r.content.clone(),
                file_path: r.file_path.clone(),
                initial_score: r.score,
                entity_type: if r.kind.is_empty() {
                    None
                } else {
                    Some(r.kind.clone())
                },
                metadata: r.metadata.clone(),
            })
            .collect();
        let before = current.len();
        let started = std::time::Instant::now();
        let result =
            tokio::task::spawn_blocking(move || plugin.filter_results(&query, candidates)).await;
        match result {
            Ok(Ok(Some(entries))) => {
                let mut new_results = Vec::with_capacity(current.len());
                for mut r in current {
                    if let Some(entry) = entries.iter().find(|e| e.id == r.id) {
                        if entry.remove {
                            continue;
                        }
                        if let Some(boost) = entry.boost {
                            r.score += boost;
                        }
                    }
                    new_results.push(r);
                }
                tracing::debug!(
                    plugin = %plugin_id,
                    before,
                    after = new_results.len(),
                    elapsed_ms = started.elapsed().as_millis() as u64,
                    "ResultFilter chain step applied"
                );
                current = new_results;
            }
            Ok(Ok(None)) => {}
            Ok(Err(e)) => {
                tracing::warn!(
                    plugin = %plugin_id,
                    error = %e,
                    "filter_results failed, keeping results"
                );
            }
            Err(e) => {
                tracing::warn!(
                    plugin = %plugin_id,
                    error = %e,
                    "filter_results panicked, keeping results"
                );
            }
        }
    }
    current
}
