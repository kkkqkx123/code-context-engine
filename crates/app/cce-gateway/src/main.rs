//! Standalone resident gateway daemon.
//!
//! The daemon shares the library core with the one-shot CLI branch, so both
//! push the same bytes for the same repository. It runs an initial full
//! sync, then polls and pushes incremental events, writing a heartbeat file
//! for supervisors. Losing the in-memory baseline rebuilds through one full
//! sync pass, which is the crash-recovery path.

use anyhow::Result;
use clap::Parser;

use cce_gateway::{GatewayClient, SyncParams, sync_once, watch_loop};

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
    #[arg(long)]
    extensions: Option<String>,

    /// Directories to exclude, comma separated.
    #[arg(long)]
    exclude: Option<String>,

    /// Respect gitignore files; --gitignore=false forces them off.
    #[arg(
        long,
        action = clap::ArgAction::Set,
        num_args = 0..=1,
        default_missing_value = "true",
        require_equals = true
    )]
    gitignore: Option<bool>,

    /// Run once and exit instead of watching.
    #[arg(long, default_value = "false")]
    once: bool,

    /// Do not run the index commit after staging.
    #[arg(long, default_value = "false")]
    no_commit: bool,

    /// Compress chunks before upload; --compress=false forces it off and
    /// overrides the CCE_GATEWAY_COMPRESS environment value.
    #[arg(
        long,
        action = clap::ArgAction::Set,
        num_args = 0..=1,
        default_missing_value = "true",
        require_equals = true
    )]
    compress: Option<bool>,

    /// Poll interval in seconds for watch mode.
    #[arg(long)]
    interval_secs: Option<u64>,

    /// Heartbeat file for supervisors and health probes.
    #[arg(long, env = "CCE_GATEWAY_HEALTH_FILE")]
    health_file: Option<std::path::PathBuf>,

    /// Verbose logging.
    #[arg(short, long, default_value = "false")]
    verbose: bool,
}

/// Translate parsed daemon arguments over the shared defaults.
///
/// Explicit arguments outrank the environment values baked into the
/// defaults; anything the caller left out keeps the shared value.
fn params_from(args: &Args) -> SyncParams {
    let mut params = SyncParams::with_defaults(args.project_id, args.path.clone());
    if let Some(extensions) = &args.extensions {
        params.extensions = extensions.clone();
    }
    if let Some(exclude) = &args.exclude {
        params.exclude = exclude.clone();
    }
    if let Some(gitignore) = args.gitignore {
        params.gitignore = gitignore;
    }
    if args.no_commit {
        params.commit = false;
    }
    if let Some(compress) = args.compress {
        params.compress = compress;
    }
    if let Some(interval_secs) = args.interval_secs {
        params.interval_secs = interval_secs;
    }
    params.health_file = args.health_file.clone();
    params
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let client = GatewayClient::new(&args.server)?;
    let params = params_from(&args);
    if args.once {
        sync_once(&client, &params, args.verbose).await.map(|_| ())
    } else {
        watch_loop(&client, &params, args.verbose).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build daemon arguments whose every optional business knob is unset.
    fn bare_args() -> Args {
        Args {
            server: "http://127.0.0.1:3000".to_string(),
            project_id: 4,
            path: "/srv/repo".to_string(),
            extensions: None,
            exclude: None,
            gitignore: None,
            once: false,
            no_commit: false,
            compress: None,
            interval_secs: None,
            health_file: None,
            verbose: false,
        }
    }

    #[test]
    fn bare_arguments_equal_the_shared_defaults() {
        let params = params_from(&bare_args());
        assert_eq!(
            format!("{params:?}"),
            format!("{:?}", SyncParams::with_defaults(4, "/srv/repo"))
        );
    }

    #[test]
    fn explicit_arguments_outrank_the_defaults() {
        let mut args = bare_args();
        args.extensions = Some("rs".to_string());
        args.exclude = Some("vendor".to_string());
        args.gitignore = Some(false);
        args.no_commit = true;
        args.compress = Some(false);
        args.interval_secs = Some(2);
        args.health_file = Some(std::path::PathBuf::from("/run/gateway.json"));
        let params = params_from(&args);
        assert_eq!(params.extensions, "rs");
        assert_eq!(params.exclude, "vendor");
        assert!(!params.gitignore);
        assert!(!params.commit);
        assert!(!params.compress);
        assert_eq!(params.interval_secs, 2);
        assert_eq!(
            params.health_file,
            Some(std::path::PathBuf::from("/run/gateway.json"))
        );
    }
}
