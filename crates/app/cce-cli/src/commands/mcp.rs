//! MCP server command handler
//!
//! The CLI is a pure client: it never embeds the MCP server in-process.
//! This command spawns the standalone `cce-mcp` binary (which reads the
//! `[mcp]` section from the configuration) and waits for it to exit,
//! forwarding the child's exit code.

use std::process::Stdio;

use clap::Subcommand;

use anyhow::{Context, Result};
use tokio::process::Command;

pub async fn execute(cmd: &McpCommands) -> Result<()> {
    let config = match cmd {
        McpCommands::Stdio { config } | McpCommands::Http { config, .. } => config.clone(),
    };
    let mut command = Command::new("cce-mcp");
    if let Some(path) = config {
        command.env("CCE_CONFIG", path);
    }
    let mut child = command
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .context("failed to launch the cce-mcp binary; is it installed on PATH?")?;
    let status = child.wait().await.context("cce-mcp exited abnormally")?;
    std::process::exit(status.code().unwrap_or(1));
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
    },
}
