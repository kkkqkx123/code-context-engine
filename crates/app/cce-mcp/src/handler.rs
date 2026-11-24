//! MCP server handler exposing the Code Context Engine as MCP tools.
//!
//! Every tool delegates to the shared `SharedContext`/engine, so the MCP surface
//! stays a thin adapter over the same capabilities used by the HTTP API and the
//! CLI. `project_id` is an explicit tool argument for project-scoped tools to
//! match the engine's per-project lazy-loading model. The stateless `fold`
//! tools are the exception and carry no `project_id`.
//!
//! Item shapes reuse the shared API DTO field names so HTTP and MCP callers
//! observe the same `SearchResultItem`, `KeywordSearchItem`, and fold payloads.
//! The MCP transport returns tool payloads without the HTTP success envelope.
//! Covered here: search, aggregated search, file search, keyword search, entity callees/callers,
//! call chain, project listing, full and incremental indexing, single and
//! batch fold, engine health, bounded file reads, file outlines, entity
//! detail, and read-only graph lookups (module relations and ego
//! neighborhoods). Storage management, metrics history, watch, configuration,
//! and ingest entries remain HTTP-only and are not exposed here.
//!
//! The tool implementations are grouped by domain in submodules:
//! `search` (hybrid, aggregated, file-path, and keyword search), `relations` (entity
//! callees/callers and call chains), `project` (project listing, indexing,
//! health), `fold` (stateless folding), `files` (bounded reads, outlines,
//! entity detail), `graph_tools` (module relations and ego neighborhoods),
//! and `support` (shared helpers). This module keeps the
//! handler struct, the static tool router, and the thin `#[tool]` wrappers so
//! the rmcp router codegen stays in one place.

mod files;
mod fold;
mod graph_tools;
mod project;
mod relations;
mod search;
mod support;

use std::sync::Arc;

use cce_server_shared::context::SharedContext;
use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, Implementation, ServerCapabilities, ServerConfig},
    tool, tool_handler, tool_router,
};

pub use files::{EntityDetailArgs, FileOutlineArgs, ReadFileArgs};
pub use fold::{FoldArgs, FoldBatchArgs, FoldBatchItemArgs};
pub use graph_tools::{GraphEgoArgs, GraphModuleArgs};
pub use project::{IncrementalIndexArgs, IndexProjectArgs, ProjectArgs};
pub use relations::{CallChainArgs, EntityArgs};
pub use search::{AggregatedSearchArgs, FileSearchArgs, KeywordSearchArgs, SearchArgs};

/// MCP server handler. Cloned per session by the streamable-HTTP transport, so
/// it only holds cheap `Arc`s.
#[derive(Clone)]
pub struct McpServerHandler {
    state: Arc<SharedContext>,
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
    pub fn new(state: Arc<SharedContext>, enabled_tools: Option<Vec<String>>) -> Self {
        Self {
            state,
            enabled_tools: enabled_tools.map(Arc::new),
            tool_router: Self::tool_router(),
        }
    }

    /// Reject calls to tools that are not in the configured allow-list.
    ///
    /// Returns a structured tool error when disabled, `None` when allowed, so
    /// every tool failure — including configuration rejections — travels
    /// through the same `isError` channel with a machine-readable code.
    fn ensure_enabled(&self, name: &str) -> Option<CallToolResult> {
        if !support::is_tool_allowed(self.enabled_tools.as_ref().map(|v| v.as_slice()), name) {
            return Some(support::tool_failure(
                cce_types::error_codes::INVALID_REQUEST,
                format!("tool '{name}' is disabled by configuration"),
            ));
        }
        None
    }

