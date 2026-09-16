use std::path::Path;
use std::sync::Arc;

use super::EngineError;
use super::builders::{build_chat_handle, build_rerank_handler, build_summary_generator};
use cce_config::project_registry::{ProjectEntry, ProjectScope};
use cce_metrics::RelationMetrics;
use cce_metrics_infra::{PluginMetrics, SearchMetrics, WatchMetrics};
use cce_orchestrator::OperationCoordinator;
use cce_orchestrator::hot_update::HotUpdateCoordinator;
use cce_orchestrator::hot_update::processors::factory::{ProcessorConfig, ProcessorFactory};
use cce_orchestrator::index::IndexOrchestrator;
use cce_orchestrator::query::searcher::Searcher;
use cce_parser::summary::SummaryGenerator;
use cce_plugin::PluginRegistry;
use cce_plugin_runtime::FilePluginSource;
use tokio::sync::Mutex;

impl super::CodeContextEngine {
    /// Get or create project-specific hot update coordinator
    ///
    /// This method implements lazy loading with caching:
    /// 1. Try to get from cache (fast path)
    /// 2. If cache miss, create new coordinator with project context (slow path)
    /// 3. Cache for future requests
    pub async fn get_hot_update_coordinator(
        &self,
        project_id: i64,
    ) -> Result<Arc<Mutex<HotUpdateCoordinator>>, EngineError> {
        // Fast path: check cache
        if let Some(coordinator) = self.hot_update_cache.get(project_id).await {
            tracing::debug!(project_id, "Hot update coordinator cache hit");
            return Ok(coordinator);
        }

        // Cache miss - create new coordinator
        tracing::info!(project_id, "Creating project-specific HotUpdateCoordinator");

        let project_entry = self
            .project_registry
            .get_or_load(project_id)
            .await
            .map_err(|e| EngineError::Config(format!("Failed to load project config: {}", e)))?;

        // Get or create OperationCoordinator for this project
        let operation_coordinator = self.get_operation_coordinator(project_id).await?;

        let metadata_store = self
            .metadata_store
            .as_ref()
            .map(|db| db.for_project(project_id))
            .transpose()
            .map_err(|e| EngineError::Config(format!("Failed to open project database: {e}")))?
            .ok_or_else(|| {
                EngineError::Config("SQLite database not initialized for hot update".to_string())
            })?;
        let checkpoint_manager = operation_coordinator.checkpoint_manager();
        let project_group_id = cce_storage_qdrant::generate_project_group_id(
            project_id,
            &project_entry.metadata.root_path,
        );

        // Export and summary processors follow the project configuration:
        // export is gated on `export.enabled`, summary on `store_summaries`
        // (consistent with the full-index path). The export processor is no
        // longer unconditionally disabled in hot updates.
        let mut processor_config = ProcessorConfig::new();
        processor_config.enable_export = project_entry.config.export.enabled;
        processor_config.enable_summary = project_entry.config.orchestrator.indexer.store_summaries;
        processor_config.export_config = if project_entry.config.export.enabled {
            Some(cce_orchestrator::ExportConfig::from_module_config(
                &project_entry.config.export,
                Path::new(&project_entry.metadata.root_path).to_path_buf(),
                project_id,
            ))
        } else {
            None
        };
        processor_config.enable_relation = project_entry.config.relation.index.enabled
            && project_entry.config.orchestrator.indexer.build_relations
            && project_entry.config.orchestrator.hot_update.build_relations;

        let relation_publisher = self.get_relation_snapshot_publisher(project_id).await?;
        // The summary generator follows the project summary config (same as the
        // full-index path) so hot-update summaries stay consistent with it.
        let summary_generator: Option<Arc<dyn SummaryGenerator>> =
            if processor_config.enable_summary {
                Some(build_summary_generator(
                    &project_entry.config,
                    Some(&self.metrics_registry),
                )?)
            } else {
                None
            };
        let (processors, storage_coordinator) = ProcessorFactory::new()
            .create_all_processors(
                Some(self.qdrant.clone()),
                Some(self.bm25.clone()),
                Some(metadata_store.clone()),
                Some(self.embedder.clone()),
                Some(project_group_id),
                project_id,
                Some(relation_publisher),
                &project_entry.config.relation,
                Some(checkpoint_manager.clone()),
                summary_generator,
                Some(&project_entry.config.ast_to_nl),
                &project_entry.config.grouper,
                Some(&project_entry.config.summary),
                &processor_config,
                self.load_plugin_registry(project_id, &project_entry).await,
                Some(RelationMetrics::new(&self.metrics_registry, project_id)),
                Some(cce_metrics_infra::HotUpdateStorageMetrics::new(
                    &self.metrics_registry,
                    project_id,
                )),
            )
            .map_err(|e| EngineError::Config(e.to_string()))?;

        let coordinator = Arc::new(Mutex::new(
            HotUpdateCoordinator::new(
                project_entry.config.orchestrator.hot_update.clone(),
                project_id,
            )
            .map_err(|e| EngineError::Config(e.to_string()))?
            .with_project_registry(self.project_registry.clone())
            .with_metadata_store(metadata_store)
            .with_checkpoint_manager(checkpoint_manager)
            .with_storage_coordinator(storage_coordinator)
            .with_processors(processors.into_iter().map(Arc::from).collect())
            .with_operation_coordinator(operation_coordinator)
            .with_metrics(cce_metrics_infra::HotUpdateMetrics::new(
                &self.metrics_registry,
                project_id,
            ))
            .with_watch_metrics(WatchMetrics::new(&self.metrics_registry, project_id))
            .with_heartbeat_interval(std::time::Duration::from_secs(
                project_entry.config.orchestrator.heartbeat_interval_secs,
            )),
        ));

        // Double-check: another task may have inserted while we were building
        if let Some(existing) = self.hot_update_cache.get(project_id).await {
            tracing::debug!(
                project_id,
                "Hot update coordinator found after double-check"
            );
            return Ok(existing);
        }
        self.hot_update_cache
            .insert(project_id, coordinator.clone())
            .await;

        tracing::info!(project_id, "Created and cached HotUpdateCoordinator");
        Ok(coordinator)
    }

