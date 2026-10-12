//! MCP server handler exposing the Code Context Engine as MCP tools.
//!
//! Every tool delegates to the shared `AppState`/engine, so the MCP surface
//! stays a thin adapter over the same capabilities used by the HTTP API and the
//! CLI. `project_id` is an explicit tool argument for project-scoped tools to
//! match the engine's per-project lazy-loading model. The stateless `fold`
//! tools are the exception and carry no `project_id`.
//!
//! Item shapes reuse the shared API DTO field names so HTTP and MCP callers
//! observe the same `SearchResultItem`, `KeywordSearchItem`, and fold payloads.
//! The MCP transport returns tool payloads without the HTTP success envelope.
//! Covered here: search, keyword search, entity callees/callers, call chain,
//! project listing, full and incremental indexing, single and batch fold, and
//! engine health. Graph traversal, storage management, metrics history, watch,
//! configuration, and ingest entries remain HTTP-only and are not exposed here.
//!
//! The tool implementations are grouped by domain in submodules:
//! `search` (hybrid + keyword search), `relations` (entity callees/callers and
//! call chains), `project` (project listing, indexing, health), `fold`
//! (stateless folding), and `support` (shared helpers). This module keeps the
//! handler struct, the static tool router, and the thin `#[tool]` wrappers so
//! the rmcp router codegen stays in one place.

mod fold;
mod project;
mod relations;
mod search;
mod support;

use std::sync::Arc;

use cce_server_shared::state::AppState;
use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, Implementation, ServerCapabilities, ServerConfig},
    tool, tool_handler, tool_router,
};

pub use fold::{FoldArgs, FoldBatchArgs, FoldBatchItemArgs};
pub use project::{IndexProjectArgs, IncrementalIndexArgs, ProjectArgs};
pub use relations::{CallChainArgs, EntityArgs};
pub use search::{KeywordSearchArgs, SearchArgs};

/// MCP server handler. Cloned per session by the streamable-HTTP transport, so
/// it only holds cheap `Arc`s.
#[derive(Clone)]
pub struct McpServerHandler {
    state: Arc<AppState>,
    /// Optional tool allow-list: when set, tools whose name is not listed are
    /// rejected at call time. The router stays static so `#[tool_router]`
    /// codegen does not depend on runtime configuration.
    enabled_tools: Option<Arc<Vec<String>>>,
    /// Read only by the generated `ServerHandler::call_tool` impl, which the
    /// compiler cannot see through, hence the explicit allow.
    #[allow(dead_code)]
    tool_router: ToolRouter<McpServerHandler>,
}

#[tool_router]
impl McpServerHandler {
    /// Create a handler backed by the shared application state.
    pub fn new(state: Arc<AppState>, enabled_tools: Option<Vec<String>>) -> Self {
        Self {
            state,
            enabled_tools: enabled_tools.map(Arc::new),
            tool_router: Self::tool_router(),
        }
    }

    /// Reject calls to tools that are not in the configured allow-list.
    fn ensure_enabled(&self, name: &str) -> Result<(), McpError> {
        if !support::is_tool_allowed(self.enabled_tools.as_ref().map(|v| v.as_slice()), name) {
            return Err(McpError::invalid_params(
                format!("tool '{name}' is disabled by configuration"),
                None,
            ));
        }
        Ok(())
    }

