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

    // Start HTTP server - create runtime first, then build engine inside it
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        // Build engine (this needs to be inside runtime because RuntimeMetrics requires it)
        let engine = CodeContextEngine::from_config(Settings::global()?.clone()).await?;

        tracing::info!("Starting HTTP server on {}:{}", host, port);
        api::serve(engine, host, port).await
    })
}
