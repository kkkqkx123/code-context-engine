//! Local domain service implementation over the engine directly.
//!
//! Zero-copy filesystem access: no HTTP, no base64 chunking, no wire budget.
//! Parameter semantics match the remote implementation exactly.

use std::path::PathBuf;
use std::sync::Arc;

use cce_api::models::{
    CreateProjectRequest, GraphComponentsResponse, GraphCycle, GraphCyclesResponse, GraphEdge,
    GraphEntityImpactResponse, GraphImpactResponse, GraphModuleResponse, GraphNode,
    GraphPathResponse, GraphStructuralResponse, GraphSubgraphResponse, IndexRequest, IndexResponse,
    ModuleRelation, ProjectConfig, ProjectConfigReloadResponse, ProjectDeleteResponse,
    ProjectDetailResponse, ProjectListResponse, SearchRequest, SearchResponse, StartWatchRequest,
    StartWatchResponse, StopWatchResponse, StructuralRelation, WatchStatusResponse,
};
use cce_api::service_params::{
    GraphComponentsParams, GraphCyclesParams, GraphEgoParams, GraphEntityImpactParams,
    GraphExportParams, GraphImpactParams, GraphModuleParams, GraphPathParams,
    GraphStructuralParams, GraphSubgraphParams,
};

use crate::context::SharedContext;
use crate::engine::CodeContextEngine;
use crate::services::search::{
    AggregatedQuerySpec, SearchService, SharedQueryFields, SingleQuerySpec, SubQuerySpec,
};
use cce_codegraph::index::snapshot_query::SnapshotEntityQueryOps;

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

impl From<crate::services::project::ProjectCreateError> for ServiceError {
    fn from(e: crate::services::project::ProjectCreateError) -> Self {
        use crate::services::project::ProjectCreateError as E;
        match e {
            E::Invalid(m) => ServiceError::Invalid(m),
            E::MetadataUnavailable => {
                ServiceError::Engine("metadata store not initialized".to_string())
            }
            E::PathExists => {
                ServiceError::Invalid("project already exists at this path".to_string())
            }
            E::Conflict(m) => ServiceError::Engine(m),
            E::Storage(m) => ServiceError::Engine(m),
        }
    }
}

impl From<crate::services::watch::WatchStatusError> for ServiceError {
    fn from(e: crate::services::watch::WatchStatusError) -> Self {
        use crate::services::watch::WatchStatusError as E;
        match e {
            E::InvalidProjectId => ServiceError::Invalid("project_id must be positive".to_string()),
        }
    }
}

impl From<crate::services::watch::WatchStartError> for ServiceError {
    fn from(e: crate::services::watch::WatchStartError) -> Self {
        use crate::services::watch::WatchStartError as E;
        match e {
            E::InvalidProjectId => ServiceError::Invalid("project_id must be positive".to_string()),
            E::GatewaySupplied => ServiceError::Invalid(
                "project is gateway-supplied; local watching is disabled".to_string(),
            ),
            E::ProjectLoad(m) => ServiceError::Engine(m),
            E::RootNotAccessible(m) => {
                ServiceError::Invalid(format!("project root is not accessible: {m}"))
            }
            E::WatchNotAccessible(m) => {
                ServiceError::Invalid(format!("watch path is not accessible: {m}"))
            }
            E::NotWithinRoot { .. } => {
                ServiceError::Invalid("watch path is not within project root".to_string())
            }
            E::PathNotFound(m) => {
                ServiceError::Invalid(format!("watch path is not accessible: {m}"))
            }
            E::HotCoordinator(m) => ServiceError::Engine(m),
            E::StartWatch(m) => ServiceError::Engine(m),
            E::EventLoop(m) => ServiceError::Engine(m),
        }
    }
}

impl From<crate::services::watch::WatchStopError> for ServiceError {
    fn from(e: crate::services::watch::WatchStopError) -> Self {
        use crate::services::watch::WatchStopError as E;
        match e {
            E::InvalidProjectId => ServiceError::Invalid("project_id must be positive".to_string()),
            E::HotCoordinator(m) => ServiceError::Engine(m),
            E::StopWatch(m) => ServiceError::Engine(m),
        }
    }
}

/// Local service holding shared application state.
pub struct LocalService {
    state: SharedContext,
}

impl LocalService {
    /// Build from an engine instance.
    pub async fn from_engine(engine: &CodeContextEngine) -> Self {
        Self {
            state: SharedContext::from_engine(engine, None).await,
        }
    }

    /// Build from existing shared state.
    pub fn from_state(state: SharedContext) -> Self {
        Self { state }
    }

    fn engine_error(error: impl std::fmt::Display) -> ServiceError {
        ServiceError::Engine(error.to_string())
    }

    fn record_to_config(record: &cce_storage_metadb_sqlite::ProjectRecord) -> ProjectConfig {
        let profile = cce_config::ProjectAppConfig::from_json_str(&record.profile_json).ok();
        let (extensions, exclude_dirs, respect_gitignore, ignore_patterns) = match profile {
            Some(profile) => (
                profile
                    .orchestrator
                    .as_ref()
                    .and_then(|o| o.indexer.as_ref())
                    .and_then(|i| i.extensions.clone())
                    .unwrap_or_default(),
                profile
                    .orchestrator
                    .as_ref()
                    .and_then(|o| o.indexer.as_ref())
                    .and_then(|i| i.exclude_dirs.clone())
                    .unwrap_or_default(),
                profile
                    .scanner
                    .as_ref()
                    .and_then(|s| s.respect_gitignore)
                    .unwrap_or(true),
                profile
                    .scanner
                    .as_ref()
                    .and_then(|s| s.gitignore_patterns.clone())
                    .unwrap_or_default(),
            ),
            None => (Vec::new(), Vec::new(), true, Vec::new()),
        };
        ProjectConfig {
            id: record.id.to_string(),
            name: record.name.clone(),
            root_path: record.root_path.clone(),
            extensions,
            exclude_dirs,
            respect_gitignore,
            ignore_patterns,
            created_at: record.created_at.to_string(),
            last_indexed: record.last_indexed.clone(),
        }
    }

