//! Watch command handlers

use clap::Subcommand;

use anyhow::Result;

use crate::output::{print_error, print_success};
use cce_api::models::StartWatchRequest;

/// Execute watch commands through the client abstraction.
pub async fn execute_with_client(
    cmd: &WatchCommands,
    client: &crate::client::ApiClient,
    verbose: bool,
) -> Result<()> {
    match cmd {
        WatchCommands::Start {
            project_id,
            path,
            extensions,
            debounce,
        } => {
            let ext_list: Vec<String> = extensions
                .as_ref()
                .map(|s| s.split(',').map(|e| e.trim().to_string()).collect())
                .unwrap_or_default();
            let request = StartWatchRequest {
                path: path.to_string(),
                extensions: ext_list,
                debounce_ms: *debounce,
            };
            if verbose {
                println!("Starting watch on: {}", path);
            }
            match client.watch_start(*project_id, request).await {
                Ok(response) => {
                    if response.success {
                        print_success(&format!("Watch started on: {}", response.path));
                    } else {
                        print_error("Failed to start watch");
                    }
                }
                Err(e) => print_error(&format!("Failed to start watch: {e}")),
            }
            Ok(())
        }
        WatchCommands::Stop { project_id } => match client.watch_stop(*project_id).await {
            Ok(response) => {
                if response.success {
                    print_success("Watch stopped");
                } else {
                    print_error("Failed to stop watch");
                }
                Ok(())
            }
            Err(e) => {
                print_error(&format!("Failed to stop watch: {e}"));
                Ok(())
            }
        },
        WatchCommands::Status { project_id } => match client.watch_status(*project_id).await {
            Ok(response) => {
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
            Err(e) => {
                print_error(&format!("Failed to get watch status: {e}"));
                Ok(())
            }
        },
    }
}

/// Watch commands (server-side listeners; the server must access the path)
#[derive(Subcommand)]
pub enum WatchCommands {
    /// Start a server-side directory watch
    Start {
        /// Project ID
        #[arg(short = 'P', long)]
        project_id: i64,

        /// Directory to watch
        #[arg(short, long)]
        path: String,

        /// File extensions to watch
        #[arg(long)]
        extensions: Option<String>,

        /// Debounce interval in milliseconds
        #[arg(long, default_value = "500")]
        debounce: u64,
    },

    /// Stop a server-side watch
    Stop {
        /// Project ID
        #[arg(short = 'P', long)]
        project_id: i64,
    },

    /// Get server-side watch status
    Status {
        /// Project ID
        #[arg(short = 'P', long)]
        project_id: i64,
    },
}
