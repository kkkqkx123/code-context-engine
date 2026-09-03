//! Config command handlers
//!
//! Commands for managing and inspecting server configuration.
//! Supports reload, info, and validate operations.

use anyhow::Result;

use crate::client::ApiClient;
use crate::output::{print_error, print_success, print_warning};
use cce_api::models::{ConfigInfoResponse, ConfigReloadResponse, ConfigValidateResponse};

/// Execute configuration reload
pub async fn execute_reload(server: &str, project_id: i64, verbose: bool) -> Result<()> {
    let client = ApiClient::new(server)?;

    if verbose {
        println!(
            "Triggering configuration reload for project {}...",
            project_id
        );
    }

    let url = format!("/api/config/reload?project_id={}", project_id);
    let empty_body = serde_json::json!({});

    match client
        .post::<serde_json::Value, ConfigReloadResponse>(&url, &empty_body)
        .await
    {
        Ok(response) => {
            if response.success {
                print_success("Configuration reloaded successfully");

                if !response.message.is_empty() {
                    println!("  {}", response.message);
                }
            } else {
                if !response.message.is_empty() {
                    print_error(&format!(
                        "Configuration reload failed: {}",
                        response.message
                    ));
                } else {
                    print_error("Configuration reload failed");
                }
            }
        }
        Err(e) => {
            print_error(&format!("Failed to reload configuration: {}", e));
        }
    }

    Ok(())
}

/// Execute config info
pub async fn execute_info(server: &str, verbose: bool) -> Result<()> {
    let client = ApiClient::new(server)?;

    if verbose {
        println!("Fetching configuration info...");
    }

    match client.get::<ConfigInfoResponse>("/api/config").await {
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
pub async fn execute_validate(server: &str, verbose: bool) -> Result<()> {
    let client = ApiClient::new(server)?;

    if verbose {
        println!("Validating configuration...");
    }

    match client
        .get::<ConfigValidateResponse>("/api/config/validate")
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
                    dep_warning.level, dep_warning.module, dep_warning.message
                );
                match dep_warning.level.as_str() {
                    "error" => print_error(&msg),
                    "warning" => print_warning(&msg),
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
