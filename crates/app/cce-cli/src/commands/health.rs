//! Health checking and retry queue management commands

use clap::Subcommand;

use anyhow::Result;
use colored::Colorize;

use cce_transport::SharedHttpClient;
use crate::icons;
use crate::output::{print_error, print_output, print_success, print_warning};
use cce_api::models::{
    AdmissionStatsResponse, Bm25HealthResponse, EmbeddingHealthResponse, HealthStatus,
    QdrantHealthResponse, RetryQueueClearResponse, RetryQueueDeadClearResponse,
    RetryQueueDeadResponse, RetryQueueProcessResponse, RetryQueueStatusResponse,
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
    /// List dead retry-queue entries
    DeadList,
    /// Clear dead retry-queue entries
    DeadClear,
    /// Show admission counters and per-token audit
    AdmissionStats,
}

/// Execute a health command
pub async fn execute_with_client(
    cmd: &HealthCommand,
    client: &SharedHttpClient,
    verbose: bool,
    format: crate::cli::OutputFormat,
) -> Result<()> {
    match cmd {
        HealthCommand::Check => health_check(client, verbose, format).await,
        HealthCommand::Qdrant => health_qdrant(client, verbose).await,
        HealthCommand::Embedding => health_embedding(client, verbose).await,
        HealthCommand::Bm25 => health_bm25(client, verbose).await,
        HealthCommand::QueueStatus => retry_queue_status(client, verbose).await,
        HealthCommand::QueueProcess => retry_queue_process(client, verbose).await,
        HealthCommand::QueueClear => retry_queue_clear(client, verbose).await,
        HealthCommand::DeadList => retry_queue_dead_list(client, verbose).await,
        HealthCommand::DeadClear => retry_queue_dead_clear(client, verbose).await,
        HealthCommand::AdmissionStats => admission_stats(client, verbose).await,
    }
}

/// Full health check
async fn health_check(
    client: &SharedHttpClient,
    verbose: bool,
    format: crate::cli::OutputFormat,
) -> Result<()> {
    if verbose {
        println!("{}", "Fetching unified health status...".cyan());
    }

    match client.get::<HealthStatus>(cce_api::paths::HEALTH).await {
        Ok(response) => {
            if matches!(format, crate::cli::OutputFormat::Json) {
                print_output(format, &response);
            } else {
                println!("{}", "Unified Health Status".bold());
                println!("{}", icons::SEPARATOR.repeat(50));
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
    let icon = if status.reachable {
        icons::SUCCESS
    } else {
        icons::ERROR
    };
    let colored_name = if status.reachable {
        format!("{} {}", icon, name).green()
    } else {
        format!("{} {}", icon, name).red()
    };
    println!("{}", colored_name);
    println!("  Message: {}", status.message);
}

/// Qdrant health check
async fn health_qdrant(client: &SharedHttpClient, verbose: bool) -> Result<()> {
    if verbose {
        println!("{}", "Fetching Qdrant health...".cyan());
    }

    match client
        .get::<QdrantHealthResponse>(cce_api::paths::HEALTH_QDRANT)
        .await
    {
        Ok(response) => {
            println!("{}", "Qdrant Health".bold());
            println!("{}", icons::SEPARATOR.repeat(50));
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
async fn health_embedding(client: &SharedHttpClient, verbose: bool) -> Result<()> {
    if verbose {
        println!("{}", "Fetching Embedding health...".cyan());
    }

    match client
        .get::<EmbeddingHealthResponse>(cce_api::paths::HEALTH_EMBEDDING)
        .await
    {
        Ok(response) => {
            println!("{}", "Embedding Health".bold());
            println!("{}", icons::SEPARATOR.repeat(50));
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
async fn health_bm25(client: &SharedHttpClient, verbose: bool) -> Result<()> {
    if verbose {
        println!("{}", "Fetching BM25 health...".cyan());
    }

    match client
        .get::<Bm25HealthResponse>(cce_api::paths::HEALTH_BM25)
        .await
    {
        Ok(response) => {
            println!("{}", "BM25 Health".bold());
            println!("{}", icons::SEPARATOR.repeat(50));
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
async fn retry_queue_status(client: &SharedHttpClient, verbose: bool) -> Result<()> {
    if verbose {
        println!("{}", "Fetching retry queue status...".cyan());
    }

    match client
        .get::<RetryQueueStatusResponse>(cce_api::paths::RETRY_QUEUE)
        .await
    {
        Ok(response) => {
            println!("{}", "Retry Queue Status".bold());
            println!("{}", icons::SEPARATOR.repeat(50));
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
async fn retry_queue_process(client: &SharedHttpClient, verbose: bool) -> Result<()> {
    if verbose {
        println!("{}", "Processing retry queue...".cyan());
    }

    match client
        .post::<(), RetryQueueProcessResponse>(cce_api::paths::RETRY_QUEUE_PROCESS, &())
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
async fn retry_queue_clear(client: &SharedHttpClient, verbose: bool) -> Result<()> {
    if verbose {
        println!("{}", "Clearing retry queue...".cyan());
    }

    match client
        .delete::<RetryQueueClearResponse>(cce_api::paths::RETRY_QUEUE)
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

/// List dead retry-queue entries
async fn retry_queue_dead_list(client: &SharedHttpClient, verbose: bool) -> Result<()> {
    if verbose {
        println!("{}", "Fetching dead retry queue entries...".cyan());
    }
    match client
        .get::<RetryQueueDeadResponse>(cce_api::paths::RETRY_QUEUE_DEAD)
        .await
    {
        Ok(response) => {
            println!("dead_count={}", response.dead_count);
        }
        Err(e) => {
            print_error(&format!("Failed to list dead entries: {}", e));
        }
    }
    Ok(())
}

/// Clear dead retry-queue entries
async fn retry_queue_dead_clear(client: &SharedHttpClient, verbose: bool) -> Result<()> {
    if verbose {
        println!("{}", "Clearing dead retry queue entries...".cyan());
    }
    match client
        .delete::<RetryQueueDeadClearResponse>(cce_api::paths::RETRY_QUEUE_DEAD)
        .await
    {
        Ok(response) => {
            print_success(&format!("Cleared: {}", response.message));
        }
        Err(e) => {
            print_error(&format!("Failed to clear dead entries: {}", e));
        }
    }
    Ok(())
}

/// Show admission counters and per-token audit
async fn admission_stats(client: &SharedHttpClient, verbose: bool) -> Result<()> {
    if verbose {
        println!("{}", "Fetching admission stats...".cyan());
    }
    match client
        .get::<AdmissionStatsResponse>(cce_api::paths::ADMISSION_STATS)
        .await
    {
        Ok(response) => {
            println!("admitted={}", response.global.admitted);
            println!("tokens={}", response.tokens.len());
        }
        Err(e) => {
            print_error(&format!("Failed to get admission stats: {}", e));
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

/// Health monitoring and retry queue commands
#[derive(Subcommand)]
pub enum HealthCommands {
    /// Unified health check for all external services
    Check,

    /// Qdrant detailed diagnostics (circuit breaker, collection info)
    Qdrant,

    /// Embedding service health (per-provider status)
    Embedding,

    /// BM25 index health
    Bm25,

    /// Get retry queue status (pending query count)
    QueueStatus,

    /// Manually trigger retry queue processing
    QueueProcess,

    /// Clear retry queue (discard all pending queries)
    QueueClear,

    /// List dead retry-queue entries
    DeadList,

    /// Clear dead retry-queue entries
    DeadClear,

    /// Show admission counters and per-token audit
    AdmissionStats,
}
