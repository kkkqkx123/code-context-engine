//! Project-level configuration
//!
//! This module defines project-specific configuration that can override
//! selected global settings. Project config focuses on indexing, scanning,
//! and processing options, while sensitive settings (API keys, URLs) remain
//! in global configuration.
//!
//! # Configuration Hierarchy
//!
//! ```text
//! Global Config (~/.cce/config.toml)
//!     ↓ inherits
//! Project Config (<project>/.cce/config.toml)
//!     ↓ merges
//! Runtime Config
//! ```
//!
//! # Design Principles
//!
//! - Project config only overrides indexing/scanning/processing options
//! - API keys and URLs are defined globally and referenced by name
//! - Sensitive data stays in global config or environment variables
//! - Project config is optional; global config provides defaults

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::validation::{Validate, ValidationResult};
use cce_types::error::config::ConfigValidationError;

use super::modules::{
    AstToNlConfig, IndexerConfig, NestProcessorConfig, RelationConfig, ScannerConfig, SummaryConfig,
};

// Re-use shared default value functions
use super::modules::defaults::default_true;

// Re-export storage config types from modules for use in project config
pub use super::modules::storage::{
    Bm25AlgorithmConfig, Bm25Config, HnswConfig, IndexManagerConfig, QuantizationConfig,
    VectorStorageConfig, WalConfig,
};

/// Project-level application configuration
///
/// This structure contains only the configuration items that make sense
/// to customize per-project. Sensitive settings (API keys, URLs) are
/// referenced from global configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectAppConfig {
    /// Project name (for display purposes)
    #[serde(default)]
    pub name: Option<String>,

    /// Project root path (optional, defaults to config file directory)
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
    #[serde(default)]
    pub scanner: Option<ScannerConfig>,

    /// Grouper configuration (entity grouping, pattern detection)
    #[serde(default)]
    pub grouper: Option<NestProcessorConfig>,

    /// Orchestrator configuration (indexing, hot update)
    #[serde(default)]
    pub orchestrator: Option<ProjectOrchestratorConfig>,

    /// Relation configuration (call chains, dependencies)
    #[serde(default)]
    pub relation: Option<RelationConfig>,

    /// AST to NL configuration (chunking, conversion)
    #[serde(default)]
    pub ast_to_nl: Option<AstToNlConfig>,

    /// Summary configuration
    #[serde(default)]
    pub summary: Option<SummaryConfig>,

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

/// Project-level embedder configuration
///
/// Allows projects to specify which embedding model to use and override runtime behaviors.
/// Model metadata (dimension, provider) is defined in global config.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectEmbedderConfig {
    /// The specific embedding model to use for this project.
    /// References a model key defined in global [llm.embedding_models].
    #[serde(default)]
    pub model: Option<String>,

    /// Optional preprocessor override for this project.
    /// Useful if the same model needs different prefixes/templates in different projects.
    #[serde(default)]
    pub preprocessor: Option<super::modules::PreprocessorConfig>,
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
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectOrchestratorConfig {
    /// Batch processing configuration
    #[serde(default)]
    pub batch: Option<super::modules::BatchConfig>,

    /// Hot update configuration
    #[serde(default)]
    pub hot_update: Option<super::modules::HotUpdateConfig>,

    /// Indexer configuration (file processing, storage options)
    #[serde(default)]
    pub indexer: Option<IndexerConfig>,

    /// Cache configuration
    #[serde(default)]
    pub cache: Option<super::modules::CacheConfig>,

    /// Checkpoint recovery freshness window and cleanup TTL in seconds
    /// (overrides the global `orchestrator.checkpoint_ttl_seconds`)
    #[serde(default)]
    pub checkpoint_ttl_seconds: Option<u64>,
}

/// Project-level storage configuration
///
/// Allows projects to override storage settings without exposing sensitive data.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectStorageConfig {
    /// Qdrant vector storage configuration override
    #[serde(default)]
    pub qdrant: Option<ProjectQdrantConfig>,

    /// BM25 index configuration override
    #[serde(default)]
    pub bm25: Option<ProjectBm25Config>,

    /// Index manager configuration override
    #[serde(default)]
    pub index_manager: Option<IndexManagerConfigOverride>,
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
#[serde(default)]
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
#[serde(default)]
pub struct VectorStorageConfigOverride {
    /// Store vectors on disk
    /// Reduces memory usage significantly
    pub on_disk: Option<bool>,
}

