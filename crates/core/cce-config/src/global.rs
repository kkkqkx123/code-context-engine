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
mod metrics;
mod resolved;
mod sqlite;
#[cfg(test)]
mod tests;

pub use logging::{LogFormat, LogLevel, LogOutput, LoggingConfig};
pub use metrics::{MetricsAggregationConfig, MetricsConfig};
pub use resolved::{ResolvedChatConfig, ResolvedEmbeddingConfig, ResolvedLlmConnection};
pub use sqlite::{SqliteConfig, SqliteSyncMode};

use serde::{Deserialize, Serialize};

use crate::modules::{
    AstToNlConfig, EmbedderConfig, ExportModuleConfig, GlobalCacheConfig, LicenseHeaderConfig,
    McpConfig, NestProcessorConfig, OrchestratorConfig, ProviderConfig, RelationConfig,
    RerankConfig, ScannerConfig, SearchModuleConfig, SummaryConfig, SymbolResolutionConfig,
};
use crate::modules::{Bm25Config, LocalVectorConfig, QdrantConfig, VectorBackend};
use crate::modules::{ChatModelConfig, EmbeddingModelConfig, RerankModelConfig};
use crate::modules::{
    FulltextBackend, FulltextRemoteConfig, RelationBackend, RelationRemoteConfig,
};
use crate::validation::{ConfigWarning, Validate, ValidationResult};
use cce_types::error::config::ConfigValidationError;

/// Database configuration (combines vector backends, SQLite, and BM25)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DatabaseConfig {
    /// Vector backend selection (local default, qdrant optional).
    pub vector_backend: VectorBackend,
    /// Embedded local vector engine configuration.
    pub vector_local: LocalVectorConfig,
    /// Qdrant vector database configuration (remote branch only).
    pub qdrant: QdrantConfig,
    /// SQLite metadata database configuration
    pub sqlite: SqliteConfig,
    /// BM25 index configuration
    pub bm25: Bm25Config,
    /// Relation storage backend selection (phase 1: local only).
    pub relation_backend: RelationBackend,
    /// Reserved remote parameters for the relation branch (phase 3).
    pub relation_remote: RelationRemoteConfig,
    /// Fulltext storage backend selection (phase 1: local only).
    pub fulltext_backend: FulltextBackend,
    /// Reserved remote parameters for the fulltext branch (phase 3).
    pub fulltext_remote: FulltextRemoteConfig,
    /// Advanced switch allowing non-preset backend combinations.
    ///
    /// Official presets always pass; any other combination is rejected
    /// unless this flag is set explicitly (risk acknowledged by operator).
    pub allow_nonstandard_backends: bool,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            vector_backend: VectorBackend::Local,
            vector_local: LocalVectorConfig::default(),
            qdrant: QdrantConfig::default(),
            sqlite: SqliteConfig::default(),
            bm25: Bm25Config::default(),
            relation_backend: RelationBackend::Local,
            relation_remote: RelationRemoteConfig::default(),
            fulltext_backend: FulltextBackend::Local,
            fulltext_remote: FulltextRemoteConfig::default(),
            allow_nonstandard_backends: false,
        }
    }
}

impl DatabaseConfig {
    /// Whether the local vector backend is active.
    pub fn is_local_vector(&self) -> bool {
        matches!(self.vector_backend, VectorBackend::Local)
    }

    /// Whether the local relation backend is active.
    pub fn is_local_relation(&self) -> bool {
        self.relation_backend.is_local()
    }

    /// Whether the local fulltext backend is active.
    pub fn is_local_fulltext(&self) -> bool {
        self.fulltext_backend.is_local()
    }

    /// Effective vector dimension for the active backend.
    pub fn vector_dimension(&self) -> usize {
        match self.vector_backend {
            VectorBackend::Local => self.vector_local.vector_size,
            VectorBackend::Qdrant => self.qdrant.vector_size,
        }
    }

