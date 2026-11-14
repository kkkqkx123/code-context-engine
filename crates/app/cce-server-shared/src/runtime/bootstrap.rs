//! Process lifecycle bootstrap shared by every server entry point.
//!
//! Starts the background tasks an engine instance needs in order to stay
//! healthy (metrics collection, backend probes, generation GC, checkpoint
//! cleanup, dead-letter retry) and runs startup recovery for the projects
//! already registered in the metadata store.
//!
//! Transport entry points (`cce-server`, `cce-mcp`) call this before they
//! accept connections so both process shapes own the same background
//! behavior. The function never builds a transport, never registers routes
//! and never serves.

use std::sync::Arc;

use cce_storage_common::MetadataStorage;
use cce_storage_vector_qdrant::QdrantProcessHandle;

use crate::engine::{CodeContextEngine, EngineError};
use crate::runtime::StartupCoordinator;

/// Cadence of the periodic collectors and probes, in seconds.
const RUNTIME_METRICS_INTERVAL_SECS: u64 = 60;
const SYSTEM_METRICS_INTERVAL_SECS: u64 = 60;
const QUEUE_METRICS_INTERVAL_SECS: u64 = 10;
const RENDER_CACHE_INTERVAL_SECS: u64 = 5;

/// Cadence and retention of the generation GC pass.
const GENERATION_GC_INTERVAL_SECS: u64 = 3600;
const GENERATION_GC_KEEP_ACTIVE: usize = 2;
const GENERATION_GC_STALE_AFTER_SECS: u64 = 3600;

/// Engine plus the handles a running process must keep alive.
pub struct Bootstrapped {
    /// Engine with every background task already spawned.
    pub engine: Arc<CodeContextEngine>,
    /// Qdrant subprocess handle when this deployment auto-starts Qdrant.
    pub qdrant_control: Option<QdrantProcessHandle>,
    /// Number of projects whose recovery completed without error.
    pub recovered_projects: usize,
}

impl Bootstrapped {
    /// Engine reference for building the transport state.
    pub fn engine_ref(&self) -> &CodeContextEngine {
        &self.engine
    }
}

/// Start background tasks and run startup recovery.
///
/// Probe intervals come from the global settings so deployments tune them
/// through configuration instead of code.
pub async fn bootstrap_engine(mut engine: CodeContextEngine) -> Result<Bootstrapped, EngineError> {
    let config = cce_config::Settings::global()
        .map_err(|e| EngineError::Config(format!("global settings unavailable: {e}")))?;
    let llm_probe_interval = config.metrics.probe.llm_interval_secs;
    let qdrant_probe_interval = config.metrics.probe.qdrant_interval_secs;

    let qdrant_control = engine.start_qdrant_process_manager();
    engine.start_qdrant_connection_monitor(qdrant_probe_interval);
    engine.start_metrics_aggregation_with_cleanup();
    engine.start_runtime_metrics_collection(RUNTIME_METRICS_INTERVAL_SECS);
    engine.start_system_metrics_collection(SYSTEM_METRICS_INTERVAL_SECS);
    engine.start_llm_health_monitor(llm_probe_interval);
    engine.start_queue_metrics(QUEUE_METRICS_INTERVAL_SECS);
    engine.start_render_cache(RENDER_CACHE_INTERVAL_SECS).await;

    let engine = Arc::new(engine);

    let coordinator = StartupCoordinator::new(engine.clone());
    coordinator.start_periodic_checkpoint_cleanup();
    coordinator.start_periodic_dead_letter_retry();
    engine.start_generation_gc_worker(
        GENERATION_GC_INTERVAL_SECS,
        GENERATION_GC_KEEP_ACTIVE,
        GENERATION_GC_STALE_AFTER_SECS,
    );

    let recovered_projects = recover_registered_projects(&coordinator, engine.as_ref()).await;

    Ok(Bootstrapped {
        engine,
        qdrant_control,
        recovered_projects,
    })
}

/// Recover every project the metadata store knows about.
///
/// Returns the number of projects recovered. A project whose recovery fails
/// is logged and skipped so one broken project cannot keep the process from
/// serving the others.
async fn recover_registered_projects(
    coordinator: &StartupCoordinator,
    engine: &CodeContextEngine,
) -> usize {
    let Some(store) = engine.metadata_store() else {
        tracing::debug!("No relation store; skipping startup recovery");
        return 0;
    };

    let project_ids: Vec<i64> = match store.projects_all().await {
        Ok(records) => records.into_iter().map(|record| record.id).collect(),
        Err(e) => {
            tracing::warn!(error = %e, "Failed to enumerate projects for startup recovery");
            return 0;
        }
    };

    if project_ids.is_empty() {
        tracing::debug!("No registered projects; skipping startup recovery");
        return 0;
    }

    tracing::info!(count = project_ids.len(), "Starting startup recovery");
    match coordinator.execute_startup(&project_ids).await {
        Ok(recovered) => {
            tracing::info!(
                recovered,
                total = project_ids.len(),
                "Startup recovery completed"
            );
            recovered
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                "Startup recovery completed with errors (non-critical)"
            );
            0
        }
    }
}
