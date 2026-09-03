//! Health checking and retry queue management commands

use anyhow::Result;
use colored::Colorize;

use crate::client::ApiClient;
use crate::output::{print_error, print_output, print_success, print_warning};
use cce_api::models::{
    Bm25HealthResponse, EmbeddingHealthResponse, HealthStatus, QdrantHealthResponse,
    RetryQueueClearResponse, RetryQueueProcessResponse, RetryQueueStatusResponse,
};

/// Health command variants
pub enum HealthCommand {
    /// Full health check
    Check,
    /// Qdrant health check
    Qdrant,
    /// Embedding health check
    Embedding,
    /// BM25 health check
    Bm25,
    /// Get retry queue status
    QueueStatus,
    /// Process retry queue
    QueueProcess,
    /// Clear retry queue
    QueueClear,
}

/// Execute a health command
pub async fn execute(
    cmd: &HealthCommand,
    server: &str,
    verbose: bool,
    format: crate::cli::OutputFormat,
) -> Result<()> {
    let client = ApiClient::new(server)?;

    match cmd {
        HealthCommand::Check => health_check(&client, verbose, format).await,
        HealthCommand::Qdrant => health_qdrant(&client, verbose).await,
        HealthCommand::Embedding => health_embedding(&client, verbose).await,
        HealthCommand::Bm25 => health_bm25(&client, verbose).await,
        HealthCommand::QueueStatus => retry_queue_status(&client, verbose).await,
        HealthCommand::QueueProcess => retry_queue_process(&client, verbose).await,
        HealthCommand::QueueClear => retry_queue_clear(&client, verbose).await,
    }
}

/// Full health check
async fn health_check(
    client: &ApiClient,
    verbose: bool,
    format: crate::cli::OutputFormat,
) -> Result<()> {
    if verbose {
        println!("{}", "Fetching unified health status...".cyan());
    }

    match client.get::<HealthStatus>("/api/health").await {
        Ok(response) => {
            if matches!(format, crate::cli::OutputFormat::Json) {
                print_output(format, &response);
            } else {
                println!("{}", "Unified Health Status".bold());
                println!("{}", "═".repeat(50));
                println!();
                print_service_status("Qdrant", &response.qdrant);
                print_service_status("BM25", &response.bm25);
                print_service_status("Embedding", &response.embedding);
                println!();
                if response.healthy {
                    print_success("All services are healthy");
                } else {
                    print_error("Some services are unhealthy");
                }
            }
        }
        Err(e) => {
            if matches!(format, crate::cli::OutputFormat::Json) {
                let error = serde_json::json!({
                    "success": false,
                    "error": format!("Failed to get health status: {}", e)
                });
                print_output(format, &error);
            } else {
                print_error(&format!("Failed to get health status: {}", e));
            }
        }
    }

    Ok(())
}

/// Print individual service status
fn print_service_status(name: &str, status: &cce_api::models::ServiceStatus) {
    let icon = if status.reachable { "✓" } else { "✗" };
    let colored_name = if status.reachable {
        format!("{} {}", icon, name).green()
    } else {
        format!("{} {}", icon, name).red()
    };
    println!("{}", colored_name);
    println!("  Message: {}", status.message);
}

/// Qdrant health check
async fn health_qdrant(client: &ApiClient, verbose: bool) -> Result<()> {
    if verbose {
        println!("{}", "Fetching Qdrant health...".cyan());
    }

    match client
        .get::<QdrantHealthResponse>("/api/health/qdrant")
        .await
    {
        Ok(response) => {
            println!("{}", "Qdrant Health".bold());
            println!("{}", "═".repeat(50));
            println!();
            print_health_bool("Healthy", response.healthy);
            print_value("Circuit Breaker", &response.circuit_breaker);
            println!();
            println!("{}", "Diagnostic:".bold());
            print_health_bool("  Reachable", response.diagnostic.reachable);
            if let Some(version) = &response.diagnostic.version {
                print_value("  Version", version);
            }
            print_health_bool("  Collection Exists", response.diagnostic.collection_exists);
            print_value(
                "  Points Count",
                &response.diagnostic.points_count.to_string(),
            );
            if let Some(error) = &response.diagnostic.error {
                println!("  Error: {}", error.red());
            }
        }
        Err(e) => {
            print_error(&format!("Failed to get Qdrant health: {}", e));
        }
    }

    Ok(())
}

