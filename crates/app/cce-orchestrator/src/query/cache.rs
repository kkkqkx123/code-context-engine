//! Query result cache implementation
//!
//! Caches complete query results to avoid redundant retrieval work.
//!
//! Query-text embeddings are NOT cached here: they are memoized at the
//! searcher level (`super::cached_embedder::CachedEmbedder`), which is where
//! all query-side embedder consumers share a single instance.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::future::Future;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Duration;

use moka::future::Cache;

use super::error::Result;
use super::types::{AggregatedQueryOptions, QueryOptions, QueryResult};
use crate::query::filter::QueryFilter;

fn hash_one(value: &impl Hash) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

/// Stable fingerprint of the full search configuration.
///
/// Serializes the configuration and sorts every object key before hashing:
/// map iteration order is hash-random per instance (and the workspace build
/// unifies `preserve_order`, so insertion order is preserved verbatim),
/// while a cache key must be identical for identical configurations.
fn config_fingerprint(config: &super::types::SearchConfig) -> u64 {
    let mut value = serde_json::to_value((
        &config.vector,
        &config.bm25,
        &config.fusion,
        &config.result,
        &config.rerank,
        &config.score,
        &config.plugin,
        &config.annotation,
        &config.timeout_ms,
    ))
    .unwrap_or(serde_json::Value::Null);
    sort_json_keys(&mut value);
    hash_one(&serde_json::to_string(&value).unwrap_or_default())
}

fn sort_json_keys(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            map.sort_keys();
            for item in map.values_mut() {
                sort_json_keys(item);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                sort_json_keys(item);
            }
        }
        _ => {}
    }
}

/// Cache key for query results
///
/// Includes project_id and the full epoch view (own + inherited parent +
/// overridden-file set hash) so a publication, adoption or override change
/// can never serve stale entries.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CacheKey {
    /// Project ID for query scoping
    project_id: i64,
    /// Own data epoch of the active generation
    epoch: i64,
    /// Inherited parent epoch (`-1` when the generation is full)
    parent_epoch: i64,
    /// Hash of the overridden-file exclusion set
    excluded_hash: u64,
    /// Hash of the query text
    query_hash: u64,
    /// Hash of search sources
    sources_hash: u64,
    /// Hash of filters
    filters_hash: u64,
    /// Whether the query carries a neighbour resolver: expanded results must
    /// never be served to a request without expansion (and vice versa).
    expansion: bool,
    /// Result limit
    limit: usize,
    /// Hash of the full search configuration debug form, so any effective
    /// parameter change (fusion, rerank, threshold, annotation, plugin
    /// switches, content budget) produces a new key even when the epoch is
    /// unchanged.
    config_hash: u64,
}

impl CacheKey {
    /// Build cache key from query options
    pub fn from_options(options: &QueryOptions) -> Self {
        Self::from_options_with_view(options, &QueryFilter::default())
    }

    /// Build a cache key for the active project epoch view.
    pub fn from_options_with_view(options: &QueryOptions, view: &QueryFilter) -> Self {
        // Hash the epoch view (own + parent + exclusion set)
        let mut hasher = DefaultHasher::new();
        options.project_id.hash(&mut hasher);
        view.epoch_value().hash(&mut hasher);
        view.parent_epoch().hash(&mut hasher);
        view.excluded_files().hash(&mut hasher);
        let excluded_hash = hasher.finish();
        let parent_epoch = view.parent_epoch().unwrap_or(-1);

        // Hash query text
        let mut hasher = DefaultHasher::new();
        options.query.hash(&mut hasher);
        let query_hash = hasher.finish();

        // Hash sources
        let mut hasher = DefaultHasher::new();
        options.sources.hash(&mut hasher);
        let sources_hash = hasher.finish();

        // Hash filters
        let mut hasher = DefaultHasher::new();
        options.directory_prefix.hash(&mut hasher);
        options.exclude_content_types.hash(&mut hasher);
        options.include_categories.hash(&mut hasher);
        options.exclude_categories.hash(&mut hasher);
        options.exclude_patterns.hash(&mut hasher);
        options.include_patterns.hash(&mut hasher);
        let filters_hash = hasher.finish();

        // Hash per-request toggles that change the effective pipeline.
        let mut hasher = DefaultHasher::new();
        options.query_intent.hash(&mut hasher);
        options.enable_rerank.hash(&mut hasher);
        options.enable_result_filter.hash(&mut hasher);
        options.with_source.hash(&mut hasher);
        let toggle_hash = hasher.finish();

        let mut hasher = DefaultHasher::new();
        sources_hash.hash(&mut hasher);
        toggle_hash.hash(&mut hasher);
        let sources_hash = hasher.finish();

        // The configuration fingerprint covers every effective parameter so
        // future fields join the key without further code changes.
        let config_hash = config_fingerprint(&options.config);

        Self {
            project_id: options.project_id,
            epoch: view.epoch_value(),
            parent_epoch,
            excluded_hash,
            query_hash,
            sources_hash,
            filters_hash,
            expansion: options.neighbour_resolver.is_some(),
            limit: options.config.result.limit,
            config_hash,
        }
    }

