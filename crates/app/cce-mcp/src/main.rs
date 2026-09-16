//! Code Context Engine - MCP server entry point.
//!
//! A standalone, independently-built binary. It loads configuration, builds
//! the shared engine state, and serves the MCP surface over stdio or the
//! streamable HTTP transport, as selected by the `[mcp]` configuration section.

use std::path::Path;
use std::sync::Arc;

use cce_config::modules::McpTransport;
use cce_config::{AppConfig, Settings};
use cce_server_shared::engine::CodeContextEngine;
use cce_server_shared::state::AppState;
use tokio_util::sync::CancellationToken;

fn main() -> anyhow::Result<()> {
    let config_path = std::env::var("CCE_CONFIG").unwrap_or_else(|_| "config.toml".to_string());
    let config_path = Path::new(&config_path);

    Settings::init_from_file(Some(config_path)).unwrap_or_else(|error| {
        eprintln!("Failed to load config from {config_path:?}: {error}; using defaults");
        let default_config = AppConfig::default();
        Settings::init(default_config).expect("failed to initialize default config");
    });

    // MCP is a protocol server: logs must never touch stdout in stdio mode.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config = Settings::global()?.clone();
    let mcp_config = config.mcp.clone();
    if !mcp_config.enabled {
        anyhow::bail!("MCP server is disabled; set `[mcp] enabled = true` in the configuration");
    }

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async move {
        let engine = CodeContextEngine::from_config(config.clone()).await?;
        let state = Arc::new(AppState::from_engine(&engine, None).await);
        let enabled_tools = mcp_config.enabled_tools.clone();

        match mcp_config.transport {
            McpTransport::Stdio => {
                tracing::info!("Starting MCP server over stdio");
                cce_mcp::run_stdio(state, enabled_tools).await
            }
            McpTransport::StreamableHttp => {
                let cancellation_token = CancellationToken::new();
                let shutdown_token = cancellation_token.clone();
                tokio::spawn(async move {
                    if tokio::signal::ctrl_c().await.is_ok() {
                        tracing::info!("Shutdown signal received, stopping MCP server");
                        shutdown_token.cancel();
                    }
                });
                cce_mcp::run_http(
                    state,
                    mcp_config.http.clone(),
                    enabled_tools,
                    cancellation_token,
                )
                .await
            }
        }
    })
}
