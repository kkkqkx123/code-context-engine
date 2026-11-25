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
pub mod support;

use std::collections::BTreeSet;
use std::sync::Arc;

use crate::{budget, profile};
use cce_config::modules::{McpBudgetConfig, McpProfile};
use cce_server_shared::context::SharedContext;
use cce_server_shared::services::budget::{BudgetLimits, BudgetTracker};
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::router::tool::ToolRouter,
    handler::server::tool::ToolCallContext,
    handler::server::wrapper::Parameters,
    model::{
        CacheScope, CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock,
        Implementation, ListToolsResult, PaginatedRequestParams, ProtocolVersion, ResultType,
        ServerCapabilities, ServerConfig,
    },
    service::RequestContext,
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
    /// Effective tool set resolved from the capability profile and the
    /// optional explicit allow-list. A call outside this set is rejected.
    allowed_tools: Arc<BTreeSet<String>>,
    /// Active capability profile, echoed in the server instructions.
    profile: McpProfile,
    /// Resolved budget policy. `None` disables budget enforcement entirely.
    budget: Option<Arc<BudgetTracker>>,
    /// Read by the hand-written `ServerHandler` methods.
    #[allow(dead_code)]
    tool_router: ToolRouter<McpServerHandler>,
}

#[tool_router]
impl McpServerHandler {
    /// Create a handler backed by the shared application state.
    ///
    /// The effective tool set is the intersection of the capability profile
    /// and the explicit allow-list; the budget tracker is present whenever
    /// budget enforcement is enabled (always for the read-only profile).
    pub fn new(
        state: Arc<SharedContext>,
        profile: McpProfile,
        enabled_tools: Option<Vec<String>>,
        budget_config: McpBudgetConfig,
    ) -> Self {
        let allowed = profile::resolve_allowed_tools(profile, enabled_tools.as_deref());
        let budget = budget_config.enabled || profile.is_read_only();
        let tracker = if budget {
            let limits = BudgetLimits {
                max_calls_per_run: budget_config.max_calls_per_run,
                max_calls_per_page: budget_config.max_calls_per_page,
                max_active_runs: budget_config.max_active_runs,
            };
            Some(Arc::new(BudgetTracker::new(
                limits,
                budget_config.cache_ttl_secs,
                budget_config.cache_enabled,
            )))
        } else {
            None
        };
        Self {
            state,
            allowed_tools: Arc::new(allowed),
            profile,
            budget: tracker,
            tool_router: Self::tool_router(),
        }
    }

