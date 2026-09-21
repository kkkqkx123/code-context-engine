//! MCP server handler exposing the Code Context Engine as MCP tools.
//!
//! Every tool delegates to the shared `AppState`/engine, so the MCP surface
//! stays a thin adapter over the same capabilities used by the HTTP API and the
//! CLI. `project_id` is always an explicit tool argument to match the engine's
//! per-project lazy-loading model.

use std::path::PathBuf;
use std::sync::Arc;

use cce_orchestrator::query::RelationQueryOptions;
use cce_orchestrator::query::types::QueryOptions;
use cce_orchestrator::{IndexOptions, KeywordSearchTool};
use cce_server_shared::state::AppState;
use cce_types::EntityId;
use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerConfig},
    tool, tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

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

/// Arguments for hybrid search.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchArgs {
    /// Project ID to search within (required).
    pub project_id: i64,
    /// Natural-language or code query string.
    pub query: String,
    /// Maximum number of results to return (default 10).
    #[serde(default)]
    pub limit: Option<usize>,
    /// Optional directory prefix to restrict results (e.g. "src/parser").
    #[serde(default)]
    pub directory_prefix: Option<String>,
}

/// Arguments for keyword (BM25) search.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct KeywordSearchArgs {
    /// Project ID to search within (required).
    pub project_id: i64,
    /// Keyword query string.
    pub query: String,
    /// Maximum number of results (default 10).
    #[serde(default)]
    pub top_n: Option<usize>,
}

/// Arguments identifying an entity by ID.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct EntityArgs {
    /// Project ID the entity belongs to (required).
    pub project_id: i64,
    /// Numeric entity ID.
    pub entity_id: u64,
}

/// Arguments for call-chain traversal.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CallChainArgs {
    /// Project ID the entity belongs to (required).
    pub project_id: i64,
    /// Numeric entity ID.
    pub entity_id: u64,
    /// Traversal direction: "forward" (callees, default) or "backward" (callers).
    #[serde(default)]
    pub direction: Option<String>,
    /// Maximum traversal depth (default 3).
    #[serde(default)]
    pub max_depth: Option<usize>,
}

/// Arguments for full indexing.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct IndexProjectArgs {
    /// Project ID to index (required).
    pub project_id: i64,
    /// Optional root directory override. When omitted the project's registered
    /// root path is used.
    #[serde(default)]
    pub root_dir: Option<String>,
    /// File extensions to include (e.g. ["rs", "py"]). Empty uses project config.
    #[serde(default)]
    pub extensions: Option<Vec<String>>,
    /// Directories to exclude. Empty uses project config.
    #[serde(default)]
    pub exclude_dirs: Option<Vec<String>>,
}

/// Arguments for incremental indexing of explicit file changes.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct IncrementalIndexArgs {
    /// Project ID to update (required).
    pub project_id: i64,
    /// Changed files, each path relative to the project root.
    pub files: Vec<String>,
    /// Whether each file was deleted. Empty (all false), a single boolean
    /// applied to all files, or one boolean per file.
    #[serde(default)]
    pub deleted: Option<Vec<bool>>,
}

