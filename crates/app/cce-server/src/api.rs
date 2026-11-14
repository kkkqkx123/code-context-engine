//! API module - HTTP server interaction layer
//!
//! Provides HTTP API endpoints for the Code Context Engine. This module only
//! owns the axum transport: request extraction, response mapping, routing and
//! the transport-specific middleware. Shared use cases live in
//! `cce_server_shared::services` and the engine facade.

pub mod handlers;
pub mod log_paths;
pub mod middleware;
pub mod openapi;
pub mod response;
pub mod router;
pub mod validation;

#[cfg(feature = "admission")]
use std::sync::Arc;

use cce_server_shared::context::SharedContext;
use cce_server_shared::engine::CodeContextEngine;

/// Start the HTTP server.
///
/// Hands the engine to the shared lifecycle bootstrap (background tasks plus
/// startup recovery), builds the transport state, then serves the axum
/// router. The `shutdown` future is supplied by the caller (process-level
/// lifecycle concern, e.g. signal handling in `main`); when it resolves the
/// server stops accepting connections and drains in-flight requests.
pub async fn serve(
    engine: CodeContextEngine,
    host: &str,
    port: u16,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> anyhow::Result<()> {
    let bootstrapped = cce_server_shared::runtime::bootstrap_engine(engine).await?;
    tracing::info!(
        recovered_projects = bootstrapped.recovered_projects,
        "Background lifecycle started"
    );

    // Reclaim expired ingest chunk staging left by interrupted pushes
    // before any request can resume from it.
    #[cfg(feature = "admission")]
    handlers::ingest::sweep_expired_staging(handlers::ingest::INGEST_STAGING_RETENTION);

    let app_state = SharedContext::from_engine(
        bootstrapped.engine_ref(),
        bootstrapped.qdrant_control.clone(),
    )
    .await;
    spawn_config_watcher();
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

// Spawn the optional global config file watcher.
//
// Disabled by default; enabled with `CCE_CONFIG_WATCH=1`. Polls the
// resolved config file for modification time changes and applies safe
// section reloads. Dangerous changes log a warning and keep serving the
// previous configuration until a restart.
fn spawn_config_watcher() {
    if !cce_config::Settings::config_watch_enabled() {
        return;
    }
    let path = cce_config::Settings::resolve_config_path(None);
    tracing::info!(path = %path, "Config file watcher enabled");
    tokio::spawn(async move {
        let mut last_mtime: Option<std::time::SystemTime> =
            std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            let current = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
            if current != last_mtime {
                last_mtime = current;
                let probe = std::path::PathBuf::from(&path);
                match cce_config::Settings::reload_safe_from_file(Some(probe.as_path())) {
                    Ok(version) => {
                        tracing::info!(version, "Global configuration reloaded from watched file")
                    }
                    Err(error) => tracing::warn!(
                        error = %error,
                        "Watched config change refused; restart required for dangerous sections"
                    ),
                }
            }
        }
    });
}

#[cfg(feature = "admission")]
/// Assemble the router for admission-enabled builds.
///
/// A loopback bind without configured tokens keeps the local router so
/// single-host use is unaffected; any other bind requires configured tokens
/// and serves the admission router with the gateway ingest entries.
fn admission_app(app_state: SharedContext, host: &str) -> anyhow::Result<axum::Router> {
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
