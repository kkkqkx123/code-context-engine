//! Code Context Engine - HTTP Server Entry Point
//!
//! Minimal entry that initializes configuration and starts the HTTP server.
//! All HTTP logic is in `api::handlers`, all business logic is in `engine`.

use std::path::Path;

use cce_config::{AppConfig, Settings};
use cce_server::logger;

use cce_server::api;
use cce_server::engine::CodeContextEngine;

fn main() -> anyhow::Result<()> {
    // Initialize configuration from file
    let config_path = std::env::var("CCE_CONFIG").unwrap_or_else(|_| "config.toml".to_string());
    let config_path = Path::new(&config_path);

    Settings::init_from_file(Some(config_path)).unwrap_or_else(|e| {
        eprintln!("Failed to load config from file: {}, using defaults", e);
        let default_config = AppConfig::default();
        Settings::init(default_config).expect("Failed to initialize default config");
    });

    // Initialize logger with configuration
    let logger_config =
        Settings::logger().map_err(|e| anyhow::anyhow!("Failed to get logger config: {}", e))?;

    logger::init(&logger_config).unwrap_or_else(|e| {
        eprintln!("Failed to initialize logger: {}, using default tracing", e);
        tracing_subscriber::fmt::init();
    });

    tracing::info!("Configuration loaded successfully");

    // Get server configuration
    let server_config =
        Settings::server().map_err(|e| anyhow::anyhow!("Failed to get server config: {}", e))?;
    let host = server_config.host.as_str();
    let port = server_config.port;

    // Defense in depth: even if validation was bypassed (e.g. programmatic
    // init), never serve a wildcard bind in production. Dev keeps 0.0.0.0
    // for intranet remote debugging via the explicit config file value.
    let environment = cce_config::AppConfig::runtime_environment();
    if let Err(e) = server_config.validate_for_environment(&environment) {
        return Err(anyhow::anyhow!(
            "Refusing to start in environment '{environment}': {e}"
        ));
    }

    // Start HTTP server - create runtime first, then build engine inside it
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        // Build engine (this needs to be inside runtime because RuntimeMetrics requires it)
        let engine = CodeContextEngine::from_config(Settings::global()?.clone()).await?;

        // Process-level shutdown: SIGINT/SIGTERM stop accepting new
        // connections and let in-flight requests drain before exit.
        let shutdown = async {
            let ctrl_c = tokio::signal::ctrl_c();
            #[cfg(unix)]
            {
                let mut sigterm =
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                        .expect("SIGTERM handler must register on unix");
                tokio::select! {
                    _ = ctrl_c => {}
                    _ = sigterm.recv() => {}
                }
            }
            #[cfg(not(unix))]
            {
                let _ = ctrl_c.await;
            }
            tracing::info!("Shutdown signal received, draining in-flight requests");
        };

        tracing::info!("Starting HTTP server on {}:{}", host, port);
        api::serve(engine, host, port, shutdown).await
    })
}
