//! Project-level configuration
//!
//! This module defines the project profile that a client submits through
//! explicit API calls. The server persists the profile in the metadata
//! database and merges it with the global configuration at request time.
//! The server never reads project configuration files.
//!
//! # Configuration Hierarchy
//!
//! ```text
//! Global Config (server config.toml)
//!     ↓ inherits
//! Project profile (metadata database, supplied by client)
//!     ↓ merges
//! Runtime Config
//! ```
//!
//! # Design Principles
//!
//! - Project profile only overrides indexing/scanning/processing options
//! - API keys and URLs are defined globally and referenced by name
//! - Sensitive data stays in global config or environment variables

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::validation::{Validate, ValidationResult};
use cce_types::error::config::ConfigValidationError;

use super::modules::{GlossaryScope, LicenseHeaderRule, SummaryGenerationStrategy};

// Re-use shared default value functions
use super::modules::defaults::default_true;

// Re-export storage config types from modules for use in project config
pub use super::modules::storage::{
    Bm25AlgorithmConfig, Bm25Config, HnswConfig, IndexManagerConfig, LocalVectorConfig,
    QuantizationConfig, VectorStorageConfig, WalConfig,
};

/// Project-level application configuration
///
/// This structure contains only the configuration items that make sense
/// to customize per-project. Sensitive settings (API keys, URLs) are
/// referenced from global configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct ProjectAppConfig {
    /// Project name (for display purposes)
    #[serde(default)]
    pub name: Option<String>,

    /// Project root path (optional, defaults to config file directory)
    ///
    /// Server-side physical path for local supply mode. Remote gateway
    /// projects leave this empty since the server never touches the
    /// filesystem; the registry root_path remains the logical identifier.
    #[serde(default)]
    pub root_path: Option<PathBuf>,

    /// Embedder configuration (model selection, preprocessing)
    /// Only model-related settings; API keys and base_url come from global config
    #[serde(default)]
    pub embedder: Option<ProjectEmbedderConfig>,

    /// LLM configuration for chat/generation and rerank models
    /// Allows projects to specify which models to use for different tasks
    #[serde(default)]
    pub llm: Option<ProjectLlmConfig>,

    /// Scanner configuration (file scanning, filtering)
    ///
    /// Patch form: every field is optional. Absent fields inherit the global
    /// value; only explicitly given fields override it.
    #[serde(default)]
    pub scanner: Option<ProjectScannerPatch>,

    /// Grouper configuration (entity grouping, pattern detection)
    ///
    /// Patch form: every field is optional. Absent fields inherit the global
    /// value; only explicitly given fields override it.
    #[serde(default)]
    pub grouper: Option<ProjectGrouperPatch>,

    /// Orchestrator configuration (indexing, hot update)
    #[serde(default)]
    pub orchestrator: Option<ProjectOrchestratorConfig>,

    /// Relation configuration (call chains, dependencies)
    ///
    /// Patch form: every field is optional. Absent fields inherit the global
    /// value; only explicitly given fields override it.
    #[serde(default)]
    pub relation: Option<ProjectRelationPatch>,

    /// AST to NL configuration (chunking, conversion)
    ///
    /// Patch form: every field is optional. Absent fields inherit the global
    /// value; only explicitly given fields override it.
    #[serde(default)]
    pub ast_to_nl: Option<ProjectAstToNlPatch>,

    /// License header filtering configuration
    ///
    /// Patch form: every field is optional. Absent fields inherit the global
    /// value; only explicitly given fields override it.
    #[serde(default)]
    pub license_header: Option<ProjectLicenseHeaderPatch>,

    /// Summary configuration
    ///
    /// Patch form: every field is optional. Absent fields inherit the global
    /// value; only explicitly given fields override it.
    #[serde(default)]
    pub summary: Option<ProjectSummaryPatch>,

    /// Storage configuration (Qdrant, BM25)
    /// Allows project-specific storage tuning without exposing sensitive data
    #[serde(default)]
    pub storage: Option<ProjectStorageConfig>,

    /// Plugin configuration
    /// Allows project-specific plugin loading and management
    #[serde(default)]
    pub plugins: Option<ProjectPluginConfig>,
}

/// Project-level plugin configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectPluginConfig {
    /// Whether to enable the plugin system for this project
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Path to the plugin registry file (relative to project root)
    /// Default: ".cce/plugins.json"
    #[serde(default)]
    pub registry_file: Option<String>,

    /// Policy when a plugin language claims an extension already owned by a
    /// built-in language (e.g. a plugin declaring `.rs`).
    /// - `warn` (default): load the plugin, log a warning.
    /// - `deny`: refuse to load the conflicting plugin.
    /// - `allow`: load silently (explicit opt-in to override built-in parsing).
    #[serde(default)]
    pub language_extension_conflict: Option<LanguageExtensionConflictPolicy>,

    /// Policy when a plugin grammar's tree-sitter ABI version is incompatible
    /// with the host.
    /// - `deny` (default): refuse to register the plugin language.
    /// - `warn`: register it, log a warning (parse-time validation still applies).
    #[serde(default)]
    pub grammar_abi_policy: Option<GrammarAbiPolicy>,

    /// Policy when a plugin language ships without a `LangHeuristics`
    /// companion scoped to it (no stdlib/test-file/entity-kind provider).
    /// - `warn` (default): register the language, log a warning. Entity
    ///   stdlib classification for its files stays conservative.
    /// - `deny`: refuse to register the plugin language (strict projects
    ///   and CI gates). The refusal is logged as a warning.
    #[serde(default)]
    pub missing_heuristics_policy: Option<MissingHeuristicsPolicy>,
}

/// Policy for plugin extension claims conflicting with built-in languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LanguageExtensionConflictPolicy {
    #[default]
    Warn,
    Deny,
    Allow,
}

/// Policy for plugin grammar tree-sitter ABI version mismatches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GrammarAbiPolicy {
    #[default]
    Deny,
    Warn,
}

