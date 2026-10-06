//! Standalone resident gateway daemon.
//!
//! The daemon shares the library core with the one-shot CLI branch, so both
//! push the same bytes for the same repository. It runs an initial full
//! sync, then polls and pushes incremental events, writing a heartbeat file
//! for supervisors. Losing the in-memory baseline rebuilds through one full
//! sync pass, which is the crash-recovery path.

use anyhow::Result;
use clap::Parser;

use cce_gateway::{GatewayClient, SyncParams, compression_enabled, sync_once, watch_loop};

/// Resident file supply gateway.
#[derive(Debug, Parser)]
#[command(
    name = "cce-gateway",
    version,
    about = "Push local files to a remote CCE host"
)]
struct Args {
    /// Remote server base URL, for example http://10.0.0.10:9000.
    #[arg(long, env = "CCE_SERVER_URL", default_value = "http://127.0.0.1:3000")]
    server: String,

    /// Remote project id.
    #[arg(short = 'P', long, env = "CCE_PROJECT_ID")]
    project_id: i64,

    /// Local directory to supply.
    #[arg(short, long, env = "CCE_GATEWAY_PATH")]
    path: String,

    /// File extensions to include, comma separated, empty means all text.
    #[arg(long, default_value = "")]
    extensions: String,

    /// Directories to exclude, comma separated.
    #[arg(long, default_value = "node_modules,target,.git,vendor")]
    exclude: String,

    /// Respect gitignore files.
    #[arg(long, default_value = "true")]
    gitignore: bool,

    /// Run once and exit instead of watching.
    #[arg(long, default_value = "false")]
    once: bool,

    /// Do not run the index commit after staging.
    #[arg(long, default_value = "false")]
    no_commit: bool,

    /// Compress chunks before upload, off by default.
    #[arg(long, default_value = "false")]
    compress: bool,

    /// Poll interval in seconds for watch mode.
    #[arg(long, default_value = "5")]
    interval_secs: u64,

    /// Heartbeat file for supervisors and health probes.
    #[arg(long, env = "CCE_GATEWAY_HEALTH_FILE")]
    health_file: Option<std::path::PathBuf>,

    /// Verbose logging.
    #[arg(short, long, default_value = "false")]
    verbose: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let client = GatewayClient::new(&args.server)?;
    let params = SyncParams {
        project_id: args.project_id,
        path: args.path,
        extensions: args.extensions,
        exclude: args.exclude,
        gitignore: args.gitignore,
        commit: !args.no_commit,
        compress: args.compress || compression_enabled(),
    };
    if args.once {
        sync_once(&client, &params, args.verbose).await
    } else {
        watch_loop(
            &client,
            &params,
            args.interval_secs,
            args.verbose,
            args.health_file,
        )
        .await
    }
}
