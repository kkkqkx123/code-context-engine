//! Domain service abstraction for local and remote execution.
//!
//! The CLI programs against this trait. Local execution holds the engine
//! directly with zero-copy filesystem access; remote execution encodes the
//! same parameters into the wire contract via `cce-transport`. Parameter
//! semantics are identical; only the transport differs.

use async_trait::async_trait;
use cce_api::models::*;

/// Execution mode selecting the service implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ServiceMode {
    /// Direct in-process engine calls, no HTTP or encoding.
    Local,
    /// Remote server via HTTP transport.
    #[default]
    Remote,
}

impl ServiceMode {
    /// Resolve from the process environment.
    ///
    /// `CCE_SERVICE_MODE=local` selects local execution, anything else
    /// selects remote. Explicit CLI flags outrank this value.
    pub fn from_env() -> Self {
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

/// Domain service failure.
#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    /// Transport or protocol failure.
    #[error("transport error: {0}")]
    Transport(String),
    /// Invalid request parameters.
    #[error("invalid request: {0}")]
    Invalid(String),
    /// Requested resource was not found.
    #[error("not found: {0}")]
    NotFound(String),
    /// Underlying engine failure.
    #[error("engine error: {0}")]
    Engine(String),
}

impl From<anyhow::Error> for ServiceError {
    fn from(error: anyhow::Error) -> Self {
        Self::Transport(format!("{error:#}"))
    }
}

/// Domain operations covering project, index, retrieval, graph, and watch.
///
/// Parameters reuse the wire models so local and remote share one semantic
/// definition. Implementations differ only in transport: local calls the
/// engine directly, remote encodes the same models onto HTTP paths.
///
/// Graph parameters mirror the HTTP query surface field for field so the two
/// modes cannot drift. Comma-separated filters stay strings on both sides.
#[derive(Debug, Clone)]
pub struct GraphEgoParams {
    /// Symbol seed: stable id, file-scoped name, or bare name.
    pub entity_id: String,
    /// Traversal depth before project depth capping.
    pub depth: usize,
    /// Direction: in, out, or both.
    pub direction: String,
    /// Pagination offset over nodes.
    pub offset: usize,
    /// Pagination limit over nodes.
    pub limit: usize,
    /// Comma-separated relation domains.
    pub domains: String,
    /// Comma-separated relation types.
    pub relation_types: String,
    /// Whether to keep edges pointing outside the project.
    pub include_external: bool,
}

/// Two-point path parameters.
#[derive(Debug, Clone)]
pub struct GraphPathParams {
    /// Start symbol seed.
    pub start: String,
    /// End symbol seed.
    pub end: String,
    /// Maximum search depth before project depth capping.
    pub max_depth: usize,
    /// Comma-separated relation domains.
    pub domains: String,
    /// Comma-separated relation types.
    pub relation_types: String,
    /// Whether to keep external edges.
    pub include_external: bool,
}

/// Explicit subgraph parameters.
#[derive(Debug, Clone)]
pub struct GraphSubgraphParams {
    /// Symbol seeds in encounter order.
    pub ids: Vec<String>,
    /// Pagination offset over nodes.
    pub offset: usize,
    /// Pagination limit over nodes.
    pub limit: usize,
    /// Comma-separated relation domains.
    pub domains: String,
    /// Comma-separated relation types.
    pub relation_types: String,
    /// Whether to keep external edges.
    pub include_external: bool,
}

/// Connected components parameters.
#[derive(Debug, Clone)]
pub struct GraphComponentsParams {
    /// Pagination offset over components.
    pub offset: usize,
    /// Maximum components to report.
    pub limit: usize,
    /// Comma-separated relation domains.
    pub domains: String,
    /// Comma-separated relation types.
    pub relation_types: String,
    /// Whether to keep external edges.
    pub include_external: bool,
}

/// Full export parameters.
#[derive(Debug, Clone)]
pub struct GraphExportParams {
    /// Maximum nodes to materialize.
    pub limit: usize,
    /// Pagination offset over nodes.
    pub offset: usize,
    /// Comma-separated relation domains.
    pub domains: String,
    /// Comma-separated relation types.
    pub relation_types: String,
    /// Whether to keep external edges.
    pub include_external: bool,
}

/// File impact parameters.
#[derive(Debug, Clone)]
pub struct GraphImpactParams {
    /// Changed file path.
    pub file: String,
}

/// Entity impact parameters.
#[derive(Debug, Clone)]
pub struct GraphEntityImpactParams {
    /// Changed symbol seed.
    pub entity_id: String,
    /// Maximum dependent depth.
    pub max_depth: usize,
    /// Impact scope: entity or file.
    pub scope: String,
}

/// Dependency cycle parameters.
#[derive(Debug, Clone)]
pub struct GraphCyclesParams {
    /// Cycle level: entity or file.
    pub level: String,
    /// Maximum cycles to report.
    pub limit: usize,
}

/// Structural relation parameters.
#[derive(Debug, Clone)]
pub struct GraphStructuralParams {
    /// Symbol seed.
    pub entity_id: String,
    /// Relation family label.
    pub kind: String,
    /// Direction: out or in.
    pub direction: String,
    /// Maximum relations.
    pub limit: usize,
}

/// Module relation parameters.
#[derive(Debug, Clone)]
pub struct GraphModuleParams {
    /// File path.
    pub file: String,
}

#[async_trait]
pub trait CodeContextService: Send + Sync {
    /// List all projects.
    async fn list_projects(&self) -> Result<ProjectListResponse, ServiceError>;
    /// Create a project from a creation request.
    async fn create_project(
        &self,
        request: CreateProjectRequest,
    ) -> Result<ProjectDetailResponse, ServiceError>;
    /// Fetch one project by numeric id.
    async fn get_project(&self, project_id: i64) -> Result<ProjectDetailResponse, ServiceError>;
    /// Delete one project by numeric id.
    async fn delete_project(&self, project_id: i64) -> Result<ProjectDeleteResponse, ServiceError>;
    /// Run a full index over the given request.
    async fn index(&self, request: IndexRequest) -> Result<IndexResponse, ServiceError>;
    /// Run a single search query.
    async fn search(&self, request: SearchRequest) -> Result<SearchResponse, ServiceError>;
    /// Run an aggregated multi-query search.
    async fn search_aggregated(
        &self,
        request: AggregatedSearchRequest,
    ) -> Result<SearchResponse, ServiceError>;
    /// Fetch an ego neighborhood with full filter and pagination parity.
    async fn graph_ego(
        &self,
        project_id: i64,
        params: GraphEgoParams,
    ) -> Result<GraphSubgraphResponse, ServiceError>;
    /// Shortest path between two seeds.
    async fn graph_path(
        &self,
        project_id: i64,
        params: GraphPathParams,
    ) -> Result<GraphPathResponse, ServiceError>;
    /// Induced subgraph over explicit seeds.
    async fn graph_subgraph(
        &self,
        project_id: i64,
        params: GraphSubgraphParams,
    ) -> Result<GraphSubgraphResponse, ServiceError>;
    /// Connected components over internal edges.
    async fn graph_components(
        &self,
        project_id: i64,
        params: GraphComponentsParams,
    ) -> Result<GraphComponentsResponse, ServiceError>;
    /// Full project graph export.
    async fn graph_export(
        &self,
        project_id: i64,
        params: GraphExportParams,
    ) -> Result<GraphSubgraphResponse, ServiceError>;
    /// File change impact.
    async fn graph_impact(
        &self,
        project_id: i64,
        params: GraphImpactParams,
    ) -> Result<GraphImpactResponse, ServiceError>;
    /// Entity change impact.
    async fn graph_entity_impact(
        &self,
        project_id: i64,
        params: GraphEntityImpactParams,
    ) -> Result<GraphEntityImpactResponse, ServiceError>;
    /// Dependency cycles.
    async fn graph_cycles(
        &self,
        project_id: i64,
        params: GraphCyclesParams,
    ) -> Result<GraphCyclesResponse, ServiceError>;
    /// Structural relations of one entity.
    async fn graph_structural(
        &self,
        project_id: i64,
        params: GraphStructuralParams,
    ) -> Result<GraphStructuralResponse, ServiceError>;
    /// Module relations of one file.
    async fn graph_module(
        &self,
        project_id: i64,
        params: GraphModuleParams,
    ) -> Result<GraphModuleResponse, ServiceError>;
    /// Report watch status for a project.
    async fn watch_status(&self, project_id: i64) -> Result<WatchStatusResponse, ServiceError>;
    /// Start watching a project directory.
    async fn watch_start(
        &self,
        project_id: i64,
        request: StartWatchRequest,
    ) -> Result<StartWatchResponse, ServiceError>;
    /// Stop watching a project.
    async fn watch_stop(&self, project_id: i64) -> Result<StopWatchResponse, ServiceError>;
    /// Reload project configuration and return the fresh version.
    async fn reload_project(
        &self,
        project_id: i64,
    ) -> Result<ProjectConfigReloadResponse, ServiceError>;
}