    /// Official preset combinations.
    ///
    /// Supported presets: local-first (`local/local/local`), remote-vector
    /// (`qdrant/local/local`), and the remote preset
    /// (`qdrant/remote/remote`). Any other combination is a non-preset mix
    /// and requires `allow_nonstandard_backends`. Remote branches validate
    /// their own parameters when selected.
    pub fn validate_backend_combination(&self) -> ValidationResult {
        use crate::modules::VectorBackend as VB;
        if !self.relation_backend.is_local()
            && let Err(e) = self.relation_remote.validate_structured()
        {
            return Err(ConfigValidationError::invalid_field(
                "database.relation_remote",
                format!("remote relation parameters are invalid: {e}"),
            ));
        }
        if !self.fulltext_backend.is_local()
            && let Err(e) = self.fulltext_remote.validate_structured()
        {
            return Err(ConfigValidationError::invalid_field(
                "database.fulltext_remote",
                format!("remote fulltext parameters are invalid: {e}"),
            ));
        }
        if !self.relation_backend.is_local() && self.relation_remote.url.is_none() {
            return Err(ConfigValidationError::missing_field(
                "database.relation_remote.url",
            ));
        }
        if !self.fulltext_backend.is_local() && self.fulltext_remote.url.is_none() {
            return Err(ConfigValidationError::missing_field(
                "database.fulltext_remote.url",
            ));
        }
        let official = matches!(
            (
                &self.vector_backend,
                &self.relation_backend,
                &self.fulltext_backend
            ),
            (VB::Local, RelationBackend::Local, FulltextBackend::Local)
                | (VB::Qdrant, RelationBackend::Local, FulltextBackend::Local)
                | (VB::Qdrant, RelationBackend::Remote, FulltextBackend::Remote)
        );
        if official || self.allow_nonstandard_backends {
            Ok(())
        } else {
            Err(ConfigValidationError::invalid_field(
                "database.vector_backend",
                "non-preset backend combination requires database.allow_nonstandard_backends = true",
            ))
        }
    }
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
    /// License header filtering configuration
    #[serde(default)]
    pub license_header: LicenseHeaderConfig,
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
    /// MCP server configuration (optional, disabled by default)
    #[serde(default)]
    pub mcp: McpConfig,
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
            host: "127.0.0.1".to_string(),
            port: 9000,
        }
    }
}

impl ServerConfig {
    /// Whether the listener binds all network interfaces.
    ///
    /// Only the dev/intranet configuration opts into this explicitly; the
    /// code default is loopback so production never exposes the service by
    /// accident.
    pub fn is_wildcard_bind(&self) -> bool {
        let host = self.host.trim().to_lowercase();
        host == "0.0.0.0" || host == "::" || host == "[::]"
    }

    /// Reject a wildcard bind outside development environments.
    ///
    /// `environment` follows `CCE_ENV` (`dev` by default); values `prod` and
    /// `production` (case-insensitive) require an explicit loopback or site
    /// address instead of a wildcard.
    pub fn validate_for_environment(&self, environment: &str) -> ValidationResult {
        let env = environment.trim().to_lowercase();
        if (env == "prod" || env == "production") && self.is_wildcard_bind() {
            return Err(ConfigValidationError::invalid_field(
                "server.host",
                "wildcard bind (0.0.0.0 or ::) is not allowed in production; \
                 set an explicit loopback or site address",
            ));
        }
        Ok(())
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

        match self.database.vector_backend {
            crate::modules::VectorBackend::Local => {
                if let Err(e) = self.database.vector_local.validate_structured() {
                    errors.push(e);
                }
            }
            crate::modules::VectorBackend::Qdrant => {
                if let Err(e) = self.database.qdrant.validate_structured() {
                    errors.push(e);
                }
            }
        }
        if let Err(e) = self.database.validate_backend_combination() {
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
        if let Err(e) = self.search.validate_structured() {
            errors.push(e);
        }
        if let Err(e) = self.rerank.validate_structured() {
            errors.push(e);
        }
        if let Err(e) = self.license_header.validate_structured() {
            errors.push(e);
        }

        if let Err(e) = self.metrics.aggregation.validate_metrics_aggregation() {
            errors.push(e);
        }
        if let Err(e) = self.metrics.validate_metrics_probe() {
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
    /// Runtime environment name driving environment-sensitive validation.
    ///
    /// Read from `CCE_ENV` (`dev` when unset). Production deployments set
    /// `CCE_ENV=prod` (or `production`).
    pub fn runtime_environment() -> String {
        match std::env::var("CCE_ENV") {
            Ok(env) if !env.trim().is_empty() => env.trim().to_string(),
            _ => "dev".to_string(),
        }
    }

    /// Environment-sensitive validation (currently the server bind rule).
    ///
    /// Structural validation stays in [`Validate::validate_structured`];
    /// this adds the deployment safeguard that a wildcard bind is only
    /// acceptable in non-production environments.
    pub fn validate_for_current_environment(&self) -> ValidationResult {
        self.server
            .validate_for_environment(&Self::runtime_environment())
    }

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
            vector_backend: self.database.vector_backend,
            relation_backend: self.database.relation_backend,
            fulltext_backend: self.database.fulltext_backend,
            allow_nonstandard_backends: self.database.allow_nonstandard_backends,
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
            resolve_storage_dependencies_with_backend,
        };

        let mut infos = Vec::new();

        infos.extend(resolve_export_dependencies(
            self.export.include_summary,
            self.export.enable_relation_enhancement,
            &mut self.orchestrator.indexer.store_summaries,
            &mut self.orchestrator.indexer.build_relations,
            &mut self.relation.index.enabled,
        ));

        infos.extend(resolve_storage_dependencies_with_backend(
            self.orchestrator.indexer.store_vectors,
            self.orchestrator.indexer.store_bm25,
            &mut self.database.qdrant.enabled,
            &mut self.database.bm25.enabled,
            self.database.vector_backend,
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
