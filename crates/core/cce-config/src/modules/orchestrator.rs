//! Orchestrator configuration
//!
//! This module provides configuration for the orchestrator module.

use serde::{Deserialize, Serialize};

use crate::validation::{Validate, ValidationResult};
use cce_types::error::config::ConfigValidationError;

use super::ScannerConfig;

// Re-use shared default value functions
use super::defaults::default_true;

/// Orchestrator configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorConfig {
    /// Batch processing configuration
    #[serde(default)]
    pub batch: BatchConfig,
    /// Hot update configuration
    #[serde(default)]
    pub hot_update: HotUpdateConfig,
    /// Cache configuration
    #[serde(default)]
    pub cache: CacheConfig,
    /// Indexer configuration
    #[serde(default)]
    pub indexer: IndexerConfig,
    /// Checkpoint recovery freshness window in seconds (default: 86400 = 24h)
    ///
    /// Startup recovery only replays in_progress checkpoints whose
    /// `updated_at` falls within this window; older ones are marked Failed
    /// (`last_error = "stale recovery skipped"`) instead of being replayed.
    /// The same value drives the periodic TTL cleanup of completed/failed
    /// checkpoints.
    #[serde(default = "default_checkpoint_ttl_seconds")]
    pub checkpoint_ttl_seconds: u64,
    /// Checkpoint TTL cleanup period in seconds (default: 3600 = 1h)
    ///
    /// The server runtime periodically deletes completed/failed checkpoints
    /// older than `checkpoint_ttl_seconds` on this cadence.
    #[serde(default = "default_checkpoint_cleanup_interval_secs")]
    pub checkpoint_cleanup_interval_secs: u64,
    /// Hot-update heartbeat interval in seconds (default: 60 = 1m)
    ///
    /// Long-running hot-update operations periodically refresh
    /// `checkpoint.last_heartbeat` so the stale-active cleanup
    /// (`OperationQueue::cleanup_stale_operations`) does not mistake a live
    /// operation for a crashed one. The default is one third of the cleanup
    /// cadence cap and is intentionally shorter than
    /// `heartbeat_timeout_secs` (300s) used by the queue.
    #[serde(default = "default_heartbeat_interval_secs")]
    pub heartbeat_interval_secs: u64,
}

fn default_checkpoint_ttl_seconds() -> u64 {
    86400
}

fn default_checkpoint_cleanup_interval_secs() -> u64 {
    3600
}

fn default_heartbeat_interval_secs() -> u64 {
    60
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            batch: BatchConfig::default(),
            hot_update: HotUpdateConfig::default(),
            cache: CacheConfig::default(),
            indexer: IndexerConfig::default(),
            checkpoint_ttl_seconds: default_checkpoint_ttl_seconds(),
            checkpoint_cleanup_interval_secs: default_checkpoint_cleanup_interval_secs(),
            heartbeat_interval_secs: default_heartbeat_interval_secs(),
        }
    }
}

/// Batch processing configuration for streaming pipeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchConfig {
    /// Number of files to scan before yielding to callback
    /// Controls memory usage during directory scanning
    pub scan_batch_size: usize,
    /// Maximum concurrent file parsing tasks
    /// Controls CPU and memory usage during parsing
    pub parse_concurrency: usize,
    /// Maximum concurrent file processing tasks
    /// Includes entity grouping and NL conversion
    pub process_concurrency: usize,
    /// Number of chunks to accumulate before storing
    /// Controls memory during chunking phase
    pub store_batch_size: usize,
    /// Number of texts to send per embedding API call
    /// Should match API token limits (typically 32-64)
    pub embedding_batch_size: usize,
    /// Milliseconds to sleep between embedding batches
    /// Helps avoid API rate limits
    pub embedding_batch_delay_ms: u64,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            scan_batch_size: 100,
            parse_concurrency: 10,
            process_concurrency: 5,
            store_batch_size: 50,
            embedding_batch_size: 32,
            embedding_batch_delay_ms: 100,
        }
    }
}