    pub async fn resolve_project_id(
        &self,
        project_id: Option<i64>,
        project_path: Option<&str>,
    ) -> Result<i64, ServiceError> {
        match (project_id, project_path) {
            (Some(id), None) => {
                if id <= 0 {
                    return Err(ServiceError::Invalid(
                        "project_id must be positive".to_string(),
                    ));
                }
                Ok(id)
            }
            (None, Some(path)) => {
                if path.is_empty() {
                    return Err(ServiceError::Invalid(
                        "project path cannot be empty".to_string(),
                    ));
                }
                let entry = self
                    .state
                    .engine
                    .project_registry()
                    .find_by_path(std::path::Path::new(path))
                    .await
                    .map_err(|e| {
                        ServiceError::NotFound(format!("project path lookup failed: {e}"))
                    })?;
                Ok(entry.metadata.id)
            }
            (Some(_), Some(_)) => Err(ServiceError::Invalid(
                "specify either project_id or project_path, not both".to_string(),
            )),
            (None, None) => Err(ServiceError::Invalid(
                "project_id or project_path is required".to_string(),
            )),
        }
    }

    pub async fn search_single(
        &self,
        project_id: i64,
        request: SearchRequest,
    ) -> Result<SearchResponse, ServiceError> {
        const MAX_LIMIT: usize = 100;
        if request.limit == 0 || request.limit > MAX_LIMIT {
            return Err(ServiceError::Invalid(format!(
                "limit must be within 1-{MAX_LIMIT}"
            )));
        }
        if request.query.trim().is_empty() {
            return Err(ServiceError::Invalid(
                "query text cannot be empty".to_string(),
            ));
        }
        for pattern in request
            .exclude_patterns
            .iter()
            .chain(request.include_patterns.iter())
        {
            validate_glob(pattern)?;
        }
        let service = SearchService::new(&self.state);
        let sources = SearchService::parse_search_sources(&request.query_type, "query type")
            .map_err(|e| ServiceError::Invalid(e.to_string()))?;
        let mut exclude_content_types = Vec::with_capacity(request.exclude_content_types.len());
        for raw in &request.exclude_content_types {
            exclude_content_types.push(
                SearchService::parse_exclude_content_type(raw)
                    .map_err(|e| ServiceError::Invalid(e.to_string()))?,
            );
        }
        let mut include_categories = Vec::with_capacity(request.include_categories.len());
        for raw in &request.include_categories {
            include_categories.push(
                SearchService::parse_file_category(raw, "include")
                    .map_err(|e| ServiceError::Invalid(e.to_string()))?,
            );
        }
        let mut exclude_categories = Vec::with_capacity(request.exclude_categories.len());
        for raw in &request.exclude_categories {
            exclude_categories.push(
                SearchService::parse_file_category(raw, "exclude")
                    .map_err(|e| ServiceError::Invalid(e.to_string()))?,
            );
        }
        let query_intent = SearchService::parse_query_intent(request.query_intent.as_deref())
            .map_err(|e| ServiceError::Invalid(e.to_string()))?;
        let start = std::time::Instant::now();
        let spec = SingleQuerySpec {
            query: request.query,
            sources,
            fields: SharedQueryFields {
                limit: Some(request.limit),
                min_score: request.min_score,
                enable_rerank: request.enable_rerank,
                rerank_max_candidates: request.rerank_max_candidates,
                directory_prefix: request.directory_prefix,
                exclude_content_types,
                include_categories,
                exclude_categories,
                exclude_patterns: request.exclude_patterns,
                include_patterns: request.include_patterns,
                query_intent,
                expand: request.expand.unwrap_or(false),
            },
        };
        let outcome = service
            .run_single(project_id, spec)
            .await
            .map_err(Self::engine_error)?;
        Ok(SearchResponse {
            success: true,
            total: outcome.total,
            items: outcome.items,
            elapsed_ms: start.elapsed().as_millis() as u64,
            sources_used: outcome.sources,
            failed_sub_queries: outcome.failed_sub_queries,
            relation_epoch: outcome.relation_epoch,
            relation_stale: outcome.relation_stale,
        })
    }
}

fn validate_glob(pattern: &str) -> Result<(), ServiceError> {
    if pattern.contains('\0') {
        return Err(ServiceError::Invalid(
            "invalid glob pattern: contains null byte".to_string(),
        ));
    }
    if pattern.len() > 1000 {
        return Err(ServiceError::Invalid(
            "invalid glob pattern: too long".to_string(),
        ));
    }
    if pattern.is_empty() {
        return Err(ServiceError::Invalid(
            "invalid glob pattern: empty".to_string(),
        ));
    }
    if pattern.starts_with('/') {
        return Err(ServiceError::Invalid(
            "invalid glob pattern: must be relative".to_string(),
        ));
    }
    if pattern.contains("***") {
        return Err(ServiceError::Invalid(
            "invalid glob pattern: '***'".to_string(),
        ));
    }
    Ok(())
}