/// Embedding health check
async fn health_embedding(client: &ApiClient, verbose: bool) -> Result<()> {
    if verbose {
        println!("{}", "Fetching Embedding health...".cyan());
    }

    match client
        .get::<EmbeddingHealthResponse>("/api/health/embedding")
        .await
    {
        Ok(response) => {
            println!("{}", "Embedding Health".bold());
            println!("{}", "═".repeat(50));
            println!();
            print_health_bool("Healthy", response.healthy);
            if let Some(model_name) = &response.model_name {
                print_value("Model Name", model_name);
            }
            print_value("Message", &response.message);
        }
        Err(e) => {
            print_error(&format!("Failed to get Embedding health: {}", e));
        }
    }

    Ok(())
}

/// BM25 health check
async fn health_bm25(client: &ApiClient, verbose: bool) -> Result<()> {
    if verbose {
        println!("{}", "Fetching BM25 health...".cyan());
    }

    match client.get::<Bm25HealthResponse>("/api/health/bm25").await {
        Ok(response) => {
            println!("{}", "BM25 Health".bold());
            println!("{}", "═".repeat(50));
            println!();
            print_health_bool("Enabled", response.enabled);
            print_health_bool("Connected", response.connected);
            if let Some(path) = &response.index_path {
                print_value("Index Path", path);
            }
        }
        Err(e) => {
            print_error(&format!("Failed to get BM25 health: {}", e));
        }
    }

    Ok(())
}

/// Get retry queue status
async fn retry_queue_status(client: &ApiClient, verbose: bool) -> Result<()> {
    if verbose {
        println!("{}", "Fetching retry queue status...".cyan());
    }

    match client
        .get::<RetryQueueStatusResponse>("/api/retry-queue")
        .await
    {
        Ok(response) => {
            println!("{}", "Retry Queue Status".bold());
            println!("{}", "═".repeat(50));
            println!();
            print_value("Pending Count", &response.pending_count.to_string());
            if response.is_empty {
                println!("{}", "Queue is empty".green());
            } else {
                print_warning(&format!(
                    "Queue has {} pending queries",
                    response.pending_count
                ));
            }
        }
        Err(e) => {
            print_error(&format!("Failed to get retry queue status: {}", e));
        }
    }

    Ok(())
}

/// Process retry queue
async fn retry_queue_process(client: &ApiClient, verbose: bool) -> Result<()> {
    if verbose {
        println!("{}", "Processing retry queue...".cyan());
    }

    match client
        .post::<(), RetryQueueProcessResponse>("/api/retry-queue/process", &())
        .await
    {
        Ok(response) => {
            print_success(&format!(
                "Processed {} queries: {}",
                response.processed, response.message
            ));
        }
        Err(e) => {
            print_error(&format!("Failed to process retry queue: {}", e));
        }
    }

    Ok(())
}

/// Clear retry queue
async fn retry_queue_clear(client: &ApiClient, verbose: bool) -> Result<()> {
    if verbose {
        println!("{}", "Clearing retry queue...".cyan());
    }

    match client
        .delete::<RetryQueueClearResponse>("/api/retry-queue")
        .await
    {
        Ok(response) => {
            print_success(&format!(
                "Cleared {} queries: {}",
                response.cleared, response.message
            ));
        }
        Err(e) => {
            print_error(&format!("Failed to clear retry queue: {}", e));
        }
    }

    Ok(())
}

/// Print a boolean health value
fn print_health_bool(label: &str, value: bool) {
    let colored = if value {
        format!("{}: true", label).green()
    } else {
        format!("{}: false", label).red()
    };
    println!("{}", colored);
}

/// Print a key-value pair
fn print_value(label: &str, value: &str) {
    println!("{}: {}", label, value);
}
