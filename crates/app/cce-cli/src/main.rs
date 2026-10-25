//! CCE CLI - Command Line Interface for Code Context Engine
//!
//! A standalone CLI client that communicates with the CCE server via HTTP API.

mod cli;
mod client;
mod commands;
mod icons;
mod output;
mod remote;
mod service;

use anyhow::Result;
use clap::Parser;

use crate::cli::Cli;

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let cli = Cli::parse();

    // Initialize logger from configuration
    let _ = cce_config::Settings::init_with_fallback(None);
    if let Ok(cfg) = cce_config::Settings::logger() {
        let _ = cce_logging::init(&cfg);
    } else {
        tracing_subscriber::fmt::init();
    }

    // Execute command
    cli.execute().await
}