impl LocalService {
    pub async fn graph_context(
        &self,
        project_id: i64,
    ) -> Result<
        (
            std::sync::Arc<crate::runtime::PublishedSnapshot>,
            std::sync::Arc<cce_orchestrator::query::RelationSearcher>,
            usize,
            std::sync::Arc<crate::runtime::RelationRuntime>,
        ),
        ServiceError,
    > {
        let runtime = self
            .state
            .engine
            .get_relation_runtime(project_id)
            .await
            .map_err(Self::engine_error)?;
        if !runtime.can_serve_queries().await {
            return Err(ServiceError::Engine(
                "relation index not available".to_string(),
            ));
        }
        let snapshot = runtime
            .get_snapshot()
            .await
            .ok_or_else(|| ServiceError::Engine("no relation snapshot available".to_string()))?;
        let searcher = self
            .state
            .get_relation_searcher(project_id)
            .await
            .map_err(Self::engine_error)?;
        let entry = self
            .state
            .engine
            .project_registry()
            .get_or_load(project_id)
            .await
            .map_err(Self::engine_error)?;
        let max_depth = entry.config.relation.max_call_depth;
        Ok((snapshot, searcher, max_depth, runtime))
    }
}

fn parse_graph_direction(
    raw: &str,
) -> Result<cce_orchestrator::query::GraphDirection, ServiceError> {
    match raw.to_lowercase().as_str() {
        "in" | "backward" | "up" => Ok(cce_orchestrator::query::GraphDirection::Backward),
        "out" | "forward" | "down" => Ok(cce_orchestrator::query::GraphDirection::Forward),
        "both" | "bidirectional" => Ok(cce_orchestrator::query::GraphDirection::Both),
        _ => Err(ServiceError::Invalid(
            "direction must be one of in, out, or both".to_string(),
        )),
    }
}

fn parse_comma_list(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| part.to_lowercase())
        .collect()
}

fn parse_graph_filter(
    domains: &str,
    relation_types: &str,
    include_external: bool,
) -> cce_orchestrator::query::GraphFilter {
    cce_orchestrator::query::GraphFilter {
        relation_domains: parse_comma_list(domains),
        relation_types: parse_comma_list(relation_types),
        include_external,
    }
}

fn parse_graph_pagination(offset: usize, limit: usize) -> cce_orchestrator::query::GraphPagination {
    cce_orchestrator::query::GraphPagination { offset, limit }
}

fn convert_graph(
    graph: &cce_orchestrator::query::SubGraph,
) -> Result<(Vec<GraphNode>, Vec<GraphEdge>), ServiceError> {
    let nodes = graph
        .nodes
        .iter()
        .map(|node| GraphNode {
            id: node.id.clone(),
            label: node.label.clone(),
            kind: node.kind.clone(),
            source_file: node.source_file.clone(),
            source_location: node.source_location.clone(),
            scoped_name: node.scoped_name.clone(),
            signature: node.signature.clone(),
        })
        .collect::<Vec<_>>();
    let edges = graph
        .edges
        .iter()
        .map(|edge| GraphEdge {
            source: edge.source.clone(),
            target: edge.target.clone(),
            relation: edge.relation.clone(),
            domain: edge.domain.clone(),
            confidence: edge.confidence.to_string(),
            call_context: edge.call_context.clone(),
            is_external: edge.is_external,
            weight: edge.weight,
            cfg_condition: edge.cfg_condition.clone(),
            callsite: edge.callsite.clone(),
            edge_id: edge.edge_id.clone(),
        })
        .collect::<Vec<_>>();
    for node in &nodes {
        node.validate()
            .map_err(|message| ServiceError::Engine(format!("invalid graph node: {message}")))?;
    }
    for edge in &edges {
        edge.validate()
            .map_err(|message| ServiceError::Engine(format!("invalid graph edge: {message}")))?;
    }
    Ok((nodes, edges))
}

fn convert_paged_graph(
    graph: &cce_orchestrator::query::PagedSubGraph,
) -> Result<(Vec<GraphNode>, Vec<GraphEdge>), ServiceError> {
    let staged = cce_orchestrator::query::SubGraph {
        nodes: graph.nodes.clone(),
        edges: graph.edges.clone(),
    };
    convert_graph(&staged)
}

fn stable_id_of(
    index: &Arc<cce_codegraph::index::LayeredSnapshotIndex>,
    entity: cce_types::EntityId,
) -> String {
    use cce_codegraph::index::snapshot_query::SnapshotSymbolQueryOps;
    index
        .get_symbol_key_by_entity_id(entity)
        .map(|key| key.stable_id().0)
        .unwrap_or_else(|| format!("entity:{}", entity.0))
}

fn resolve_graph_seed(
    index: &Arc<cce_codegraph::index::LayeredSnapshotIndex>,
    seed: &str,
) -> Result<cce_types::EntityId, ServiceError> {
    use cce_codegraph::index::snapshot_query::{SnapshotEntityQueryOps, SnapshotSymbolQueryOps};
    let trimmed = seed.trim();
    if trimmed.is_empty() {
        return Err(ServiceError::Invalid(
            "symbol seed must not be empty".to_string(),
        ));
    }
    if let Some((file, name)) = trimmed.split_once('#') {
        let file = file.trim();
        let name = name.trim();
        if name.is_empty() {
            return Err(ServiceError::NotFound(format!("symbol not found: {seed}")));
        }
        let mut ids = index.get_function_ids_by_name(name);
        if !file.is_empty() {
            let normalized = cce_types::normalize_project_path(file);
            ids.retain(|id| {
                index
                    .get_file_path_by_entity(*id)
                    .is_some_and(|path| cce_types::normalize_project_path(&path) == normalized)
            });
        }
        ids.sort();
        ids.dedup();
        return match ids.len() {
            1 => Ok(ids[0]),
            0 => Err(ServiceError::NotFound(format!("symbol not found: {seed}"))),
            _ => Err(ServiceError::Invalid(format!("ambiguous symbol: {seed}"))),
        };
    }
    if trimmed.starts_with("sym_") {
        if let Some(id) = index.get_entity_id_by_stable_symbol_id(trimmed) {
            return Ok(id);
        }
        return Err(ServiceError::NotFound(format!("symbol not found: {seed}")));
    }
    let ids = index.get_function_ids_by_name(trimmed);
    match ids.len() {
        1 => Ok(ids[0]),
        0 => Err(ServiceError::NotFound(format!("symbol not found: {seed}"))),
        _ => Err(ServiceError::Invalid(format!("ambiguous symbol: {seed}"))),
    }
}