    /// Build a cache key for an aggregated query over one epoch view.
    ///
    /// Sub-query texts, sources and weights join the key alongside the
    /// global configuration, so any merge-affecting change recomputes
    /// instead of serving a stale union.
    pub fn for_aggregated(
        project_id: i64,
        view: &QueryFilter,
        agg: &AggregatedQueryOptions,
    ) -> Self {
        let mut hasher = DefaultHasher::new();
        project_id.hash(&mut hasher);
        view.epoch_value().hash(&mut hasher);
        view.parent_epoch().hash(&mut hasher);
        view.excluded_files().hash(&mut hasher);
        let excluded_hash = hasher.finish();
        let parent_epoch = view.parent_epoch().unwrap_or(-1);

        let mut hasher = DefaultHasher::new();
        for sub in &agg.sub_queries {
            sub.text.hash(&mut hasher);
            sub.sources.hash(&mut hasher);
            sub.weight.to_bits().hash(&mut hasher);
        }
        format!("{:?}", agg.filters).hash(&mut hasher);
        agg.exclude_patterns.hash(&mut hasher);
        agg.include_patterns.hash(&mut hasher);
        let query_hash = hasher.finish();

        let mut hasher = DefaultHasher::new();
        agg.enable_rerank.hash(&mut hasher);
        agg.enable_result_filter.hash(&mut hasher);
        agg.query_intent.hash(&mut hasher);
        agg.neighbour_resolver.is_some().hash(&mut hasher);
        let sources_hash = hasher.finish();

        let config_hash = config_fingerprint(&agg.global_config);

        Self {
            project_id,
            epoch: view.epoch_value(),
            parent_epoch,
            excluded_hash,
            query_hash,
            sources_hash,
            filters_hash: 0,
            expansion: agg.neighbour_resolver.is_some(),
            limit: agg.global_config.result.limit,
            config_hash,
        }
    }
}

/// Cache configuration
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Maximum number of result cache entries
    pub max_results: u64,
    /// Time-to-live for result cache in seconds
    pub result_ttl_secs: u64,
    /// Enable caching
    pub enabled: bool,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_results: 1000,
            result_ttl_secs: 300, // 5 minutes
            enabled: true,
        }
    }
}

/// Derive the query result cache configuration from the global `[cache]`
/// section. Serving cached results requires both the master switch and the
/// per-subsystem `query_result.enabled` flag to be on.
impl From<&cce_config::modules::GlobalCacheConfig> for CacheConfig {
    fn from(global: &cce_config::modules::GlobalCacheConfig) -> Self {
        Self {
            max_results: global.query_result.max_entries,
            result_ttl_secs: global.query_result.ttl_secs,
            enabled: global.enabled && global.query_result.enabled,
        }
    }
}

/// Query result cache
///
/// Results are keyed by project + epoch view + query identity, so any
/// publication, adoption or override change produces new keys and can never
/// serve stale entries.
pub struct QueryCache {
    /// Result cache
    result_cache: Cache<CacheKey, QueryResult>,
    /// Per-key single-flight guards for in-flight computations.
    ///
    /// Concurrent lookups of the same key coalesce onto one computation: the
    /// first caller installs a guard and computes; followers clone the guard,
    /// wait for the leader to release it, then read the published result from
    /// `result_cache` instead of re-running the pipeline. An entry lives only
    /// while a computation is in flight — the leader removes its own entry on
    /// completion, so idle keys never accumulate.
    inflight: Arc<tokio::sync::Mutex<HashMap<CacheKey, Arc<tokio::sync::Mutex<()>>>>>,
    /// Configuration
    config: CacheConfig,
}

impl QueryCache {
    /// Create a new query cache
    pub fn new(config: CacheConfig) -> Self {
        let result_cache = Cache::builder()
            .max_capacity(config.max_results)
            .time_to_live(Duration::from_secs(config.result_ttl_secs))
            .build();

        Self {
            result_cache,
            inflight: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            config,
        }
    }

    /// Create a disabled cache (no caching)
    pub fn disabled() -> Self {
        Self {
            result_cache: Cache::new(0),
            inflight: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            config: CacheConfig {
                enabled: false,
                ..Default::default()
            },
        }
    }