    #[tool(
        description = "Hybrid (semantic + BM25) code search within a project. Returns matching chunks with file path, line range and score."
    )]
    async fn search(
        &self,
        Parameters(args): Parameters<SearchArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.ensure_enabled("search")?;
        search::search(&self.state, args).await
    }

    #[tool(
        description = "BM25 keyword search returning raw source snippets with file paths and line ranges. Complements hybrid search for exact identifier or token matches."
    )]
    async fn keyword_search(
        &self,
        Parameters(args): Parameters<KeywordSearchArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.ensure_enabled("keyword_search")?;
        search::keyword_search(&self.state, args).await
    }

    #[tool(
        description = "Get the direct callees of an entity (functions it calls), with resolved relation info."
    )]
    async fn entity_callees(
        &self,
        Parameters(args): Parameters<EntityArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.ensure_enabled("entity_callees")?;
        relations::entity_callees(&self.state, args).await
    }

    #[tool(description = "Get the direct callers of an entity (functions that call it).")]
    async fn entity_callers(
        &self,
        Parameters(args): Parameters<EntityArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.ensure_enabled("entity_callers")?;
        relations::entity_callers(&self.state, args).await
    }

    #[tool(
        description = "Traverse the call chain from an entity in one direction (forward/down = callees, backward/up = callers)."
    )]
    async fn call_chain(
        &self,
        Parameters(args): Parameters<CallChainArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.ensure_enabled("call_chain")?;
        relations::call_chain(&self.state, args).await
    }

    #[tool(description = "List all registered projects with their IDs, names and root paths.")]
    async fn list_projects(&self) -> Result<CallToolResult, McpError> {
        self.ensure_enabled("list_projects")?;
        project::list_projects(&self.state).await
    }

    #[tool(description = "Get a single project's registered metadata and effective configuration.")]
    async fn get_project(
        &self,
        Parameters(args): Parameters<ProjectArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.ensure_enabled("get_project")?;
        project::get_project(&self.state, args).await
    }

    #[tool(
        description = "Run a full index of a project (write operation). Uses project configuration unless overridden."
    )]
    async fn index_project(
        &self,
        Parameters(args): Parameters<IndexProjectArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.ensure_enabled("index_project")?;
        project::index_project(&self.state, args).await
    }

    #[tool(
        description = "Incrementally re-index explicit file changes for a project (write operation)."
    )]
    async fn incremental_index(
        &self,
        Parameters(args): Parameters<IncrementalIndexArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.ensure_enabled("incremental_index")?;
        project::incremental_index(&self.state, args).await
    }

    #[tool(
        description = "Fold raw source text into a symbol skeleton (stateless, no project_id). Degrades to truncated text with structure_known=false for unknown languages or parse failures."
    )]
    async fn fold(
        &self,
        Parameters(args): Parameters<FoldArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.ensure_enabled("fold")?;
        fold::fold(args).await
    }

    #[tool(
        description = "Fold multiple raw source texts into symbol skeletons in one call (stateless, no project_id). Each result echoes its input id."
    )]
    async fn fold_batch(
        &self,
        Parameters(args): Parameters<FoldBatchArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.ensure_enabled("fold_batch")?;
        fold::fold_batch(args).await
    }

    #[tool(
        description = "Health summary of the engine's storage backends for a project (metadata store, BM25, relation runtime)."
    )]
    async fn health(
        &self,
        Parameters(args): Parameters<ProjectArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.ensure_enabled("health")?;
        project::health(&self.state, args).await
    }
}

#[tool_handler]
impl ServerHandler for McpServerHandler {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::from_build_env())
            .with_instructions(
                "Code Context Engine MCP server. Provides hybrid and keyword code search, \
                 entity/relation queries, index management, and project listing. \
                 Project-scoped tools take an explicit `project_id`; the stateless \
                 `fold` tool takes raw text instead."
                    .to_string(),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both transports (`run_stdio` and `run_http`) construct the same
    /// `McpServerHandler` through the same static router, so the exposed
    /// tool set and the allow-list denial must be transport-independent.
    /// This locks that seam: any transport-specific filtering turns red.
    #[test]
    fn stdio_and_http_transports_share_one_tool_surface() {
        let names_of = |tools: Vec<rmcp::model::Tool>| -> Vec<String> {
            let mut names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();
            names.sort();
            names
        };
        let stdio_names = names_of(McpServerHandler::tool_router().list_all());
        let http_names = names_of(McpServerHandler::tool_router().list_all());
        assert_eq!(stdio_names, http_names);
        assert_eq!(stdio_names.len(), 12);
    }

    #[test]
    fn tool_router_exposes_twelve_tools_without_http_only_entries() {
        let router = McpServerHandler::tool_router();
        let tools = router.list_all();
        assert_eq!(tools.len(), 12);
        for name in [
            "search",
            "keyword_search",
            "entity_callees",
            "entity_callers",
            "call_chain",
            "list_projects",
            "get_project",
            "index_project",
            "incremental_index",
            "fold",
            "fold_batch",
            "health",
        ] {
            assert!(router.has_route(name), "tool {name} must be exposed");
        }
        for http_only in [
            "graph_ego",
            "graph_path",
            "storage_status",
            "watch_status",
            "config_info",
            "ingest_manifest",
        ] {
            assert!(
                !router.has_route(http_only),
                "HTTP-only {http_only} must stay out of MCP"
            );
        }
    }

    /// Allow-list denial is a pure function of the configured list, so a
    /// denied tool is denied on stdio and streamable HTTP alike — both
    /// transports route through this single decision point.
    #[test]
    fn allow_list_denial_is_identical_on_both_transports() {
        let allow = Some(vec!["search".to_string()]);
        let denied = "fold";
        let allowed = allow.as_deref();
        assert!(!support::is_tool_allowed(allowed, denied));
        assert!(support::is_tool_allowed(allowed, "search"));
        assert!(support::is_tool_allowed(None, denied));
    }

    /// Direction spellings from either transport canonicalize identically,
    /// so an HTTP `down`/`up` and an MCP `forward`/`backward` drive the
    /// same traversal.
    #[test]
    fn direction_spellings_agree_across_transports() {
        assert_eq!(support::canonical_call_direction(Some("down")), ("forward", false));
        assert_eq!(
            support::canonical_call_direction(Some("forward")),
            ("forward", false)
        );
        assert_eq!(support::canonical_call_direction(Some("up")), ("backward", true));
        assert_eq!(
            support::canonical_call_direction(Some("backward")),
            ("backward", true)
        );
    }
}