async fn stale_info(runtime: &Arc<crate::runtime::RelationRuntime>) -> Option<serde_json::Value> {
    let info = runtime.get_capability_info().await;
    info.stale.then(|| info.to_json_map())
}

fn index_response_from_result(result: cce_orchestrator::IndexResult) -> IndexResponse {
    let has_errors = !result.errors().is_empty();
    IndexResponse {
        success: result.is_success(),
        files_scanned: result.total_files,
        files_indexed: result.indexed_files,
        failed_files: result.failed_files,
        degraded_files: result.degraded_files,
        skipped_permanent: result.skipped_permanent,
        circuit_open: result.circuit_open,
        total_entities: result.total_entities,
        total_relations: result.total_relations,
        total_vectors: result.total_vectors,
        elapsed_ms: result.elapsed_ms,
        message: if !has_errors {
            format!(
                "Indexing completed, a total of {} files were processed, {} entities were extracted, and {} relationships were identified.",
                result.indexed_files, result.total_entities, result.total_relations
            )
        } else {
            format!(
                "The indexing process is complete. {} files were successful, and {} files failed.",
                result.indexed_files, result.failed_files
            )
        },
        errors: result.errors().to_vec(),
    }
}

impl LocalService {
    pub async fn list_projects(&self) -> Result<ProjectListResponse, ServiceError> {
        let store =
            self.state.engine.metadata_store().ok_or_else(|| {
                ServiceError::Engine("metadata store not initialized".to_string())
            })?;
        let records = store
            .as_ref()
            .with_transaction(|tx| cce_storage_metadb_sqlite::ProjectRepository::get_all(tx))
            .map_err(Self::engine_error)?;
        let projects: Vec<_> = records.iter().map(Self::record_to_config).collect();
        Ok(ProjectListResponse {
            success: true,
            total: projects.len(),
            projects,
        })
    }

    pub async fn create_project(
        &self,
        request: CreateProjectRequest,
    ) -> Result<ProjectDetailResponse, ServiceError> {
        let store =
            self.state.engine.metadata_store().ok_or_else(|| {
                ServiceError::Engine("metadata store not initialized".to_string())
            })?;
        let created = crate::services::project::create_project(store, &request)
            .map_err(ServiceError::from)?;
        Ok(ProjectDetailResponse {
            success: true,
            project: ProjectConfig {
                id: created.project_id.to_string(),
                name: created.name,
                root_path: created.root_path,
                extensions: created.extensions,
                exclude_dirs: created.exclude_dirs,
                respect_gitignore: created.respect_gitignore,
                ignore_patterns: created.ignore_patterns,
                created_at: chrono::Utc::now().to_rfc3339(),
                last_indexed: None,
            },
        })
    }

    pub async fn get_project(
        &self,
        project_id: i64,
    ) -> Result<ProjectDetailResponse, ServiceError> {
        if project_id <= 0 {
            return Err(ServiceError::Invalid(
                "project_id must be positive".to_string(),
            ));
        }
        let store =
            self.state.engine.metadata_store().ok_or_else(|| {
                ServiceError::Engine("metadata store not initialized".to_string())
            })?;
        let record = store
            .as_ref()
            .with_transaction(|tx| {
                cce_storage_metadb_sqlite::ProjectRepository::get_by_id(tx, project_id)
            })
            .map_err(Self::engine_error)?
            .ok_or_else(|| ServiceError::NotFound("project does not exist".to_string()))?;
        Ok(ProjectDetailResponse {
            success: true,
            project: Self::record_to_config(&record),
        })
    }

    pub async fn delete_project(
        &self,
        project_id: i64,
    ) -> Result<ProjectDeleteResponse, ServiceError> {
        if project_id <= 0 {
            return Err(ServiceError::Invalid(
                "project_id must be positive".to_string(),
            ));
        }
        let store =
            self.state.engine.metadata_store().ok_or_else(|| {
                ServiceError::Engine("metadata store not initialized".to_string())
            })?;
        let existed = store
            .as_ref()
            .with_transaction(|tx| {
                cce_storage_metadb_sqlite::ProjectRepository::get_by_id(tx, project_id)
            })
            .map_err(Self::engine_error)?
            .is_some();
        if !existed {
            return Err(ServiceError::NotFound("project does not exist".to_string()));
        }
        self.maintenance_delete(project_id).await?;
        Ok(ProjectDeleteResponse {
            success: true,
            message: format!("project {project_id} deleted"),
            project_id,
        })
    }

