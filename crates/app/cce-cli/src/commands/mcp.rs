//! MCP server command handler
//!
//! Runs the MCP server locally (in-process) rather than proxying through the
//! HTTP API: MCP is a protocol endpoint, so the CLI simply builds the shared
//! engine state and hands it to `cce-mcp`.

use clap::Subcommand;

use anyhow::Result;

use cce_config::{AppConfig, Settings};
use tokio_util::sync::CancellationToken;

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
    Ok(Settings::init_with_fallback(explicit))
}

/// MCP server commands
#[derive(Subcommand)]
pub enum McpCommands {
    /// Serve MCP over stdio (for local MCP clients)
    Stdio {
        /// Optional config file path (defaults to config.toml or $CCE_CONFIG)
        #[arg(short, long)]
        config: Option<String>,
    },

    /// Serve MCP over the streamable HTTP transport
    Http {
        /// Optional config file path (defaults to config.toml or $CCE_CONFIG)
        #[arg(short, long)]
        config: Option<String>,

        /// Override the bind host (defaults to the [mcp.http] config)
        #[arg(long)]
        host: Option<String>,

        /// Override the bind port (defaults to the [mcp.http] config)
        #[arg(long)]
        port: Option<u16>,
    },
}