    /// Get or create project-specific IndexOrchestrator
    ///
    /// This method implements lazy loading with caching:
    /// 1. Try to get from cache (fast path)
    /// 2. If cache miss, create new orchestrator with project config (slow path)
    /// 3. Cache for future requests
    pub async fn get_orchestrator(
        &self,
        project_id: i64,
    ) -> Result<Arc<Mutex<IndexOrchestrator>>, EngineError> {
        // Fast path: check cache
        if let Some(orchestrator) = self.orchestrator_cache.get(project_id).await {
            tracing::debug!(project_id, "IndexOrchestrator cache hit");
            return Ok(orchestrator);
        }

        // Cache miss - create new orchestrator with project config
        tracing::info!(project_id, "Creating project-specific IndexOrchestrator");

        let project_entry = self
            .project_registry
            .get_or_load(project_id)
            .await
            .map_err(|e| EngineError::Config(format!("Failed to load project config: {}", e)))?;

        let config = &project_entry.config;
        let project_group_id = cce_storage_qdrant::generate_project_group_id(
            project_id,
            &project_entry.metadata.root_path,
        );
        let metadata_store = self
            .metadata_store
            .as_ref()
            .map(|db| db.for_project(project_id))
            .transpose()
            .map_err(|e| EngineError::Config(format!("Failed to open project database: {e}")))?
            .ok_or_else(|| {
                EngineError::Config("SQLite database not initialized for indexing".to_string())
            })?;
        let operation_coordinator = self.get_operation_coordinator(project_id).await?;

        // Build orchestrator with project-specific configurations
        let project_progress_tracker = self.get_project_progress_tracker(project_id).await;
        let relation_publisher = self.get_relation_snapshot_publisher(project_id).await?;
        let mut orchestrator_builder =
            IndexOrchestrator::with_batch_config(project_id, config.orchestrator.batch.clone())
                .map_err(|e| EngineError::Config(e.to_string()))?
                .with_qdrant(self.qdrant.clone())
                .with_bm25(self.bm25.clone())
                .with_embedder(self.embedder.clone())
                .with_metadata_store(metadata_store)
                .with_checkpoint_manager(operation_coordinator.checkpoint_manager())
                .with_progress_tracker(project_progress_tracker)
                // Apply project-specific grouper (pre-processor) and ast_to_nl configs
                .with_file_processor_configs(config.grouper.clone(), &config.ast_to_nl)
                // Apply chunk cache capacity from orchestrator config
                .with_chunk_cache_size(config.orchestrator.cache.chunk_cache_size)
                // Apply project-specific summary config
                .with_summary_config(config.summary.clone())
                .with_project_fingerprint(project_group_id.clone())
                .with_relation_publisher(relation_publisher)
                .with_relation_config(config.relation.clone())
                // Attach global metrics registry for pipeline-level metrics
                .with_metrics_registry(self.metrics_registry.clone());

        // Create NL document exporter only if export module is enabled
        if config.export.enabled {
            let export_config = cce_orchestrator::ExportConfig::from_module_config(
                &config.export,
                Path::new(&project_entry.metadata.root_path).to_path_buf(),
                project_id,
            );
            let nl_exporter = Arc::new(cce_orchestrator::NlDocumentExporter::new(export_config));
            orchestrator_builder = orchestrator_builder.with_nl_exporter(nl_exporter);
        }

        // Wire the model-enhanced summary generator when a chat model is
        // configured and the strategy requires it.
        if let Some(handle) = build_chat_handle(config, Some(&self.metrics_registry))? {
            orchestrator_builder =
                orchestrator_builder.with_llm_client(handle.client, handle.config);
        }

        // Load plugins from project configuration
        if let Some(registry) = self.load_plugin_registry(project_id, &project_entry).await {
            orchestrator_builder = orchestrator_builder.with_plugin_registry(registry);
        }

        let orchestrator = Arc::new(Mutex::new(orchestrator_builder));

        // Double-check: another task may have inserted while we were building
        if let Some(existing) = self.orchestrator_cache.get(project_id).await {
            tracing::debug!(project_id, "IndexOrchestrator found after double-check");
            return Ok(existing);
        }
        self.orchestrator_cache
            .insert(project_id, orchestrator.clone())
            .await;

        tracing::info!(
            project_id,
            batch_size = config.orchestrator.batch.scan_batch_size,
            "Created and cached IndexOrchestrator"
        );
        Ok(orchestrator)
    }