/// Policy for plugin languages missing a `LangHeuristics` companion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MissingHeuristicsPolicy {
    #[default]
    Warn,
    Deny,
}

/// Project-level embedder configuration
///
/// Allows projects to specify which embedding model to use and override runtime behaviors.
/// Model metadata (dimension, provider) is defined in global config.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct ProjectEmbedderConfig {
    /// The specific embedding model to use for this project.
    /// References a model key defined in global [llm.embedding_models].
    #[serde(default)]
    pub model: Option<String>,
}

/// Project-level LLM configuration
///
/// Allows projects to specify which models to use for chat/generation and reranking.
/// Model metadata is defined in global config under [llm.chat_models] and [llm.rerank_models].
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectLlmConfig {
    /// The specific chat model to use for this project (e.g., for summary enhancement).
    /// References a model key defined in global [llm.chat_models].
    #[serde(default)]
    pub chat_model: Option<String>,

    /// The specific rerank model to use for this project.
    /// References a model key defined in global [llm.rerank_models].
    #[serde(default)]
    pub rerank_model: Option<String>,

    /// Whether reranking is enabled for this project (overrides global setting).
    #[serde(default)]
    pub enable_rerank: Option<bool>,

    /// Maximum number of candidates to rerank (overrides model default).
    #[serde(default)]
    pub rerank_max_candidates: Option<usize>,
}

/// Project-level orchestrator configuration
///
/// Focuses on indexing and hot update settings, excluding
/// infrastructure concerns.
///
/// Every section is a patch: absent sections and absent fields inherit the
/// global value; only explicitly given fields override it.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectOrchestratorConfig {
    /// Batch processing configuration patch
    #[serde(default)]
    pub batch: Option<ProjectBatchPatch>,

    /// Hot update configuration patch
    #[serde(default)]
    pub hot_update: Option<ProjectHotUpdatePatch>,

    /// Indexer configuration patch (file processing, storage options)
    #[serde(default)]
    pub indexer: Option<ProjectIndexerPatch>,

    /// Cache configuration patch
    #[serde(default)]
    pub cache: Option<ProjectCachePatch>,

    /// Checkpoint recovery freshness window and cleanup TTL in seconds
    /// (overrides the global `orchestrator.checkpoint_ttl_seconds`)
    #[serde(default)]
    pub checkpoint_ttl_seconds: Option<u64>,
}

/// Project-level scanner patch.
///
/// Every field is optional: `None` inherits the global value, `Some`
/// overrides it. This keeps partial profile submissions from resetting
/// sibling fields to defaults.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ProjectScannerPatch {
    pub follow_symlinks: Option<bool>,
    pub respect_gitignore: Option<bool>,
    pub exclude_patterns: Option<Vec<String>>,
    pub include_patterns: Option<Vec<String>>,
    pub gitignore_patterns: Option<Vec<String>>,
    pub binary_check_size: Option<usize>,
    pub max_hash_file_size: Option<u64>,
    pub default_max_content_size: Option<u64>,
    #[serde(
        default,
        deserialize_with = "crate::serde_helpers::explicit_null_as_some_none",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_file_size: Option<Option<u64>>,
    pub plugin_filter_enabled: Option<bool>,
    #[serde(
        default,
        deserialize_with = "crate::serde_helpers::explicit_null_as_some_none",
        skip_serializing_if = "Option::is_none"
    )]
    pub parallelism: Option<Option<usize>>,
}

/// Project-level indexer patch.
///
/// Every field is optional: `None` inherits the global value, `Some`
/// overrides it.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ProjectIndexerPatch {
    pub extensions: Option<Vec<String>>,
    pub exclude_dirs: Option<Vec<String>>,
    pub store_vectors: Option<bool>,
    pub store_bm25: Option<bool>,
    pub store_summaries: Option<bool>,
    pub embed_summaries: Option<bool>,
    pub build_relations: Option<bool>,
    pub dead_letter_truncate_retry: Option<bool>,
    pub embed_input_token_limit: Option<usize>,
}

/// Project-level batch processing patch.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ProjectBatchPatch {
    pub scan_batch_size: Option<usize>,
    pub parse_concurrency: Option<usize>,
    pub process_concurrency: Option<usize>,
    pub store_batch_size: Option<usize>,
    pub embedding_batch_size: Option<usize>,
    pub embedding_batch_delay_ms: Option<u64>,
    pub embedding_stage_timeout_secs: Option<u64>,
}

/// Project-level debounce patch.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ProjectDebouncePatch {
    pub pending_interval_secs: Option<u64>,
    pub max_wait_time_secs: Option<u64>,
}

/// Project-level file watch patch.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ProjectFileWatchPatch {
    pub enabled: Option<bool>,
    pub event_threshold: Option<usize>,
    pub fallback_interval_secs: Option<u64>,
    pub verification_interval_secs: Option<u64>,
    pub watch_config_files: Option<bool>,
    pub storm_duration_secs: Option<u64>,
    pub recovery_threshold: Option<usize>,
    pub recovery_duration_secs: Option<u64>,
}

/// Project-level hot update patch.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ProjectHotUpdatePatch {
    pub enabled: Option<bool>,
    pub debounce: Option<ProjectDebouncePatch>,
    #[serde(
        default,
        deserialize_with = "crate::serde_helpers::explicit_null_as_some_none",
        skip_serializing_if = "Option::is_none"
    )]
    pub scanner: Option<Option<ProjectScannerPatch>>,
    pub build_relations: Option<bool>,
    pub store_summaries: Option<bool>,
    pub store_vectors: Option<bool>,
    pub store_bm25: Option<bool>,
    pub batch_size: Option<usize>,
    pub embedding_batch_size: Option<usize>,
    pub storage_batch_size: Option<usize>,
    pub file_watch: Option<ProjectFileWatchPatch>,
}

/// Project-level cache patch.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ProjectCachePatch {
    pub chunk_cache_size: Option<usize>,
    pub enabled: Option<bool>,
}

