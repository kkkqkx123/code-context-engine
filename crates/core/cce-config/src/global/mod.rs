//! Global configuration types
//!
//! This module defines the global configuration structure that integrates
//! all module-specific configurations.
//!
//! # Single Entry Point
//!
//! `AppConfig` is the only entry point for user configuration.
//! All module configurations are defined in `config/modules/`.
//!
//! # Configuration Merging
//!
//! Global configuration can be merged with project-level configuration
//! to create runtime configuration. See `merge_with_project()` method.

mod logging;
mod merge;
mod resolved;
mod sqlite;

pub use logging::{LogFormat, LogLevel, LogOutput, LoggingConfig};
pub use resolved::{ResolvedChatConfig, ResolvedEmbeddingConfig, ResolvedLlmConnection};
pub use sqlite::{SqliteConfig, SqliteSyncMode};

use serde::{Deserialize, Serialize};

use crate::modules::{
    AstToNlConfig, EmbedderConfig, ExportModuleConfig, GlobalCacheConfig, NestProcessorConfig,
    OrchestratorConfig, ProviderConfig, RelationConfig, RerankConfig, ScannerConfig,
    SearchModuleConfig, SummaryConfig, SymbolResolutionConfig,
};
use crate::modules::{Bm25Config, QdrantConfig};
use crate::modules::{ChatModelConfig, EmbeddingModelConfig, RerankModelConfig};
use crate::validation::{ConfigWarning, Validate, ValidationResult};
use cce_types::error::config::ConfigValidationError;

/// Database configuration (combines Qdrant, SQLite, and BM25)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DatabaseConfig {
    /// Qdrant vector database configuration
    pub qdrant: QdrantConfig,
    /// SQLite metadata database configuration
    pub sqlite: SqliteConfig,
    /// BM25 index configuration
    pub bm25: Bm25Config,
}

/// Global application configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    /// Server configuration
    #[serde(default)]
    pub server: ServerConfig,
    /// Database configuration
    #[serde(default)]
    pub database: DatabaseConfig,
    /// Embedder configuration (for vector embeddings)
    #[serde(default)]
    pub embedder: EmbedderConfig,
    /// LLM configuration (for chat/completion, used by summary enhancement)
    #[serde(default)]
    pub llm: LlmConfigSection,
    /// Scanner configuration
    #[serde(default)]
    pub scanner: ScannerConfig,
    /// Grouper configuration
    #[serde(default)]
    pub grouper: NestProcessorConfig,
    /// Logging configuration
    #[serde(default)]
    pub logger: LoggingConfig,
    /// Orchestrator configuration (includes indexer config)
    #[serde(default)]
    pub orchestrator: OrchestratorConfig,
    /// Relation configuration
    #[serde(default)]
    pub relation: RelationConfig,
    /// Symbol resolution configuration
    #[serde(default)]
    pub symbol_resolution: SymbolResolutionConfig,
    /// AST to NL configuration
    #[serde(default)]
    pub ast_to_nl: AstToNlConfig,
    /// Summary configuration
    #[serde(default)]
    pub summary: SummaryConfig,
    /// Export configuration
    #[serde(default)]
    pub export: ExportModuleConfig,
    /// Rerank configuration
    #[serde(default)]
    pub rerank: RerankConfig,
    /// Metrics configuration
    #[serde(default)]
    pub metrics: MetricsConfig,

    /// Plugin configuration
    #[serde(default)]
    pub plugins: crate::project::ProjectPluginConfig,
    /// Search configuration (search pipeline parameters)
    #[serde(default)]
    pub search: SearchModuleConfig,
    /// Unified cache configuration
    #[serde(default)]
    pub cache: GlobalCacheConfig,
}