    /// Load (and cache) the project's plugin registry.
    ///
    /// Returns `None` when plugins are disabled, none load, or loading fails.
    /// `AstLanguage` plugins are registered into the global language tables
    /// exactly once per load.
    pub(crate) async fn load_plugin_registry(
        &self,
        project_id: i64,
        project_entry: &ProjectEntry,
    ) -> Option<Arc<PluginRegistry>> {
        // Fast path: cached registry.
        if let Some(registry) = self.plugin_registry_cache.get(project_id).await {
            return Some(registry);
        }

        let config = &project_entry.config;
        let plugin_config = &config.plugins;
        if !plugin_config.enabled {
            return None;
        }

        let mut registry = PluginRegistry::new();
        let project_root = Path::new(&project_entry.metadata.root_path);
        let source =
            FilePluginSource::from_project(project_root, plugin_config.registry_file.as_deref())
                .with_metrics(PluginMetrics::new(&self.metrics_registry));
        let loaded = match registry.load_source(&source) {
            Ok(count) if count > 0 => Some(count),
            Ok(_) => {
                tracing::warn!(
                    project_id,
                    "Plugin system enabled but no plugins loaded from {}",
                    source.registry_path().display()
                );
                None
            }
            Err(e) => {
                tracing::warn!(
                    project_id,
                    error = %e,
                    "Failed to load plugins from {}",
                    source.registry_path().display()
                );
                None
            }
        };
        loaded?;

        // Register `AstLanguage` plugins into the global language tables so
        // detection/parsing can route custom languages.
        let extension_conflict_policy = plugin_config
            .language_extension_conflict
            .unwrap_or_default();
        let grammar_abi_policy = plugin_config.grammar_abi_policy.unwrap_or_default();
        let ast_languages = cce_parser::tree_sitter_init::register_ast_language_plugins(
            &registry,
            extension_conflict_policy,
            grammar_abi_policy,
        );
        if ast_languages > 0 {
            tracing::info!(
                project_id,
                ast_languages,
                "Registered AstLanguage plugins for custom-language parsing"
            );
        }

        let registry = Arc::new(registry);
        self.plugin_registry_cache
            .insert(project_id, registry.clone())
            .await;
        Some(registry)
    }

    /// Load (or fetch from cache) the plugin registry for a project.
    pub async fn get_plugin_registry(&self, project_id: i64) -> Option<Arc<PluginRegistry>> {
        let entry = self.project_registry.get_or_load(project_id).await.ok()?;
        self.load_plugin_registry(project_id, &entry).await
    }

