//! Local file gateway CLI branch.
//!
//! The one-shot sync capability lives here as a thin translation over the
//! shared gateway core. The resident daemon and this branch push through the
//! same `cce-gateway` supply, fingerprint, transport, and observe layers, so
//! both reach the same remote index state for the same repository. This file
//! holds no traversal, hashing, or batching logic of its own.

use anyhow::Result;

use cce_gateway::{sync_once, watch_loop, GatewayClient, SyncParams};

use crate::cli::{GatewayCommands, OutputFormat};

/// Apply the overrides both gateway variants share.
fn apply_shared(
    params: &mut SyncParams,
    extensions: &Option<String>,
    exclude: &Option<String>,
    gitignore: &Option<bool>,
    compress: &Option<bool>,
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
                health_file,
            );
            if let Some(interval_secs) = interval_secs {
                params.interval_secs = *interval_secs;
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
        assert!(params.json_progress);
    }
}
