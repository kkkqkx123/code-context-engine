//! Local file gateway CLI branch.
//!
//! The one-shot sync capability lives here as a thin translation over the
//! shared gateway core. The resident daemon and this branch push through the
//! same `cce-gateway` supply, fingerprint, transport, and observe layers, so
//! both reach the same remote index state for the same repository. This file
//! holds no traversal, hashing, or batching logic of its own.

use clap::Subcommand;

use anyhow::Result;

use cce_gateway::{sync_once, watch_loop, GatewayClient, SyncParams};

use crate::cli::OutputFormat;

/// Apply the overrides both gateway variants share.
fn apply_shared(
    params: &mut SyncParams,
    extensions: &Option<String>,
    exclude: &Option<String>,
    gitignore: &Option<bool>,
    compress: &Option<bool>,
    cache_file: &Option<std::path::PathBuf>,
    health_file: &Option<std::path::PathBuf>,
) {
    if let Some(extensions) = extensions {
        params.extensions = extensions.clone();
    }
    if let Some(exclude) = exclude {
        params.exclude = exclude.clone();
    }
    if let Some(gitignore) = gitignore {
        params.gitignore = *gitignore;
    }
    if let Some(compress) = compress {
        params.compress = *compress;
    }
    if let Some(cache_file) = cache_file {
        params.cache_file = Some(cache_file.clone());
    }
    params.health_file = health_file.clone();
}

/// Translate one gateway subcommand over the shared defaults.
///
/// Explicit arguments outrank the environment values baked into the
/// defaults; anything the caller left out keeps the shared value.
fn params_for(cmd: &GatewayCommands, format: OutputFormat) -> SyncParams {
    let json_progress = matches!(format, OutputFormat::Json);
    match cmd {
        GatewayCommands::Sync {
            project_id,
            path,
            extensions,
            exclude,
            gitignore,
            no_commit,
            compress,
            cache_file,
            health_file,
            token: _,
        } => {
            let mut params = SyncParams::with_defaults(*project_id, path.clone());
            apply_shared(
                &mut params,
                extensions,
                exclude,
                gitignore,
                compress,
                cache_file,
                health_file,
            );
            if *no_commit {
                params.commit = false;
            }
            params.json_progress = json_progress;
            params
        }
        GatewayCommands::Watch {
            project_id,
            path,
            extensions,
            exclude,
            gitignore,
            interval_secs,
            compress,
            cache_file,
            adaptive_interval,
            health_file,
            token: _,
        } => {
            let mut params = SyncParams::with_defaults(*project_id, path.clone());
            apply_shared(
                &mut params,
                extensions,
                exclude,
                gitignore,
                compress,
                cache_file,
                health_file,
            );
            if let Some(interval_secs) = interval_secs {
                params.interval_secs = *interval_secs;
            }
            if let Some(adaptive_interval) = adaptive_interval {
                params.adaptive_interval = *adaptive_interval;
            }
            params.json_progress = json_progress;
            params
        }
    }
}

fn explicit_token(cmd: &GatewayCommands) -> Option<String> {
    match cmd {
        GatewayCommands::Sync { token, .. } | GatewayCommands::Watch { token, .. } => token.clone(),
    }
}