    /// Get or create project-specific OperationCoordinator
    ///
    /// This method implements lazy loading with caching for operation coordinators.
    /// One coordinator per project to manage indexing operation queuing and priority.
    pub(crate) async fn get_operation_coordinator(
        &self,
        project_id: i64,
    ) -> Result<Arc<OperationCoordinator>, EngineError> {
        // Fast path: check cache
        if let Some(coordinator) = self.operation_coordinator_cache.get(project_id).await {
            tracing::debug!(project_id, "OperationCoordinator cache hit");
            return Ok(coordinator);
        }

        // Cache miss - create new coordinator
        tracing::info!(project_id, "Creating project-specific OperationCoordinator");

        let db = self
            .metadata_store
            .as_ref()
            .map(|db| db.for_project(project_id))
            .transpose()
            .map_err(|e| EngineError::Config(format!("Failed to open project database: {e}")))?
            .ok_or_else(|| {
                EngineError::Config(
                    "SQLite database not initialized for OperationCoordinator".to_string(),
                )
            })?;

        let coordinator = Arc::new(
            OperationCoordinator::new_for_project(project_id, db)
                .map_err(|e| EngineError::Config(e.to_string()))?
                // Recovery freshness window defaults to the checkpoint TTL;
                // project-level config may override it.
                .with_recovery_freshness(self.project_ttl_seconds(project_id).await),
        );

        // Double-check: another task may have inserted while we were building
        if let Some(existing) = self.operation_coordinator_cache.get(project_id).await {
            tracing::debug!(project_id, "OperationCoordinator found after double-check");
            return Ok(existing);
        }
        self.operation_coordinator_cache
            .insert(project_id, coordinator.clone())
            .await;

        tracing::info!(project_id, "Created and cached OperationCoordinator");
        Ok(coordinator)
    }

    /// Get the checkpoint TTL / recovery freshness window for a project.
    ///
    /// Falls back to the global default (24h) when the project configuration
    /// cannot be loaded.
    async fn project_ttl_seconds(&self, project_id: i64) -> u64 {
        const DEFAULT_CHECKPOINT_TTL_SECONDS: u64 = 86400;
        match self.project_registry.get_or_load(project_id).await {
            Ok(entry) => entry.config.orchestrator.checkpoint_ttl_seconds,
            Err(error) => {
                tracing::warn!(
                    project_id,
                    error = %error,
                    "Failed to load project config for checkpoint TTL, using default"
                );
                DEFAULT_CHECKPOINT_TTL_SECONDS
            }
        }
    }