/// LLM configuration section
///
/// Unified provider and model configuration for all LLM services (embedding, chat, rerank).
/// API keys should be injected via environment variables (e.g., OPENAI_API_KEY).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LlmConfigSection {
    /// Whether LLM features are enabled
    #[serde(default)]
    pub enabled: bool,

    /// Provider registry - connection details indexed by provider ID
    #[serde(default)]
    pub providers: std::collections::HashMap<String, ProviderConfig>,

    /// Embedding model registry
    #[serde(default)]
    pub embedding_models: std::collections::HashMap<String, EmbeddingModelConfig>,

    /// Chat model registry
    #[serde(default)]
    pub chat_models: std::collections::HashMap<String, ChatModelConfig>,

    /// Rerank model registry
    #[serde(default)]
    pub rerank_models: std::collections::HashMap<String, RerankModelConfig>,

    /// Default model selections
    #[serde(default)]
    pub defaults: ModelDefaults,
}

/// Default model selections
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelDefaults {
    /// Default embedding model
    #[serde(default)]
    pub embedding: Option<String>,

    /// Default chat model
    #[serde(default)]
    pub chat: Option<String>,

    /// Default rerank model
    #[serde(default)]
    pub rerank: Option<String>,
}

/// Server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    /// Server host
    pub host: String,
    /// Server port
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 9000,
        }
    }
}

impl Validate for ServerConfig {
    fn validate_structured(&self) -> ValidationResult {
        let mut errors = Vec::new();

        if self.port == 0 {
            errors.push(ConfigValidationError::invalid_field(
                "port",
                "must be greater than 0",
            ));
        }
        if self.host.is_empty() {
            errors.push(ConfigValidationError::missing_field("host"));
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(ConfigValidationError::multiple(errors))
        }
    }
}

impl Validate for AppConfig {
    fn validate_structured(&self) -> ValidationResult {
        let mut errors = Vec::new();

        if let Err(e) = self.server.validate_structured() {
            errors.push(e);
        }

        if let Err(e) = self.database.qdrant.validate_structured() {
            errors.push(e);
        }

        if let Err(e) = self.embedder.validate_structured() {
            errors.push(e);
        }

        for (provider_id, provider) in &self.llm.providers {
            if let Err(e) = provider.validate_structured() {
                errors.push(ConfigValidationError::dependency_conflict(format!(
                    "Provider '{}' validation failed: {}",
                    provider_id, e
                )));
            }
        }

        if self.logger.output == LogOutput::File && self.logger.file.is_none() {
            errors.push(ConfigValidationError::invalid_field(
                "logger.file",
                "must be specified when output is 'file'",
            ));
        }

        if let Err(e) = self.orchestrator.batch.validate_structured() {
            errors.push(e);
        }
        if let Err(e) = self.orchestrator.hot_update.validate_structured() {
            errors.push(e);
        }

        if let Err(e) = self.ast_to_nl.validate_structured() {
            errors.push(e);
        }
        if let Err(e) = self.grouper.validate_structured() {
            errors.push(e);
        }
        if let Err(e) = self.relation.validate_structured() {
            errors.push(e);
        }
        if let Err(e) = self.symbol_resolution.validate_structured() {
            errors.push(e);
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(ConfigValidationError::multiple(errors))
        }
    }
}

impl AppConfig {
    /// Validate configuration dependencies
    ///
    /// Returns a list of warnings for configuration issues where
    /// features are enabled but their dependencies are not.
    pub fn validate_dependencies(&self) -> Vec<ConfigWarning> {
        use crate::validation::{
            DependencyParams, validate_all_dependencies, validate_provider_rate_limit_conflicts,
        };

        let mut warnings = validate_provider_rate_limit_conflicts(&self.llm.providers);

        let params = DependencyParams {
            export_include_summary: self.export.include_summary,
            export_enable_relation_enhancement: self.export.enable_relation_enhancement,
            indexer_store_summaries: self.orchestrator.indexer.store_summaries,
            indexer_build_relations: self.orchestrator.indexer.build_relations,
            indexer_store_vectors: self.orchestrator.indexer.store_vectors,
            indexer_store_bm25: self.orchestrator.indexer.store_bm25,
            qdrant_enabled: self.database.qdrant.enabled,
            bm25_enabled: self.database.bm25.enabled,
            relation_index_enabled: self.relation.index.enabled,
            llm_enabled: self.llm.enabled,
            has_llm_provider: !self.llm.providers.is_empty(),
            has_chat_model: self
                .llm
                .defaults
                .chat
                .as_ref()
                .is_some_and(|chat_model| self.llm.chat_models.contains_key(chat_model)),
        };

        warnings.extend(validate_all_dependencies(&params));
        warnings
    }