    pub async fn index(&self, request: IndexRequest) -> Result<IndexResponse, ServiceError> {
        if request.project_id <= 0 {
            return Err(ServiceError::Invalid(
                "project_id must be positive".to_string(),
            ));
        }
        let root_dir = PathBuf::from(&request.path);
        if !root_dir.exists() {
            return Err(ServiceError::Invalid(format!(
                "directory does not exist: {}",
                request.path
            )));
        }
        if !root_dir.is_dir() {
            return Err(ServiceError::Invalid(format!(
                "path is not a directory: {}",
                request.path
            )));
        }
        let options = crate::services::index::build_index_options(&request);
        let result = self
            .state
            .engine
            .index(request.project_id, options)
            .await
            .map_err(Self::engine_error)?;
        Ok(index_response_from_result(result))
    }

    pub async fn search(&self, request: SearchRequest) -> Result<SearchResponse, ServiceError> {
        let project_id = self
            .resolve_project_id(request.project_id, request.project_path.as_deref())
            .await?;
        let request = SearchRequest {
            project_id: Some(project_id),
            project_path: None,
            ..request
        };
        self.search_single(project_id, request).await
    }

    pub async fn search_aggregated(
        &self,
        request: cce_api::models::AggregatedSearchRequest,
    ) -> Result<SearchResponse, ServiceError> {
        const MAX_LIMIT: usize = 100;
        const MAX_SUB_QUERIES: usize = 10;
        if request.limit == 0 || request.limit > MAX_LIMIT {
            return Err(ServiceError::Invalid(format!(
                "limit must be within 1-{MAX_LIMIT}"
            )));
        }
        if request.sub_queries.is_empty() || request.sub_queries.len() > MAX_SUB_QUERIES {
            return Err(ServiceError::Invalid(format!(
                "sub_queries must list 1-{MAX_SUB_QUERIES} entries"
            )));
        }
        for pattern in request
            .exclude_patterns
            .iter()
            .chain(request.include_patterns.iter())
        {
            validate_glob(pattern)?;
        }
        let project_id = self
            .resolve_project_id(request.project_id, request.project_path.as_deref())
            .await?;
        let service = SearchService::new(&self.state);
        let mut sub_queries = Vec::with_capacity(request.sub_queries.len());
        for sq in &request.sub_queries {
            let sources =
                SearchService::parse_search_sources(&sq.query_type, "query type in sub-query")
                    .map_err(|e| ServiceError::Invalid(e.to_string()))?;
            if !sq.weight.is_finite() || sq.weight < 0.0 {
                return Err(ServiceError::Invalid(format!(
                    "invalid weight in sub-query '{}'",
                    sq.text
                )));
            }
            sub_queries.push(SubQuerySpec {
                text: sq.text.clone(),
                sources,
                weight: sq.weight,
            });
        }
        let mut exclude_content_types = Vec::with_capacity(request.exclude_content_types.len());
        for raw in &request.exclude_content_types {
            exclude_content_types.push(
                SearchService::parse_exclude_content_type(raw)
                    .map_err(|e| ServiceError::Invalid(e.to_string()))?,
            );
        }
        let mut include_categories = Vec::with_capacity(request.include_categories.len());
        for raw in &request.include_categories {
            include_categories.push(
                SearchService::parse_file_category(raw, "include")
                    .map_err(|e| ServiceError::Invalid(e.to_string()))?,
            );
        }
        let mut exclude_categories = Vec::with_capacity(request.exclude_categories.len());
        for raw in &request.exclude_categories {
            exclude_categories.push(
                SearchService::parse_file_category(raw, "exclude")
                    .map_err(|e| ServiceError::Invalid(e.to_string()))?,
            );
        }
        let query_intent = SearchService::parse_query_intent(request.query_intent.as_deref())
            .map_err(|e| ServiceError::Invalid(e.to_string()))?;
        let start = std::time::Instant::now();
        let spec = AggregatedQuerySpec {
            sub_queries,
            fields: SharedQueryFields {
                limit: Some(request.limit),
                min_score: request.min_score,
                enable_rerank: request.enable_rerank,
                rerank_max_candidates: request.rerank_max_candidates,
                directory_prefix: request.directory_prefix,
                exclude_content_types,
                include_categories,
                exclude_categories,
                exclude_patterns: request.exclude_patterns,
                include_patterns: request.include_patterns,
                query_intent,
                expand: request.expand.unwrap_or(false),
            },
        };
        let outcome = service
            .run_aggregated(project_id, spec)
            .await
            .map_err(Self::engine_error)?;
        Ok(SearchResponse {
            success: true,
            total: outcome.total,
            items: outcome.items,
            elapsed_ms: start.elapsed().as_millis() as u64,
            sources_used: outcome.sources,
            failed_sub_queries: outcome.failed_sub_queries,
            relation_epoch: outcome.relation_epoch,
            relation_stale: outcome.relation_stale,
        })
    }

    pub async fn graph_ego(
        &self,
        project_id: i64,
        params: GraphEgoParams,
    ) -> Result<GraphSubgraphResponse, ServiceError> {
        if project_id <= 0 {
            return Err(ServiceError::Invalid(
                "project_id must be positive".to_string(),
            ));
        }
        let (snapshot, searcher, max_depth, runtime) = self.graph_context(project_id).await?;
        let direction = parse_graph_direction(&params.direction)?;
        let entity = resolve_graph_seed(&snapshot.index, &params.entity_id)?;
        let service = cce_orchestrator::query::GraphService::new(searcher);
        let filter = parse_graph_filter(
            &params.domains,
            &params.relation_types,
            params.include_external,
        );
        let pagination = parse_graph_pagination(params.offset, params.limit.max(1));
        let graph = service
            .ego_graph_with_options(
                entity,
                params.depth.min(max_depth),
                direction,
                &filter,
                pagination,
            )
            .map_err(Self::engine_error)?;
        let (nodes, edges) = convert_paged_graph(&graph)?;
        let truncated = graph.total_nodes > nodes.len() || graph.total_edges > edges.len();
        Ok(GraphSubgraphResponse {
            success: true,
            relation_epoch: snapshot.relation_epoch,
            total_nodes: graph.total_nodes,
            total_edges: graph.total_edges,
            truncated,
            nodes,
            edges,
            relation_info: stale_info(&runtime).await,
        })
    }