    /// Get or create project-specific Searcher
    ///
    /// This method implements lazy loading with caching.
    /// Note: Searcher is mostly stateless, but we cache it to avoid
    /// recreating rerank handlers and other optional components.
    pub async fn get_searcher(&self, project_id: i64) -> Result<Arc<Mutex<Searcher>>, EngineError> {
        // Fast path: check cache
        if let Some(searcher) = self.searcher_cache.get(project_id).await {
            tracing::debug!(project_id, "Searcher cache hit");
            return Ok(searcher);
        }

        // Cache miss - create new searcher
        tracing::info!(project_id, "Creating project-specific Searcher");

        let project_entry = self
            .project_registry
            .get_or_load(project_id)
            .await
            .map_err(|e| EngineError::Config(format!("Failed to load project config: {}", e)))?;

        let config = &project_entry.config;
        let project_group_id = cce_storage_qdrant::generate_project_group_id(
            project_id,
            &project_entry.metadata.root_path,
        );

        // Create rerank handler if enabled
        let rerank_handler = build_rerank_handler(config, &self.metrics_registry)?;

        let scope = ProjectScope::new(project_id, project_group_id)
            .map_err(|e| EngineError::Config(format!("Invalid project scope: {}", e)))?;

        let mut builder = Searcher::builder(
            self.qdrant.clone(),
            self.embedder.clone(),
            self.bm25.clone(),
            scope,
        )
        .with_search_metrics(SearchMetrics::new(&self.metrics_registry, project_id));

        // Pass SQLite database for BM25 project isolation filtering and chunk enrichment
        if let Some(sqlite) = self
            .metadata_store
            .as_ref()
            .map(|db| db.for_project(project_id))
            .transpose()
            .map_err(|e| EngineError::Config(format!("Failed to open project database: {e}")))?
        {
            builder = builder.with_sqlite(sqlite);
        }

        // Add rerank handler if available
        if let Some(handler) = rerank_handler {
            builder = builder.with_rerank(handler);
        }

        // Add plugin rerankers if the project has Rerank-capability plugins.
        if let Some(registry) = self.load_plugin_registry(project_id, &project_entry).await {
            let rerank_plugins = registry
                .get_plugins(cce_plugin::PluginCapability::Rerank, None, None)
                .into_iter()
                .cloned()
                .collect::<Vec<_>>();
            if !rerank_plugins.is_empty() {
                builder = builder.with_plugin_rerank(rerank_plugins);
            }
            builder = builder.with_plugin_registry(registry);
        }

        let searcher = Arc::new(Mutex::new(builder.build()));

        // Double-check: another task may have inserted while we were building
        if let Some(existing) = self.searcher_cache.get(project_id).await {
            tracing::debug!(project_id, "Searcher found after double-check");
            return Ok(existing);
        }
        self.searcher_cache
            .insert(project_id, searcher.clone())
            .await;

        tracing::info!(project_id, "Created and cached Searcher");
        Ok(searcher)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::CodeContextEngine;
    use cce_config::AppConfig;
    use cce_config::modules::{EmbeddingModelConfig, ProviderConfig};
    use cce_storage_sqlite::{NewProjectRecord, ProjectRepository};
    use std::collections::HashMap;

    fn create_test_config() -> AppConfig {
        let mut config = AppConfig::default();

        // Use temp directory for sqlite to avoid polluting workspace
        let temp_dir = std::env::temp_dir();
        let db_path = temp_dir.join(format!("cce_test_engine_{}.db", std::process::id()));
        config.database.sqlite.path = db_path.to_string_lossy().to_string();

        // Add a mock provider
        let mut providers = HashMap::new();
        providers.insert(
            "test-provider".to_string(),
            ProviderConfig {
                id: "test-provider".to_string(),
                name: "Test Provider".to_string(),
                base_url: "http://localhost:0".to_string(),
                api_keys: vec!["test-key".to_string()],
                ..ProviderConfig::default()
            },
        );
        config.llm.providers = providers;

        // Add a mock embedding model
        let mut models = HashMap::new();
        models.insert(
            "test-model".to_string(),
            EmbeddingModelConfig {
                provider_id: "test-provider".to_string(),
                model: "test-model".to_string(),
                vector_dimension: 384,
                ..EmbeddingModelConfig::default()
            },
        );
        config.llm.embedding_models = models;
        config.embedder.default_model = "test-model".to_string();

        config
    }

    /// Concurrent get_orchestrator returns same instance
    ///
    /// Spawns two tasks simultaneously calling get_orchestrator for the same
    /// project and verifies the double-check pattern returns one instance.
    #[tokio::test]
    async fn test_concurrent_orchestrator_creation() {
        let config = create_test_config();
        let db_path = config.database.sqlite.path.clone();

        let engine = CodeContextEngine::builder()
            .config(config)
            .build()
            .await
            .expect("Engine should build with test config");

        // Insert a test project into SQLite so get_or_load can find it
        let store = engine
            .metadata_store()
            .expect("Engine should have metadata_store");
        let project_id = store
            .as_ref()
            .with_transaction(|tx| {
                ProjectRepository::insert(
                    tx,
                    &NewProjectRecord::new(
                        "test_concurrent_project".to_string(),
                        "/tmp/cce_test_concurrent".to_string(),
                    ),
                )
            })
            .expect("Failed to insert test project");

        // Spawn two concurrent tasks both calling get_orchestrator
        let engine_clone1 = engine.clone();
        let engine_clone2 = engine.clone();

        let (result1, result2) = tokio::join!(
            tokio::spawn(async move { engine_clone1.get_orchestrator(project_id).await }),
            tokio::spawn(async move { engine_clone2.get_orchestrator(project_id).await }),
        );

        let orchestrator1 = result1
            .expect("Task 1 panicked")
            .expect("get_orchestrator should succeed");
        let orchestrator2 = result2
            .expect("Task 2 panicked")
            .expect("get_orchestrator should succeed");

        // Both should point to the same underlying allocation
        assert!(
            Arc::ptr_eq(&orchestrator1, &orchestrator2),
            "Concurrent get_orchestrator calls should return the same Arc"
        );

        // Clean up temp file
        let _ = std::fs::remove_file(&db_path);
    }
}
