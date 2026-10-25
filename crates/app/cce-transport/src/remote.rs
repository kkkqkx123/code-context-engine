//! Remote service implementation over HTTP transport.
//!
//! Parameter semantics match the local implementation exactly; this type
//! only encodes the same domain models onto the wire contract paths.

use async_trait::async_trait;
use cce_api::models::*;
use cce_service::{
    CodeContextService, GraphComponentsParams, GraphCyclesParams, GraphEgoParams,
    GraphEntityImpactParams, GraphExportParams, GraphImpactParams, GraphModuleParams,
    GraphPathParams, GraphStructuralParams, GraphSubgraphParams, ServiceError,
};

use crate::client::SharedHttpClient;

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

    /// Build with an explicit token outranking the environment.
    pub fn new_with_token(base_url: &str, explicit_token: Option<String>) -> anyhow::Result<Self> {
        Ok(Self {
            client: SharedHttpClient::new_with_token(base_url, explicit_token)?,
        })
    }

    fn map(anyhow: anyhow::Error) -> ServiceError {
        let text = format!("{anyhow:#}");
        if text.contains("404") || text.contains("not found") {
            ServiceError::NotFound(text)
        } else {
            ServiceError::Transport(text)
        }
    }
}

#[async_trait]
impl CodeContextService for RemoteService {
    async fn list_projects(&self) -> Result<ProjectListResponse, ServiceError> {
        self.client
            .get(cce_api::paths::PROJECT)
            .await
            .map_err(Self::map)
    }

    async fn create_project(
        &self,
        request: CreateProjectRequest,
    ) -> Result<ProjectDetailResponse, ServiceError> {
        self.client
            .post(cce_api::paths::PROJECT, &request)
            .await
            .map_err(Self::map)
    }

    async fn get_project(&self, project_id: i64) -> Result<ProjectDetailResponse, ServiceError> {
        self.client
            .get(&cce_api::paths::project_detail(project_id))
            .await
            .map_err(Self::map)
    }

    async fn delete_project(&self, project_id: i64) -> Result<ProjectDeleteResponse, ServiceError> {
        self.client
            .delete(&cce_api::paths::project_detail(project_id))
            .await
            .map_err(Self::map)
    }

    async fn index(&self, request: IndexRequest) -> Result<IndexResponse, ServiceError> {
        self.client
            .post(cce_api::paths::INDEX, &request)
            .await
            .map_err(Self::map)
    }

    async fn search(&self, request: SearchRequest) -> Result<SearchResponse, ServiceError> {
        self.client
            .post(cce_api::paths::SEARCH, &request)
            .await
            .map_err(Self::map)
    }

    async fn search_aggregated(
        &self,
        request: AggregatedSearchRequest,
    ) -> Result<SearchResponse, ServiceError> {
        self.client
            .post_aggregated_search(&request)
            .await
            .map_err(Self::map)
    }

    async fn graph_ego(
        &self,
        project_id: i64,
        params: GraphEgoParams,
    ) -> Result<GraphSubgraphResponse, ServiceError> {
        let mut path = cce_api::paths::GRAPH_EGO_TEMPLATE.replace(
            cce_api::paths::PROJECT_ID_PLACEHOLDER,
            &project_id.to_string(),
        );
        path.push_str(&format!(
            "?entity_id={}&depth={}&direction={}&offset={}&limit={}&domains={}&relation_types={}&include_external={}",
            urlencoding_sanitize(&params.entity_id),
            params.depth,
            urlencoding_sanitize(&params.direction),
            params.offset,
            params.limit,
            urlencoding_sanitize(&params.domains),
            urlencoding_sanitize(&params.relation_types),
            params.include_external
        ));
        self.client.get(&path).await.map_err(Self::map)
    }

    async fn graph_path(
        &self,
        project_id: i64,
        params: GraphPathParams,
    ) -> Result<GraphPathResponse, ServiceError> {
        let mut path = cce_api::paths::GRAPH_PATH_TEMPLATE.replace(
            cce_api::paths::PROJECT_ID_PLACEHOLDER,
            &project_id.to_string(),
        );
        path.push_str(&format!(
            "?start={}&end={}&max_depth={}&domains={}&relation_types={}&include_external={}",
            urlencoding_sanitize(&params.start),
            urlencoding_sanitize(&params.end),
            params.max_depth,
            urlencoding_sanitize(&params.domains),
            urlencoding_sanitize(&params.relation_types),
            params.include_external
        ));
        self.client.get(&path).await.map_err(Self::map)
    }

    async fn graph_subgraph(
        &self,
        project_id: i64,
        params: GraphSubgraphParams,
    ) -> Result<GraphSubgraphResponse, ServiceError> {
        let mut path = cce_api::paths::GRAPH_SUBGRAPH_TEMPLATE.replace(
            cce_api::paths::PROJECT_ID_PLACEHOLDER,
            &project_id.to_string(),
        );
        path.push_str(&format!(
            "?ids={}&offset={}&limit={}&domains={}&relation_types={}&include_external={}",
            urlencoding_sanitize(&params.ids.join(",")),
            params.offset,
            params.limit,
            urlencoding_sanitize(&params.domains),
            urlencoding_sanitize(&params.relation_types),
            params.include_external
        ));
        self.client.get(&path).await.map_err(Self::map)
    }