    pub async fn graph_path(
        &self,
        project_id: i64,
        params: GraphPathParams,
    ) -> Result<GraphPathResponse, ServiceError> {
        if project_id <= 0 {
            return Err(ServiceError::Invalid(
                "project_id must be positive".to_string(),
            ));
        }
        let (snapshot, searcher, max_depth, runtime) = self.graph_context(project_id).await?;
        let start = resolve_graph_seed(&snapshot.index, &params.start)?;
        let end = resolve_graph_seed(&snapshot.index, &params.end)?;
        let service = cce_orchestrator::query::GraphService::new(searcher);
        let filter = parse_graph_filter(
            &params.domains,
            &params.relation_types,
            params.include_external,
        );
        let path = service
            .shortest_path_with_options(start, end, params.max_depth.min(max_depth), &filter)
            .map_err(Self::engine_error)?;
        let (nodes, edges) = match path.as_ref() {
            Some(graph) => convert_graph(graph)?,
            None => Default::default(),
        };
        Ok(GraphPathResponse {
            success: true,
            relation_epoch: snapshot.relation_epoch,
            path_found: path.is_some(),
            nodes,
            edges,
            relation_info: stale_info(&runtime).await,
        })
    }

    pub async fn graph_subgraph(
        &self,
        project_id: i64,
        params: GraphSubgraphParams,
    ) -> Result<GraphSubgraphResponse, ServiceError> {
        const MAX_IDS: usize = 200;
        if project_id <= 0 {
            return Err(ServiceError::Invalid(
                "project_id must be positive".to_string(),
            ));
        }
        if params.ids.is_empty() || params.ids.len() > MAX_IDS {
            return Err(ServiceError::Invalid(format!(
                "ids must list 1-{MAX_IDS} stable symbol IDs"
            )));
        }
        let (snapshot, searcher, _, runtime) = self.graph_context(project_id).await?;
        let mut entity_ids = Vec::with_capacity(params.ids.len());
        for raw in &params.ids {
            entity_ids.push(resolve_graph_seed(&snapshot.index, raw)?);
        }
        let service = cce_orchestrator::query::GraphService::new(searcher);
        let filter = parse_graph_filter(
            &params.domains,
            &params.relation_types,
            params.include_external,
        );
        let pagination = parse_graph_pagination(params.offset, params.limit.max(1));
        let graph = service
            .subgraph_with_options(&entity_ids, &filter, pagination)
            .map_err(Self::engine_error)?;
        let (nodes, edges) = convert_paged_graph(&graph)?;
        let truncated = graph.total_nodes > nodes.len() || graph.total_edges > edges.len();
        Ok(GraphSubgraphResponse {
            success: true,
            relation_epoch: snapshot.relation_epoch,
            total_nodes: graph.total_nodes,
            total_edges: graph.total_edges,
            truncated,
            nodes,
            edges,
            relation_info: stale_info(&runtime).await,
        })
    }

    pub async fn graph_components(
        &self,
        project_id: i64,
        params: GraphComponentsParams,
    ) -> Result<GraphComponentsResponse, ServiceError> {
        const MAX_COMPONENTS: usize = 5_000;
        if project_id <= 0 {
            return Err(ServiceError::Invalid(
                "project_id must be positive".to_string(),
            ));
        }
        if params.limit == 0 || params.limit > MAX_COMPONENTS {
            return Err(ServiceError::Invalid(format!(
                "limit must be within 1-{MAX_COMPONENTS}"
            )));
        }
        let (snapshot, searcher, _, runtime) = self.graph_context(project_id).await?;
        let service = cce_orchestrator::query::GraphService::new(searcher);
        let filter = parse_graph_filter(
            &params.domains,
            &params.relation_types,
            params.include_external,
        );
        let pagination = parse_graph_pagination(params.offset, params.limit);
        let paged = service
            .connected_components_with_options(&filter, pagination)
            .map_err(Self::engine_error)?;
        let groups = paged
            .components
            .into_iter()
            .map(|group| {
                group
                    .into_iter()
                    .map(|id| stable_id_of(&snapshot.index, id))
                    .collect()
            })
            .collect();
        Ok(GraphComponentsResponse {
            success: true,
            relation_epoch: snapshot.relation_epoch,
            components: groups,
            total_components: paged.total_components,
            relation_info: stale_info(&runtime).await,
        })
    }

    pub async fn graph_export(
        &self,
        project_id: i64,
        params: GraphExportParams,
    ) -> Result<GraphSubgraphResponse, ServiceError> {
        const MAX_EXPORT_NODES: usize = 10_000;
        if project_id <= 0 {
            return Err(ServiceError::Invalid(
                "project_id must be positive".to_string(),
            ));
        }
        if params.limit == 0 || params.limit > MAX_EXPORT_NODES {
            return Err(ServiceError::Invalid(format!(
                "limit must be within 1-{MAX_EXPORT_NODES}"
            )));
        }
        let (snapshot, searcher, _, runtime) = self.graph_context(project_id).await?;
        let service = cce_orchestrator::query::GraphService::new(searcher);
        let filter = parse_graph_filter(
            &params.domains,
            &params.relation_types,
            params.include_external,
        );
        let pagination = parse_graph_pagination(params.offset, params.limit.max(1));
        let graph = service
            .export_full_with_options(params.limit, &filter, pagination)
            .map_err(Self::engine_error)?;
        let (nodes, edges) = convert_paged_graph(&graph)?;
        let truncated = graph.total_nodes > nodes.len() || graph.total_edges > edges.len();
        Ok(GraphSubgraphResponse {
            success: true,
            relation_epoch: snapshot.relation_epoch,
            total_nodes: graph.total_nodes,
            total_edges: graph.total_edges,
            truncated,
            nodes,
            edges,
            relation_info: stale_info(&runtime).await,
        })
    }