impl Validate for BatchConfig {
    fn validate_structured(&self) -> ValidationResult {
        let mut errors = Vec::new();

        if self.scan_batch_size == 0 {
            errors.push(ConfigValidationError::invalid_field(
                "scan_batch_size",
                "must be greater than 0",
            ));
        }
        if self.parse_concurrency == 0 {
            errors.push(ConfigValidationError::invalid_field(
                "parse_concurrency",
                "must be greater than 0",
            ));
        }
        if self.process_concurrency == 0 {
            errors.push(ConfigValidationError::invalid_field(
                "process_concurrency",
                "must be greater than 0",
            ));
        }
        if self.store_batch_size == 0 {
            errors.push(ConfigValidationError::invalid_field(
                "store_batch_size",
                "must be greater than 0",
            ));
        }
        if self.embedding_batch_size == 0 {
            errors.push(ConfigValidationError::invalid_field(
                "embedding_batch_size",
                "must be greater than 0",
            ));
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(ConfigValidationError::multiple(errors))
        }
    }
}

impl BatchConfig {
    /// Create config optimized for small projects (< 100 files)
    pub fn small_project() -> Self {
        Self {
            scan_batch_size: 50,
            parse_concurrency: 5,
            process_concurrency: 3,
            store_batch_size: 25,
            embedding_batch_size: 32,
            embedding_batch_delay_ms: 50,
        }
    }

    /// Create config optimized for large projects (> 10000 files)
    pub fn large_project() -> Self {
        Self {
            scan_batch_size: 200,
            parse_concurrency: 20,
            process_concurrency: 10,
            store_batch_size: 100,
            embedding_batch_size: 64,
            embedding_batch_delay_ms: 200,
        }
    }

    /// Create config for low-memory environments
    pub fn low_memory() -> Self {
        Self {
            scan_batch_size: 20,
            parse_concurrency: 3,
            process_concurrency: 2,
            store_batch_size: 10,
            embedding_batch_size: 16,
            embedding_batch_delay_ms: 150,
        }
    }
}

/// Hot update configuration (user-configurable)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotUpdateConfig {
    /// Enable hot update
    #[serde(default = "default_hot_update_enabled")]
    pub enabled: bool,
    /// Debounce configuration
    #[serde(default)]
    pub debounce: DebounceConfig,
    /// Scanner configuration for file discovery during hot updates
    /// None means inherit the global default scanner configuration.
    /// Some(config) overrides scanning behavior for incremental updates vs initial indexing.
    #[serde(default)]
    pub scanner: Option<ScannerConfig>,
    /// Whether to build relations during hot update.
    ///
    /// This flag controls whether the hot-update processor maintains the
    /// relation index when files change. It works in conjunction with:
    /// - `IndexerConfig.build_relations`: Global switch for relation indexing
    /// - `RelationConfig.index.enabled`: Feature-level switch for relations
    ///
    /// The effective flag is: `relation.index.enabled && indexer.build_relations && hot_update.build_relations`
    ///
    /// This allows independent control over full-index vs hot-update relation
    /// building. For example, you can disable relation updates during hot-reload
    /// while still building relations during full index.
    #[serde(default = "default_true")]
    pub build_relations: bool,
    /// Whether to store summaries during update
    #[serde(default = "default_true")]
    pub store_summaries: bool,
    /// Whether to store vectors (embeddings) during update
    #[serde(default = "default_true")]
    pub store_vectors: bool,
    /// Whether to store in BM25 index during update
    #[serde(default = "default_true")]
    pub store_bm25: bool,
    /// Batch size for file processing (number of files to process together)
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    /// Batch size for embedding storage (number of chunks per API call)
    #[serde(default = "default_embedding_batch_size")]
    pub embedding_batch_size: usize,
    /// Batch size for storage operations (number of chunks to store together)
    #[serde(default = "default_storage_batch_size")]
    pub storage_batch_size: usize,
    /// File watch configuration
    #[serde(default)]
    pub file_watch: FileWatchConfig,
}

impl Validate for HotUpdateConfig {
    fn validate_structured(&self) -> ValidationResult {
        let mut errors = Vec::new();

        if self.batch_size == 0 {
            errors.push(ConfigValidationError::invalid_field(
                "batch_size",
                "must be greater than 0",
            ));
        }
        if self.embedding_batch_size == 0 {
            errors.push(ConfigValidationError::invalid_field(
                "embedding_batch_size",
                "must be greater than 0",
            ));
        }
        if self.storage_batch_size == 0 {
            errors.push(ConfigValidationError::invalid_field(
                "storage_batch_size",
                "must be greater than 0",
            ));
        }
        if let Err(e) = self.debounce.validate_structured() {
            errors.push(e);
        }
        if let Err(e) = self.file_watch.validate_structured() {
            errors.push(e);
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(ConfigValidationError::multiple(errors))
        }
    }
}