/// Project-level grouper patch.
///
/// Every field is optional: `None` inherits the global value, `Some`
/// overrides it.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ProjectGrouperPatch {
    pub small_class_threshold: Option<usize>,
    pub simple_call_merge_threshold: Option<usize>,
    pub enable_class_method_association: Option<bool>,
    pub enable_call_merging: Option<bool>,
    pub enable_getter_setter_merging: Option<bool>,
    pub getter_setter_complexity_threshold: Option<usize>,
    pub enable_test_entity_grouping: Option<bool>,
    pub include_assertions_in_groups: Option<bool>,
    pub max_test_suite_nesting: Option<usize>,
    pub enable_nested_entity_grouping: Option<bool>,
    pub max_nesting_depth: Option<usize>,
    pub min_nested_size: Option<usize>,
    pub enable_impl_association: Option<bool>,
    pub enable_plugin_pattern_detection: Option<bool>,
    pub enable_function_member_grouping: Option<bool>,
    pub enable_small_fragment_merging: Option<bool>,
    pub small_fragment_max_span_lines: Option<usize>,
    pub enable_group_hierarchy: Option<bool>,
    pub near_merge_proximity_bytes: Option<usize>,
    pub getter_setter: Option<ProjectGetterSetterPatch>,
}

/// Project-level getter/setter detection patch.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ProjectGetterSetterPatch {
    pub strict_mode: Option<bool>,
    pub max_simple_lines: Option<usize>,
    pub kotlin_accessor_support: Option<bool>,
}

/// Project-level relation index patch.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct ProjectRelationIndexPatch {
    pub enabled: Option<bool>,
    pub max_relations_per_file: Option<usize>,
    pub resolve_call_chains: Option<bool>,
    pub filter_stdlib_calls: Option<bool>,
}

/// Project-level relation patch.
///
/// Every field is optional: `None` inherits the global value, `Some`
/// overrides it.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct ProjectRelationPatch {
    pub max_call_depth: Option<usize>,
    pub analyze_imports: Option<bool>,
    pub track_cross_file_deps: Option<bool>,
    pub max_propagation_depth: Option<usize>,
    pub max_fingerprint_scope_ratio: Option<f64>,
    pub plugin_symbols_enabled: Option<bool>,
    pub plugin_symbol_extract_enabled: Option<bool>,
    pub manifest_scan_depth: Option<usize>,
    pub index: Option<ProjectRelationIndexPatch>,
}

/// Project-level BM25 generator patch.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ProjectBm25GeneratorPatch {
    pub max_keywords: Option<usize>,
}

/// Project-level embedding generator patch.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ProjectEmbeddingGeneratorPatch {
    pub max_summary_words: Option<usize>,
    pub include_docstring: Option<bool>,
}

/// Project-level chunking patch.
///
/// Every field is optional: `None` inherits the global value, `Some`
/// overrides it.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ProjectChunkingPatch {
    pub max_tokens: Option<usize>,
    pub overlap_tokens: Option<usize>,
    pub max_bm25_terms: Option<usize>,
    pub overlap_bm25_terms: Option<usize>,
    pub max_overlap_ratio: Option<f32>,
    pub min_chunk_tokens: Option<usize>,
    pub min_chunk_bm25_terms: Option<usize>,
    pub respect_boundaries: Option<bool>,
    pub cross_group_merge_threshold: Option<usize>,
    pub glossary_reserve_ratio: Option<f32>,
    pub glossary_max_entries: Option<usize>,
    pub glossary_scope: Option<GlossaryScope>,
}

/// Project-level BM25 text cleaner patch.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ProjectTextCleanerPatch {
    pub redundant_patterns: Option<Vec<String>>,
}

/// Project-level AST to NL patch.
///
/// Every field is optional: `None` inherits the global value, `Some`
/// overrides it.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ProjectAstToNlPatch {
    pub default_mode: Option<cce_types::OutputMode>,
    pub bm25: Option<ProjectBm25GeneratorPatch>,
    pub embedding: Option<ProjectEmbeddingGeneratorPatch>,
    pub chunking: Option<ProjectChunkingPatch>,
    #[serde(
        default,
        deserialize_with = "crate::serde_helpers::explicit_null_as_some_none",
        skip_serializing_if = "Option::is_none"
    )]
    pub document_chunking: Option<Option<ProjectChunkingPatch>>,
    pub text_cleaner: Option<ProjectTextCleanerPatch>,
    pub enable_calls_prune: Option<bool>,
}

/// Project-level license header patch.
///
/// Every field is optional: `None` inherits the global value, `Some`
/// overrides it.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct ProjectLicenseHeaderPatch {
    pub enabled: Option<bool>,
    pub max_comments: Option<usize>,
    pub rules: Option<Vec<LicenseHeaderRule>>,
}

/// Project-level summary patch.
///
/// Every field is optional: `None` inherits the global value, `Some`
/// overrides it.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ProjectSummaryPatch {
    pub strategy: Option<SummaryGenerationStrategy>,
    pub max_summary_length: Option<usize>,
    pub max_entities: Option<usize>,
    pub max_imports: Option<usize>,
    pub max_concurrent: Option<usize>,
    pub max_retries: Option<usize>,
    pub request_timeout_secs: Option<u64>,
}

/// Project-level storage configuration
///
/// Allows projects to override storage settings without exposing sensitive data.
/// Storage location stays global for the shared engine; tuning knobs apply
/// globally and are not isolated per project.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct ProjectStorageConfig {
    /// Qdrant vector storage configuration override
    #[serde(default)]
    pub qdrant: Option<ProjectQdrantConfig>,

    /// Local vector engine tuning override (HNSW only, data_dir stays global).
    #[serde(default)]
    pub vector_local: Option<ProjectLocalVectorConfig>,

    /// BM25 index configuration override
    #[serde(default)]
    pub bm25: Option<ProjectBm25Config>,

    /// Index manager configuration override
    #[serde(default)]
    pub index_manager: Option<IndexManagerConfigOverride>,
}

