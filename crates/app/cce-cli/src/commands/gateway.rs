//! Local file gateway CLI branch.
//!
//! The one-shot sync capability lives here as a thin translation over the
//! shared gateway core. The resident daemon and this branch push through the
//! same `cce-gateway` supply, fingerprint, transport, and observe layers, so
//! both reach the same remote index state for the same repository. This file
//! holds no traversal, hashing, or batching logic of its own.

use anyhow::Result;

use cce_gateway::{compression_enabled, sync_once, watch_loop, GatewayClient, SyncParams};

use crate::cli::GatewayCommands;

/// Execute the gateway subcommand.
pub async fn execute(cmd: &GatewayCommands, server: &str, verbose: bool) -> Result<()> {
    let client = GatewayClient::new(server)?;
    let compress_env = compression_enabled();
    match cmd {
        GatewayCommands::Sync {
            project_id,
            path,
            extensions,
            exclude,
            gitignore,
            no_commit,
            compress,
        } => {
            let params = SyncParams {
                project_id: *project_id,
                path: path.clone(),
                extensions: extensions.clone(),
                exclude: exclude.clone(),
                gitignore: *gitignore,
                commit: !no_commit,
                compress: compress_env || *compress,
            };
            sync_once(&client, &params, verbose).await
        }
        GatewayCommands::Watch {
            project_id,
            path,
            extensions,
            exclude,
            gitignore,
            interval_secs,
            compress,
        } => {
            let params = SyncParams {
                project_id: *project_id,
                path: path.clone(),
                extensions: extensions.clone(),
                exclude: exclude.clone(),
                gitignore: *gitignore,
                commit: true,
                compress: compress_env || *compress,
            };
            watch_loop(&client, &params, *interval_secs, verbose, None).await
        }
    }
}