    async fn graph_components(
        &self,
        project_id: i64,
        params: GraphComponentsParams,
    ) -> Result<GraphComponentsResponse, ServiceError> {
        let mut path = cce_api::paths::GRAPH_COMPONENTS_TEMPLATE.replace(
            cce_api::paths::PROJECT_ID_PLACEHOLDER,
            &project_id.to_string(),
        );
        path.push_str(&format!(
            "?offset={}&limit={}&domains={}&relation_types={}&include_external={}",
            params.offset,
            params.limit,
            urlencoding_sanitize(&params.domains),
            urlencoding_sanitize(&params.relation_types),
            params.include_external
        ));
        self.client.get(&path).await.map_err(Self::map)
    }

    async fn graph_export(
        &self,
        project_id: i64,
        params: GraphExportParams,
    ) -> Result<GraphSubgraphResponse, ServiceError> {
        let mut path = cce_api::paths::GRAPH_EXPORT_TEMPLATE.replace(
            cce_api::paths::PROJECT_ID_PLACEHOLDER,
            &project_id.to_string(),
        );
        path.push_str(&format!(
            "?limit={}&offset={}&domains={}&relation_types={}&include_external={}",
            params.limit,
            params.offset,
            urlencoding_sanitize(&params.domains),
            urlencoding_sanitize(&params.relation_types),
            params.include_external
        ));
        self.client.get(&path).await.map_err(Self::map)
    }

    async fn graph_impact(
        &self,
        project_id: i64,
        params: GraphImpactParams,
    ) -> Result<GraphImpactResponse, ServiceError> {
        let mut path = cce_api::paths::GRAPH_IMPACT_TEMPLATE.replace(
            cce_api::paths::PROJECT_ID_PLACEHOLDER,
            &project_id.to_string(),
        );
        path.push_str(&format!("?file={}", urlencoding_sanitize(&params.file)));
        self.client.get(&path).await.map_err(Self::map)
    }

    async fn graph_entity_impact(
        &self,
        project_id: i64,
        params: GraphEntityImpactParams,
    ) -> Result<GraphEntityImpactResponse, ServiceError> {
        let mut path = cce_api::paths::GRAPH_ENTITY_IMPACT_TEMPLATE.replace(
            cce_api::paths::PROJECT_ID_PLACEHOLDER,
            &project_id.to_string(),
        );
        path.push_str(&format!(
            "?entity_id={}&max_depth={}&scope={}",
            urlencoding_sanitize(&params.entity_id),
            params.max_depth,
            urlencoding_sanitize(&params.scope)
        ));
        self.client.get(&path).await.map_err(Self::map)
    }

    async fn graph_cycles(
        &self,
        project_id: i64,
        params: GraphCyclesParams,
    ) -> Result<GraphCyclesResponse, ServiceError> {
        let mut path = cce_api::paths::GRAPH_CYCLES_TEMPLATE.replace(
            cce_api::paths::PROJECT_ID_PLACEHOLDER,
            &project_id.to_string(),
        );
        path.push_str(&format!(
            "?level={}&limit={}",
            urlencoding_sanitize(&params.level),
            params.limit
        ));
        self.client.get(&path).await.map_err(Self::map)
    }

    async fn graph_structural(
        &self,
        project_id: i64,
        params: GraphStructuralParams,
    ) -> Result<GraphStructuralResponse, ServiceError> {
        let mut path = cce_api::paths::GRAPH_STRUCTURAL_TEMPLATE.replace(
            cce_api::paths::PROJECT_ID_PLACEHOLDER,
            &project_id.to_string(),
        );
        path.push_str(&format!(
            "?entity_id={}&kind={}&direction={}&limit={}",
            urlencoding_sanitize(&params.entity_id),
            urlencoding_sanitize(&params.kind),
            urlencoding_sanitize(&params.direction),
            params.limit
        ));
        self.client.get(&path).await.map_err(Self::map)
    }

    async fn graph_module(
        &self,
        project_id: i64,
        params: GraphModuleParams,
    ) -> Result<GraphModuleResponse, ServiceError> {
        let mut path = cce_api::paths::GRAPH_MODULE_TEMPLATE.replace(
            cce_api::paths::PROJECT_ID_PLACEHOLDER,
            &project_id.to_string(),
        );
        path.push_str(&format!("?file={}", urlencoding_sanitize(&params.file)));
        self.client.get(&path).await.map_err(Self::map)
    }

    async fn watch_status(&self, project_id: i64) -> Result<WatchStatusResponse, ServiceError> {
        self.client
            .get(&cce_api::paths::watch_status(project_id))
            .await
            .map_err(Self::map)
    }

    async fn watch_start(
        &self,
        project_id: i64,
        request: StartWatchRequest,
    ) -> Result<StartWatchResponse, ServiceError> {
        self.client
            .post(&cce_api::paths::watch_start(project_id), &request)
            .await
            .map_err(Self::map)
    }

    async fn watch_stop(&self, project_id: i64) -> Result<StopWatchResponse, ServiceError> {
        self.client
            .post(
                &cce_api::paths::watch_stop(project_id),
                &serde_json::json!({}),
            )
            .await
            .map_err(Self::map)
    }

    async fn reload_project(
        &self,
        project_id: i64,
    ) -> Result<ProjectConfigReloadResponse, ServiceError> {
        self.client
            .post(
                &cce_api::paths::project_reload(project_id),
                &serde_json::json!({}),
            )
            .await
            .map_err(Self::map)
    }
}

fn urlencoding_sanitize(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for byte in raw.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ego_path_encodes_entity_id() {
        assert_eq!(urlencoding_sanitize("sym::foo/bar"), "sym%3A%3Afoo%2Fbar");
    }
}