/// Execute the gateway subcommand.
pub async fn execute(
    cmd: &GatewayCommands,
    server: &str,
    verbose: bool,
    format: OutputFormat,
) -> Result<()> {
    let client = GatewayClient::new_with_token(server, explicit_token(cmd))?;
    let params = params_for(cmd, format);
    match cmd {
        GatewayCommands::Sync { .. } => sync_once(&client, &params, verbose).await.map(|_| ()),
        GatewayCommands::Watch { .. } => watch_loop(&client, &params, verbose).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build the sync variant with every optional business knob unset.
    fn bare_sync() -> GatewayCommands {
        GatewayCommands::Sync {
            project_id: 5,
            path: "/srv/repo".to_string(),
            extensions: None,
            exclude: None,
            gitignore: None,
            no_commit: false,
            compress: None,
            cache_file: None,
            health_file: None,
            token: None,
        }
    }

    /// Build the watch variant with every optional business knob unset.
    fn bare_watch() -> GatewayCommands {
        GatewayCommands::Watch {
            project_id: 5,
            path: "/srv/repo".to_string(),
            extensions: None,
            exclude: None,
            gitignore: None,
            interval_secs: None,
            compress: None,
            cache_file: None,
            adaptive_interval: None,
            health_file: None,
            token: None,
        }
    }

    #[test]
    fn bare_sync_and_watch_equal_the_shared_defaults() {
        let expected = format!("{:?}", SyncParams::with_defaults(5, "/srv/repo"));
        assert_eq!(
            format!("{:?}", params_for(&bare_sync(), OutputFormat::Table)),
            expected
        );
        assert_eq!(
            format!("{:?}", params_for(&bare_watch(), OutputFormat::Table)),
            expected
        );
    }

    #[test]
    fn explicit_arguments_outrank_the_defaults() {
        let sync = GatewayCommands::Sync {
            project_id: 5,
            path: "/srv/repo".to_string(),
            extensions: Some("rs".to_string()),
            exclude: Some("vendor".to_string()),
            gitignore: Some(false),
            no_commit: true,
            compress: Some(false),
            cache_file: Some(std::path::PathBuf::from("/tmp/scan-cache.json")),
            health_file: Some(std::path::PathBuf::from("/run/gateway.json")),
            token: Some("explicit-token".to_string()),
        };
        let params = params_for(&sync, OutputFormat::Table);
        assert_eq!(params.extensions, "rs");
        assert_eq!(params.exclude, "vendor");
        assert!(!params.gitignore);
        assert!(!params.commit);
        assert!(!params.compress);
        assert_eq!(params.interval_secs, 5);
        assert_eq!(
            params.cache_file,
            Some(std::path::PathBuf::from("/tmp/scan-cache.json"))
        );
        assert_eq!(
            params.health_file,
            Some(std::path::PathBuf::from("/run/gateway.json"))
        );
        assert!(!params.json_progress);
        assert_eq!(explicit_token(&sync), Some("explicit-token".to_string()));

        let watch = GatewayCommands::Watch {
            project_id: 5,
            path: "/srv/repo".to_string(),
            extensions: Some("rs".to_string()),
            exclude: None,
            gitignore: Some(false),
            interval_secs: Some(2),
            compress: Some(true),
            cache_file: None,
            adaptive_interval: Some(true),
            health_file: None,
            token: None,
        };
        let params = params_for(&watch, OutputFormat::Json);
        assert_eq!(params.extensions, "rs");
        assert_eq!(params.exclude, "node_modules,target,.git,vendor");
        assert!(!params.gitignore);
        assert_eq!(params.interval_secs, 2);
        assert!(params.compress);
        assert!(params.commit);
        assert!(params.adaptive_interval);
        assert!(params.json_progress);
    }
}

/// Gateway commands for remote hosting
///
/// Business defaults (extensions, exclude, gitignore, commit, compression,
/// poll interval) live in `SyncParams::with_defaults`; the variants only
/// carry what the caller stated explicitly.
#[derive(Subcommand)]
pub enum GatewayCommands {
    /// Push a full sync pass: manifest, missing contents, then index commit
    Sync {
        /// Project ID on the remote host
        #[arg(short = 'P', long)]
        project_id: i64,

        /// Local directory to supply
        #[arg(short, long)]
        path: String,

        /// File extensions to include (comma-separated, empty means all text)
        #[arg(short, long)]
        extensions: Option<String>,

        /// Directories to exclude (comma-separated)
        #[arg(short, long)]
        exclude: Option<String>,

        /// Respect .gitignore; --gitignore=false forces them off
        #[arg(
            long,
            action = clap::ArgAction::Set,
            num_args = 0..=1,
            default_missing_value = "true",
            require_equals = true
        )]
        gitignore: Option<bool>,

        /// Stage files without running the index commit
        #[arg(long, default_value = "false")]
        no_commit: bool,

        /// Compress chunks before upload; --compress=false forces it off
        /// and overrides the CCE_GATEWAY_COMPRESS environment value
        #[arg(
            long,
            action = clap::ArgAction::Set,
            num_args = 0..=1,
            default_missing_value = "true",
            require_equals = true
        )]
        compress: Option<bool>,

        /// Scan cache file for cross-restart hash reuse
        #[arg(long, env = "CCE_GATEWAY_CACHE_FILE")]
        cache_file: Option<std::path::PathBuf>,

        /// Heartbeat file for supervisors and health probes
        #[arg(long, env = "CCE_GATEWAY_HEALTH_FILE")]
        health_file: Option<std::path::PathBuf>,

        /// Admission token; explicit value outranks CCE_API_TOKEN
        #[arg(long, env = "CCE_API_TOKEN")]
        token: Option<String>,
    },

    /// Sync once, then poll and push incremental changes
    Watch {
        /// Project ID on the remote host
        #[arg(short = 'P', long)]
        project_id: i64,

        /// Local directory to supply
        #[arg(short, long)]
        path: String,

        /// File extensions to include (comma-separated, empty means all text)
        #[arg(short, long)]
        extensions: Option<String>,

        /// Directories to exclude (comma-separated)
        #[arg(short, long)]
        exclude: Option<String>,

        /// Respect .gitignore; --gitignore=false forces them off
        #[arg(
            long,
            action = clap::ArgAction::Set,
            num_args = 0..=1,
            default_missing_value = "true",
            require_equals = true
        )]
        gitignore: Option<bool>,

        /// Poll interval in seconds
        #[arg(long)]
        interval_secs: Option<u64>,

        /// Compress chunks before upload; --compress=false forces it off
        /// and overrides the CCE_GATEWAY_COMPRESS environment value
        #[arg(
            long,
            action = clap::ArgAction::Set,
            num_args = 0..=1,
            default_missing_value = "true",
            require_equals = true
        )]
        compress: Option<bool>,

        /// Scan cache file for cross-restart hash reuse
        #[arg(long, env = "CCE_GATEWAY_CACHE_FILE")]
        cache_file: Option<std::path::PathBuf>,

        /// Adapt the poll interval to the baseline file count
        #[arg(
            long,
            action = clap::ArgAction::Set,
            num_args = 0..=1,
            default_missing_value = "true",
            require_equals = true
        )]
        adaptive_interval: Option<bool>,

        /// Heartbeat file for supervisors and health probes
        #[arg(long, env = "CCE_GATEWAY_HEALTH_FILE")]
        health_file: Option<std::path::PathBuf>,

        /// Admission token; explicit value outranks CCE_API_TOKEN
        #[arg(long, env = "CCE_API_TOKEN")]
        token: Option<String>,
    },
}
