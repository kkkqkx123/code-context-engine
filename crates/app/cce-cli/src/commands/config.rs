//! Config command handlers
//!
//! Commands for managing and inspecting server configuration.
//! Supports reload, info, and validate operations.

use clap::Subcommand;

use anyhow::Result;

use crate::client::CliApi;
use crate::output::{print_error, print_success, print_warning};
use cce_api::models::{ConfigInfoResponse, ConfigValidateResponse};
use cce_transport::SharedHttpClient;

/// Execute configuration reload through the client abstraction.
pub async fn execute_reload_with_client(
    client: &SharedHttpClient,
    project_id: i64,
    verbose: bool,
) -> Result<()> {
    if verbose {
        println!(
            "Triggering configuration reload for project {}...",
            project_id
        );
    }
    match client.reload_project(project_id).await {
        Ok(response) => {
            if response.success {
                print_success("Configuration reloaded successfully");
                if !response.message.is_empty() {
                    println!("  {}", response.message);
                }
            } else if !response.message.is_empty() {
                print_error(&format!(
                    "Configuration reload failed: {}",
                    response.message
                ));
            } else {
                print_error("Configuration reload failed");
            }
        }
        Err(e) => {
            print_error(&format!("Failed to reload configuration: {e}"));
        }
    }
    Ok(())
}

/// Execute config info
pub async fn execute_info_with_client(client: &SharedHttpClient, verbose: bool) -> Result<()> {
    if verbose {
        println!("Fetching configuration info...");
    }

    match client
        .get::<ConfigInfoResponse>(cce_api::paths::CONFIG_INFO)
        .await
    {
        Ok(response) => {
            println!("  Initialized: {}", response.initialized);
            println!("  Project count: {}", response.project_count);
            println!(
                "  Database: {}",
                serde_json::to_string_pretty(&response.database).unwrap_or_default()
            );
            println!(
                "  Embedder: {}",
                serde_json::to_string_pretty(&response.embedder).unwrap_or_default()
            );
        }
        Err(e) => {
            print_error(&format!("Failed to fetch config info: {}", e));
        }
    }

    Ok(())
}

/// Execute config validation
pub async fn execute_validate_with_client(client: &SharedHttpClient, verbose: bool) -> Result<()> {
    if verbose {
        println!("Validating configuration...");
    }

    match client
        .get::<ConfigValidateResponse>(cce_api::paths::CONFIG_VALIDATE)
        .await
    {
        Ok(response) => {
            if response.valid {
                print_success("Configuration is valid");
            } else {
                print_error("Configuration is invalid");
            }

            for error in &response.errors {
                print_error(&format!("  Error: {}", error));
            }

            for warning in &response.warnings {
                print_warning(&format!("  Warning: {}", warning));
            }

            for dep_warning in &response.dependency_warnings {
                let msg = format!(
                    "  [{}][{}] {}",
                    dep_warning.severity, dep_warning.field, dep_warning.suggestion
                );
                match dep_warning.severity.as_str() {
                    "Error" => print_error(&msg),
                    "Warning" => print_warning(&msg),
                    _ => println!("  {}", msg),
                }
            }
        }
        Err(e) => {
            print_error(&format!("Failed to validate configuration: {}", e));
        }
    }

    Ok(())
}

/// Config commands
#[derive(Subcommand)]
pub enum ConfigCommands {
    /// Reload configuration
    Reload {
        /// Project ID
        #[arg(short = 'P', long)]
        project_id: i64,
    },

    /// Show current configuration info
    Info,

    /// Validate configuration
    Validate,
}
