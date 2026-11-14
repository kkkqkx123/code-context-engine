//! Configuration reload service shared by the HTTP and MCP surfaces.
//!
//! A reload has two scopes: the global safe-section reload and the per-project
//! rebuild. Both invalidate the caches that derived state from configuration
//! and rebuild the hot-update processor set from the stored project profile.

use std::path::PathBuf;
use std::sync::Arc;

use cce_config::{RelationConfig, Settings};
use cce_orchestrator::ExportConfig;
use cce_orchestrator::hot_update::processors::UpdateProcessor;
use cce_orchestrator::hot_update::processors::factory::{ProcessorConfig, ProcessorFactory};

use crate::context::SharedContext;

/// Reload the global configuration's safe sections.
///
/// Dangerous sections are refused by the configuration layer; a refusal is
/// reported as an error so the caller can ask for a restart. Returns the new
/// global version.
pub async fn reload_global(state: &SharedContext) -> Result<u64, String> {
    let version = Settings::reload_safe_from_file(None).map_err(|error| error.to_string())?;
    state.engine.clear_all_component_caches().await;
    let _ = state.engine.project_registry().invalidate_cache(None).await;
    state.invalidate_relation_searcher_all().await;
    Ok(version)
}

/// Reload one project: engine components, cached searchers and the hot-update
/// processor set built from the stored profile.
pub async fn reload_project(state: &SharedContext, project_id: i64) -> Result<(), String> {
    let engine = &state.engine;
    engine
        .reload_project_config(project_id)
        .await
        .map_err(|error| error.to_string())?;
    state.invalidate_relation_searcher(project_id).await;

    let coordinator = match engine.get_hot_update_coordinator(project_id).await {
        Ok(coordinator) => coordinator,
        Err(error) => {
            tracing::warn!(
                project_id,
                error = %error,
                "HotUpdateCoordinator not available, config cache cleared but runtime not hot-reloaded"
            );
            return Ok(());
        }
    };
    let coord = coordinator.lock().await;
    let relation_config = match engine.project_registry().get_or_load(project_id).await {
        Ok(entry) => entry.config.relation.clone(),
        Err(error) => {
            tracing::warn!(
                project_id,
                error = %error,
                "Using default relation configuration during config reload"
            );
            RelationConfig::default()
        }
    };
    let project_root = coord
        .watched_dirs()
        .first()
        .cloned()
        .unwrap_or_else(|| PathBuf::from("."));
    let project_group_id =
        cce_storage_common::generate_project_group_id(project_id, &project_root.to_string_lossy());
    let relation_publisher = match engine.get_relation_snapshot_publisher(project_id).await {
        Ok(publisher) => Some(publisher),
        Err(error) => {
            tracing::warn!(
                project_id,
                error = %error,
                "Relation processor will remain unavailable during config reload"
            );
            None
        }
    };
    // Build the processor set from the project configuration so export and
    // summary processors reflect the stored profile (export gated on
    // `export.enabled`, summary generation on `store_summaries`).
    let project_config = match engine.project_registry().get_or_load(project_id).await {
        Ok(entry) => entry.config.clone(),
        Err(error) => {
            tracing::warn!(
                project_id,
                error = %error,
                "Using default configuration during config reload"
            );
            cce_config::AppConfig::default()
        }
    };
    let mut processor_config = ProcessorConfig::new();
    processor_config.enable_export = project_config.export.enabled;
    processor_config.enable_summary = project_config.orchestrator.indexer.store_summaries;
    processor_config.export_config = if project_config.export.enabled {
        Some(ExportConfig::from_module_config(
            &project_config.export,
            project_root.clone(),
            project_id,
        ))
    } else {
        None
    };
    // The summary generator follows the project summary config (same as the
    // full-index path) so hot-update summaries stay consistent.
    let summary_generator: Option<Arc<dyn cce_parser::summary::SummaryGenerator>> =
        processor_config.enable_summary.then(|| {
            let generator: Arc<dyn cce_parser::summary::SummaryGenerator> =
                Arc::new(cce_parser::summary::RuleBasedGenerator::with_config(
                    project_config.summary.clone(),
                ));
            generator
        });
    let processors = ProcessorFactory::new().create_all_processors(
        Some(state.engine.vector_clone()),
        Some(state.engine.fulltext_clone()),
        state
            .engine
            .metadata_store_clone()
            .as_ref()
            .and_then(|client| client.for_project(project_id).ok())
            .or_else(|| state.engine.metadata_store_clone()),
        Some(state.engine.embedder_clone()),
        Some(project_group_id),
        project_id,
        relation_publisher,
        &relation_config,
        coord.checkpoint_manager().await,
        summary_generator,
        Some(&project_config.ast_to_nl),
        &project_config.grouper,
        &project_config.license_header,
        Some(&project_config.summary),
        &processor_config,
        engine.get_plugin_registry(project_id).await,
        None,
        Some(cce_metrics::HotUpdateStorageMetrics::new(
            state.engine.metrics_registry(),
            project_id,
        )),
    );
    let (processors, _storage_coordinator) = match processors {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!(
                project_id,
                error = %error,
                "Failed to rebuild processors during config reload; keeping the previous set"
            );
            return Ok(());
        }
    };
    let processor_refs: Vec<&dyn UpdateProcessor> = processors
        .iter()
        .map(|processor| processor.as_ref())
        .collect();
    if let Err(error) = coord.process_pending_config_changes(&processor_refs).await {
        tracing::warn!(
            project_id,
            error = %error,
            "Hot-update config reload failed (non-critical)"
        );
    }
    Ok(())
}