/// Project-level local vector tuning override.
///
/// Only HNSW knobs are overridable per project; data_dir and dimension stay
/// global so one engine directory serves all projects via group isolation.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct ProjectLocalVectorConfig {
    pub hnsw_m: Option<u32>,
    pub hnsw_ef_construct: Option<u32>,
    pub hnsw_ef_search: Option<usize>,
    pub full_scan_threshold: Option<usize>,
}

/// Project-level Qdrant configuration override
///
/// Only allows overriding non-sensitive parameters.
/// URL, API keys remain in global config.
///
/// Override values are partial patches: each field is optional and, when
/// present, replaces the corresponding effective value derived from the
/// global preset/override chain.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct ProjectQdrantConfig {
    /// Collection preset (overrides global preset)
    #[serde(default)]
    pub preset: Option<super::modules::storage::CollectionPreset>,

    /// Partial HNSW configuration override (applied over global/preset defaults)
    #[serde(default)]
    pub hnsw: Option<HnswConfigOverride>,

    /// Partial vector storage configuration override
    #[serde(default)]
    pub vector_storage: Option<VectorStorageConfigOverride>,

    /// Partial WAL configuration override
    #[serde(default)]
    pub wal: Option<WalConfigOverride>,

    /// Quantization configuration (explicit type discriminator; replaces any global value)
    #[serde(default)]
    pub quantization: Option<QuantizationConfig>,
}

/// Partial HNSW configuration override
///
/// A field-level patch applied over the effective HNSW config derived from
/// the global preset/override chain. When specified, only the provided fields
/// are changed; unset fields keep the effective value.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct HnswConfigOverride {
    /// Number of neighbors per node (2-128)
    /// Higher values improve accuracy but increase memory and build time
    pub m: Option<u32>,

    /// Search range during index construction (10-1000)
    /// Higher values improve index quality but increase build time
    pub ef_construct: Option<u32>,

    /// Store HNSW index on disk
    /// Reduces memory usage but may impact search performance
    pub on_disk: Option<bool>,

    /// Additional HNSW connections for payload-aware routing
    pub payload_m: Option<u32>,

    /// Store vector copies directly in HNSW index files (v1.16.0+)
    /// Improves search speed but increases storage (3-4x)
    /// Requires quantization to be enabled
    pub inline_storage: Option<bool>,
}

/// Partial vector storage configuration override
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct VectorStorageConfigOverride {
    /// Store vectors on disk
    /// Reduces memory usage significantly
    pub on_disk: Option<bool>,
}

/// Partial WAL (Write-Ahead Log) configuration override
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct WalConfigOverride {
    /// WAL capacity in MB
    /// Larger capacity reduces flush frequency, improves write performance
    pub capacity_mb: Option<u32>,

    /// Number of WAL segments
    /// More segments improve concurrent write performance
    pub segments: Option<u32>,
}

/// Project-level BM25 configuration override.
///
/// Index location stays global so one shared index serves all projects via
/// project isolation. Only algorithm and enable flag are overridable per
/// project; per-project tuning applies globally to the shared engine.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct ProjectBm25Config {
    /// Enable/disable BM25 indexing
    #[serde(default)]
    pub enabled: Option<bool>,

    /// BM25 algorithm parameters (k1, b)
    #[serde(default)]
    pub algorithm: Option<Bm25AlgorithmConfig>,
}

/// Index manager configuration override
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndexManagerConfigOverride {
    /// Writer memory budget in bytes
    pub writer_memory_budget: usize,

    /// Number of writer threads (None for auto-detection)
    pub writer_num_threads: Option<usize>,

    /// Reload policy: "on_commit", "on_commit_with_delay", "manual"
    pub reload_policy: String,
}

