//! Remote service implementation over HTTP transport.
//!
//! Parameter semantics match the local implementation exactly; this type
//! only encodes the same domain models onto the wire contract paths.

use cce_api::models::*;
use cce_api::service_params::{
    GraphComponentsParams, GraphCyclesParams, GraphEgoParams, GraphEntityImpactParams,
    GraphExportParams, GraphImpactParams, GraphModuleParams, GraphPathParams,
    GraphStructuralParams, GraphSubgraphParams,
};
use anyhow::Result as AnyhowResult;

use cce_transport::{GraphQuery, HttpTransportError, QueryParam, SharedHttpClient};

/// Remote domain service speaking HTTP to a running server.
pub struct RemoteService {
    client: SharedHttpClient,
}

impl RemoteService {
    /// Build a remote service for the given server base URL.
    pub fn new(base_url: &str) -> anyhow::Result<Self> {
        Ok(Self {
            client: SharedHttpClient::new(base_url)?,
        })
    }

    fn map(error: anyhow::Error) -> anyhow::Error {
        if let Some(http) = error.downcast_ref::<HttpTransportError>() {
            let text = http.to_string();
            return if http.status == Some(404) {
                anyhow::anyhow!("not found: {text}")
            } else {
                anyhow::anyhow!("transport error: {text}")
            };
        }
        let text = format!("{error:#}");
        if text.contains("404") || text.contains("not found") {
            anyhow::anyhow!("not found: {text}")
        } else {
            anyhow::anyhow!("transport error: {text}")
        }
    }
}

impl RemoteService {
    pub async fn list_projects(&self) -> AnyhowResult<ProjectListResponse> {
        self.client
            .get(cce_api::paths::PROJECT)
            .await
            .map_err(Self::map)
    }

    pub async fn create_project(
        &self,
        request: CreateProjectRequest,
    ) -> AnyhowResult<ProjectDetailResponse> {
        self.client
            .post(cce_api::paths::PROJECT, &request)
            .await
            .map_err(Self::map)
    }

    pub async fn get_project(&self, project_id: i64) -> AnyhowResult<ProjectDetailResponse> {
        self.client
            .get(&cce_api::paths::project_detail(project_id))
            .await
            .map_err(Self::map)
    }

    pub async fn delete_project(&self, project_id: i64) -> AnyhowResult<ProjectDeleteResponse> {
        self.client
            .delete(&cce_api::paths::project_detail(project_id))
            .await
            .map_err(Self::map)
    }

    pub async fn index(&self, request: IndexRequest) -> AnyhowResult<IndexResponse> {
        self.client
            .post(cce_api::paths::INDEX, &request)
            .await
            .map_err(Self::map)
    }

    pub async fn search(&self, request: SearchRequest) -> AnyhowResult<SearchResponse> {
        self.client
            .post(cce_api::paths::SEARCH, &request)
            .await
            .map_err(Self::map)
    }

    pub async fn search_aggregated(
        &self,
        request: AggregatedSearchRequest,
    ) -> AnyhowResult<SearchResponse> {
        self.client
            .post_aggregated_search(&request)
            .await
            .map_err(Self::map)
    }

    pub async fn graph_ego(
        &self,
        project_id: i64,
        params: GraphEgoParams,
    ) -> AnyhowResult<GraphSubgraphResponse> {
        let path_template = cce_api::paths::GRAPH_EGO_TEMPLATE.replace(
            cce_api::paths::PROJECT_ID_PLACEHOLDER,
            &project_id.to_string(),
        );
        let path = GraphQuery::new(path_template)
            .param("entity_id", QueryParam::Text(params.entity_id))
            .param("depth", QueryParam::Number(params.depth))
            .param("direction", QueryParam::Text(params.direction))
            .param("offset", QueryParam::Number(params.offset))
            .param("limit", QueryParam::Number(params.limit))
            .param("domains", QueryParam::Text(params.domains))
            .param("relation_types", QueryParam::Text(params.relation_types))
            .param("include_external", QueryParam::Bool(params.include_external))
            .build();
        self.client.get(&path).await.map_err(Self::map)
    }

