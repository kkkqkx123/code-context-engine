//! API module - HTTP server interaction layer
//!
//! Provides HTTP API endpoints for the Code Context Engine.
//! The server is a thin HTTP wrapper around the Engine facade -
//! all business logic resides in the orchestrator layer.

pub mod handlers;
pub mod middleware;
pub mod response;
pub mod router;
pub mod state;
pub mod validation;

use std::sync::Arc;

use crate::engine::CodeContextEngine;
use crate::runtime::StartupCoordinator;
use cce_storage_sqlite::ProjectRepository;

/// Start the HTTP server
///
/// Accepts an Engine instance, builds AppState, starts background tasks,
/// runs startup recovery for all projects, and starts the axum server.
pub async fn serve(mut engine: CodeContextEngine, host: &str, port: u16) -> anyhow::Result<()> {
    // Start Qdrant subprocess manager (if auto_start is configured)
    let qdrant_handle = engine.start_qdrant_process_manager();

    // Start Qdrant connection health monitor
    engine.start_qdrant_connection_monitor();

    // Start metrics aggregation with automatic TTL cleanup
    engine.start_metrics_aggregation_with_cleanup();

    // Start runtime metrics collection (every 60s)
    engine.start_runtime_metrics_collection(60);

    // Start system metrics collection (every 60s)
    engine.start_system_metrics_collection(60);

    // Start queue backpressure metrics collection (every 10s)
    engine.start_queue_metrics(10);

    // Start single-core metric render cache (every 5s)
    engine.start_render_cache(5).await;

    // Schedule the periodic checkpoint TTL cleanup (interval and TTL from the
    // orchestrator config; per-project TTL overrides are applied by the task)
    let engine_arc = Arc::new(engine);
    let coordinator = StartupCoordinator::new(engine_arc.clone());
    coordinator.start_periodic_checkpoint_cleanup();

    // Start background generation GC worker (scans hourly, retains 2 active generations)
    engine_arc.start_generation_gc_worker(3600, 2, 3600);

    // Run startup recovery for all projects before accepting requests
    let project_ids: Vec<i64> = {
        if let Some(store) = engine_arc.metadata_store() {
            let client = store.as_ref();
            match client.with_transaction(|tx| ProjectRepository::get_all(tx)) {
                Ok(records) => records.into_iter().map(|r| r.id).collect(),
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "Failed to enumerate projects for startup recovery"
                    );
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        }
    };

    if !project_ids.is_empty() {
        tracing::info!(
            count = project_ids.len(),
            "Starting startup recovery for all projects"
        );
        match coordinator.execute_startup(&project_ids).await {
            Ok(recovered) => {
                tracing::info!(
                    recovered = recovered,
                    total = project_ids.len(),
                    "Startup recovery completed"
                );
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "Startup recovery completed with errors (non-critical)"
                );
            }
        }
    }
    drop(coordinator);
    engine = Arc::into_inner(engine_arc)
        .expect("engine Arc must have exactly one reference after coordinator drops");

    let app_state = state::AppState::from_engine(&engine, qdrant_handle).await;
    let app = router::create_router(app_state);

    let listener = tokio::net::TcpListener::bind(format!("{}:{}", host, port)).await?;
    tracing::info!("Server listening on http://{}:{}", host, port);

    axum::serve(listener, app).await?;

    Ok(())
}

// Re-export for convenience
pub use state::AppState;