    /// Check if caching is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Get a cached result for a specific active epoch view.
    pub async fn get_result_for_view(
        &self,
        options: &QueryOptions,
        view: &QueryFilter,
    ) -> Option<QueryResult> {
        if !self.config.enabled {
            return None;
        }

        let key = CacheKey::from_options_with_view(options, view);
        self.result_cache.get(&key).await
    }

    /// Store a result for a specific active epoch view.
    pub async fn put_result_for_view(
        &self,
        options: &QueryOptions,
        view: &QueryFilter,
        result: QueryResult,
    ) {
        if !self.config.enabled {
            return;
        }

        let key = CacheKey::from_options_with_view(options, view);
        self.result_cache.insert(key, result).await;
    }

    /// Invalidate all cached results
    pub async fn invalidate_all(&self) {
        self.result_cache.invalidate_all();
    }

    /// Return the cached result for `key`, or compute it under a per-key
    /// single-flight guard.
    ///
    /// The cache is checked before and after acquiring the guard: a follower
    /// that waited for a leader's computation reads the published entry rather
    /// than recomputing. The stored entry always carries `from_cache = false`
    /// (it is a fresh computation); the hit paths flip that flag before
    /// returning. When caching is disabled the closure runs unconditionally.
    pub async fn get_or_compute<F, Fut>(&self, key: CacheKey, compute: F) -> Result<QueryResult>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<QueryResult>>,
    {
        if !self.config.enabled {
            return compute().await;
        }

        if let Some(mut cached) = self.result_cache.get(&key).await {
            cached.from_cache = true;
            return Ok(cached);
        }

        let guard = {
            let mut inflight = self.inflight.lock().await;
            Arc::clone(
                inflight
                    .entry(key.clone())
                    .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(()))),
            )
        };
        let _lock = guard.lock().await;

        // A leader may have published while we waited for the guard.
        if let Some(mut cached) = self.result_cache.get(&key).await {
            cached.from_cache = true;
            return Ok(cached);
        }

        let result = compute().await;

        // Publish before releasing the guard so a waiter observes the entry on
        // its cache re-check. Drop the slot on both success and failure so
        // errored keys never linger.
        if let Ok(value) = &result {
            self.result_cache.insert(key.clone(), value.clone()).await;
        }

        let mut inflight = self.inflight.lock().await;
        if inflight
            .get(&key)
            .is_some_and(|existing| Arc::ptr_eq(existing, &guard))
        {
            inflight.remove(&key);
        }
        drop(inflight);

        result
    }
}

