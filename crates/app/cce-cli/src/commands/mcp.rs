//! MCP server command handler
//!
//! Runs the MCP server locally (in-process) rather than proxying through the
//! HTTP API: MCP is a protocol endpoint, so the CLI simply builds the shared
//! engine state and hands it to `cce-mcp`.

use anyhow::Result;
use std::path::Path;

use cce_config::{AppConfig, Settings};
use tokio_util::sync::CancellationToken;

use crate::cli::McpCommands;

pub async fn execute(cmd: &McpCommands) -> Result<()> {
    match cmd {
        McpCommands::Stdio { config } => {
            let config = load_config(config.as_deref())?;
            let enabled_tools = config.mcp.enabled_tools.clone();
            eprintln!("Starting MCP server over stdio (logs on stderr)");
            cce_mcp::run_stdio_from_config(config, enabled_tools).await
        }
        McpCommands::Http { config, host, port } => {
            let config = load_config(config.as_deref())?;
            let enabled_tools = config.mcp.enabled_tools.clone();
            let mut http = config.mcp.http.clone();
            if let Some(host) = host {
                http.host = host.clone();
            }
            if let Some(port) = port {
                http.port = *port;
            }
            let cancellation_token = CancellationToken::new();
            let shutdown_token = cancellation_token.clone();
            tokio::spawn(async move {
                if tokio::signal::ctrl_c().await.is_ok() {
                    shutdown_token.cancel();
                }
            });
            cce_mcp::run_http_from_config(config, http, enabled_tools, cancellation_token).await
        }
    }
}

/// Resolve the configuration, preferring an explicit path, then `$CCE_CONFIG`,
/// then the default `config.toml`, falling back to defaults.
fn load_config(explicit: Option<&str>) -> Result<AppConfig> {
    let path = explicit
        .map(str::to_string)
        .or_else(|| std::env::var("CCE_CONFIG").ok())
        .unwrap_or_else(|| "config.toml".to_string());

    match Settings::init_from_file(Some(Path::new(&path))) {
        Ok(()) => Ok(Settings::global()?.clone()),
        Err(error) => {
            eprintln!("Failed to load config from {path}: {error}; using defaults");
            let default_config = AppConfig::default();
            Settings::init(default_config.clone())?;
            Ok(default_config)
        }
    }
}
