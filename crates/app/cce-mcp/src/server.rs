//! Transport entry points for the MCP server.
//!
//! Two transports are supported, selected by configuration:
//! - `stdio` for local clients (protocol frames own stdout; logs go to stderr).
//! - streamable HTTP on an independent, loopback-by-default port.

use std::sync::Arc;

use axum::Router;
use cce_config::modules::{McpBudgetConfig, McpConfig, McpHttpConfig, McpProfile};
use cce_server_shared::context::SharedContext;
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use rmcp::{ServiceExt, transport};
use tokio_util::sync::CancellationToken;

use crate::handler::McpServerHandler;

/// Tool-surface policy resolved once at startup and shared by every session.
#[derive(Clone)]
pub struct McpPolicy {
    /// Capability profile applied to the tool surface.
    pub profile: McpProfile,
    /// Optional explicit tool allow-list.
    pub enabled_tools: Option<Vec<String>>,
    /// Server-enforced read-only call budget.
    pub budget: McpBudgetConfig,
}

impl McpPolicy {
    /// Resolve the policy from the top-level MCP configuration.
    pub fn from_config(config: &McpConfig) -> Self {
        Self {
            profile: config.profile,
            enabled_tools: config.enabled_tools.clone(),
            budget: config.budget.clone(),
        }
    }
}

/// Build a handler factory bound to the shared application state.
fn handler_factory(
    state: Arc<SharedContext>,
    policy: McpPolicy,
) -> impl Fn() -> Result<McpServerHandler, std::io::Error> + Send + Sync + 'static {
    move || {
        Ok(McpServerHandler::new(
            state.clone(),
            policy.profile,
            policy.enabled_tools.clone(),
            policy.budget.clone(),
        ))
    }
}

/// Initialize the shared engine state from an already-resolved configuration.
///
/// The engine goes through the shared lifecycle bootstrap, so an MCP process
/// runs the same background tasks and startup recovery as the HTTP server.
pub async fn build_state(config: cce_config::AppConfig) -> anyhow::Result<Arc<SharedContext>> {
    let engine = cce_server_shared::engine::CodeContextEngine::from_config(config).await?;
    let cce_server_shared::runtime::Bootstrapped {
        engine,
        qdrant_control,
        ..
    } = cce_server_shared::runtime::bootstrap_engine(engine).await?;
    Ok(Arc::new(
        SharedContext::from_engine(engine.as_ref(), qdrant_control).await,
    ))
}

/// Serve MCP over stdio, building the state from `config`.
pub async fn run_stdio_from_config(config: cce_config::AppConfig) -> anyhow::Result<()> {
    let policy = McpPolicy::from_config(&config.mcp);
    let state = build_state(config).await?;
    run_stdio(state, policy).await
}

/// Serve MCP over HTTP, building the state from `config`.
pub async fn run_http_from_config(
    config: cce_config::AppConfig,
    cancellation_token: CancellationToken,
) -> anyhow::Result<()> {
    let http = config.mcp.http.clone();
    let policy = McpPolicy::from_config(&config.mcp);
    let state = build_state(config).await?;
    run_http(state, http, policy, cancellation_token).await
}

/// Serve MCP over stdio, blocking until the client disconnects.
///
/// Callers must ensure the process's stdout is reserved for MCP frames; all
/// logging must go to stderr.
pub async fn run_stdio(state: Arc<SharedContext>, policy: McpPolicy) -> anyhow::Result<()> {
    let handler = McpServerHandler::new(state, policy.profile, policy.enabled_tools, policy.budget);
    let service = handler.serve(transport::stdio()).await?;
    service.waiting().await?;
    Ok(())
}

/// Serve MCP over the Streamable HTTP transport on an independent port.
///
/// The listener binds to `http.host` (loopback by default); `allowed_hosts`
/// guards against DNS-rebinding. Shuts down when `cancellation_token` fires.
pub async fn run_http(
    state: Arc<SharedContext>,
    http: McpHttpConfig,
    policy: McpPolicy,
    cancellation_token: CancellationToken,
) -> anyhow::Result<()> {
    let mut config = StreamableHttpServerConfig::default();
    if !http.allowed_hosts.is_empty() {
        config = config.with_allowed_hosts(http.allowed_hosts.clone());
    }
    config.cancellation_token = cancellation_token.clone();

    let service = StreamableHttpService::new(
        handler_factory(state, policy),
        LocalSessionManager::default().into(),
        config,
    );

    let app = Router::new().nest_service("/mcp", service);
    let listener = tokio::net::TcpListener::bind((http.host.as_str(), http.port)).await?;
    tracing::info!(
        "MCP streamable-http listening on http://{}:{}/mcp",
        http.host,
        http.port
    );

    axum::serve(listener, app)
        .with_graceful_shutdown(async move { cancellation_token.cancelled().await })
        .await?;
    Ok(())
}