    pub async fn graph_path(
        &self,
        project_id: i64,
        params: GraphPathParams,
    ) -> AnyhowResult<GraphPathResponse> {
        let path_template = cce_api::paths::GRAPH_PATH_TEMPLATE.replace(
            cce_api::paths::PROJECT_ID_PLACEHOLDER,
            &project_id.to_string(),
        );
        let path = GraphQuery::new(path_template)
            .param("start", QueryParam::Text(params.start))
            .param("end", QueryParam::Text(params.end))
            .param("max_depth", QueryParam::Number(params.max_depth))
            .param("domains", QueryParam::Text(params.domains))
            .param("relation_types", QueryParam::Text(params.relation_types))
            .param("include_external", QueryParam::Bool(params.include_external))
            .build();
        self.client.get(&path).await.map_err(Self::map)
    }

    pub async fn graph_subgraph(
        &self,
        project_id: i64,
        params: GraphSubgraphParams,
    ) -> AnyhowResult<GraphSubgraphResponse> {
        let path_template = cce_api::paths::GRAPH_SUBGRAPH_TEMPLATE.replace(
            cce_api::paths::PROJECT_ID_PLACEHOLDER,
            &project_id.to_string(),
        );
        let path = GraphQuery::new(path_template)
            .param("ids", QueryParam::List(params.ids))
            .param("offset", QueryParam::Number(params.offset))
            .param("limit", QueryParam::Number(params.limit))
            .param("domains", QueryParam::Text(params.domains))
            .param("relation_types", QueryParam::Text(params.relation_types))
            .param("include_external", QueryParam::Bool(params.include_external))
            .build();
        self.client.get(&path).await.map_err(Self::map)
    }

    pub async fn graph_components(
        &self,
        project_id: i64,
        params: GraphComponentsParams,
    ) -> AnyhowResult<GraphComponentsResponse> {
        let path_template = cce_api::paths::GRAPH_COMPONENTS_TEMPLATE.replace(
            cce_api::paths::PROJECT_ID_PLACEHOLDER,
            &project_id.to_string(),
        );
        let path = GraphQuery::new(path_template)
            .param("offset", QueryParam::Number(params.offset))
            .param("limit", QueryParam::Number(params.limit))
            .param("domains", QueryParam::Text(params.domains))
            .param("relation_types", QueryParam::Text(params.relation_types))
            .param("include_external", QueryParam::Bool(params.include_external))
            .build();
        self.client.get(&path).await.map_err(Self::map)
    }

    pub async fn graph_export(
        &self,
        project_id: i64,
        params: GraphExportParams,
    ) -> AnyhowResult<GraphSubgraphResponse> {
        let path_template = cce_api::paths::GRAPH_EXPORT_TEMPLATE.replace(
            cce_api::paths::PROJECT_ID_PLACEHOLDER,
            &project_id.to_string(),
        );
        let path = GraphQuery::new(path_template)
            .param("limit", QueryParam::Number(params.limit))
            .param("offset", QueryParam::Number(params.offset))
            .param("domains", QueryParam::Text(params.domains))
            .param("relation_types", QueryParam::Text(params.relation_types))
            .param("include_external", QueryParam::Bool(params.include_external))
            .build();
        self.client.get(&path).await.map_err(Self::map)
    }

    pub async fn graph_impact(
        &self,
        project_id: i64,
        params: GraphImpactParams,
    ) -> AnyhowResult<GraphImpactResponse> {
        let path_template = cce_api::paths::GRAPH_IMPACT_TEMPLATE.replace(
            cce_api::paths::PROJECT_ID_PLACEHOLDER,
            &project_id.to_string(),
        );
        let path = GraphQuery::new(path_template)
            .param("file", QueryParam::Text(params.file))
            .build();
        self.client.get(&path).await.map_err(Self::map)
    }

