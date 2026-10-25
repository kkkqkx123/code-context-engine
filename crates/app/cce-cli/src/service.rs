//! CLI service selection between local engine and remote transport.
//!
//! Both modes implement the same domain trait so command semantics stay
//! identical; only the transport differs. Local mode holds the engine
//! directly with zero-copy filesystem access. Remote mode encodes the same
//! parameters onto HTTP.

use anyhow::{Context, Result};
use cce_service::CodeContextService;

/// CLI execution mode flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum CliServiceMode {
    /// Direct in-process engine calls.
    Local,
    /// Remote server via HTTP transport.
    #[default]
    Remote,
}

impl CliServiceMode {
    /// Resolve from explicit flag, falling back to the environment.
    pub fn resolve(explicit: Option<CliServiceMode>) -> Self {
        if let Some(mode) = explicit {
            return mode;
        }
        match std::env::var("CCE_SERVICE_MODE")
            .ok()
            .map(|v| v.trim().to_lowercase())
            .as_deref()
        {
            Some("local") => Self::Local,
            _ => Self::Remote,
        }
    }
}

/// Owned service handle backing CLI domain commands.
pub enum CliService {
    /// Remote HTTP implementation.
    Remote(cce_transport::RemoteService),
    /// Local in-process implementation.
    Local(cce_server_shared::LocalService),
}

impl CliService {
    /// Build the selected implementation.
    pub async fn build(server: &str, mode: CliServiceMode) -> Result<Self> {
        match mode {
            CliServiceMode::Remote => {
                let remote = cce_transport::RemoteService::new(server)
                    .with_context(|| format!("failed to build remote service for {server}"))?;
                Ok(Self::Remote(remote))
            }
            CliServiceMode::Local => {
                let config_path = cce_config::Settings::resolve_config_path(None);
                let config = cce_config::ConfigLoader::new()
                    .with_path(std::path::PathBuf::from(&config_path))
                    .load()
                    .unwrap_or_default();
                let engine = cce_server_shared::engine::CodeContextEngine::from_config(config)
                    .await
                    .context("failed to build local engine")?;
                let local = cce_server_shared::LocalService::from_engine(&engine).await;
                Ok(Self::Local(local))
            }
        }
    }

    /// Borrow as the domain trait so commands program against one abstraction.
    pub fn as_trait(&self) -> &dyn CodeContextService {
        match self {
            Self::Remote(service) => service,
            Self::Local(service) => service,
        }
    }
}