/// Arguments that only need a project ID.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ProjectArgs {
    /// Project ID (required).
    pub project_id: i64,
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
        if let Some(allowed) = &self.enabled_tools
            && !allowed.iter().any(|tool| tool == name)
        {
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
        let mut options = QueryOptions::new(args.query.clone(), args.project_id);
        if let Some(limit) = args.limit {
            options = options.with_limit(limit);
        }
        if let Some(prefix) = args.directory_prefix.clone() {
            options = options.with_directory_prefix(prefix);
        }
        match self.state.engine.search(args.project_id, &options).await {
            Ok(result) => {
                let items: Vec<Value> = result
                    .items
                    .iter()
                    .map(|item| {
                        json!({
                            "id": item.id,
                            "entity_ids": item.entity_ids.iter().map(EntityId::to_string).collect::<Vec<_>>(),
                            "kind": item.kind,
                            "name": item.name,
                            "file_path": item.file_path,
                            "score": item.score,
                            "bm25_score": item.bm25_score,
                            "sources": item.sources,
                            "snippet": item.snippet,
                            "start_line": item.start_line,
                            "end_line": item.end_line,
                        })
                    })
                    .collect();
                Ok(CallToolResult::success(vec![ContentBlock::text(
                    json!({
                        "total": result.total,
                        "elapsed_ms": result.elapsed_ms,
                        "sources": result.sources,
                        "items": items,
                    })
                    .to_string(),
                )]))
            }
            Err(error) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "search failed: {error}"
            ))])),
        }
    }

    #[tool(
        description = "BM25 keyword search with highlighted snippets. Complements hybrid search for exact identifier or token matches."
    )]
    async fn keyword_search(
        &self,
        Parameters(args): Parameters<KeywordSearchArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.ensure_enabled("keyword_search")?;
        let Some(sqlite) = self.state.metadata_store.clone() else {
            return Ok(CallToolResult::error(vec![ContentBlock::text(
                "metadata store is not configured; keyword search unavailable",
            )]));
        };
        let tool = KeywordSearchTool::new(self.state.engine.bm25().clone()).with_sqlite(sqlite);
        let request = cce_orchestrator::KeywordSearchRequest {
            query: args.query.clone(),
            top_n: args.top_n.unwrap_or(10),
            project_id: args.project_id,
            epoch: None,
            term_operator: Default::default(),
        };
        match tool.search(request).await {
            Ok(response) => {
                let items: Vec<Value> = response
                    .results
                    .iter()
                    .map(|item| {
                        json!({
                            "chunk_id": item.chunk_id,
                            "score": item.score,
                            "file_path": item.file_path,
                            "title": item.title,
                            "highlighted_snippet": item.highlighted_snippet,
                            "start_line": item.start_line,
                            "end_line": item.end_line,
                        })
                    })
                    .collect();
                Ok(CallToolResult::success(vec![ContentBlock::text(
                    json!({
                        "query": response.query,
                        "total": response.total,
                        "items": items,
                    })
                    .to_string(),
                )]))
            }
            Err(error) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "keyword_search failed: {error}"
            ))])),
        }
    }

    #[tool(
        description = "Get the direct callees of an entity (functions it calls), with resolved relation info."
    )]
    async fn entity_callees(
        &self,
        Parameters(args): Parameters<EntityArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.ensure_enabled("entity_callees")?;
        let searcher = match self.state.get_relation_searcher(args.project_id).await {
            Ok(searcher) => searcher,
            Err(error) => {
                return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                    "relation runtime unavailable: {error}"
                ))]));
            }
        };
        let callees = searcher.get_callees(EntityId(args.entity_id));
        let items: Vec<Value> = callees
            .iter()
            .map(|relation| {
                json!({
                    "caller": relation.caller.to_string(),
                    "callee_id": relation.callee_id.map(|id| id.to_string()),
                    "callee_name": relation.callee_name,
                    "is_external": relation.is_external,
                    "start_line": relation.span.start_position.row,
                })
            })
            .collect();
        Ok(CallToolResult::success(vec![ContentBlock::text(
            json!({ "entity_id": args.entity_id, "callees": items }).to_string(),
        )]))
    }

    #[tool(description = "Get the direct callers of an entity (functions that call it).")]
    async fn entity_callers(
        &self,
        Parameters(args): Parameters<EntityArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.ensure_enabled("entity_callers")?;
        let searcher = match self.state.get_relation_searcher(args.project_id).await {
            Ok(searcher) => searcher,
            Err(error) => {
                return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                    "relation runtime unavailable: {error}"
                ))]));
            }
        };
        let callers = searcher.get_callers(EntityId(args.entity_id));
        let items: Vec<String> = callers.iter().map(EntityId::to_string).collect();
        Ok(CallToolResult::success(vec![ContentBlock::text(
            json!({ "entity_id": args.entity_id, "callers": items }).to_string(),
        )]))
    }

    #[tool(
        description = "Traverse the call chain from an entity in one direction (forward = callees, backward = callers)."
    )]
    async fn call_chain(
        &self,
        Parameters(args): Parameters<CallChainArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.ensure_enabled("call_chain")?;
        let searcher = match self.state.get_relation_searcher(args.project_id).await {
            Ok(searcher) => searcher,
            Err(error) => {
                return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                    "relation runtime unavailable: {error}"
                ))]));
            }
        };
        let max_depth = args.max_depth.unwrap_or(3);
        let options = RelationQueryOptions {
            max_depth,
            limit: usize::MAX,
            ..Default::default()
        };
        let backward = matches!(
            args.direction.as_deref().map(str::to_ascii_lowercase),
            Some(ref direction) if direction == "backward"
        );
        let result = if backward {
            searcher.query_backward(EntityId(args.entity_id), &options)
        } else {
            searcher.query_forward(EntityId(args.entity_id), &options)
        };
        match result {
            Ok(nodes) => {
                let items: Vec<Value> = nodes
                    .iter()
                    .map(|node| {
                        json!({
                            "function_id": node.function_id.to_string(),
                            "function_name": node.function_name,
                            "file_path": node.file_path,
                            "depth": node.depth,
                            "call_line": node.call_line,
                        })
                    })
                    .collect();
                Ok(CallToolResult::success(vec![ContentBlock::text(
                    json!({
                        "entity_id": args.entity_id,
                        "direction": if backward { "backward" } else { "forward" },
                        "max_depth": max_depth,
                        "nodes": items,
                    })
                    .to_string(),
                )]))
            }
            Err(error) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "call_chain failed: {error}"
            ))])),
        }
    }

    #[tool(description = "List all registered projects with their IDs, names and root paths.")]
    async fn list_projects(&self) -> Result<CallToolResult, McpError> {
        self.ensure_enabled("list_projects")?;
        let Some(store) = self.state.metadata_store.as_ref() else {
            return Ok(CallToolResult::error(vec![ContentBlock::text(
                "metadata store is not configured; cannot list projects",
            )]));
        };
        match store
            .as_ref()
            .with_transaction(|tx| cce_storage_sqlite::ProjectRepository::get_all(tx))
        {
            Ok(records) => {
                let items: Vec<Value> = records
                    .iter()
                    .map(|record| {
                        json!({
                            "id": record.id,
                            "name": record.name,
                            "root_path": record.root_path,
                        })
                    })
                    .collect();
                Ok(CallToolResult::success(vec![ContentBlock::text(
                    json!({ "projects": items }).to_string(),
                )]))
            }
            Err(error) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "list_projects failed: {error}"
            ))])),
        }
    }

    #[tool(description = "Get a single project's registered metadata and effective configuration.")]
    async fn get_project(
        &self,
        Parameters(args): Parameters<ProjectArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.ensure_enabled("get_project")?;
        match self
            .state
            .engine
            .project_registry()
            .get_or_load(args.project_id)
            .await
        {
            Ok(entry) => Ok(CallToolResult::success(vec![ContentBlock::text(
                json!({
                    "id": entry.metadata.id,
                    "name": entry.metadata.name,
                    "root_path": entry.metadata.root_path,
                    "version": entry.version,
                })
                .to_string(),
            )])),
            Err(error) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "get_project failed: {error}"
            ))])),
        }
    }

    #[tool(
        description = "Run a full index of a project (write operation). Uses project configuration unless overridden."
    )]
    async fn index_project(
        &self,
        Parameters(args): Parameters<IndexProjectArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.ensure_enabled("index_project")?;
        let mut options = match args.root_dir.clone() {
            Some(dir) => IndexOptions::new(PathBuf::from(dir)),
            None => IndexOptions::default(),
        };
        if let Some(extensions) = args.extensions.clone()
            && !extensions.is_empty()
        {
            options = options.with_extensions(extensions);
        }
        if let Some(exclude_dirs) = args.exclude_dirs.clone()
            && !exclude_dirs.is_empty()
        {
            options = options.with_exclude_dirs(exclude_dirs);
        }
        match self.state.engine.index(args.project_id, options).await {
            Ok(result) => {
                let status = if result.is_success() {
                    "success"
                } else {
                    "partial"
                };
                let payload = json!({
                    "project_id": args.project_id,
                    "status": status,
                    "total_files": result.total_files,
                    "indexed_files": result.indexed_files,
                    "failed_files": result.failed_files,
                    "total_entities": result.total_entities,
                    "total_relations": result.total_relations,
                    "errors": result.errors(),
                });
                if result.is_success() {
                    Ok(CallToolResult::success(vec![ContentBlock::text(
                        payload.to_string(),
                    )]))
                } else {
                    Ok(CallToolResult::error(vec![ContentBlock::text(
                        payload.to_string(),
                    )]))
                }
            }
            Err(error) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "index_project failed: {error}"
            ))])),
        }
    }

    #[tool(
        description = "Incrementally re-index explicit file changes for a project (write operation)."
    )]
    async fn incremental_index(
        &self,
        Parameters(args): Parameters<IncrementalIndexArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.ensure_enabled("incremental_index")?;
        let deleted = args.deleted.clone().unwrap_or_default();
        if deleted.len() > 1 && deleted.len() != args.files.len() {
            return Ok(CallToolResult::error(vec![ContentBlock::text(
                "`deleted` must be empty, length 1, or match the number of files",
            )]));
        }
        let mut changes: Vec<(PathBuf, bool)> = Vec::with_capacity(args.files.len());
        for (index, file) in args.files.iter().enumerate() {
            let is_deleted = match deleted.len() {
                0 => false,
                1 => deleted[0],
                _ => deleted[index],
            };
            changes.push((PathBuf::from(file), is_deleted));
        }
        let coordinator = match self
            .state
            .engine
            .get_hot_update_coordinator(args.project_id)
            .await
        {
            Ok(coordinator) => coordinator,
            Err(error) => {
                return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                    "hot update coordinator unavailable: {error}"
                ))]));
            }
        };
        let coordinator = coordinator.lock().await;
        match coordinator.run_explicit_changes(changes).await {
            Ok(result) => Ok(CallToolResult::success(vec![ContentBlock::text(
                json!({
                    "project_id": args.project_id,
                    "operation_id": result.operation_id,
                    "status": format!("{:?}", result.status),
                })
                .to_string(),
            )])),
            Err(error) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "incremental_index failed: {error}"
            ))])),
        }
    }

    #[tool(
        description = "Health summary of the engine's storage backends for a project (metadata store, BM25, relation runtime)."
    )]
    async fn health(
        &self,
        Parameters(args): Parameters<ProjectArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.ensure_enabled("health")?;
        let relation = self
            .state
            .engine
            .get_relation_capability_info(args.project_id)
            .await
            .ok()
            .map(|info| {
                json!({
                    "active_epoch": info.active_epoch,
                    "runtime_epoch": info.runtime_epoch,
                    "rebuild_required": info.rebuild_required,
                })
            });
        let bm25_enabled = {
            let client = self.state.engine.bm25().lock().await;
            client.is_enabled()
        };
        Ok(CallToolResult::success(vec![ContentBlock::text(
            json!({
                "project_id": args.project_id,
                "metadata_store_configured": self.state.metadata_store.is_some(),
                "bm25_enabled": bm25_enabled,
                "relation": relation,
            })
            .to_string(),
        )]))
    }
}

#[tool_handler]
impl ServerHandler for McpServerHandler {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::from_build_env())
            .with_instructions(
                "Code Context Engine MCP server. Provides hybrid and keyword code search, \
                 entity/relation queries, index management, and project listing. Every tool \
                 takes an explicit `project_id`."
                    .to_string(),
            )
    }
}
