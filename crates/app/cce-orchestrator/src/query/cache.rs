//! Query result cache implementation
//!
//! Caches complete query results to avoid redundant retrieval work.
//!
//! Query-text embeddings are NOT cached here: they are memoized at the
//! searcher level (`super::cached_embedder::CachedEmbedder`), which is where
//! all query-side embedder consumers (dense/summary strategies and summary
//! boost) share a single instance.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::Duration;

use moka::future::Cache;

use super::types::{QueryOptions, QueryResult};
use crate::query::filter::QueryFilter;

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
    /// Result limit
    limit: usize,
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

        Self {
            project_id: options.project_id,
            epoch: view.epoch_value(),
            parent_epoch,
            excluded_hash,
            query_hash,
            sources_hash,
            filters_hash,
            limit: options.config.result.limit,
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

/// Query result cache
///
/// Results are keyed by project + epoch view + query identity, so any
/// publication, adoption or override change produces new keys and can never
/// serve stale entries.
pub struct QueryCache {
    /// Result cache
    result_cache: Cache<CacheKey, QueryResult>,
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
            config,
        }
    }

    /// Create a disabled cache (no caching)
    pub fn disabled() -> Self {
        Self {
            result_cache: Cache::new(0),
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
}

impl Clone for QueryCache {
    fn clone(&self) -> Self {
        Self {
            result_cache: self.result_cache.clone(),
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

    #[tokio::test]
    async fn test_disabled_cache() {
        let cache = QueryCache::disabled();
        assert!(!cache.is_enabled());

        let options = QueryConfigBuilder::new(1).build("test");
        let view = crate::query::filter::QueryFilter::default();
        let result = cache.get_result_for_view(&options, &view).await;
        assert!(result.is_none());
    }
}
