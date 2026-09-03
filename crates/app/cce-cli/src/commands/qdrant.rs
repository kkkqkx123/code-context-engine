//! Qdrant process management command handler

use anyhow::Result;

use crate::cli::QdrantCommands;
use crate::client::ApiClient;
use crate::output::{print_error, print_success};
use cce_api::models::{QdrantActionResponse, QdrantProcessStatus, QdrantProcessStatusResponse};

pub async fn execute(cmd: &QdrantCommands, server: &str, verbose: bool) -> Result<()> {
    let client = ApiClient::new(server)?;

    match cmd {
        QdrantCommands::Process { action } => {
            let action_str = match action {
                crate::cli::QdrantProcessAction::Status => "status",
                crate::cli::QdrantProcessAction::Start => "start",
                crate::cli::QdrantProcessAction::Stop => "stop",
                crate::cli::QdrantProcessAction::Restart => "restart",
            };

            if verbose {
                println!("Qdrant process action: {}", action_str);
            }

            let url = format!("/api/qdrant/process/{}", action_str);

            if action_str == "status" {
                let response: QdrantProcessStatusResponse = client.get(&url).await?;
                println!("Qdrant process status:");
                println!("  Managed: {}", response.managed);
                println!("  Status:  {}", format_status(&response.status));
            } else {
                let response: QdrantActionResponse =
                    client.post(&url, &serde_json::json!({})).await?;
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