    pub async fn graph_entity_impact(
        &self,
        project_id: i64,
        params: GraphEntityImpactParams,
    ) -> AnyhowResult<GraphEntityImpactResponse> {
        let path_template = cce_api::paths::GRAPH_ENTITY_IMPACT_TEMPLATE.replace(
            cce_api::paths::PROJECT_ID_PLACEHOLDER,
            &project_id.to_string(),
        );
        let path = GraphQuery::new(path_template)
            .param("entity_id", QueryParam::Text(params.entity_id))
            .param("max_depth", QueryParam::Number(params.max_depth))
            .param("scope", QueryParam::Text(params.scope))
            .build();
        self.client.get(&path).await.map_err(Self::map)
    }

    pub async fn graph_cycles(
        &self,
        project_id: i64,
        params: GraphCyclesParams,
    ) -> AnyhowResult<GraphCyclesResponse> {
        let path_template = cce_api::paths::GRAPH_CYCLES_TEMPLATE.replace(
            cce_api::paths::PROJECT_ID_PLACEHOLDER,
            &project_id.to_string(),
        );
        let path = GraphQuery::new(path_template)
            .param("level", QueryParam::Text(params.level))
            .param("limit", QueryParam::Number(params.limit))
            .build();
        self.client.get(&path).await.map_err(Self::map)
    }

    pub async fn graph_structural(
        &self,
        project_id: i64,
        params: GraphStructuralParams,
    ) -> AnyhowResult<GraphStructuralResponse> {
        let path_template = cce_api::paths::GRAPH_STRUCTURAL_TEMPLATE.replace(
            cce_api::paths::PROJECT_ID_PLACEHOLDER,
            &project_id.to_string(),
        );
        let path = GraphQuery::new(path_template)
            .param("entity_id", QueryParam::Text(params.entity_id))
            .param("kind", QueryParam::Text(params.kind))
            .param("direction", QueryParam::Text(params.direction))
            .param("limit", QueryParam::Number(params.limit))
            .build();
        self.client.get(&path).await.map_err(Self::map)
    }

    pub async fn graph_module(
        &self,
        project_id: i64,
        params: GraphModuleParams,
    ) -> AnyhowResult<GraphModuleResponse> {
        let path_template = cce_api::paths::GRAPH_MODULE_TEMPLATE.replace(
            cce_api::paths::PROJECT_ID_PLACEHOLDER,
            &project_id.to_string(),
        );
        let path = GraphQuery::new(path_template)
            .param("file", QueryParam::Text(params.file))
            .build();
        self.client.get(&path).await.map_err(Self::map)
    }

    pub async fn watch_status(&self, project_id: i64) -> AnyhowResult<WatchStatusResponse> {
        self.client
            .get(&cce_api::paths::watch_status(project_id))
            .await
            .map_err(Self::map)
    }

    pub async fn watch_start(
        &self,
        project_id: i64,
        request: StartWatchRequest,
    ) -> AnyhowResult<StartWatchResponse> {
        self.client
            .post(&cce_api::paths::watch_start(project_id), &request)
            .await
            .map_err(Self::map)
    }

    pub async fn watch_stop(&self, project_id: i64) -> AnyhowResult<StopWatchResponse> {
        self.client
            .post(
                &cce_api::paths::watch_stop(project_id),
                &serde_json::json!({}),
            )
            .await
            .map_err(Self::map)
    }

    pub async fn reload_project(
        &self,
        project_id: i64,
    ) -> AnyhowResult<ProjectConfigReloadResponse> {
        self.client
            .post(
                &cce_api::paths::project_reload(project_id),
                &serde_json::json!({}),
            )
            .await
            .map_err(Self::map)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn ego_path_uses_transport_encoding() {
        // smoke: ensure module compiles with trait usage
        let _ = cce_transport::urlencoding_sanitize("sym::foo/bar");
    }
}
