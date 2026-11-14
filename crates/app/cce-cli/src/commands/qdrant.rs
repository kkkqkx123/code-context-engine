//! Qdrant process management command handler

use clap::Subcommand;

use anyhow::Result;

use crate::output::{print_error, print_success};
use cce_api::models::QdrantProcessStatus;
use cce_client::Client;

pub async fn execute_with_client(
    cmd: &QdrantCommands,
    client: &Client,
    verbose: bool,
) -> Result<()> {
    match cmd {
        QdrantCommands::Process { action } => {
            let action_str = match action {
                QdrantProcessAction::Status => "status",
                QdrantProcessAction::Start => "start",
                QdrantProcessAction::Stop => "stop",
                QdrantProcessAction::Restart => "restart",
            };

            if verbose {
                println!("Qdrant process action: {}", action_str);
            }

            let response = match action {
                QdrantProcessAction::Status => {
                    let response = client.qdrant_process_status().await?;
                    println!("Qdrant process status:");
                    println!("  Managed: {}", response.managed);
                    println!("  Status:  {}", format_status(&response.status));
                    return Ok(());
                }
                QdrantProcessAction::Start => client.qdrant_process_start().await?,
                QdrantProcessAction::Stop => client.qdrant_process_stop().await?,
                QdrantProcessAction::Restart => client.qdrant_process_restart().await?,
            };
            if response.success {
                print_success(&response.message);
            } else {
                print_error(&response.message);
            }
            println!("  Status:  {}", format_status(&response.status));

            Ok(())
        }
    }
}

fn format_status(status: &QdrantProcessStatus) -> &'static str {
    match status {
        QdrantProcessStatus::Idle => "Idle",
        QdrantProcessStatus::Starting => "Starting...",
        QdrantProcessStatus::Running => "Running",
        QdrantProcessStatus::Stopping => "Stopping...",
        QdrantProcessStatus::Crashed => "Crashed",
        QdrantProcessStatus::Stopped => "Stopped",
        QdrantProcessStatus::Failed(_) => "Failed",
    }
}

/// Qdrant process management
#[derive(Debug, Subcommand)]
pub enum QdrantCommands {
    /// Process management actions
    Process {
        #[command(subcommand)]
        action: QdrantProcessAction,
    },
}

/// Qdrant process actions
#[derive(Debug, Subcommand)]
pub enum QdrantProcessAction {
    /// Check Qdrant process status
    Status,
    /// Start Qdrant process
    Start,
    /// Stop Qdrant process
    Stop,
    /// Restart Qdrant process
    Restart,
}