impl Validate for ProjectStorageConfig {
    fn validate_structured(&self) -> ValidationResult {
        let mut errors = Vec::new();

        if let Some(ref qdrant) = self.qdrant {
            if let Err(e) = qdrant.validate_structured() {
                errors.push(e);
            }
        }
        if let Some(ref bm25) = self.bm25 {
            if let Err(e) = bm25.validate_structured() {
                errors.push(e);
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(ConfigValidationError::multiple(errors))
        }
    }
}

impl Validate for ProjectQdrantConfig {
    fn validate_structured(&self) -> ValidationResult {
        let mut errors = Vec::new();

        if let Some(ref hnsw) = self.hnsw {
            if let Err(e) = hnsw.validate_structured() {
                errors.push(e);
            }
        }
        if let Some(ref quant) = self.quantization {
            if let Err(e) = quant.validate_structured() {
                errors.push(e);
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(ConfigValidationError::multiple(errors))
        }
    }
}

impl Validate for HnswConfigOverride {
    fn validate_structured(&self) -> ValidationResult {
        HnswConfig::default().apply(self).validate_structured()
    }
}

impl Validate for ProjectAppConfig {
    fn validate_structured(&self) -> ValidationResult {
        let mut errors = Vec::new();
        if let Some(ref storage) = self.storage {
            if let Err(error) = storage.validate_structured() {
                errors.push(error);
            }
        }
        if let Some(ref orchestrator) = self.orchestrator {
            if let Some(ref batch) = orchestrator.batch {
                if let Err(error) = batch.validate_structured() {
                    errors.push(error);
                }
            }
            if let Some(ref hot_update) = orchestrator.hot_update {
                if let Err(error) = hot_update.validate_structured() {
                    errors.push(error);
                }
            }
        }
        if let Some(ref relation) = self.relation {
            if let Err(error) = relation.validate_structured() {
                errors.push(error);
            }
        }
        if let Some(ref ast_to_nl) = self.ast_to_nl {
            if let Err(error) = ast_to_nl.validate_structured() {
                errors.push(error);
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(ConfigValidationError::multiple(errors))
        }
    }
}

fn require_positive(field: &str, value: usize, errors: &mut Vec<ConfigValidationError>) {
    if value == 0 {
        errors.push(ConfigValidationError::invalid_field(
            field,
            "must be greater than 0",
        ));
    }
}

fn require_positive_u64(field: &str, value: u64, errors: &mut Vec<ConfigValidationError>) {
    if value == 0 {
        errors.push(ConfigValidationError::invalid_field(
            field,
            "must be greater than 0",
        ));
    }
}

impl Validate for ProjectBatchPatch {
    fn validate_structured(&self) -> ValidationResult {
        let mut errors = Vec::new();
        if let Some(value) = self.scan_batch_size {
            require_positive("scan_batch_size", value, &mut errors);
        }
        if let Some(value) = self.parse_concurrency {
            require_positive("parse_concurrency", value, &mut errors);
        }
        if let Some(value) = self.process_concurrency {
            require_positive("process_concurrency", value, &mut errors);
        }
        if let Some(value) = self.store_batch_size {
            require_positive("store_batch_size", value, &mut errors);
        }
        if let Some(value) = self.embedding_batch_size {
            require_positive("embedding_batch_size", value, &mut errors);
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(ConfigValidationError::multiple(errors))
        }
    }
}

impl Validate for ProjectDebouncePatch {
    fn validate_structured(&self) -> ValidationResult {
        let mut errors = Vec::new();
        if let Some(value) = self.pending_interval_secs {
            require_positive_u64("pending_interval_secs", value, &mut errors);
        }
        if let Some(value) = self.max_wait_time_secs {
            require_positive_u64("max_wait_time_secs", value, &mut errors);
        }
        if let (Some(pending), Some(max)) = (self.pending_interval_secs, self.max_wait_time_secs)
            && pending > max
        {
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

impl Validate for ProjectFileWatchPatch {
    fn validate_structured(&self) -> ValidationResult {
        let mut errors = Vec::new();
        if let Some(value) = self.event_threshold {
            require_positive("event_threshold", value, &mut errors);
        }
        if let Some(value) = self.fallback_interval_secs {
            require_positive_u64("fallback_interval_secs", value, &mut errors);
        }
        if let Some(value) = self.verification_interval_secs {
            require_positive_u64("verification_interval_secs", value, &mut errors);
        }
        if let Some(value) = self.storm_duration_secs {
            require_positive_u64("storm_duration_secs", value, &mut errors);
        }
        if let Some(value) = self.recovery_duration_secs {
            require_positive_u64("recovery_duration_secs", value, &mut errors);
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(ConfigValidationError::multiple(errors))
        }
    }
}

impl Validate for ProjectHotUpdatePatch {
    fn validate_structured(&self) -> ValidationResult {
        let mut errors = Vec::new();
        if let Some(value) = self.batch_size {
            require_positive("batch_size", value, &mut errors);
        }
        if let Some(value) = self.embedding_batch_size {
            require_positive("embedding_batch_size", value, &mut errors);
        }
        if let Some(value) = self.storage_batch_size {
            require_positive("storage_batch_size", value, &mut errors);
        }
        if let Some(ref debounce) = self.debounce {
            if let Err(error) = debounce.validate_structured() {
                errors.push(error);
            }
        }
        if let Some(ref file_watch) = self.file_watch {
            if let Err(error) = file_watch.validate_structured() {
                errors.push(error);
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(ConfigValidationError::multiple(errors))
        }
    }
}

impl Validate for ProjectRelationIndexPatch {
    fn validate_structured(&self) -> ValidationResult {
        Ok(())
    }
}

impl Validate for ProjectRelationPatch {
    fn validate_structured(&self) -> ValidationResult {
        let mut errors = Vec::new();
        if let Err(error) = self
            .index
            .as_ref()
            .map(|index| index.validate_structured())
            .unwrap_or(Ok(()))
        {
            errors.push(error);
        }
        if let Some(0) = self.max_call_depth
            && self.index.as_ref().and_then(|index| index.enabled) != Some(false)
        {
            errors.push(ConfigValidationError::invalid_field(
                "max_call_depth",
                "must be greater than 0 when index is enabled",
            ));
        }
        if let Some(ratio) = self.max_fingerprint_scope_ratio
            && !(0.0..=1.0).contains(&ratio)
        {
            errors.push(ConfigValidationError::out_of_range(
                "max_fingerprint_scope_ratio",
                ratio.to_string(),
                "0.0",
                "1.0",
            ));
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(ConfigValidationError::multiple(errors))
        }
    }
}

impl Validate for ProjectBm25GeneratorPatch {
    fn validate_structured(&self) -> ValidationResult {
        if let Some(0) = self.max_keywords {
            return Err(ConfigValidationError::invalid_field(
                "bm25.max_keywords",
                "must be greater than 0",
            ));
        }
        Ok(())
    }
}

impl Validate for ProjectEmbeddingGeneratorPatch {
    fn validate_structured(&self) -> ValidationResult {
        if let Some(0) = self.max_summary_words {
            return Err(ConfigValidationError::invalid_field(
                "embedding.max_summary_words",
                "must be greater than 0",
            ));
        }
        Ok(())
    }
}

impl Validate for ProjectChunkingPatch {
    fn validate_structured(&self) -> ValidationResult {
        let mut errors = Vec::new();
        if let Some(0) = self.max_tokens {
            errors.push(ConfigValidationError::invalid_field(
                "chunking.max_tokens",
                "must be greater than 0",
            ));
        }
        if let Some(0) = self.max_bm25_terms {
            errors.push(ConfigValidationError::invalid_field(
                "chunking.max_bm25_terms",
                "must be greater than 0",
            ));
        }
        if let Some(0) = self.min_chunk_tokens {
            errors.push(ConfigValidationError::invalid_field(
                "chunking.min_chunk_tokens",
                "must be greater than 0",
            ));
        }
        if let Some(0) = self.min_chunk_bm25_terms {
            errors.push(ConfigValidationError::invalid_field(
                "chunking.min_chunk_bm25_terms",
                "must be greater than 0",
            ));
        }
        if let Some(overlap) = self.overlap_tokens
            && let Some(max) = self.max_tokens
            && overlap >= max
        {
            errors.push(ConfigValidationError::invalid_field(
                "chunking.overlap_tokens",
                "must be less than max_tokens",
            ));
        }
        if let Some(overlap) = self.overlap_bm25_terms
            && let Some(max) = self.max_bm25_terms
            && overlap >= max
        {
            errors.push(ConfigValidationError::invalid_field(
                "chunking.overlap_bm25_terms",
                "must be less than max_bm25_terms",
            ));
        }
        if let Some(min) = self.min_chunk_tokens
            && let Some(max) = self.max_tokens
            && min > max
        {
            errors.push(ConfigValidationError::invalid_field(
                "chunking.min_chunk_tokens",
                "must be no greater than max_tokens",
            ));
        }
        if let Some(min) = self.min_chunk_bm25_terms
            && let Some(max) = self.max_bm25_terms
            && min > max
        {
            errors.push(ConfigValidationError::invalid_field(
                "chunking.min_chunk_bm25_terms",
                "must be no greater than max_bm25_terms",
            ));
        }
        if let Some(ratio) = self.max_overlap_ratio
            && (ratio <= 0.0 || ratio > 1.0)
        {
            errors.push(ConfigValidationError::out_of_range(
                "chunking.max_overlap_ratio",
                ratio.to_string(),
                "0",
                "1",
            ));
        }
        if let Some(ratio) = self.glossary_reserve_ratio
            && !(0.0..1.0).contains(&ratio)
        {
            errors.push(ConfigValidationError::out_of_range(
                "chunking.glossary_reserve_ratio",
                ratio.to_string(),
                "0",
                "1",
            ));
        }
        if let Some(threshold) = self.cross_group_merge_threshold
            && let Some(max) = self.max_tokens
            && threshold != 0
            && threshold > max
        {
            errors.push(ConfigValidationError::invalid_field(
                "chunking.cross_group_merge_threshold",
                "must be 0 or no greater than max_tokens",
            ));
        }
        if let Some(0) = self.glossary_max_entries {
            errors.push(ConfigValidationError::invalid_field(
                "chunking.glossary_max_entries",
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

impl Validate for ProjectTextCleanerPatch {
    fn validate_structured(&self) -> ValidationResult {
        Ok(())
    }
}

impl Validate for ProjectAstToNlPatch {
    fn validate_structured(&self) -> ValidationResult {
        let mut errors = Vec::new();
        if let Some(ref bm25) = self.bm25 {
            if let Err(error) = bm25.validate_structured() {
                errors.push(error);
            }
        }
        if let Some(ref embedding) = self.embedding {
            if let Err(error) = embedding.validate_structured() {
                errors.push(error);
            }
        }
        if let Some(ref chunking) = self.chunking {
            if let Err(error) = chunking.validate_structured() {
                errors.push(error);
            }
        }
        if let Some(Some(ref chunking)) = self.document_chunking {
            if let Err(error) = chunking.validate_structured() {
                errors.push(error);
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(ConfigValidationError::multiple(errors))
        }
    }
}

impl ProjectAppConfig {
    pub fn is_empty(&self) -> bool {
        self.name.is_none()
            && self.root_path.is_none()
            && self.embedder.is_none()
            && self.llm.is_none()
            && self.scanner.is_none()
            && self.grouper.is_none()
            && self.orchestrator.is_none()
            && self.relation.is_none()
            && self.ast_to_nl.is_none()
            && self.license_header.is_none()
            && self.summary.is_none()
            && self.storage.is_none()
            && self.plugins.is_none()
    }

    pub fn to_canonical_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }

    pub fn from_json_str(raw: &str) -> Result<Self, String> {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed == "{}" {
            return Ok(Self::default());
        }
        serde_json::from_str(trimmed).map_err(|error| format!("Invalid project profile: {error}"))
    }

    pub fn from_filter_args(
        extensions: Vec<String>,
        exclude_dirs: Vec<String>,
        respect_gitignore: Option<bool>,
        ignore_patterns: Vec<String>,
    ) -> Self {
        let extensions_opt = if extensions.is_empty() {
            None
        } else {
            Some(extensions)
        };
        let exclude_opt = if exclude_dirs.is_empty() {
            None
        } else {
            Some(exclude_dirs)
        };
        let ignore_opt = if ignore_patterns.is_empty() {
            None
        } else {
            Some(ignore_patterns)
        };
        Self::from_scan_lists(extensions_opt, exclude_opt, respect_gitignore, ignore_opt)
    }

    pub fn from_scan_lists(
        extensions: Option<Vec<String>>,
        exclude_dirs: Option<Vec<String>>,
        respect_gitignore: Option<bool>,
        ignore_patterns: Option<Vec<String>>,
    ) -> Self {
        let mut profile = Self::default();
        if extensions.is_some() || exclude_dirs.is_some() {
            let mut indexer = ProjectIndexerPatch::default();
            if let Some(value) = extensions {
                indexer.extensions = Some(value);
            }
            if let Some(value) = exclude_dirs {
                indexer.exclude_dirs = Some(value);
            }
            profile.orchestrator = Some(ProjectOrchestratorConfig {
                indexer: Some(indexer),
                ..Default::default()
            });
        }
        if respect_gitignore.is_some() || ignore_patterns.is_some() {
            let mut scanner = ProjectScannerPatch::default();
            if let Some(value) = respect_gitignore {
                scanner.respect_gitignore = Some(value);
            }
            if let Some(value) = ignore_patterns {
                scanner.gitignore_patterns = Some(value);
            }
            profile.scanner = Some(scanner);
        }
        profile
    }
}

impl HnswConfig {
    /// Apply a partial override patch onto this resolved configuration
    pub fn apply(&self, patch: &HnswConfigOverride) -> Self {
        Self {
            m: patch.m.unwrap_or(self.m),
            ef_construct: patch.ef_construct.unwrap_or(self.ef_construct),
            on_disk: patch.on_disk.unwrap_or(self.on_disk),
            payload_m: patch.payload_m.or(self.payload_m),
            inline_storage: patch.inline_storage.or(self.inline_storage),
        }
    }
}

impl WalConfig {
    /// Apply a partial override patch onto this resolved configuration
    pub fn apply(&self, patch: &WalConfigOverride) -> Self {
        Self {
            capacity_mb: patch.capacity_mb.unwrap_or(self.capacity_mb),
            segments: patch.segments.unwrap_or(self.segments),
        }
    }
}

impl VectorStorageConfig {
    /// Apply a partial override patch onto this resolved configuration
    pub fn apply(&self, patch: &VectorStorageConfigOverride) -> Self {
        Self {
            on_disk: patch.on_disk.unwrap_or(self.on_disk),
        }
    }
}

impl Validate for ProjectBm25Config {
    fn validate_structured(&self) -> ValidationResult {
        Ok(())
    }
}

impl Validate for IndexManagerConfigOverride {
    fn validate_structured(&self) -> ValidationResult {
        let valid_policies = ["on_commit", "on_commit_with_delay", "manual"];
        if !valid_policies.contains(&self.reload_policy.as_str()) {
            return Err(ConfigValidationError::invalid_field(
                "reload_policy",
                format!("must be one of {:?}", valid_policies),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_config_default() {
        let config = ProjectAppConfig::default();
        assert!(config.name.is_none());
        assert!(config.root_path.is_none());
        assert!(config.scanner.is_none());
    }

    #[test]
    fn test_project_config_serialization() {
        let toml_str = r#"
name = "test-project"

[scanner]
follow_symlinks = false
respect_gitignore = true
exclude_patterns = ["node_modules", "dist"]
include_patterns = []
gitignore_patterns = []
binary_check_size = 8192
max_hash_file_size = 10485760
default_max_content_size = 1048576
max_file_size = 512000
"#;
        let config: ProjectAppConfig = toml::from_str(toml_str).expect("Failed to parse");
        assert_eq!(config.name, Some("test-project".to_string()));
        assert!(config.scanner.is_some());
    }

    #[test]
    fn test_partial_scanner_patch_keeps_other_fields_absent() {
        let config: ProjectAppConfig = toml::from_str(
            r#"[scanner]
respect_gitignore = false
"#,
        )
        .expect("partial patch must parse");
        let scanner = config.scanner.expect("scanner patch must be present");
        assert_eq!(scanner.respect_gitignore, Some(false));
        assert_eq!(scanner.follow_symlinks, None);
        assert_eq!(scanner.exclude_patterns, None);
        assert_eq!(scanner.max_file_size, None);
    }

    #[test]
    fn test_explicit_null_clears_nullable_patch_field() {
        let config: ProjectAppConfig = toml::from_str(
            r#"[scanner]
max_file_size = 512000
"#,
        )
        .expect("valued patch must parse");
        let scanner = config.scanner.expect("scanner patch must be present");
        assert_eq!(scanner.max_file_size, Some(Some(512000)));

        let cleared: ProjectAppConfig =
            serde_json::from_str(r#"{"scanner": {"max_file_size": null}}"#)
                .expect("explicit null must parse");
        let scanner = cleared.scanner.expect("scanner patch must be present");
        assert_eq!(scanner.max_file_size, Some(None));

        let omitted: ProjectAppConfig =
            serde_json::from_str(r#"{"scanner": {}}"#).expect("empty patch must parse");
        let scanner = omitted.scanner.expect("scanner patch must be present");
        assert_eq!(scanner.max_file_size, None);
    }

    #[test]
    fn test_patch_json_roundtrip_preserves_explicit_null() {
        let profile = ProjectAppConfig {
            scanner: Some(ProjectScannerPatch {
                max_file_size: Some(None),
                respect_gitignore: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        };
        let raw = profile.to_canonical_json();
        let restored = ProjectAppConfig::from_json_str(&raw).expect("canonical JSON must parse");
        let scanner = restored.scanner.expect("scanner patch must survive");
        assert_eq!(scanner.max_file_size, Some(None));
        assert_eq!(scanner.respect_gitignore, Some(true));
    }

    #[test]
    fn test_project_orchestrator_config_default() {
        let config = ProjectOrchestratorConfig::default();
        assert!(config.batch.is_none());
        assert!(config.hot_update.is_none());
        assert!(config.indexer.is_none());
    }

    #[test]
    fn test_project_embedder_config_default() {
        let config = ProjectEmbedderConfig::default();
        assert!(config.model.is_none());
    }

    #[test]
    fn test_project_llm_config_default() {
        let config = ProjectLlmConfig::default();
        assert!(config.chat_model.is_none());
        assert!(config.rerank_model.is_none());
        assert!(config.enable_rerank.is_none());
        assert!(config.rerank_max_candidates.is_none());
    }

    #[test]
    fn test_project_llm_config_serialization() {
        let toml_str = r#"
chat_model = "gpt-4o"
rerank_model = "gpt-4o-mini-rerank"
enable_rerank = true
rerank_max_candidates = 30
"#;
        let config: ProjectLlmConfig = toml::from_str(toml_str).expect("Failed to parse");
        assert_eq!(config.chat_model, Some("gpt-4o".to_string()));
        assert_eq!(config.rerank_model, Some("gpt-4o-mini-rerank".to_string()));
        assert_eq!(config.enable_rerank, Some(true));
        assert_eq!(config.rerank_max_candidates, Some(30));
    }

    #[test]
    fn test_project_embedder_config_serialization() {
        let toml_str = r#"
model = "bge-m3"
"#;
        let config: ProjectEmbedderConfig = toml::from_str(toml_str).expect("Failed to parse");
        assert_eq!(config.model, Some("bge-m3".to_string()));
    }

    #[test]
    fn test_project_config_with_embedder() {
        let toml_str = r#"
name = "test-project"

[embedder]
model = "all-MiniLM-L6-v2"

[scanner]
follow_symlinks = false
respect_gitignore = true
exclude_patterns = ["node_modules", "dist"]
include_patterns = []
gitignore_patterns = []
binary_check_size = 8192
max_hash_file_size = 10485760
default_max_content_size = 1048576
max_file_size = 512000
"#;
        let config: ProjectAppConfig = toml::from_str(toml_str).expect("Failed to parse");
        assert_eq!(config.name, Some("test-project".to_string()));
        assert!(config.embedder.is_some());
        let embedder = config.embedder.unwrap();
        assert_eq!(embedder.model, Some("all-MiniLM-L6-v2".to_string()));
        // Note: vector_dimension is no longer part of ProjectEmbedderConfig
    }

    #[test]
    fn test_project_storage_config_default() {
        let config = ProjectStorageConfig::default();
        assert!(config.qdrant.is_none());
        assert!(config.bm25.is_none());
    }

    #[test]
    fn test_project_qdrant_config_serialization() {
        let toml_str = r#"
preset = "large"

[hnsw]
m = 64
ef_construct = 512
on_disk = true
inline_storage = true

[quantization]
type = "scalar"
quant_type = "int8"
quantile = 0.99
always_ram = false
"#;
        let config: ProjectQdrantConfig = toml::from_str(toml_str).expect("Failed to parse");
        assert!(config.preset.is_some());
        assert!(config.hnsw.is_some());
        assert!(config.quantization.is_some());
    }

    #[test]
    fn test_quantization_config_scalar() {
        let toml_str = r#"
type = "scalar"
quant_type = "int8"
quantile = 0.99
always_ram = false
"#;
        let config: QuantizationConfig = toml::from_str(toml_str).expect("Failed to parse");
        match config {
            QuantizationConfig::Scalar(config) => {
                assert_eq!(config.quant_type, "int8");
                assert_eq!(config.quantile, 0.99);
                assert!(!config.always_ram);
            }
            _ => panic!("Expected Scalar variant"),
        }
    }

    #[test]
    fn test_quantization_config_product() {
        let toml_str = r#"
type = "product"
compression = "x64"
always_ram = false
"#;
        let config: QuantizationConfig = toml::from_str(toml_str).expect("Failed to parse");
        match config {
            QuantizationConfig::Product(config) => {
                assert_eq!(config.compression, "x64");
                assert!(!config.always_ram);
            }
            _ => panic!("Expected Product variant"),
        }
    }

    #[test]
    fn test_quantization_config_disabled() {
        let toml_str = r#"type = "disabled""#;
        let config: QuantizationConfig = toml::from_str(toml_str).expect("Failed to parse");
        match config {
            QuantizationConfig::Disabled => {}
            _ => panic!("Expected Disabled variant"),
        }
    }

    #[test]
    fn test_hnsw_config_validation() {
        let hnsw = HnswConfigOverride {
            m: Some(64),
            ef_construct: Some(512),
            on_disk: Some(true),
            payload_m: Some(64),
            inline_storage: Some(true),
        };
        assert!(hnsw.validate_structured().is_ok());

        let invalid_hnsw = HnswConfigOverride {
            m: Some(200), // Invalid: > 128
            ef_construct: Some(512),
            on_disk: Some(true),
            payload_m: Some(200),
            inline_storage: None,
        };
        assert!(invalid_hnsw.validate_structured().is_err());
    }

    #[test]
    fn test_hnsw_patch_partial_apply() {
        // A partial patch must only override the fields it sets, keeping the
        // effective base values for the rest.
        let base = HnswConfig::medium();
        let patch = HnswConfigOverride {
            m: Some(64),
            ..Default::default()
        };
        let resolved = base.apply(&patch);
        assert_eq!(resolved.m, 64);
        assert_eq!(resolved.ef_construct, base.ef_construct);
        assert_eq!(resolved.on_disk, base.on_disk);
        assert_eq!(resolved.payload_m, base.payload_m);
    }

    #[test]
    fn test_wal_patch_partial_apply() {
        let base = WalConfig::medium();
        let patch = WalConfigOverride {
            capacity_mb: Some(128),
            ..Default::default()
        };
        let resolved = base.apply(&patch);
        assert_eq!(resolved.capacity_mb, 128);
        assert_eq!(resolved.segments, base.segments);
    }

    #[test]
    fn test_project_bm25_config_serialization() {
        let toml_str = r#"
enabled = true

[algorithm]
k1 = 2.0
b = 0.3
"#;
        let config: ProjectBm25Config = toml::from_str(toml_str).expect("Failed to parse");
        assert_eq!(config.enabled, Some(true));
        assert!(config.algorithm.is_some());
    }

    #[test]
    fn test_index_manager_config_override() {
        let toml_str = r#"
writer_memory_budget = 100000000
writer_num_threads = 4
reload_policy = "on_commit_with_delay"
"#;
        let config: IndexManagerConfigOverride = toml::from_str(toml_str).expect("Failed to parse");
        assert_eq!(config.writer_memory_budget, 100000000);
        assert_eq!(config.writer_num_threads, Some(4));
        assert_eq!(config.reload_policy, "on_commit_with_delay");
    }

    #[test]
    fn test_project_config_with_storage() {
        let toml_str = r#"
name = "test-project"

[storage.qdrant]
preset = "large"

[storage.qdrant.hnsw]
m = 64
ef_construct = 512
on_disk = true

[storage.qdrant.quantization]
type = "scalar"
quant_type = "int8"
quantile = 0.99
always_ram = false

[storage.bm25]
enabled = true
"#;
        let config: ProjectAppConfig = toml::from_str(toml_str).expect("Failed to parse");
        assert_eq!(config.name, Some("test-project".to_string()));
        assert!(config.storage.is_some());
        let storage = config.storage.unwrap();
        assert!(storage.qdrant.is_some());
        assert!(storage.bm25.is_some());
    }
}