impl Default for HotUpdateConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            debounce: DebounceConfig::default(),
            scanner: None,
            build_relations: true,
            store_summaries: true,
            store_vectors: true,
            store_bm25: true,
            batch_size: 10,
            embedding_batch_size: 32,
            storage_batch_size: 50,
            file_watch: FileWatchConfig::default(),
        }
    }
}

/// File watch configuration
///
/// Note: Debounce is configured separately in HotUpdateConfig::debounce
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileWatchConfig {
    /// Enable file watching
    pub enabled: bool,
    /// Event storm threshold (events per second)
    pub event_threshold: usize,
    /// Fallback scan interval in seconds (when degraded)
    pub fallback_interval_secs: u64,
    /// Verification interval in seconds
    pub verification_interval_secs: u64,
    /// Watch configuration files
    pub watch_config_files: bool,
    /// Storm duration threshold (seconds) - how long storm must persist before switching
    #[serde(default = "default_storm_duration_secs")]
    pub storm_duration_secs: u64,
    /// Recovery threshold (events per second) - threshold to switch back to file watch
    #[serde(default = "default_recovery_threshold")]
    pub recovery_threshold: usize,
    /// Recovery duration threshold (seconds) - how long recovery must persist
    #[serde(default = "default_recovery_duration_secs")]
    pub recovery_duration_secs: u64,
}

fn default_storm_duration_secs() -> u64 {
    10
}
fn default_recovery_threshold() -> usize {
    50
}
fn default_recovery_duration_secs() -> u64 {
    30
}

// Default value functions for HotUpdateConfig
fn default_hot_update_enabled() -> bool {
    true
}
fn default_batch_size() -> usize {
    10
}
fn default_embedding_batch_size() -> usize {
    32
}
fn default_storage_batch_size() -> usize {
    50
}

impl Validate for FileWatchConfig {
    fn validate_structured(&self) -> ValidationResult {
        let mut errors = Vec::new();

        if self.event_threshold == 0 {
            errors.push(ConfigValidationError::invalid_field(
                "event_threshold",
                "must be greater than 0",
            ));
        }
        if self.fallback_interval_secs == 0 {
            errors.push(ConfigValidationError::invalid_field(
                "fallback_interval_secs",
                "must be greater than 0",
            ));
        }
        if self.verification_interval_secs == 0 {
            errors.push(ConfigValidationError::invalid_field(
                "verification_interval_secs",
                "must be greater than 0",
            ));
        }
        if self.storm_duration_secs == 0 {
            errors.push(ConfigValidationError::invalid_field(
                "storm_duration_secs",
                "must be greater than 0",
            ));
        }
        if self.recovery_duration_secs == 0 {
            errors.push(ConfigValidationError::invalid_field(
                "recovery_duration_secs",
                "must be greater than 0",
            ));
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(ConfigValidationError::multiple(errors))
        }
    }
}

impl Default for FileWatchConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            event_threshold: 100,
            fallback_interval_secs: 30,
            verification_interval_secs: 600,
            watch_config_files: true,
            storm_duration_secs: 10,
            recovery_threshold: 50,
            recovery_duration_secs: 30,
        }
    }
}

/// Debounce configuration
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DebounceConfig {
    /// Short interval when changes are pending in seconds (default: 30)
    pub pending_interval_secs: u64,
    /// Maximum time to wait after a change before triggering in seconds (default: 300)
    pub max_wait_time_secs: u64,
}

impl Validate for DebounceConfig {
    fn validate_structured(&self) -> ValidationResult {
        let mut errors = Vec::new();

        if self.pending_interval_secs == 0 {
            errors.push(ConfigValidationError::invalid_field(
                "pending_interval_secs",
                "must be greater than 0",
            ));
        }
        if self.max_wait_time_secs == 0 {
            errors.push(ConfigValidationError::invalid_field(
                "max_wait_time_secs",
                "must be greater than 0",
            ));
        }
        if self.pending_interval_secs > self.max_wait_time_secs {
            errors.push(ConfigValidationError::dependency_conflict(
                "pending_interval_secs cannot be greater than max_wait_time_secs",
            ));
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(ConfigValidationError::multiple(errors))
        }
    }
}

