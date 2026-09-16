//! Transport entry points for the MCP server.
//!
//! Two transports are supported, selected by configuration:
//! - `stdio` for local clients (protocol frames own stdout; logs go to stderr).
//! - streamable HTTP on an independent, loopback-by-default port.

use std::sync::Arc;

use axum::Router;
use cce_config::modules::McpHttpConfig;
use cce_server_shared::state::AppState;
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use rmcp::{ServiceExt, transport};
use tokio_util::sync::CancellationToken;

use crate::handler::McpServerHandler;

/// Build a handler factory bound to the shared application state.
fn handler_factory(
    state: Arc<AppState>,
    enabled_tools: Option<Vec<String>>,
) -> impl Fn() -> Result<McpServerHandler, std::io::Error> + Send + Sync + 'static {
    move || Ok(McpServerHandler::new(state.clone(), enabled_tools.clone()))
}

/// Initialize the shared engine state from an already-resolved configuration.
///
/// Convenience for callers (e.g. the CLI) that only have an `AppConfig`.
pub async fn build_state(config: cce_config::AppConfig) -> anyhow::Result<Arc<AppState>> {
    let engine = cce_server_shared::engine::CodeContextEngine::from_config(config).await?;
    Ok(Arc::new(AppState::from_engine(&engine, None).await))
}

/// Serve MCP over stdio, building the state from `config`.
///
/// Convenience wrapper around the shared state + [`run_stdio`].
pub async fn run_stdio_from_config(
    config: cce_config::AppConfig,
    enabled_tools: Option<Vec<String>>,
) -> anyhow::Result<()> {
    let state = build_state(config).await?;
    run_stdio(state, enabled_tools).await
}

/// Serve MCP over HTTP, building the state from `config`.
pub async fn run_http_from_config(
    config: cce_config::AppConfig,
    http: McpHttpConfig,
    enabled_tools: Option<Vec<String>>,
    cancellation_token: CancellationToken,
) -> anyhow::Result<()> {
    let state = build_state(config).await?;
    run_http(state, http, enabled_tools, cancellation_token).await
}

/// Serve MCP over stdio, blocking until the client disconnects.
///
/// Callers must ensure the process's stdout is reserved for MCP frames; all
/// logging must go to stderr.
pub async fn run_stdio(
    state: Arc<AppState>,
    enabled_tools: Option<Vec<String>>,
) -> anyhow::Result<()> {
    let handler = McpServerHandler::new(state, enabled_tools);
    let service = handler.serve(transport::stdio()).await?;
    service.waiting().await?;
    Ok(())
}

/// Serve MCP over the Streamable HTTP transport on an independent port.
///
/// The listener binds to `http.host` (loopback by default); `allowed_hosts`
/// guards against DNS-rebinding. Shuts down when `cancellation_token` fires.
pub async fn run_http(
    state: Arc<AppState>,
    http: McpHttpConfig,
    enabled_tools: Option<Vec<String>>,
    cancellation_token: CancellationToken,
) -> anyhow::Result<()> {
    let mut config = StreamableHttpServerConfig::default();
    if !http.allowed_hosts.is_empty() {
        config = config.with_allowed_hosts(http.allowed_hosts.clone());
    }
    config.cancellation_token = cancellation_token.clone();

    let service = StreamableHttpService::new(
        handler_factory(state, enabled_tools),
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