    pub async fn graph_impact(
        &self,
        project_id: i64,
        params: GraphImpactParams,
    ) -> Result<GraphImpactResponse, ServiceError> {
        if project_id <= 0 {
            return Err(ServiceError::Invalid(
                "project_id must be positive".to_string(),
            ));
        }
        if params.file.trim().is_empty() {
            return Err(ServiceError::Invalid("file must not be empty".to_string()));
        }
        let (snapshot, searcher, _, runtime) = self.graph_context(project_id).await?;
        let impact = searcher.get_change_impact(&params.file);
        Ok(GraphImpactResponse {
            success: true,
            relation_epoch: snapshot.relation_epoch,
            changed_file: impact.changed,
            direct_dependents: impact.direct_dependents,
            indirect_dependents: impact.indirect_dependents,
            impact_score: impact.impact_score,
            relation_info: stale_info(&runtime).await,
        })
    }

    pub async fn graph_entity_impact(
        &self,
        project_id: i64,
        params: GraphEntityImpactParams,
    ) -> Result<GraphEntityImpactResponse, ServiceError> {
        const MAX_IMPACT_DEPTH: usize = 50;
        if project_id <= 0 {
            return Err(ServiceError::Invalid(
                "project_id must be positive".to_string(),
            ));
        }
        if params.max_depth == 0 || params.max_depth > MAX_IMPACT_DEPTH {
            return Err(ServiceError::Invalid(format!(
                "max_depth must be within 1-{MAX_IMPACT_DEPTH}"
            )));
        }
        let scope = params.scope.to_lowercase();
        if scope != "entity" && scope != "file" {
            return Err(ServiceError::Invalid(
                "scope must be one of entity, file".to_string(),
            ));
        }
        let (snapshot, searcher, _, runtime) = self.graph_context(project_id).await?;
        let entity = resolve_graph_seed(&snapshot.index, &params.entity_id)?;
        let impact = searcher.get_entity_impact(entity, params.max_depth, &scope);
        Ok(GraphEntityImpactResponse {
            success: true,
            relation_epoch: snapshot.relation_epoch,
            changed_entity: stable_id_of(&snapshot.index, entity),
            direct_dependents: impact
                .direct_dependents
                .iter()
                .map(|s| s.as_str().to_string())
                .collect(),
            indirect_dependents: impact
                .indirect_dependents
                .iter()
                .map(|s| s.as_str().to_string())
                .collect(),
            impact_score: impact.impact_score,
            relation_info: stale_info(&runtime).await,
        })
    }

    pub async fn graph_cycles(
        &self,
        project_id: i64,
        params: GraphCyclesParams,
    ) -> Result<GraphCyclesResponse, ServiceError> {
        const MAX_CYCLES: usize = 5_000;
        if project_id <= 0 {
            return Err(ServiceError::Invalid(
                "project_id must be positive".to_string(),
            ));
        }
        let level = params.level.to_lowercase();
        if level != "entity" && level != "file" {
            return Err(ServiceError::Invalid(
                "level must be one of entity, file".to_string(),
            ));
        }
        if params.limit == 0 || params.limit > MAX_CYCLES {
            return Err(ServiceError::Invalid(format!(
                "limit must be within 1-{MAX_CYCLES}"
            )));
        }
        let (snapshot, searcher, _, runtime) = self.graph_context(project_id).await?;
        let probe = params.limit + 1;
        let cycles = if level == "entity" {
            searcher
                .find_entity_cycles(probe)
                .into_iter()
                .map(|members| GraphCycle {
                    members: members
                        .into_iter()
                        .map(|id| stable_id_of(&snapshot.index, id))
                        .collect(),
                })
                .collect::<Vec<_>>()
        } else {
            searcher
                .find_file_cycles(probe)
                .into_iter()
                .map(|members| GraphCycle { members })
                .collect::<Vec<_>>()
        };
        let truncated = cycles.len() > params.limit;
        let total_cycles = if truncated {
            params.limit + 1
        } else {
            cycles.len()
        };
        let cycles = cycles.into_iter().take(params.limit).collect();
        Ok(GraphCyclesResponse {
            success: true,
            relation_epoch: snapshot.relation_epoch,
            level,
            cycles,
            total_cycles,
            truncated,
            relation_info: stale_info(&runtime).await,
        })
    }