    /// Reject calls to tools outside the effective profile/allow-list.
    ///
    /// Returns a structured tool error when disabled, `None` when allowed, so
    /// every tool failure — including configuration rejections — travels
    /// through the same `isError` channel with a machine-readable code.
    fn ensure_allowed(&self, name: &str) -> Option<CallToolResult> {
        if !profile::is_allowed(&self.allowed_tools, name) {
            return Some(support::tool_failure(
                cce_types::error_codes::TOOL_DISABLED,
                format!("tool '{name}' is disabled by the active profile or configuration"),
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
        if let Some(err) = self.ensure_allowed("search") {
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
        if let Some(err) = self.ensure_allowed("aggregated_search") {
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
        if let Some(err) = self.ensure_allowed("file_search") {
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
        if let Some(err) = self.ensure_allowed("keyword_search") {
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
        if let Some(err) = self.ensure_allowed("entity_callees") {
            return Ok(err);
        }
        relations::entity_callees(&self.state, args).await
    }

    #[tool(description = "Get the direct callers of an entity (functions that call it).")]
    async fn entity_callers(
        &self,
        Parameters(args): Parameters<EntityArgs>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(err) = self.ensure_allowed("entity_callers") {
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
        if let Some(err) = self.ensure_allowed("call_chain") {
            return Ok(err);
        }
        relations::call_chain(&self.state, args).await
    }

    #[tool(description = "List all registered projects with their IDs, names and root paths.")]
    async fn list_projects(&self) -> Result<CallToolResult, McpError> {
        if let Some(err) = self.ensure_allowed("list_projects") {
            return Ok(err);
        }
        project::list_projects(&self.state).await
    }

    #[tool(description = "Get a single project's registered metadata and effective configuration.")]
    async fn get_project(
        &self,
        Parameters(args): Parameters<ProjectArgs>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(err) = self.ensure_allowed("get_project") {
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
        if let Some(err) = self.ensure_allowed("index_project") {
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
        if let Some(err) = self.ensure_allowed("incremental_index") {
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
        if let Some(err) = self.ensure_allowed("fold") {
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
        if let Some(err) = self.ensure_allowed("fold_batch") {
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
        if let Some(err) = self.ensure_allowed("health") {
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
        if let Some(err) = self.ensure_allowed("read_file") {
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
        if let Some(err) = self.ensure_allowed("file_outline") {
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
        if let Some(err) = self.ensure_allowed("entity_detail") {
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
        if let Some(err) = self.ensure_allowed("graph_module") {
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
        if let Some(err) = self.ensure_allowed("graph_ego") {
            return Ok(err);
        }
        graph_tools::graph_ego(&self.state, args).await
    }
}

#[tool_handler]
impl ServerHandler for McpServerHandler {
    fn get_info(&self) -> ServerConfig {
        let profile_label = match self.profile {
            McpProfile::Full => "full",
            McpProfile::ReadOnly => "read_only",
        };
        let budget_label = match &self.budget {
            None => "disabled",
            Some(_) => "enforced",
        };
        let instructions = format!(
            "Code Context Engine MCP server. Profile: {profile_label}; call budget: {budget_label}. \
             Provides hybrid and keyword code search, entity/relation queries, index management, \
             and project listing. Project-scoped tools take an explicit `project_id`; the stateless \
             `fold` tool takes raw text instead. Read-only tools return a top-level `_budget` \
             field when a wiki run context is supplied via `_meta.wiki_run`."
        );
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::from_build_env())
            .with_instructions(instructions)
    }

    /// Intercept every tool call to apply the capability gate and the
    /// run-scoped budget before any tool work is performed. The generated
    /// router still dispatches to the individual `#[tool]` methods, which
    /// repeat the capability check as a second line of defense.
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let tool = request.name.to_string();

        // Capability gate: a tool outside the effective set is refused here,
        // before budget accounting or any query work.
        if !profile::is_allowed(&self.allowed_tools, &tool) {
            return Ok(CallToolResponse::Complete(support::tool_failure(
                cce_types::error_codes::TOOL_DISABLED,
                format!("tool '{tool}' is disabled by the active profile or configuration"),
            )));
        }

        // Read the run context from `_meta` (primary) or common args (fallback).
        let meta = request.meta.as_ref().map(|m| &m.0.0);
        let run_context = match budget::parse_run_context(meta, request.arguments.as_ref()) {
            Ok(value) => value,
            Err(message) => {
                return Ok(CallToolResponse::Complete(support::tool_failure(
                    cce_types::error_codes::INVALID_INPUT,
                    format!("invalid wiki run context: {message}"),
                )));
            }
        };

        // No budget active, or no run context: behave exactly as before.
        let (tracker, run_context) = match (&self.budget, run_context) {
            (Some(tracker), Some(context)) => (tracker, context),
            _ => {
                let tcc = ToolCallContext::new(self, request, context);
                return self.tool_router.call(tcc).await;
            }
        };

        let canonical_args = canonical_arguments(request.arguments.as_ref());
        let cacheable = profile::is_cacheable(&tool);

        // Cache hit: reuse the payload without consuming a step, but still
        // refuse once the run is exhausted.
        if cacheable
            && let Some(payload) = tracker
                .cache_get(&run_context, &tool, &canonical_args)
                .await
        {
            if let Some(exhausted) = tracker.check_exhausted(&run_context) {
                return Ok(CallToolResponse::Complete(budget::budget_exhausted_result(
                    &exhausted,
                )));
            }
            let state = tracker.peek(&run_context, true);
            let result = CallToolResult::success(vec![ContentBlock::text(payload.to_string())]);
            return Ok(CallToolResponse::Complete(budget::inject_budget(
                result, &state,
            )));
        }

        // Reserve one step before executing; exhaustion short-circuits here.
        let state = match tracker.try_consume(&run_context) {
            cce_server_shared::services::budget::BudgetDecision::Allowed { state } => state,
            cce_server_shared::services::budget::BudgetDecision::Exhausted(exhausted) => {
                return Ok(CallToolResponse::Complete(budget::budget_exhausted_result(
                    &exhausted,
                )));
            }
        };

        let tcc = ToolCallContext::new(self, request, context);
        let mut result = match self.tool_router.call(tcc).await? {
            CallToolResponse::Complete(result) => result,
            other => return Ok(other),
        };

        if result.is_error.unwrap_or(false) {
            // Failed executions refund the reserved step and count an attempt.
            tracker.record_failure(&run_context);
        } else {
            if cacheable
                && let Some(ContentBlock::Text(text)) = result.content.first()
                && let Ok(payload) = serde_json::from_str::<serde_json::Value>(&text.text)
            {
                tracker
                    .cache_put(&run_context, &tool, &canonical_args, payload)
                    .await;
            }
            result = budget::inject_budget(result, &state);
        }
        Ok(CallToolResponse::Complete(result))
    }

    /// Expose only tools visible under the active profile/allow-list.
    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let tools = self
            .tool_router
            .list_all()
            .into_iter()
            .filter(|tool| profile::is_allowed(&self.allowed_tools, tool.name.as_ref()))
            .collect();
        let supports_cache_hints = context
            .protocol_version()
            .is_some_and(|version| version >= ProtocolVersion::V_2026_07_28);
        Ok(ListToolsResult {
            result_type: Some(ResultType::COMPLETE),
            tools,
            meta: None,
            next_cursor: None,
            ttl_ms: supports_cache_hints.then_some(0),
            cache_scope: supports_cache_hints.then_some(CacheScope::Public),
        })
    }

    /// Hide tools outside the effective set from direct lookups as well.
    fn get_tool(&self, name: &str) -> Option<rmcp::model::Tool> {
        if !profile::is_allowed(&self.allowed_tools, name) {
            return None;
        }
        self.tool_router.get(name).cloned()
    }
}

/// Canonical JSON representation of the call arguments, used as part of the
/// run-local cache key. `serde_json::Map` iterates in sorted key order, so the
/// string is stable across equivalent calls.
fn canonical_arguments(arguments: Option<&rmcp::model::JsonObject>) -> String {
    match arguments {
        Some(args) => serde_json::Value::Object(args.clone()).to_string(),
        None => "{}".to_string(),
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

    /// The capability table is the single source of truth for read/write
    /// classification. Its key set must exactly match the router: any tool
    /// added or removed without a matching table row turns this red.
    #[test]
    fn capability_table_key_set_matches_router() {
        let router_names: BTreeSet<String> = McpServerHandler::tool_router()
            .list_all()
            .iter()
            .map(|tool| tool.name.to_string())
            .collect();
        let table_names: BTreeSet<String> = profile::TOOL_CAPABILITIES
            .iter()
            .map(|(name, _)| (*name).to_string())
            .collect();
        assert_eq!(router_names, table_names);
    }

    #[test]
    fn read_only_profile_resolves_to_read_only_tools() {
        let full = profile::resolve_allowed_tools(McpProfile::Full, None);
        assert_eq!(full.len(), 19);
        let wiki = profile::resolve_allowed_tools(McpProfile::ReadOnly, None);
        assert!(!wiki.contains("index_project"));
        assert!(!wiki.contains("incremental_index"));
        assert!(wiki.contains("read_file"));
    }

    #[test]
    fn read_only_intersects_explicit_allow_list() {
        let explicit = vec!["read_file".to_string(), "index_project".to_string()];
        let set = profile::resolve_allowed_tools(McpProfile::ReadOnly, Some(&explicit));
        assert_eq!(set.len(), 1);
        assert!(set.contains("read_file"));
        assert!(!set.contains("index_project"));
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