    /// Resolve configuration dependencies by auto-enabling required features
    ///
    /// This method modifies the configuration in-place to ensure that
    /// all feature dependencies are satisfied.
    ///
    /// Returns a list of info messages for auto-enabled features.
    pub fn resolve_dependencies(&mut self) -> Vec<ConfigWarning> {
        use crate::validation::{
            resolve_export_dependencies, resolve_relation_dependencies,
            resolve_storage_dependencies,
        };

        let mut infos = Vec::new();

        infos.extend(resolve_export_dependencies(
            self.export.include_summary,
            self.export.enable_relation_enhancement,
            &mut self.orchestrator.indexer.store_summaries,
            &mut self.orchestrator.indexer.build_relations,
            &mut self.relation.index.enabled,
        ));

        infos.extend(resolve_storage_dependencies(
            self.orchestrator.indexer.store_vectors,
            self.orchestrator.indexer.store_bm25,
            &mut self.database.qdrant.enabled,
            &mut self.database.bm25.enabled,
        ));

        infos.extend(resolve_relation_dependencies(
            self.orchestrator.indexer.build_relations,
            &mut self.relation.index.enabled,
        ));

        infos
    }

    /// Validate and resolve dependencies
    ///
    /// This method first validates the configuration and logs warnings,
    /// then resolves dependencies by auto-enabling required features.
    ///
    /// Returns all warnings and info messages.
    pub fn validate_and_resolve_dependencies(&mut self) -> Vec<ConfigWarning> {
        let warnings = self.validate_dependencies();

        for warning in &warnings {
            tracing::warn!("{}", warning.to_log_message());
        }

        let infos = self.resolve_dependencies();

        for info in &infos {
            tracing::info!("{}", info.to_log_message());
        }

        let mut all_messages = warnings;
        all_messages.extend(infos);
        all_messages
    }
}

/// Metrics configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetricsConfig {
    /// Aggregation configuration
    #[serde(default)]
    pub aggregation: MetricsAggregationConfig,
}

/// Metrics aggregation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsAggregationConfig {
    /// Whether to enable automatic aggregation (default: true)
    #[serde(default = "default_aggregation_enabled")]
    pub enabled: bool,
    /// Aggregation interval in seconds (default: 300 = 5 minutes)
    #[serde(default = "default_aggregation_interval")]
    pub interval_secs: u64,
    /// Retention period in seconds for aggregated data (default: 604800 = 7 days)
    #[serde(default = "default_aggregation_retention")]
    pub retention_seconds: u64,
    /// Cleanup interval in seconds (default: 3600 = 1 hour)
    #[serde(default = "default_aggregation_cleanup_interval")]
    pub cleanup_interval_secs: u64,
}

impl Default for MetricsAggregationConfig {
    fn default() -> Self {
        Self {
            enabled: default_aggregation_enabled(),
            interval_secs: default_aggregation_interval(),
            retention_seconds: default_aggregation_retention(),
            cleanup_interval_secs: default_aggregation_cleanup_interval(),
        }
    }
}

fn default_aggregation_enabled() -> bool {
    true
}

fn default_aggregation_interval() -> u64 {
    300
}

fn default_aggregation_retention() -> u64 {
    604800
}

