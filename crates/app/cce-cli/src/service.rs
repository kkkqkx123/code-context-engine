//! CLI service selection between local engine and remote transport.
//!
//! Both modes implement the same domain operations so command semantics stay
//! identical; only the transport differs. Local mode holds the engine
//! directly with zero-copy filesystem access. Remote mode encodes the same
//! parameters onto HTTP.

use crate::remote::RemoteService;
use anyhow::{Context, Result};
use cce_api::models::*;
use cce_api::service_params::*;
use cce_server_shared::service_local::LocalService;

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
    Remote(RemoteService),
    /// Local in-process implementation.
    Local(LocalService),
}

impl CliService {
    /// Build the selected implementation.
    pub async fn build(server: &str, mode: CliServiceMode) -> Result<Self> {
        match mode {
            CliServiceMode::Remote => {
                let remote = RemoteService::new(server)
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
                let local =
                    cce_server_shared::service_local::LocalService::from_engine(&engine).await;
                Ok(Self::Local(local))
            }
        }
    }

    /// Borrow as the domain service so commands program against one abstraction.
    pub fn as_trait(&self) -> &Self {
        self
    }

    // Dispatch methods (replacing the former CodeContextService trait).
    pub async fn list_projects(&self) -> anyhow::Result<ProjectListResponse> {
        match self {
            Self::Remote(s) => s.list_projects().await,
            Self::Local(s) => s.list_projects().await.map_err(Into::into),
        }
    }

    pub async fn create_project(
        &self,
        request: CreateProjectRequest,
    ) -> anyhow::Result<ProjectDetailResponse> {
        match self {
            Self::Remote(s) => s.create_project(request).await,
            Self::Local(s) => s.create_project(request).await.map_err(Into::into),
        }
    }

    pub async fn get_project(&self, project_id: i64) -> anyhow::Result<ProjectDetailResponse> {
        match self {
            Self::Remote(s) => s.get_project(project_id).await,
            Self::Local(s) => s.get_project(project_id).await.map_err(Into::into),
        }
    }

    pub async fn delete_project(&self, project_id: i64) -> anyhow::Result<ProjectDeleteResponse> {
        match self {
            Self::Remote(s) => s.delete_project(project_id).await,
            Self::Local(s) => s.delete_project(project_id).await.map_err(Into::into),
        }
    }

    pub async fn index(&self, request: IndexRequest) -> anyhow::Result<IndexResponse> {
        match self {
            Self::Remote(s) => s.index(request).await,
            Self::Local(s) => s.index(request).await.map_err(Into::into),
        }
    }

    pub async fn search(&self, request: SearchRequest) -> anyhow::Result<SearchResponse> {
        match self {
            Self::Remote(s) => s.search(request).await,
            Self::Local(s) => s.search(request).await.map_err(Into::into),
        }
    }

    pub async fn search_aggregated(
        &self,
        request: AggregatedSearchRequest,
    ) -> anyhow::Result<SearchResponse> {
        match self {
            Self::Remote(s) => s.search_aggregated(request).await,
            Self::Local(s) => s.search_aggregated(request).await.map_err(Into::into),
        }
    }

    pub async fn graph_ego(
        &self,
        project_id: i64,
        params: GraphEgoParams,
    ) -> anyhow::Result<GraphSubgraphResponse> {
        match self {
            Self::Remote(s) => s.graph_ego(project_id, params).await,
            Self::Local(s) => s.graph_ego(project_id, params).await.map_err(Into::into),
        }
    }

    pub async fn graph_path(
        &self,
        project_id: i64,
        params: GraphPathParams,
    ) -> anyhow::Result<GraphPathResponse> {
        match self {
            Self::Remote(s) => s.graph_path(project_id, params).await,
            Self::Local(s) => s.graph_path(project_id, params).await.map_err(Into::into),
        }
    }

