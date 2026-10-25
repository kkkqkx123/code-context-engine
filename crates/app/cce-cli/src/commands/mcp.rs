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

use super::process as proc;

pub async fn execute(cmd: &McpCommands) -> Result<()> {
    let (config, binary) = match cmd {
        McpCommands::Stdio { config, binary } | McpCommands::Http { config, binary } => {
            (config.clone(), binary.clone())
        }
    };
    let binary_path = proc::resolve_binary(binary.as_deref(), proc::MCP_BINARY_NAME)?;
    let mut command = Command::new(&binary_path);
    if let Some(path) = config {
        command.env("CCE_CONFIG", path);
    }
    let mut child = command
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| {
            format!(
                "failed to launch {}; is it installed on PATH?",
                binary_path.display()
            )
        })?;
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
        /// Explicit MCP binary path (default: auto-discovered `cce-mcp`)
        #[arg(long)]
        binary: Option<String>,
    },

    /// Serve MCP over the streamable HTTP transport
    Http {
        /// Optional config file path (defaults to config.toml or $CCE_CONFIG)
        #[arg(short, long)]
        config: Option<String>,
        /// Explicit MCP binary path (default: auto-discovered `cce-mcp`)
        #[arg(long)]
        binary: Option<String>,
    },
}