impl Default for DebounceConfig {
    fn default() -> Self {
        Self {
            pending_interval_secs: 30, // 30 seconds
            max_wait_time_secs: 300,   // 5 minutes
        }
    }
}

/// Cache configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Maximum number of files to cache chunk results
    #[serde(default = "default_chunk_cache_size")]
    pub chunk_cache_size: usize,
    /// Whether caching is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_chunk_cache_size() -> usize {
    100
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            chunk_cache_size: 100,
            enabled: true,
        }
    }
}

/// Indexer configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexerConfig {
    /// File extensions to include in indexing
    #[serde(default = "default_extensions")]
    pub extensions: Vec<String>,
    /// Directories to exclude from indexing
    #[serde(default = "default_exclude_dirs")]
    pub exclude_dirs: Vec<String>,
    /// Store in vector database
    #[serde(default = "default_true")]
    pub store_vectors: bool,
    /// Store in BM25 index
    #[serde(default = "default_true")]
    pub store_bm25: bool,
    /// Store file summaries
    #[serde(default = "default_true")]
    pub store_summaries: bool,
    /// Build relation index during full indexing.
    ///
    /// This flag controls whether the relation index is built during
    /// full (non-incremental) indexing. It works in conjunction with:
    /// - `HotUpdateConfig.build_relations`: Hot-update switch for relations
    /// - `RelationConfig.index.enabled`: Feature-level switch for relations
    ///
    /// The effective flag for full indexing is: `indexer.build_relations && relation.index.enabled`
    /// The effective flag for hot updates is: `relation.index.enabled && indexer.build_relations && hot_update.build_relations`
    ///
    /// This allows independent control over full-index vs hot-update relation
    /// building. For example, you can disable relation updates during hot-reload
    /// while still building relations during full index.
    #[serde(default = "default_true")]
    pub build_relations: bool,
}

fn default_extensions() -> Vec<String> {
    vec![
        "rs".to_string(),
        "py".to_string(),
        "js".to_string(),
        "ts".to_string(),
        "c".to_string(),
        "cpp".to_string(),
        "java".to_string(),
        "go".to_string(),
    ]
}

fn default_exclude_dirs() -> Vec<String> {
    vec![
        "node_modules".to_string(),
        "target".to_string(),
        ".git".to_string(),
        "vendor".to_string(),
        "dist".to_string(),
        "build".to_string(),
    ]
}

impl Default for IndexerConfig {
    fn default() -> Self {
        Self {
            extensions: vec![
                "rs".to_string(),
                "py".to_string(),
                "js".to_string(),
                "ts".to_string(),
                "c".to_string(),
                "cpp".to_string(),
                "java".to_string(),
                "go".to_string(),
            ],
            exclude_dirs: vec![
                "node_modules".to_string(),
                "target".to_string(),
                ".git".to_string(),
                "vendor".to_string(),
                "dist".to_string(),
                "build".to_string(),
            ],
            store_vectors: false,
            store_bm25: false,
            store_summaries: true,
            build_relations: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_orchestrator_config() {
        let config = OrchestratorConfig::default();
        assert_eq!(config.batch.scan_batch_size, 100);
        assert_eq!(config.batch.parse_concurrency, 10);
        assert!(config.hot_update.enabled);
        assert!(config.cache.enabled);
    }

    #[test]
    fn test_batch_config_presets() {
        let small = BatchConfig::small_project();
        assert!(small.scan_batch_size < 100);

        let large = BatchConfig::large_project();
        assert!(large.scan_batch_size > 100);

        let low_mem = BatchConfig::low_memory();
        assert!(low_mem.parse_concurrency < 5);
    }

    #[test]
    fn test_default_hot_update_config() {
        let config = HotUpdateConfig::default();
        assert!(config.enabled);
        assert!(config.build_relations);
        assert!(config.store_summaries);
        assert_eq!(config.batch_size, 10);
    }

    #[test]
    fn test_default_debounce_config() {
        let config = DebounceConfig::default();
        assert_eq!(config.pending_interval_secs, 30);
        assert_eq!(config.max_wait_time_secs, 300);
    }

    #[test]
    fn test_default_cache_config() {
        let config = CacheConfig::default();
        assert_eq!(config.chunk_cache_size, 100);
        assert!(config.enabled);
    }

    #[test]
    fn test_default_indexer_config() {
        let config = IndexerConfig::default();
        assert!(!config.store_vectors);
        assert!(config.build_relations);
    }
}
