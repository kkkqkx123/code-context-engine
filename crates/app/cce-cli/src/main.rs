//! CCE CLI - Command Line Interface for Code Context Engine
//!
//! Two responsibilities live side by side here:
//!
//! - **HTTP client**: a standalone CLI that talks to a running CCE server
//!   through the API client crate.
//! - **Local operations entrypoint**: process supervision for the server
//!   binary and the embedded gateway. This is a controlled local-deployment
//!   exception, not a place to add business logic; domain behavior belongs to
//!   the shared application services.

mod cli;
mod commands;
mod icons;
mod output;

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