/// Partial WAL (Write-Ahead Log) configuration override
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct WalConfigOverride {
    /// WAL capacity in MB
    /// Larger capacity reduces flush frequency, improves write performance
    pub capacity_mb: Option<u32>,

    /// Number of WAL segments
    /// More segments improve concurrent write performance
    pub segments: Option<u32>,
}

/// Project-level BM25 configuration override
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectBm25Config {
    /// Enable/disable BM25 indexing
    #[serde(default)]
    pub enabled: Option<bool>,

    /// Index path override (project-specific index location)
    #[serde(default)]
    pub index_path: Option<String>,

    /// BM25 algorithm parameters (k1, b)
    #[serde(default)]
    pub algorithm: Option<Bm25AlgorithmConfig>,
}

/// Index manager configuration override
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexManagerConfigOverride {
    /// Writer memory budget in bytes
    pub writer_memory_budget: usize,

    /// Number of writer threads (None for auto-detection)
    pub writer_num_threads: Option<usize>,

    /// Reload policy: "on_commit", "on_commit_with_delay", "manual"
    pub reload_policy: String,
}

/// Project configuration file locations
///
/// Defines where to look for project configuration files.
pub struct ProjectConfigPaths;

impl ProjectConfigPaths {
    /// Get default project config file name
    pub const fn config_file_name() -> &'static str {
        ".cce.toml"
    }

    /// Get local override config file name
    pub const fn local_config_file_name() -> &'static str {
        ".cce.local.toml"
    }

    /// Get project config directory name
    pub const fn config_dir_name() -> &'static str {
        ".cce"
    }

    /// Find project config file starting from a directory
    ///
    /// Searches upward from the given directory for a `.cce/.cce.toml` file.
    /// Returns the first config file found, using the directory containing
    /// the `.cce` directory as the project root.
    ///
    /// Direct `.cce.toml` in the root directory is not supported — all project
    /// configuration must reside under the `.cce/` directory for consistency
    /// with local override discovery (`.cce/.cce.local.toml`).
    pub fn find_project_config(start_dir: &Path) -> Option<PathBuf> {
        let mut current = Some(start_dir);

        while let Some(dir) = current {
            let config_path = dir
                .join(Self::config_dir_name())
                .join(Self::config_file_name());
            if config_path.exists() {
                return Some(config_path);
            }

            current = dir.parent();
        }

        None
    }

    /// Find local override config file
    ///
    /// Returns the path to `config.local.toml` in the same directory as the project config.
    pub fn find_local_config(project_config_path: &Path) -> Option<PathBuf> {
        project_config_path
            .parent()
            .map(|dir| dir.join(Self::local_config_file_name()))
            .filter(|path| path.exists())
    }
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
        assert!(config.preprocessor.is_none());
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

[preprocessor]
type = "prefix"
prefix = "query: "
"#;
        let config: ProjectEmbedderConfig = toml::from_str(toml_str).expect("Failed to parse");
        assert_eq!(config.model, Some("bge-m3".to_string()));
        // Note: vector_dimension is no longer part of ProjectEmbedderConfig
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
    fn test_find_project_config_in_cce_dir() {
        use std::fs::{self, File};
        use std::io::Write;
        use tempfile::TempDir;

        // Create a temporary directory structure
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cce_dir = temp_dir.path().join(".cce");
        fs::create_dir(&cce_dir).expect("Failed to create .cce dir");

        let config_path = cce_dir.join(ProjectConfigPaths::config_file_name());
        let mut file = File::create(&config_path).expect("Failed to create config");
        writeln!(file, "name = \"test\"").unwrap();

        // Should find config in .cce directory
        let found = ProjectConfigPaths::find_project_config(temp_dir.path());
        assert!(found.is_some());
        assert_eq!(found.unwrap(), config_path);
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
index_path = "./.cce/bm25"

[algorithm]
k1 = 2.0
b = 0.3
"#;
        let config: ProjectBm25Config = toml::from_str(toml_str).expect("Failed to parse");
        assert_eq!(config.enabled, Some(true));
        assert_eq!(config.index_path, Some("./.cce/bm25".to_string()));
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
index_path = "./.cce/bm25"
"#;
        let config: ProjectAppConfig = toml::from_str(toml_str).expect("Failed to parse");
        assert_eq!(config.name, Some("test-project".to_string()));
        assert!(config.storage.is_some());
        let storage = config.storage.unwrap();
        assert!(storage.qdrant.is_some());
        assert!(storage.bm25.is_some());
    }
}