impl Clone for QueryCache {
    fn clone(&self) -> Self {
        Self {
            result_cache: self.result_cache.clone(),
            inflight: Arc::clone(&self.inflight),
            config: self.config.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::types::{QueryConfigBuilder, SearchSources};

    #[test]
    fn test_cache_key_from_options() {
        let options1 = QueryConfigBuilder::new(1)
            .build("test query")
            .with_sources(SearchSources::default())
            .with_limit(10);

        let options2 = QueryConfigBuilder::new(1)
            .build("test query")
            .with_sources(SearchSources::default())
            .with_limit(10);

        let key1 = CacheKey::from_options(&options1);
        let key2 = CacheKey::from_options(&options2);

        // Same options should produce same key
        assert_eq!(key1, key2);

        // Different limit should produce different key
        let options3 = QueryConfigBuilder::new(1)
            .build("test query")
            .with_sources(SearchSources::default())
            .with_limit(20);
        let key3 = CacheKey::from_options(&options3);
        assert_ne!(key1, key3);

        // Different project_id should produce different key
        let options4 = QueryConfigBuilder::new(2)
            .build("test query")
            .with_sources(SearchSources::default())
            .with_limit(10);
        let key4 = CacheKey::from_options(&options4);
        assert_ne!(key1, key4);
        assert_eq!(key4.project_id, 2);
    }

    #[test]
    fn test_cache_config_default() {
        let config = CacheConfig::default();
        assert!(config.enabled);
        assert_eq!(config.max_results, 1000);
        assert_eq!(config.result_ttl_secs, 300);
    }

    #[test]
    fn test_cache_config_from_global() {
        use cce_config::modules::GlobalCacheConfig;

        let mut global = GlobalCacheConfig::default();
        global.query_result.max_entries = 42;
        global.query_result.ttl_secs = 7;
        global.query_result.enabled = true;
        let config = CacheConfig::from(&global);
        assert_eq!(config.max_results, 42);
        assert_eq!(config.result_ttl_secs, 7);
        assert!(config.enabled);

        // The master switch gates the per-subsystem flag.
        global.enabled = false;
        assert!(!CacheConfig::from(&global).enabled);

        global.enabled = true;
        global.query_result.enabled = false;
        assert!(!CacheConfig::from(&global).enabled);
    }

    #[tokio::test]
    async fn test_disabled_cache() {
        let cache = QueryCache::disabled();
        assert!(!cache.is_enabled());

        let options = QueryConfigBuilder::new(1).build("test");
        let view = crate::query::filter::QueryFilter::default();
        let result = cache.get_result_for_view(&options, &view).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_get_or_compute_single_flight() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use tokio::sync::Barrier;

        let cache = QueryCache::new(CacheConfig::default());
        let options = QueryConfigBuilder::new(1).build("test query");
        let view = crate::query::filter::QueryFilter::default();
        let key = CacheKey::from_options_with_view(&options, &view);

        let calls = Arc::new(AtomicUsize::new(0));
        // One leader plus four followers all race on the same key.
        let barrier = Arc::new(Barrier::new(5));

        let mut handles = Vec::new();
        for _ in 0..5 {
            let cache = cache.clone();
            let key = key.clone();
            let calls = Arc::clone(&calls);
            let barrier = Arc::clone(&barrier);
            handles.push(tokio::spawn(async move {
                barrier.wait().await;
                cache
                    .get_or_compute(key, || async {
                        calls.fetch_add(1, Ordering::SeqCst);
                        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                        Ok(QueryResult::default())
                    })
                    .await
            }));
        }

        let mut computed = 0;
        for handle in handles {
            let result = handle.await.expect("task join").expect("compute");
            if !result.from_cache {
                computed += 1;
            }
        }

        // Exactly one computation ran; every follower read the published entry.
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(computed, 1);
    }

    #[tokio::test]
    async fn test_get_or_compute_error_is_not_cached() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let cache = QueryCache::new(CacheConfig::default());
        let options = QueryConfigBuilder::new(1).build("test query");
        let view = crate::query::filter::QueryFilter::default();
        let key = CacheKey::from_options_with_view(&options, &view);

        let calls = AtomicUsize::new(0);

        let failed = cache
            .get_or_compute(key.clone(), || async {
                calls.fetch_add(1, Ordering::SeqCst);
                Err(crate::query::error::QueryError::invalid("boom"))
            })
            .await;
        assert!(failed.is_err());

        // A failed computation is not cached, so the next call recomputes.
        let succeeded = cache
            .get_or_compute(key, || async {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(QueryResult::default())
            })
            .await;
        assert!(succeeded.is_ok());
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn test_cache_key_differs_by_effective_config() {
        use crate::query::types::QueryIntent;

        let base = QueryConfigBuilder::new(1).build("test query");
        let view = crate::query::filter::QueryFilter::default();
        let base_key = CacheKey::from_options_with_view(&base, &view);

        let other_intent = QueryConfigBuilder::new(1)
            .build("test query")
            .with_query_intent(Some(QueryIntent::Keyword));
        assert_ne!(
            base_key,
            CacheKey::from_options_with_view(&other_intent, &view)
        );

        let no_rerank = QueryConfigBuilder::new(1)
            .build("test query")
            .with_enable_rerank(false);
        assert_ne!(
            base_key,
            CacheKey::from_options_with_view(&no_rerank, &view)
        );

        let no_filter = QueryConfigBuilder::new(1)
            .build("test query")
            .with_enable_result_filter(false);
        assert_ne!(
            base_key,
            CacheKey::from_options_with_view(&no_filter, &view)
        );

        let mut config = crate::query::types::SearchConfig::default();
        config.result.min_score = 0.9;
        let strict = QueryConfigBuilder::new(1)
            .build("test query")
            .with_config(config);
        assert_ne!(base_key, CacheKey::from_options_with_view(&strict, &view));
    }

    #[test]
    fn test_aggregated_key_tracks_sub_queries_and_weights() {
        use crate::query::types::{AggregatedQueryOptions, SubQuery};

        let view = crate::query::filter::QueryFilter::default();
        let agg = || AggregatedQueryOptions {
            project_id: 1,
            sub_queries: vec![SubQuery {
                text: "auth flow".to_string(),
                sources: SearchSources::default(),
                weight: 1.0,
            }],
            ..Default::default()
        };
        let base_key = CacheKey::for_aggregated(1, &view, &agg());
        assert_eq!(base_key, CacheKey::for_aggregated(1, &view, &agg()));

        let mut heavier = agg();
        heavier.sub_queries[0].weight = 2.0;
        assert_ne!(base_key, CacheKey::for_aggregated(1, &view, &heavier));

        let mut stricter = agg();
        stricter.global_config.result.limit = 3;
        assert_ne!(base_key, CacheKey::for_aggregated(1, &view, &stricter));
    }
}