    pub async fn graph_subgraph(
        &self,
        project_id: i64,
        params: GraphSubgraphParams,
    ) -> anyhow::Result<GraphSubgraphResponse> {
        match self {
            Self::Remote(s) => s.graph_subgraph(project_id, params).await,
            Self::Local(s) => s
                .graph_subgraph(project_id, params)
                .await
                .map_err(Into::into),
        }
    }

    pub async fn graph_components(
        &self,
        project_id: i64,
        params: GraphComponentsParams,
    ) -> anyhow::Result<GraphComponentsResponse> {
        match self {
            Self::Remote(s) => s.graph_components(project_id, params).await,
            Self::Local(s) => s
                .graph_components(project_id, params)
                .await
                .map_err(Into::into),
        }
    }

    pub async fn graph_export(
        &self,
        project_id: i64,
        params: GraphExportParams,
    ) -> anyhow::Result<GraphSubgraphResponse> {
        match self {
            Self::Remote(s) => s.graph_export(project_id, params).await,
            Self::Local(s) => s.graph_export(project_id, params).await.map_err(Into::into),
        }
    }

    pub async fn graph_impact(
        &self,
        project_id: i64,
        params: GraphImpactParams,
    ) -> anyhow::Result<GraphImpactResponse> {
        match self {
            Self::Remote(s) => s.graph_impact(project_id, params).await,
            Self::Local(s) => s.graph_impact(project_id, params).await.map_err(Into::into),
        }
    }

    pub async fn graph_entity_impact(
        &self,
        project_id: i64,
        params: GraphEntityImpactParams,
    ) -> anyhow::Result<GraphEntityImpactResponse> {
        match self {
            Self::Remote(s) => s.graph_entity_impact(project_id, params).await,
            Self::Local(s) => s
                .graph_entity_impact(project_id, params)
                .await
                .map_err(Into::into),
        }
    }

    pub async fn graph_cycles(
        &self,
        project_id: i64,
        params: GraphCyclesParams,
    ) -> anyhow::Result<GraphCyclesResponse> {
        match self {
            Self::Remote(s) => s.graph_cycles(project_id, params).await,
            Self::Local(s) => s.graph_cycles(project_id, params).await.map_err(Into::into),
        }
    }

    pub async fn graph_structural(
        &self,
        project_id: i64,
        params: GraphStructuralParams,
    ) -> anyhow::Result<GraphStructuralResponse> {
        match self {
            Self::Remote(s) => s.graph_structural(project_id, params).await,
            Self::Local(s) => s
                .graph_structural(project_id, params)
                .await
                .map_err(Into::into),
        }
    }

    pub async fn graph_module(
        &self,
        project_id: i64,
        params: GraphModuleParams,
    ) -> anyhow::Result<GraphModuleResponse> {
        match self {
            Self::Remote(s) => s.graph_module(project_id, params).await,
            Self::Local(s) => s.graph_module(project_id, params).await.map_err(Into::into),
        }
    }

    pub async fn watch_status(&self, project_id: i64) -> anyhow::Result<WatchStatusResponse> {
        match self {
            Self::Remote(s) => s.watch_status(project_id).await,
            Self::Local(s) => s.watch_status(project_id).await.map_err(Into::into),
        }
    }

    pub async fn watch_start(
        &self,
        project_id: i64,
        request: StartWatchRequest,
    ) -> anyhow::Result<StartWatchResponse> {
        match self {
            Self::Remote(s) => s.watch_start(project_id, request).await,
            Self::Local(s) => s.watch_start(project_id, request).await.map_err(Into::into),
        }
    }

    pub async fn watch_stop(&self, project_id: i64) -> anyhow::Result<StopWatchResponse> {
        match self {
            Self::Remote(s) => s.watch_stop(project_id).await,
            Self::Local(s) => s.watch_stop(project_id).await.map_err(Into::into),
        }
    }

    pub async fn reload_project(
        &self,
        project_id: i64,
    ) -> anyhow::Result<ProjectConfigReloadResponse> {
        match self {
            Self::Remote(s) => s.reload_project(project_id).await,
            Self::Local(s) => s.reload_project(project_id).await.map_err(Into::into),
        }
    }
}