    pub async fn graph_structural(
        &self,
        project_id: i64,
        params: GraphStructuralParams,
    ) -> Result<GraphStructuralResponse, ServiceError> {
        use cce_orchestrator::query::{StructuralDirection, StructuralKind};
        const MAX_STRUCTURAL: usize = 2_000;
        if project_id <= 0 {
            return Err(ServiceError::Invalid(
                "project_id must be positive".to_string(),
            ));
        }
        let kind = StructuralKind::parse(&params.kind).ok_or_else(|| {
            ServiceError::Invalid(format!(
                "unknown kind '{}'; expected one of: {}",
                params.kind,
                StructuralKind::labels().join(", ")
            ))
        })?;
        let direction = match params.direction.to_lowercase().as_str() {
            "out" | "outgoing" | "forward" => StructuralDirection::Outgoing,
            "in" | "incoming" | "backward" => StructuralDirection::Incoming,
            other => {
                return Err(ServiceError::Invalid(format!(
                    "direction must be out or in, got '{other}'"
                )));
            }
        };
        if params.limit == 0 || params.limit > MAX_STRUCTURAL {
            return Err(ServiceError::Invalid(format!(
                "limit must be within 1-{MAX_STRUCTURAL}"
            )));
        }
        let (snapshot, searcher, _, runtime) = self.graph_context(project_id).await?;
        let entity = resolve_graph_seed(&snapshot.index, &params.entity_id)?;
        let resolved = searcher.structural_relations(entity, kind, direction);
        let total_relations = resolved.len();
        let truncated = total_relations > params.limit;
        let relations = resolved
            .into_iter()
            .take(params.limit)
            .map(|relation| StructuralRelation {
                entity_id: stable_id_of(&snapshot.index, relation.entity_id),
                label: relation.label,
                relation: relation.relation_type.to_string(),
                domain: relation.relation_type.domain().to_string(),
                source_file: snapshot
                    .index
                    .as_ref()
                    .get_file_path_by_entity(relation.entity_id)
                    .unwrap_or_default(),
            })
            .collect();
        Ok(GraphStructuralResponse {
            success: true,
            relation_epoch: snapshot.relation_epoch,
            kind: params.kind,
            direction: params.direction,
            relations,
            total_relations,
            truncated,
            relation_info: stale_info(&runtime).await,
        })
    }

    pub async fn graph_module(
        &self,
        project_id: i64,
        params: GraphModuleParams,
    ) -> Result<GraphModuleResponse, ServiceError> {
        const MAX_MODULE_EDGES: usize = 5_000;
        if project_id <= 0 {
            return Err(ServiceError::Invalid(
                "project_id must be positive".to_string(),
            ));
        }
        if params.file.trim().is_empty() {
            return Err(ServiceError::Invalid("file must not be empty".to_string()));
        }
        let (snapshot, searcher, _, runtime) = self.graph_context(project_id).await?;
        let modules = searcher.get_module_relations(&params.file);
        let imports = modules
            .imports
            .into_iter()
            .take(MAX_MODULE_EDGES)
            .map(|relation| ModuleRelation {
                entity_id: relation
                    .callee_id
                    .map(|id| stable_id_of(&snapshot.index, id))
                    .unwrap_or_default(),
                target: relation.callee_name,
                relation: relation.relation_type.to_string(),
                domain: relation.relation_type.domain().to_string(),
            })
            .collect();
        Ok(GraphModuleResponse {
            success: true,
            relation_epoch: snapshot.relation_epoch,
            file: params.file,
            exports: modules
                .exports
                .into_iter()
                .map(|id| stable_id_of(&snapshot.index, id))
                .collect(),
            caller_files: modules.callers,
            imports,
            relation_info: stale_info(&runtime).await,
        })
    }

    pub async fn watch_status(&self, project_id: i64) -> Result<WatchStatusResponse, ServiceError> {
        crate::services::watch::watch_status(&self.state, project_id)
            .await
            .map_err(ServiceError::from)
    }

    pub async fn watch_start(
        &self,
        project_id: i64,
        request: StartWatchRequest,
    ) -> Result<StartWatchResponse, ServiceError> {
        crate::services::watch::watch_start(&self.state, project_id, &request)
            .await
            .map_err(ServiceError::from)
    }

    pub async fn watch_stop(&self, project_id: i64) -> Result<StopWatchResponse, ServiceError> {
        crate::services::watch::watch_stop(&self.state, project_id)
            .await
            .map_err(ServiceError::from)
    }

    pub async fn reload_project(
        &self,
        project_id: i64,
    ) -> Result<ProjectConfigReloadResponse, ServiceError> {
        if project_id <= 0 {
            return Err(ServiceError::Invalid(
                "project_id must be positive".to_string(),
            ));
        }
        self.state
            .engine
            .reload_project_config(project_id)
            .await
            .map_err(Self::engine_error)?;
        let entry = self
            .state
            .engine
            .project_registry()
            .get_or_load(project_id)
            .await
            .map_err(Self::engine_error)?;
        Ok(ProjectConfigReloadResponse {
            success: true,
            project_id,
            message: format!("project {project_id} configuration reloaded"),
            config_version: entry.version,
        })
    }
    /// Remove watch state, delete all backend indexes, and invalidate caches
    /// for a project. Idempotent across backends; fails if any backend delete
    /// fails.
    async fn maintenance_delete(&self, project_id: i64) -> Result<(), ServiceError> {
        {
            let mut map = self.state.watch_status.write().await;
            map.remove(&project_id);
        }
        let maintenance = crate::maintenance::ProjectIndexMaintenanceService::new(
            self.state.engine.clone(),
            Some(self.state.engine.vector_clone()),
            Some(self.state.engine.fulltext_clone()),
            self.state.engine.metadata_store_clone(),
        );
        let result = maintenance.delete_project(project_id).await;
        if !result.success {
            let detail = result
                .backends
                .iter()
                .filter(|b| !b.ok)
                .map(|b| format!("{}: {}", b.backend, b.detail))
                .collect::<Vec<_>>()
                .join("; ");
            return Err(ServiceError::Engine(format!(
                "project deletion failed: {detail}"
            )));
        }
        self.state.invalidate_relation_searcher(project_id).await;
        self.state.engine.invalidate_query_cache(project_id).await;
        Ok(())
    }
}