fn default_aggregation_cleanup_interval() -> u64 {
    3600
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::ServiceType;
    use crate::project::ProjectAppConfig;

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();
        assert_eq!(config.server.host, "0.0.0.0");
        assert_eq!(config.server.port, 9000);
        assert_eq!(config.database.sqlite.path, "metadata.db");
        assert_eq!(config.logger.level, LogLevel::Info);
        assert!(!config.rerank.enabled);
        assert_eq!(config.rerank.model, "gpt-4o-mini");
        assert_eq!(config.rerank.max_candidates, 50);
    }

    #[test]
    fn test_config_serialization() {
        let config = AppConfig::default();
        let toml_str = toml::to_string_pretty(&config).expect("Failed to serialize");
        let deserialized: AppConfig = toml::from_str(&toml_str).expect("Failed to deserialize");
        assert_eq!(config.server.host, deserialized.server.host);
        assert_eq!(config.rerank.enabled, deserialized.rerank.enabled);
        assert_eq!(config.rerank.model, deserialized.rerank.model);
        assert_eq!(
            config.rerank.max_candidates,
            deserialized.rerank.max_candidates
        );
    }

    #[test]
    fn test_validate_allows_remote_provider_without_api_key() {
        let mut config = AppConfig::default();
        config.embedder.default_model = "dummy-model".to_string();
        config.llm.providers.insert(
            "openai".to_string(),
            ProviderConfig {
                id: "openai".to_string(),
                name: "OpenAI".to_string(),
                enabled: true,
                provider_type: cce_types::llm::ProviderType::Remote,
                api_keys: Vec::new(),
                base_url: "https://api.openai.com/v1".to_string(),
                endpoints: std::collections::HashMap::new(),
                timeout_secs: 30,
                max_retries: 3,
                retry_delay_ms: 1000,
                retry_jitter: 0.2,
                rate_limit_max_retries: 5,
                rate_limit_max_delay_ms: 60000,
                rate_limit: 60,
                circuit_breaker: crate::modules::CircuitBreakerConfig::default(),
                proxy_url: None,
                extra_headers: std::collections::HashMap::new(),
                api_key_file: None,
            },
        );

        assert!(config.validate_structured().is_ok());
    }

    #[test]
    fn test_validate_rejects_provider_with_empty_base_url() {
        let mut config = AppConfig::default();
        config.embedder.default_model = "dummy-model".to_string();
        config.llm.providers.insert(
            "openai".to_string(),
            ProviderConfig {
                id: "openai".to_string(),
                name: "OpenAI".to_string(),
                enabled: true,
                provider_type: cce_types::llm::ProviderType::Remote,
                api_keys: vec!["key".to_string()],
                base_url: String::new(),
                endpoints: std::collections::HashMap::new(),
                timeout_secs: 30,
                max_retries: 3,
                retry_delay_ms: 1000,
                retry_jitter: 0.2,
                rate_limit_max_retries: 5,
                rate_limit_max_delay_ms: 60000,
                rate_limit: 60,
                circuit_breaker: crate::modules::CircuitBreakerConfig::default(),
                proxy_url: None,
                extra_headers: std::collections::HashMap::new(),
                api_key_file: None,
            },
        );

        let err = config
            .validate_structured()
            .expect_err("should reject empty base_url");
        assert!(err.to_string().contains("empty base_url"));
    }

    #[test]
    fn test_validate_dependencies_no_warnings() {
        let mut config = AppConfig::default();
        config.database.bm25.enabled = true;

        let warnings = config.validate_dependencies();
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_validate_dependencies_summary_warning() {
        let mut config = AppConfig::default();
        config.database.bm25.enabled = true;
        config.export.include_summary = true;
        config.orchestrator.indexer.store_summaries = false;

        let warnings = config.validate_dependencies();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].field, "export.include_summary");
    }

    #[test]
    fn test_validate_dependencies_relation_warnings() {
        let mut config = AppConfig::default();
        config.database.bm25.enabled = true;
        config.export.enable_relation_enhancement = true;
        config.relation.index.enabled = false;
        config.orchestrator.indexer.build_relations = false;

        let warnings = config.validate_dependencies();
        assert_eq!(warnings.len(), 2);
    }

    #[test]
    fn test_resolve_dependencies_auto_enable() {
        let mut config = AppConfig::default();
        config.database.bm25.enabled = true;
        config.export.include_summary = true;
        config.export.enable_relation_enhancement = true;
        config.orchestrator.indexer.store_summaries = false;
        config.orchestrator.indexer.build_relations = false;
        config.relation.index.enabled = false;

        let infos = config.resolve_dependencies();

        assert_eq!(infos.len(), 3);
        assert!(config.orchestrator.indexer.store_summaries);
        assert!(config.orchestrator.indexer.build_relations);
        assert!(config.relation.index.enabled);
    }

    #[test]
    fn test_validate_and_resolve_dependencies() {
        let mut config = AppConfig::default();
        config.database.bm25.enabled = true;
        config.export.include_summary = true;
        config.export.enable_relation_enhancement = true;
        config.orchestrator.indexer.store_summaries = false;
        config.orchestrator.indexer.build_relations = false;
        config.relation.index.enabled = false;

        let messages = config.validate_and_resolve_dependencies();

        assert_eq!(messages.len(), 6);
        assert!(config.orchestrator.indexer.store_summaries);
        assert!(config.orchestrator.indexer.build_relations);
        assert!(config.relation.index.enabled);
    }

    #[test]
    fn test_merge_with_project_empty() {
        let global = AppConfig::default();
        let project = ProjectAppConfig::default();
        let merged = global.merge_with_project(&project);

        assert_eq!(merged.server.host, global.server.host);
        assert_eq!(
            merged.scanner.follow_symlinks,
            global.scanner.follow_symlinks
        );
    }

    #[test]
    fn test_merge_with_project_checkpoint_ttl() {
        let global = AppConfig::default();
        assert_eq!(global.orchestrator.checkpoint_ttl_seconds, 86400);

        let project = ProjectAppConfig {
            orchestrator: Some(crate::ProjectOrchestratorConfig {
                checkpoint_ttl_seconds: Some(7200),
                ..Default::default()
            }),
            ..Default::default()
        };

        let merged = global.merge_with_project(&project);
        assert_eq!(merged.orchestrator.checkpoint_ttl_seconds, 7200);

        let untouched = global.merge_with_project(&ProjectAppConfig::default());
        assert_eq!(untouched.orchestrator.checkpoint_ttl_seconds, 86400);
    }

    #[test]
    fn test_merge_with_project_scanner() {
        let global = AppConfig::default();
        let project = ProjectAppConfig {
            scanner: Some(ScannerConfig {
                respect_gitignore: false,
                ..Default::default()
            }),
            ..Default::default()
        };

        let merged = global.merge_with_project(&project);

        assert!(!merged.scanner.respect_gitignore);
        assert_eq!(merged.server.host, global.server.host);
        assert_eq!(merged.database.sqlite.path, global.database.sqlite.path);
    }

    #[test]
    fn test_merge_with_project_preserves_sensitive() {
        let mut global = AppConfig::default();
        use std::collections::HashMap;
        let mut providers = HashMap::new();
        providers.insert(
            "openai".to_string(),
            crate::modules::ProviderConfig {
                id: "openai".to_string(),
                name: "OpenAI".to_string(),
                base_url: "https://api.example.com".to_string(),
                api_keys: vec!["secret-key".to_string()],
                ..Default::default()
            },
        );
        global.llm.providers = providers;

        let project = ProjectAppConfig {
            scanner: Some(ScannerConfig {
                ..Default::default()
            }),
            ..Default::default()
        };

        let merged = global.merge_with_project(&project);

        let provider = merged.llm.providers.get("openai").unwrap();
        assert_eq!(provider.base_url, "https://api.example.com");
        assert_eq!(provider.api_keys, vec!["secret-key".to_string()]);
    }

    #[test]
    fn test_merge_with_project_embedder_model() {
        use crate::ProjectEmbedderConfig;
        use std::collections::HashMap;

        let mut global = AppConfig::default();
        global.embedder.default_model = "all-MiniLM-L6-v2".to_string();

        let mut providers = HashMap::new();
        providers.insert(
            "openai".to_string(),
            crate::modules::ProviderConfig {
                id: "openai".to_string(),
                name: "OpenAI".to_string(),
                base_url: "https://api.example.com".to_string(),
                api_keys: vec![],
                ..Default::default()
            },
        );
        global.llm.providers = providers;

        let mut models = HashMap::new();
        models.insert(
            "all-MiniLM-L6-v2".to_string(),
            crate::modules::EmbeddingModelConfig {
                provider_id: "openai".to_string(),
                model: "all-MiniLM-L6-v2".to_string(),
                vector_dimension: 384,
                ..Default::default()
            },
        );
        global.llm.embedding_models = models;

        let project = ProjectAppConfig {
            embedder: Some(ProjectEmbedderConfig {
                model: Some("bge-m3".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let merged = global.merge_with_project(&project);

        assert_eq!(merged.embedder.default_model, "bge-m3");
        let provider = merged.llm.providers.get("openai").unwrap();
        assert_eq!(provider.base_url, "https://api.example.com");
    }

    #[test]
    fn test_merge_with_project_llm_models() {
        use crate::ProjectLlmConfig;

        let mut global = AppConfig::default();
        global.llm.defaults.chat = Some("gpt-3.5-turbo".to_string());
        global.llm.defaults.rerank = Some("cross-encoder-base".to_string());
        global.rerank.enabled = false;
        global.rerank.max_candidates = 50;

        let project = ProjectAppConfig {
            llm: Some(ProjectLlmConfig {
                chat_model: Some("gpt-4o".to_string()),
                rerank_model: Some("gpt-4o-mini-rerank".to_string()),
                enable_rerank: Some(true),
                rerank_max_candidates: Some(30),
            }),
            ..Default::default()
        };

        let merged = global.merge_with_project(&project);

        assert_eq!(merged.llm.defaults.chat, Some("gpt-4o".to_string()));
        assert_eq!(
            merged.llm.defaults.rerank,
            Some("gpt-4o-mini-rerank".to_string())
        );
        assert!(merged.rerank.enabled);
        assert_eq!(merged.rerank.max_candidates, 30);
    }

    #[test]
    fn test_merge_with_project_storage_qdrant_preset() {
        use crate::modules::storage::CollectionPreset;
        use crate::project::{ProjectQdrantConfig, ProjectStorageConfig};

        let mut global = AppConfig::default();
        global.database.qdrant.preset = CollectionPreset::Medium;

        let project = ProjectAppConfig {
            storage: Some(ProjectStorageConfig {
                qdrant: Some(ProjectQdrantConfig {
                    preset: Some(CollectionPreset::Large),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };

        let merged = global.merge_with_project(&project);

        assert_eq!(merged.database.qdrant.preset, CollectionPreset::Large);
    }

    #[test]
    fn test_merge_with_project_storage_bm25() {
        use crate::project::{ProjectBm25Config, ProjectStorageConfig};

        let mut global = AppConfig::default();
        global.database.bm25.enabled = false;

        let project = ProjectAppConfig {
            storage: Some(ProjectStorageConfig {
                bm25: Some(ProjectBm25Config {
                    enabled: Some(true),
                    index_path: Some("./.cce/bm25".to_string()),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };

        let merged = global.merge_with_project(&project);

        assert!(merged.database.bm25.enabled);
        assert_eq!(
            merged.database.bm25.index_path,
            Some("./.cce/bm25".to_string())
        );
    }

    #[test]
    fn test_merge_with_project_storage_full_config() {
        use crate::modules::storage::{Bm25AlgorithmConfig, CollectionPreset};
        use crate::project::*;

        let mut global = AppConfig::default();
        global.database.qdrant.preset = CollectionPreset::Medium;
        global.database.bm25.enabled = false;

        let project = ProjectAppConfig {
            storage: Some(ProjectStorageConfig {
                qdrant: Some(ProjectQdrantConfig {
                    preset: Some(CollectionPreset::Large),
                    hnsw: Some(HnswConfigOverride {
                        m: Some(64),
                        ef_construct: Some(512),
                        on_disk: Some(true),
                        payload_m: Some(64),
                        inline_storage: Some(true),
                    }),
                    quantization: Some(QuantizationConfig::Scalar(
                        crate::modules::storage::ScalarQuantizationConfig {
                            quant_type: "int8".to_string(),
                            quantile: 0.99,
                            always_ram: false,
                        },
                    )),
                    ..Default::default()
                }),
                bm25: Some(ProjectBm25Config {
                    enabled: Some(true),
                    algorithm: Some(Bm25AlgorithmConfig { k1: 2.0, b: 0.3 }),
                    ..Default::default()
                }),
                index_manager: None,
            }),
            ..Default::default()
        };

        let merged = global.merge_with_project(&project);

        assert_eq!(merged.database.qdrant.preset, CollectionPreset::Large);

        let hnsw = merged.database.qdrant.hnsw.expect("hnsw resolved");
        assert_eq!(hnsw.m, 64);
        assert_eq!(hnsw.ef_construct, 512);
        assert_eq!(hnsw.inline_storage, Some(true));
        let quant = merged
            .database
            .qdrant
            .quantization
            .expect("quantization resolved");
        assert!(matches!(
            quant,
            crate::modules::storage::QuantizationConfig::Scalar(_)
        ));

        assert!(merged.database.bm25.enabled);
        assert_eq!(merged.database.bm25.algorithm.k1, 2.0);
    }

    #[test]
    fn test_merge_qdrant_partial_patch_preserves_base() {
        use crate::modules::storage::{CollectionPreset, HnswConfig, QuantizationConfig};
        use crate::project::{HnswConfigOverride, ProjectQdrantConfig, ProjectStorageConfig};

        let mut global = AppConfig::default();
        global.database.qdrant.preset = CollectionPreset::Medium;
        global.database.qdrant.hnsw = Some(HnswConfig {
            m: 32,
            ef_construct: 256,
            on_disk: true,
            payload_m: Some(32),
            inline_storage: Some(true),
        });

        let project = ProjectAppConfig {
            storage: Some(ProjectStorageConfig {
                qdrant: Some(ProjectQdrantConfig {
                    hnsw: Some(HnswConfigOverride {
                        m: Some(48),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };

        let merged = global.merge_with_project(&project);

        let hnsw = merged.database.qdrant.hnsw.expect("hnsw resolved");
        assert_eq!(hnsw.m, 48, "patch field overridden");
        assert_eq!(hnsw.ef_construct, 256, "unpatched field preserved");
        assert_eq!(hnsw.inline_storage, Some(true), "unpatched field preserved");

        global.database.qdrant.quantization = Some(QuantizationConfig::Scalar(Default::default()));
        let merged = global.merge_with_project(&project);
        assert!(matches!(
            merged.database.qdrant.quantization,
            Some(QuantizationConfig::Scalar(_))
        ));
    }

    fn config_with_provider_and_models() -> AppConfig {
        let mut config = AppConfig::default();
        config.llm.providers.insert(
            "mock-provider".to_string(),
            ProviderConfig {
                id: "mock-provider".to_string(),
                name: "Mock".to_string(),
                base_url: "https://api.mock.example.com/v1".to_string(),
                api_keys: vec!["test-key".to_string()],
                endpoints: std::collections::HashMap::from([
                    (ServiceType::Chat, "custom/chat".to_string()),
                    (ServiceType::Rerank, "custom/rerank".to_string()),
                ]),
                ..ProviderConfig::default()
            },
        );

        config.llm.embedding_models.insert(
            "emb-model".to_string(),
            EmbeddingModelConfig {
                provider_id: "mock-provider".to_string(),
                model: "bge-m3".to_string(),
                vector_dimension: 1024,
                ..EmbeddingModelConfig::default()
            },
        );

        config.llm.chat_models.insert(
            "chat-model".to_string(),
            ChatModelConfig {
                provider_id: "mock-provider".to_string(),
                model: "Qwen/Qwen3.5-4B".to_string(),
                temperature: 0.2,
                max_tokens: 4096,
                max_input_tokens: 8192,
                extra_params: std::collections::HashMap::from([(
                    "extra".to_string(),
                    serde_json::json!("value"),
                )]),
                ..ChatModelConfig::default()
            },
        );

        config.llm.rerank_models.insert(
            "rerank-model".to_string(),
            RerankModelConfig {
                provider_id: "mock-provider".to_string(),
                model: "BAAI/bge-reranker-v2-m3".to_string(),
                mode: crate::modules::RerankMode::CrossEncoder,
            },
        );
        config
    }

    #[test]
    fn test_resolve_llm_connection_honors_endpoint_overrides() {
        let config = config_with_provider_and_models();

        let chat = config
            .resolve_llm_connection("chat-model", ServiceType::Chat)
            .expect("chat resolution must succeed");
        assert_eq!(chat.provider_id, "mock-provider");
        assert_eq!(chat.base_url, "https://api.mock.example.com/v1");
        assert_eq!(chat.endpoint_path, "custom/chat");
        assert_eq!(chat.api_keys, vec!["test-key".to_string()]);
        assert_eq!(
            chat.extra_params.get("extra"),
            Some(&serde_json::json!("value")),
            "chat model extra_params must flow into the connection"
        );

        let rerank = config
            .resolve_llm_connection("rerank-model", ServiceType::Rerank)
            .expect("rerank resolution must succeed");
        assert_eq!(rerank.endpoint_path, "custom/rerank");
        assert!(rerank.extra_params.is_empty());

        let embedding = config
            .resolve_llm_connection("emb-model", ServiceType::Embedding)
            .expect("embedding resolution must succeed");
        assert_eq!(
            embedding.endpoint_path, "embeddings",
            "unoverridden services keep their default path"
        );
    }

    #[test]
    fn test_resolve_llm_connection_missing_model() {
        let config = config_with_provider_and_models();
        assert!(
            config
                .resolve_llm_connection("nope", ServiceType::Chat)
                .is_err()
        );
        assert!(
            config
                .resolve_llm_connection("chat-model", ServiceType::Completion)
                .is_err(),
            "Completion has no registry"
        );
    }

    #[test]
    fn test_resolve_chat_config_merges_model_and_provider() {
        let config = config_with_provider_and_models();
        let resolved = config
            .resolve_chat_config("chat-model")
            .expect("chat resolution must succeed");

        assert_eq!(resolved.model, "Qwen/Qwen3.5-4B");
        assert_eq!(resolved.temperature, 0.2);
        assert_eq!(resolved.max_tokens, 4096);
        assert_eq!(resolved.max_input_tokens, 8192);
        assert_eq!(resolved.endpoint_path, "custom/chat");
        assert_eq!(resolved.base_url, "https://api.mock.example.com/v1");
        assert_eq!(
            resolved.extra_params.get("extra"),
            Some(&serde_json::json!("value"))
        );
    }

    #[test]
    fn test_resolve_embedding_config_includes_endpoint_path() {
        let config = config_with_provider_and_models();
        let resolved = config
            .resolve_embedding_config("emb-model")
            .expect("embedding resolution must succeed");
        assert_eq!(resolved.endpoint_path, "embeddings");
        assert_eq!(resolved.model, "bge-m3");
        assert_eq!(resolved.vector_dimension, 1024);
    }
}
