//! Status command handler

use anyhow::Result;
use colored::Colorize;
use serde::Serialize;

use cce_transport::SharedHttpClient;
use crate::icons;
use crate::output::{print_error, print_output, print_success};
use cce_api::models::{HealthStatus, StorageComponentStatus, StorageStatus, StorageStatusResponse};

/// Combined status response for JSON output
#[derive(Debug, Serialize)]
struct CombinedStatus {
    server_url: String,
    healthy: bool,
    storage: Option<StorageStatus>,
}

pub async fn execute_with_client(client: &SharedHttpClient, verbose: bool, format: crate::cli::OutputFormat) -> Result<()> {
    if verbose {
        println!("Checking server status: {}", client.base_url());
    }

    let healthy: bool;
    let mut storage_status = None;

    match client.get::<HealthStatus>(cce_api::paths::HEALTH).await {
        Ok(health) => {
            healthy = health.healthy;

            if !matches!(format, crate::cli::OutputFormat::Json) {
                if health.healthy {
                    print_success(&format!("Server is healthy at {}", client.base_url()));
                } else {
                    print_error(&format!("Server is unhealthy at {}", client.base_url()));
                }

                println!();
                println!("Service Status:");
                println!("{}", "─".repeat(50));
                print_service_status("Qdrant", health.qdrant.reachable, &health.qdrant.message);
                print_service_status("BM25", health.bm25.reachable, &health.bm25.message);
                print_service_status(
                    "Embedding",
                    health.embedding.reachable,
                    &health.embedding.message,
                );
                println!();
            }

            // Also fetch storage status for detailed info
            if let Ok(response) = client
                .get::<StorageStatusResponse>(cce_api::paths::STORAGE_STATUS)
                .await
            {
                if response.success {
                    storage_status = Some(response.status);

                    if !matches!(format, crate::cli::OutputFormat::Json) {
                        println!("Storage Status:");
                        println!("{}", "─".repeat(50));

                        let status = storage_status.as_ref().unwrap();

                        println!("\nVector Storage (Qdrant):");
                        print_component_status("  ", &status.vector_storage);

                        println!("\nBM25 Storage:");
                        print_component_status("  ", &status.bm25_storage);

                        println!("\nRelation Storage:");
                        print_component_status("  ", &status.relation_storage);

                        println!("\n{}", "─".repeat(50));
                        println!("Total Disk Usage: {:.2} MB", status.total_disk_usage_mb);
                    }
                }
            }
        }
        Err(e) => {
            if matches!(format, crate::cli::OutputFormat::Json) {
                let status = CombinedStatus {
                    server_url: client.base_url().to_string(),
                    healthy: false,
                    storage: None,
                };
                print_output(format, &status);
            } else {
                print_error(&format!("Failed to get health status: {}", e));
            }
            return Ok(());
        }
    }

    if matches!(format, crate::cli::OutputFormat::Json) {
        let status = CombinedStatus {
            server_url: client.base_url().to_string(),
            healthy,
            storage: storage_status,
        };
        print_output(format, &status);
    }

    Ok(())
}

/// Print component status with formatting
fn print_component_status(prefix: &str, component: &StorageComponentStatus) {
    let status_icon = if component.connected {
        icons::SUCCESS.to_string()
    } else {
        icons::ERROR.to_string()
    };

    let status_color = if component.connected {
        format!("{} Connected", status_icon).green()
    } else {
        format!("{} Disconnected", status_icon).red()
    };

    println!("{}Status: {}", prefix, status_color);
    println!("{}Items: {}", prefix, component.item_count);
    println!("{}Disk Usage: {:.2} MB", prefix, component.disk_usage_mb);

    if let Some(ref error) = component.last_error {
        println!("{}Last Error: {}", prefix, error.red());
    }
}

/// Print individual service status
fn print_service_status(name: &str, reachable: bool, message: &str) {
    let icon = if reachable {
        icons::SUCCESS
    } else {
        icons::ERROR
    };
    let colored_name = if reachable {
        format!("{} {}", icon, name).green()
    } else {
        format!("{} {}", icon, name).red()
    };
    println!("{}", colored_name);
    println!("  Message: {}", message);
}
