//! Unified cache configuration
//!
//! Consolidates all cache-related parameters into a single top-level
//! `[cache]` section. Each sub-system gets a namespaced sub-table.

use serde::{Deserialize, Serialize};

/// Global cache configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GlobalCacheConfig {
    /// Master switch for all caches
    pub enabled: bool,
    /// Chunk / file processing LRU cache
    pub chunk: ChunkCacheConfig,
    /// Query result cache (moka)
    pub query_result: QueryResultCacheConfig,
    /// Query embedding memoization cache (moka)
    pub query_embedding: QueryEmbeddingCacheConfig,
    /// SQLite page cache
    pub sqlite: SqliteCacheConfig,
    /// BM25 reader cache
    pub bm25_reader: Bm25ReaderCacheConfig,
    /// Symbol resolution cache
    pub symbol_resolution: SymbolResolutionCacheConfig,
    /// Engine-level per-project component caches
    pub engine: EngineCacheConfig,
}

/// Chunk / file processing LRU cache configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ChunkCacheConfig {
    /// Maximum number of files to cache chunk results
    pub max_entries: usize,
}

/// Query result cache configuration (moka)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct QueryResultCacheConfig {
    /// Maximum number of cached query results
    pub max_entries: u64,
    /// Time-to-live for result cache in seconds
    pub ttl_secs: u64,
    /// Enable query result caching
    pub enabled: bool,
}

/// Query embedding memoization cache configuration (moka)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct QueryEmbeddingCacheConfig {
    /// Maximum number of cached query embeddings per searcher
    pub max_entries: u64,
    /// Time-to-live for one cached query embedding in seconds
    pub ttl_secs: u64,
}

/// SQLite page cache configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SqliteCacheConfig {
    /// Cache size in KB (negative = KB, positive = pages)
    /// Default: -64000 (64 MB)
    pub size: i32,
}

/// BM25 reader cache configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Bm25ReaderCacheConfig {
    /// Enable reader caching
    pub enabled: bool,
    /// Reload policy: "on_commit", "on_commit_with_delay", "manual"
    pub reload_policy: String,
}

/// Symbol resolution cache configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SymbolResolutionCacheConfig {
    /// Maximum number of cached resolution entries
    pub max_entries: usize,
}

/// Engine-level per-project component cache configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct EngineCacheConfig {
    /// Maximum number of projects whose components stay cached
    /// (0 = unlimited; currently informational since ProjectCache is unbounded)
    pub max_projects: usize,
}

impl Default for GlobalCacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            chunk: ChunkCacheConfig::default(),
            query_result: QueryResultCacheConfig::default(),
            query_embedding: QueryEmbeddingCacheConfig::default(),
            sqlite: SqliteCacheConfig::default(),
            bm25_reader: Bm25ReaderCacheConfig::default(),
            symbol_resolution: SymbolResolutionCacheConfig::default(),
            engine: EngineCacheConfig::default(),
        }
    }
}

impl Default for ChunkCacheConfig {
    fn default() -> Self {
        Self { max_entries: 100 }
    }
}

impl Default for QueryResultCacheConfig {
    fn default() -> Self {
        Self {
            max_entries: 1000,
            ttl_secs: 300,
            enabled: true,
        }
    }
}

impl Default for QueryEmbeddingCacheConfig {
    fn default() -> Self {
        Self {
            max_entries: 512,
            ttl_secs: 600,
        }
    }
}

impl Default for SqliteCacheConfig {
    fn default() -> Self {
        Self { size: -64000 }
    }
}

impl Default for Bm25ReaderCacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            reload_policy: "on_commit_with_delay".to_string(),
        }
    }
}

impl Default for SymbolResolutionCacheConfig {
    fn default() -> Self {
        Self { max_entries: 4096 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = GlobalCacheConfig::default();
        assert!(config.enabled);
        assert_eq!(config.chunk.max_entries, 100);
        assert_eq!(config.query_result.max_entries, 1000);
        assert_eq!(config.query_result.ttl_secs, 300);
        assert!(config.query_result.enabled);
        assert_eq!(config.query_embedding.max_entries, 512);
        assert_eq!(config.query_embedding.ttl_secs, 600);
        assert_eq!(config.sqlite.size, -64000);
        assert!(config.bm25_reader.enabled);
        assert_eq!(config.bm25_reader.reload_policy, "on_commit_with_delay");
        assert_eq!(config.symbol_resolution.max_entries, 4096);
    }

    #[test]
    fn test_deserialize_partial_config() {
        let toml_str = r#"
            chunk.max_entries = 200

            query_result.max_entries = 500
            query_result.ttl_secs = 60
        "#;

        let config: GlobalCacheConfig = toml::from_str(toml_str).unwrap();
        assert!(config.enabled);
        assert_eq!(config.chunk.max_entries, 200);
        assert_eq!(config.query_result.max_entries, 500);
        assert_eq!(config.query_result.ttl_secs, 60);
        // Defaults for unspecified fields
        assert_eq!(config.query_embedding.max_entries, 512);
        assert_eq!(config.sqlite.size, -64000);
    }

    #[test]
    fn test_serialize_roundtrip() {
        let config = GlobalCacheConfig::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let deserialized: GlobalCacheConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(config.chunk.max_entries, deserialized.chunk.max_entries);
        assert_eq!(
            config.query_result.max_entries,
            deserialized.query_result.max_entries
        );
    }
}