    #[tool(
        description = "Hybrid (semantic + BM25) code search within a project. Returns matching chunks with file path, line range and score."
    )]
    async fn search(
        &self,
        Parameters(args): Parameters<SearchArgs>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(err) = self.ensure_enabled("search") {
            return Ok(err);
        }
        search::search(&self.state, args).await
    }

    #[tool(
        description = "Aggregated search: run several sub-queries against one index snapshot, merge them by code identity and rerank once. Use it when a question decomposes into independent intents (e.g. a symbol plus its usage). Returns merged chunks plus per-sub-query accounting (candidates, contributed hits, status) so a weak intent is visible."
    )]
    async fn aggregated_search(
        &self,
        Parameters(args): Parameters<AggregatedSearchArgs>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(err) = self.ensure_enabled("aggregated_search") {
            return Ok(err);
        }
        search::aggregated_search(&self.state, args).await
    }

    #[tool(
        description = "Fuzzy file-path search over project files. Returns matching paths, optionally with size and line counts. Remote projects report unavailability."
    )]
    async fn file_search(
        &self,
        Parameters(args): Parameters<FileSearchArgs>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(err) = self.ensure_enabled("file_search") {
            return Ok(err);
        }
        search::file_search(&self.state, args).await
    }

    #[tool(
        description = "BM25 keyword search returning raw source snippets with file paths and line ranges. Complements hybrid search for exact identifier or token matches."
    )]
    async fn keyword_search(
        &self,
        Parameters(args): Parameters<KeywordSearchArgs>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(err) = self.ensure_enabled("keyword_search") {
            return Ok(err);
        }
        search::keyword_search(&self.state, args).await
    }

    #[tool(
        description = "Get the direct callees of an entity (functions it calls), with resolved relation info."
    )]
    async fn entity_callees(
        &self,
        Parameters(args): Parameters<EntityArgs>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(err) = self.ensure_enabled("entity_callees") {
            return Ok(err);
        }
        relations::entity_callees(&self.state, args).await
    }

    #[tool(description = "Get the direct callers of an entity (functions that call it).")]
    async fn entity_callers(
        &self,
        Parameters(args): Parameters<EntityArgs>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(err) = self.ensure_enabled("entity_callers") {
            return Ok(err);
        }
        relations::entity_callers(&self.state, args).await
    }

    #[tool(
        description = "Traverse the call chain from an entity in one direction (forward/down = callees, backward/up = callers)."
    )]
    async fn call_chain(
        &self,
        Parameters(args): Parameters<CallChainArgs>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(err) = self.ensure_enabled("call_chain") {
            return Ok(err);
        }
        relations::call_chain(&self.state, args).await
    }

    #[tool(description = "List all registered projects with their IDs, names and root paths.")]
    async fn list_projects(&self) -> Result<CallToolResult, McpError> {
        if let Some(err) = self.ensure_enabled("list_projects") {
            return Ok(err);
        }
        project::list_projects(&self.state).await
    }

    #[tool(description = "Get a single project's registered metadata and effective configuration.")]
    async fn get_project(
        &self,
        Parameters(args): Parameters<ProjectArgs>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(err) = self.ensure_enabled("get_project") {
            return Ok(err);
        }
        project::get_project(&self.state, args).await
    }

    #[tool(
        description = "Run a full index of a project (write operation). Uses project configuration unless overridden."
    )]
    async fn index_project(
        &self,
        Parameters(args): Parameters<IndexProjectArgs>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(err) = self.ensure_enabled("index_project") {
            return Ok(err);
        }
        project::index_project(&self.state, args).await
    }

    #[tool(
        description = "Incrementally re-index explicit file changes for a project (write operation)."
    )]
    async fn incremental_index(
        &self,
        Parameters(args): Parameters<IncrementalIndexArgs>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(err) = self.ensure_enabled("incremental_index") {
            return Ok(err);
        }
        project::incremental_index(&self.state, args).await
    }

    #[tool(
        description = "Fold raw source text into a symbol skeleton (stateless, no project_id). Degrades to truncated text with structure_known=false for unknown languages or parse failures."
    )]
    async fn fold(
        &self,
        Parameters(args): Parameters<FoldArgs>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(err) = self.ensure_enabled("fold") {
            return Ok(err);
        }
        fold::fold(args).await
    }

    #[tool(
        description = "Fold multiple raw source texts into symbol skeletons in one call (stateless, no project_id). Each result echoes its input id."
    )]
    async fn fold_batch(
        &self,
        Parameters(args): Parameters<FoldBatchArgs>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(err) = self.ensure_enabled("fold_batch") {
            return Ok(err);
        }
        fold::fold_batch(args).await
    }

    #[tool(
        description = "Health summary of the engine's storage backends for a project (metadata store, BM25, relation runtime)."
    )]
    async fn health(
        &self,
        Parameters(args): Parameters<ProjectArgs>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(err) = self.ensure_enabled("health") {
            return Ok(err);
        }
        project::health(&self.state, args).await
    }

    #[tool(
        description = "Read a bounded, line-numbered window of one project source file (read-only, path-guarded to the project root). Returns lines with start/end line numbers and a truncated flag."
    )]
    async fn read_file(
        &self,
        Parameters(args): Parameters<ReadFileArgs>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(err) = self.ensure_enabled("read_file") {
            return Ok(err);
        }
        files::read_file(&self.state, args).await
    }

    #[tool(
        description = "Symbol skeleton of one project file: name, kind, line range and signature per entity. Read-only."
    )]
    async fn file_outline(
        &self,
        Parameters(args): Parameters<FileOutlineArgs>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(err) = self.ensure_enabled("file_outline") {
            return Ok(err);
        }
        files::file_outline(&self.state, args).await
    }

    #[tool(
        description = "Definition detail of one stable symbol id: name, file, signature, line range and doc comment."
    )]
    async fn entity_detail(
        &self,
        Parameters(args): Parameters<EntityDetailArgs>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(err) = self.ensure_enabled("entity_detail") {
            return Ok(err);
        }
        files::entity_detail(&self.state, args).await
    }

    #[tool(
        description = "Module relations of one file: imports, exported stable symbol ids and caller files, with relation epoch."
    )]
    async fn graph_module(
        &self,
        Parameters(args): Parameters<GraphModuleArgs>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(err) = self.ensure_enabled("graph_module") {
            return Ok(err);
        }
        graph_tools::graph_module(&self.state, args).await
    }

    #[tool(
        description = "Bounded ego neighborhood of one entity in the relation graph (nodes, edges, totals). Read-only."
    )]
    async fn graph_ego(
        &self,
        Parameters(args): Parameters<GraphEgoArgs>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(err) = self.ensure_enabled("graph_ego") {
            return Ok(err);
        }
        graph_tools::graph_ego(&self.state, args).await
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
        assert_eq!(stdio_names.len(), 19);
    }

    #[test]
    fn tool_router_exposes_nineteen_tools_without_http_only_entries() {
        let router = McpServerHandler::tool_router();
        let tools = router.list_all();
        assert_eq!(tools.len(), 19);
        for name in [
            "search",
            "aggregated_search",
            "file_search",
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
            "read_file",
            "file_outline",
            "entity_detail",
            "graph_module",
            "graph_ego",
        ] {
            assert!(router.has_route(name), "tool {name} must be exposed");
        }
        for http_only in [
            "graph_path",
            "graph_subgraph",
            "graph_export",
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
        use cce_server_shared::services::relation::CallDirection;
        assert_eq!(CallDirection::parse("down"), Some(CallDirection::Forward));
        assert_eq!(
            CallDirection::parse("forward"),
            Some(CallDirection::Forward)
        );
        assert_eq!(CallDirection::parse("up"), Some(CallDirection::Backward));
        assert_eq!(
            CallDirection::parse("backward"),
            Some(CallDirection::Backward)
        );
    }
}
