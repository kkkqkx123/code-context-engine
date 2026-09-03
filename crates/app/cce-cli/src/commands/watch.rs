//! Watch command handlers

use anyhow::Result;

use crate::cli::WatchCommands;
use crate::client::ApiClient;
use crate::output::{print_error, print_success};
use cce_api::models::{StartWatchRequest, WatchStatusResponse};

pub async fn execute(cmd: &WatchCommands, server: &str, verbose: bool) -> Result<()> {
    let client = ApiClient::new(server)?;

    match cmd {
        WatchCommands::Start {
            project_id,
            path,
            extensions,
            debounce,
        } => start_watch(&client, *project_id, path, extensions, *debounce, verbose).await,
        WatchCommands::Stop { project_id } => stop_watch(&client, *project_id, verbose).await,
        WatchCommands::Status { project_id } => watch_status(&client, *project_id, verbose).await,
    }
}

async fn start_watch(
    client: &ApiClient,
    project_id: i64,
    path: &str,
    extensions: &Option<String>,
    debounce: u64,
    verbose: bool,
) -> Result<()> {
    let ext_list: Vec<String> = extensions
        .as_ref()
        .map(|s| s.split(',').map(|e| e.trim().to_string()).collect())
        .unwrap_or_default();

    let request = StartWatchRequest {
        path: path.to_string(),
        extensions: ext_list,
        debounce_ms: debounce,
    };

    if verbose {
        println!("Starting watch on: {}", path);
    }

    let url = format!("/api/project/{}/watch/start", project_id);
    let response: serde_json::Value = client.post(&url, &request).await?;

    if response["success"].as_bool().unwrap_or(false) {
        print_success(&format!("Watch started on: {}", path));
    } else {
        print_error("Failed to start watch");
    }

    Ok(())
}

async fn stop_watch(client: &ApiClient, project_id: i64, verbose: bool) -> Result<()> {
    if verbose {
        println!("Stopping watch...");
    }

    let url = format!("/api/project/{}/watch/stop", project_id);
    let response: serde_json::Value = client.post(&url, &serde_json::json!({})).await?;

    if response["success"].as_bool().unwrap_or(false) {
        print_success("Watch stopped");
    } else {
        print_error("Failed to stop watch");
    }

    Ok(())
}

async fn watch_status(client: &ApiClient, project_id: i64, verbose: bool) -> Result<()> {
    if verbose {
        println!("Fetching watch status...");
    }

    let url = format!("/api/project/{}/watch/status", project_id);
    let response: WatchStatusResponse = client.get(&url).await?;

    if response.success {
        let status = &response.status;
        println!("Watch status:");
        println!("  Active:            {}", status.active);
        println!("  Events processed:  {}", status.events_processed);

        if let Some(ref started_at) = status.started_at {
            println!("  Started at:        {}", started_at);
        }

        if !status.watched_dirs.is_empty() {
            println!("  Watched dirs:");
            for dir in &status.watched_dirs {
                println!("    - {}", dir);
            }
        }
    } else {
        print_error("Failed to get watch status");
    }

    Ok(())
}
