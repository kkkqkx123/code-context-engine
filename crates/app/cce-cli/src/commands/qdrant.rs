//! Qdrant process management command handler

use clap::Subcommand;

use anyhow::Result;

use crate::client::ApiClient;
use crate::output::{print_error, print_success};
use cce_api::models::{QdrantActionResponse, QdrantProcessStatus, QdrantProcessStatusResponse};

pub async fn execute(cmd: &QdrantCommands, server: &str, verbose: bool) -> Result<()> {
    let client = ApiClient::new(server)?;

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

            let path = match action_str {
                "status" => cce_api::paths::QDRANT_PROCESS_STATUS.to_string(),
                "start" => cce_api::paths::QDRANT_PROCESS_START.to_string(),
                "stop" => cce_api::paths::QDRANT_PROCESS_STOP.to_string(),
                _ => cce_api::paths::QDRANT_PROCESS_RESTART.to_string(),
            };

            if action_str == "status" {
                let response: QdrantProcessStatusResponse = client.get(&path).await?;
                println!("Qdrant process status:");
                println!("  Managed: {}", response.managed);
                println!("  Status:  {}", format_status(&response.status));
            } else {
                let response: QdrantActionResponse =
                    client.post(&path, &serde_json::json!({})).await?;
                if response.success {
                    print_success(&response.message);
                } else {
                    print_error(&response.message);
                }
                println!("  Status:  {}", format_status(&response.status));
            }

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
