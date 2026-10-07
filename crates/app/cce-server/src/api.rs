//! API module - HTTP server interaction layer
//!
//! Provides HTTP API endpoints for the Code Context Engine.
//! The server is a thin HTTP wrapper around the Engine facade -
//! all business logic resides in the orchestrator layer.

pub mod handlers;
pub mod middleware;
pub mod openapi;
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
///
/// The `shutdown` future is supplied by the caller (process-level lifecycle
/// concern, e.g. signal handling in `main`); when it resolves the server
/// stops accepting connections and drains in-flight requests.
pub async fn serve(
    mut engine: CodeContextEngine,
    host: &str,
    port: u16,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> anyhow::Result<()> {
    // Start Qdrant subprocess manager (if auto_start is configured)
    let qdrant_handle = engine.start_qdrant_process_manager();

    // Health-probe cadences come from the metrics config so deployments can
    // tune them without code changes. Fall back to historical defaults when
    // global settings are unavailable (tests, embedded usage).
    let (llm_probe_interval_secs, qdrant_probe_interval_secs) = cce_config::Settings::global()
        .map_or((60, 30), |config| {
            (
                config.metrics.probe.llm_interval_secs,
                config.metrics.probe.qdrant_interval_secs,
            )
        });

    // Start Qdrant connection health monitor
    engine.start_qdrant_connection_monitor(qdrant_probe_interval_secs);

    // Start metrics aggregation with automatic TTL cleanup
    engine.start_metrics_aggregation_with_cleanup();

    // Start runtime metrics collection (every 60s)
    engine.start_runtime_metrics_collection(60);

    // Start system metrics collection (every 60s)
    engine.start_system_metrics_collection(60);

    // Start LLM provider health probing
    engine.start_llm_health_monitor(llm_probe_interval_secs);

    // Start queue backpressure metrics collection (every 10s)
    engine.start_queue_metrics(10);

    // Start single-core metric render cache (every 5s)
    engine.start_render_cache(5).await;

    // Schedule the periodic checkpoint TTL cleanup (interval and TTL from the
    // orchestrator config; per-project TTL overrides are applied by the task)
    let engine_arc = Arc::new(engine);
    let coordinator = StartupCoordinator::new(engine_arc.clone());
    coordinator.start_periodic_checkpoint_cleanup();
    coordinator.start_periodic_dead_letter_retry();

    // Start background generation GC worker (scans hourly, retains 2 active generations)
    engine_arc.start_generation_gc_worker(3600, 2, 3600);

    // Reclaim expired ingest chunk staging left by interrupted pushes
    // before any request can resume from it.
    #[cfg(feature = "admission")]
    handlers::ingest::sweep_expired_staging(handlers::ingest::INGEST_STAGING_RETENTION);

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
    #[cfg(feature = "admission")]
    let app = admission_app(app_state, host)?;
    #[cfg(not(feature = "admission"))]
    let app = router::create_router(app_state);

    let listener = tokio::net::TcpListener::bind(format!("{}:{}", host, port)).await?;
    tracing::info!("Server listening on http://{}:{}", host, port);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await?;

    tracing::info!("Server stopped");

    Ok(())
}

// Re-export for convenience
pub use state::AppState;

#[cfg(feature = "admission")]
/// Assemble the router for admission-enabled builds.
///
/// A loopback bind without configured tokens keeps the local router so
/// single-host use is unaffected; any other bind requires configured tokens
/// and serves the admission router with the gateway ingest entries.
fn admission_app(app_state: state::AppState, host: &str) -> anyhow::Result<axum::Router> {
    use cce_metrics::HttpMetrics;

    let config = cce_admission::AdmissionConfig::from_env()
        .map_err(|e| anyhow::anyhow!("Failed to load admission config: {e}"))?;
    if cce_admission::requires_admission(host) && !config.is_configured() {
        return Err(anyhow::anyhow!(
            "Refusing to serve non-loopback host '{host}' without admission tokens; set CCE_ADMISSION_TOKENS"
        ));
    }
    if !config.is_configured() {
        return Ok(router::create_router(app_state));
    }
    tracing::info!("Admission layer enabled for remote hosting");
    let admission_metrics = Arc::new(cce_admission::AdmissionMetrics::default());
    let gate = Arc::new(cce_admission::AdmissionGate::new(
        &config,
        Arc::clone(&admission_metrics),
    ));
    let http_metrics = HttpMetrics::new(app_state.engine.metrics_registry());
    let admission_layer = axum::middleware::from_fn_with_state(
        Arc::clone(&gate),
        cce_admission::admission_middleware,
    );
    let timeout_layer =
        axum::middleware::from_fn_with_state(Arc::clone(&gate), cce_admission::timeout_middleware);
    let cors_layer =
        axum::middleware::from_fn_with_state(Arc::clone(&gate), cce_admission::cors_middleware);
    Ok(router::api_routes()
        .merge(handlers::ingest::ingest_routes(admission_metrics))
        .with_state(app_state)
        .layer(admission_layer)
        .layer(timeout_layer)
        .layer(cors_layer)
        .layer(axum::middleware::from_fn(middleware::metrics_middleware(
            http_metrics,
        ))))
}
