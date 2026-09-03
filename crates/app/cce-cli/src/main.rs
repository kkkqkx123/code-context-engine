//! CCE CLI - Command Line Interface for Code Context Engine
//!
//! A standalone CLI client that communicates with the CCE server via HTTP API.

mod cli;
mod client;
mod commands;
mod output;

use anyhow::Result;
use clap::Parser;

use crate::cli::Cli;

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let cli = Cli::parse();

    // Initialize logger
    tracing_subscriber::fmt::init();

    // Execute command
    cli.execute().await
}
