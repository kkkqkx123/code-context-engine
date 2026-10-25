//! HTTP client for communicating with CCE server
//!
//! Single client layer for the CLI: generic transport methods plus typed
//! domain methods encoding request parameters onto the wire contract paths.

use anyhow::Result;
use serde::{de::DeserializeOwned, Serialize};

use cce_api::models::*;
use cce_api::service_params::{
    GraphComponentsParams, GraphCyclesParams, GraphEgoParams, GraphEntityImpactParams,
    GraphExportParams, GraphImpactParams, GraphModuleParams, GraphPathParams,
    GraphStructuralParams, GraphSubgraphParams,
};
use cce_transport::{GraphQuery, HttpTransportError, QueryParam, SharedHttpClient};

/// API client
pub struct ApiClient {
    inner: SharedHttpClient,
}

impl ApiClient {
    /// Create a new API client
    pub fn new(base_url: &str) -> Result<Self> {
        Self::new_with_token(base_url, None)
    }

    /// Create a client with an explicit token outranking the environment.
    pub fn new_with_token(base_url: &str, explicit_token: Option<String>) -> Result<Self> {
        Ok(Self {
            inner: SharedHttpClient::new_with_token(base_url, explicit_token)?,
        })
    }

    /// Make a GET request
    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.inner.get(path).await
    }

    /// Make a GET request returning raw text.
    pub async fn get_text(&self, path: &str) -> Result<String> {
        self.inner.get_text(path).await
    }

    /// Make a POST request
    pub async fn post<T: Serialize, R: DeserializeOwned>(&self, path: &str, body: &T) -> Result<R> {
        self.inner.post(path, body).await
    }

    /// Make a DELETE request
    pub async fn delete<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.inner.delete(path).await
    }

    /// Make a PUT request
    pub async fn put<T: Serialize, R: DeserializeOwned>(&self, path: &str, body: &T) -> Result<R> {
        self.inner.put(path, body).await
    }

    /// Make a DELETE request with body
    pub async fn delete_with_body<T: Serialize, R: DeserializeOwned>(
        &self,
        path: &str,
        body: &T,
    ) -> Result<R> {
        self.inner.delete_with_body(path, body).await
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

impl ApiClient {
    pub async fn list_projects(&self) -> Result<ProjectListResponse> {
        self.get(cce_api::paths::PROJECT).await.map_err(Self::map)
    }

    pub async fn create_project(
        &self,
        request: CreateProjectRequest,
    ) -> Result<ProjectDetailResponse> {
        self.post(cce_api::paths::PROJECT, &request)
            .await
            .map_err(Self::map)
    }

    pub async fn get_project(&self, project_id: i64) -> Result<ProjectDetailResponse> {
        self.get(&cce_api::paths::project_detail(project_id))
            .await
            .map_err(Self::map)
    }

    pub async fn delete_project(&self, project_id: i64) -> Result<ProjectDeleteResponse> {
        self.delete(&cce_api::paths::project_detail(project_id))
            .await
            .map_err(Self::map)
    }

    pub async fn index(&self, request: IndexRequest) -> Result<IndexResponse> {
        self.post(cce_api::paths::INDEX, &request)
            .await
            .map_err(Self::map)
    }

    pub async fn search(&self, request: SearchRequest) -> Result<SearchResponse> {
        self.post(cce_api::paths::SEARCH, &request)
            .await
            .map_err(Self::map)
    }

    pub async fn search_aggregated(
        &self,
        request: AggregatedSearchRequest,
    ) -> Result<SearchResponse> {
        self.inner
            .post_aggregated_search(&request)
            .await
            .map_err(Self::map)
    }

    pub async fn graph_ego(
        &self,
        project_id: i64,
        params: GraphEgoParams,
    ) -> Result<GraphSubgraphResponse> {
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
            .param(
                "include_external",
                QueryParam::Bool(params.include_external),
            )
            .build();
        self.get(&path).await.map_err(Self::map)
    }

    pub async fn graph_path(
        &self,
        project_id: i64,
        params: GraphPathParams,
    ) -> Result<GraphPathResponse> {
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
            .param(
                "include_external",
                QueryParam::Bool(params.include_external),
            )
            .build();
        self.get(&path).await.map_err(Self::map)
    }

    pub async fn graph_subgraph(
        &self,
        project_id: i64,
        params: GraphSubgraphParams,
    ) -> Result<GraphSubgraphResponse> {
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
            .param(
                "include_external",
                QueryParam::Bool(params.include_external),
            )
            .build();
        self.get(&path).await.map_err(Self::map)
    }

    pub async fn graph_components(
        &self,
        project_id: i64,
        params: GraphComponentsParams,
    ) -> Result<GraphComponentsResponse> {
        let path_template = cce_api::paths::GRAPH_COMPONENTS_TEMPLATE.replace(
            cce_api::paths::PROJECT_ID_PLACEHOLDER,
            &project_id.to_string(),
        );
        let path = GraphQuery::new(path_template)
            .param("offset", QueryParam::Number(params.offset))
            .param("limit", QueryParam::Number(params.limit))
            .param("domains", QueryParam::Text(params.domains))
            .param("relation_types", QueryParam::Text(params.relation_types))
            .param(
                "include_external",
                QueryParam::Bool(params.include_external),
            )
            .build();
        self.get(&path).await.map_err(Self::map)
    }

    pub async fn graph_export(
        &self,
        project_id: i64,
        params: GraphExportParams,
    ) -> Result<GraphSubgraphResponse> {
        let path_template = cce_api::paths::GRAPH_EXPORT_TEMPLATE.replace(
            cce_api::paths::PROJECT_ID_PLACEHOLDER,
            &project_id.to_string(),
        );
        let path = GraphQuery::new(path_template)
            .param("limit", QueryParam::Number(params.limit))
            .param("offset", QueryParam::Number(params.offset))
            .param("domains", QueryParam::Text(params.domains))
            .param("relation_types", QueryParam::Text(params.relation_types))
            .param(
                "include_external",
                QueryParam::Bool(params.include_external),
            )
            .build();
        self.get(&path).await.map_err(Self::map)
    }

    pub async fn graph_impact(
        &self,
        project_id: i64,
        params: GraphImpactParams,
    ) -> Result<GraphImpactResponse> {
        let path_template = cce_api::paths::GRAPH_IMPACT_TEMPLATE.replace(
            cce_api::paths::PROJECT_ID_PLACEHOLDER,
            &project_id.to_string(),
        );
        let path = GraphQuery::new(path_template)
            .param("file", QueryParam::Text(params.file))
            .build();
        self.get(&path).await.map_err(Self::map)
    }

    pub async fn graph_entity_impact(
        &self,
        project_id: i64,
        params: GraphEntityImpactParams,
    ) -> Result<GraphEntityImpactResponse> {
        let path_template = cce_api::paths::GRAPH_ENTITY_IMPACT_TEMPLATE.replace(
            cce_api::paths::PROJECT_ID_PLACEHOLDER,
            &project_id.to_string(),
        );
        let path = GraphQuery::new(path_template)
            .param("entity_id", QueryParam::Text(params.entity_id))
            .param("max_depth", QueryParam::Number(params.max_depth))
            .param("scope", QueryParam::Text(params.scope))
            .build();
        self.get(&path).await.map_err(Self::map)
    }

    pub async fn graph_cycles(
        &self,
        project_id: i64,
        params: GraphCyclesParams,
    ) -> Result<GraphCyclesResponse> {
        let path_template = cce_api::paths::GRAPH_CYCLES_TEMPLATE.replace(
            cce_api::paths::PROJECT_ID_PLACEHOLDER,
            &project_id.to_string(),
        );
        let path = GraphQuery::new(path_template)
            .param("level", QueryParam::Text(params.level))
            .param("limit", QueryParam::Number(params.limit))
            .build();
        self.get(&path).await.map_err(Self::map)
    }

    pub async fn graph_structural(
        &self,
        project_id: i64,
        params: GraphStructuralParams,
    ) -> Result<GraphStructuralResponse> {
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
        self.get(&path).await.map_err(Self::map)
    }

    pub async fn graph_module(
        &self,
        project_id: i64,
        params: GraphModuleParams,
    ) -> Result<GraphModuleResponse> {
        let path_template = cce_api::paths::GRAPH_MODULE_TEMPLATE.replace(
            cce_api::paths::PROJECT_ID_PLACEHOLDER,
            &project_id.to_string(),
        );
        let path = GraphQuery::new(path_template)
            .param("file", QueryParam::Text(params.file))
            .build();
        self.get(&path).await.map_err(Self::map)
    }

    pub async fn watch_status(&self, project_id: i64) -> Result<WatchStatusResponse> {
        self.get(&cce_api::paths::watch_status(project_id))
            .await
            .map_err(Self::map)
    }

    pub async fn watch_start(
        &self,
        project_id: i64,
        request: StartWatchRequest,
    ) -> Result<StartWatchResponse> {
        self.post(&cce_api::paths::watch_start(project_id), &request)
            .await
            .map_err(Self::map)
    }

    pub async fn watch_stop(&self, project_id: i64) -> Result<StopWatchResponse> {
        self.post(
            &cce_api::paths::watch_stop(project_id),
            &serde_json::json!({}),
        )
        .await
        .map_err(Self::map)
    }

    pub async fn reload_project(&self, project_id: i64) -> Result<ProjectConfigReloadResponse> {
        self.post(
            &cce_api::paths::project_reload(project_id),
            &serde_json::json!({}),
        )
        .await
        .map_err(Self::map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    };
    use std::time::Duration;

    struct Recorded {
        method: String,
        path: String,
        auth: Option<String>,
        body: String,
    }

    struct Stub {
        url: String,
        recorded: Arc<Mutex<Vec<Recorded>>>,
        shutdown: Arc<AtomicBool>,
        handle: Option<std::thread::JoinHandle<()>>,
    }

    impl Stub {
        fn spawn(payload: serde_json::Value) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("stub binds loopback");
            let url = format!("http://{}", listener.local_addr().expect("stub addr"));
            let recorded = Arc::new(Mutex::new(Vec::new()));
            let shutdown = Arc::new(AtomicBool::new(false));
            let text = serde_json::to_string(&payload).expect("stub payload serializes");
            listener.set_nonblocking(true).expect("stub nonblocking");
            let thread_recorded = Arc::clone(&recorded);
            let thread_shutdown = Arc::clone(&shutdown);
            let handle = std::thread::spawn(move || {
                while !thread_shutdown.load(Ordering::Relaxed) {
                    let (stream, _) = match listener.accept() {
                        Ok(pair) => pair,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(10));
                            continue;
                        }
                        Err(_) => break,
                    };
                    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
                    let mut reader = BufReader::new(stream.try_clone().expect("clone"));
                    let mut request_line = String::new();
                    if reader.read_line(&mut request_line).is_err() {
                        continue;
                    }
                    let mut parts = request_line.split_whitespace();
                    let method = parts.next().unwrap_or("").to_string();
                    let path = parts.next().unwrap_or("").to_string();
                    let mut content_length = 0usize;
                    let mut auth = None;
                    loop {
                        let mut line = String::new();
                        if reader.read_line(&mut line).is_err() {
                            break;
                        }
                        let line = line.trim_end();
                        if line.is_empty() {
                            break;
                        }
                        if let Some((name, value)) = line.split_once(':') {
                            if name.trim().eq_ignore_ascii_case("content-length") {
                                content_length = value.trim().parse().unwrap_or(0);
                            }
                            if name.trim().eq_ignore_ascii_case("authorization") {
                                auth = Some(value.trim().to_string());
                            }
                        }
                    }
                    let mut body = vec![0u8; content_length];
                    if content_length > 0 {
                        let _ = reader.read_exact(&mut body);
                    }
                    thread_recorded.lock().expect("recorded").push(Recorded {
                        method,
                        path,
                        auth,
                        body: String::from_utf8_lossy(&body).into_owned(),
                    });
                    let mut stream = reader.into_inner();
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
                        text.len()
                    );
                    let _ = stream.flush();
                }
            });
            Self {
                url,
                recorded,
                shutdown,
                handle: Some(handle),
            }
        }

        fn recorded(&self) -> Vec<(String, String, Option<String>, String)> {
            self.recorded
                .lock()
                .expect("recorded")
                .iter()
                .map(|r| {
                    (
                        r.method.clone(),
                        r.path.clone(),
                        r.auth.clone(),
                        r.body.clone(),
                    )
                })
                .collect()
        }
    }

    impl Drop for Stub {
        fn drop(&mut self) {
            self.shutdown.store(true, Ordering::Relaxed);
            if let Some(handle) = self.handle.take() {
                let _ = handle.join();
            }
        }
    }

    #[tokio::test]
    async fn get_hits_contract_path_verbatim() {
        let stub = Stub::spawn(serde_json::json!({"ok": true}));
        let client = ApiClient::new(&stub.url).expect("client builds");
        let value: serde_json::Value = client
            .get("/api/project/3/graph/ego?entity_id=x")
            .await
            .expect("stub get succeeds");
        assert_eq!(value, serde_json::json!({"ok": true}));
        let recorded = stub.recorded();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].0, "GET");
        assert_eq!(recorded[0].1, "/api/project/3/graph/ego?entity_id=x");
    }

    #[tokio::test]
    async fn post_sends_json_body_to_contract_path() {
        let stub = Stub::spawn(serde_json::json!({"success": true}));
        let client = ApiClient::new(&stub.url).expect("client builds");
        let value: serde_json::Value = client
            .post("/api/search", &serde_json::json!({"query": "fold"}))
            .await
            .expect("stub post succeeds");
        assert_eq!(value["success"], true);
        let recorded = stub.recorded();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].0, "POST");
        assert_eq!(recorded[0].1, "/api/search");
        let body: serde_json::Value =
            serde_json::from_str(&recorded[0].3).expect("post body is json");
        assert_eq!(body["query"], "fold");
    }

    #[tokio::test]
    async fn explicit_token_outranks_environment_as_bearer() {
        let stub = Stub::spawn(serde_json::json!({"ok": true}));
        let client = ApiClient::new_with_token(&stub.url, Some("explicit-1".to_string()))
            .expect("client builds");
        let _: serde_json::Value = client.get("/api/status").await.expect("stub get succeeds");
        let recorded = stub.recorded();
        assert_eq!(recorded[0].2.as_deref(), Some("Bearer explicit-1"));
    }

    #[tokio::test]
    async fn trailing_slash_base_url_does_not_double_slash() {
        let stub = Stub::spawn(serde_json::json!({"ok": true}));
        let client = ApiClient::new(&format!("{}/", stub.url)).expect("client builds");
        let _: serde_json::Value = client.get("/api/status").await.expect("stub get succeeds");
        let recorded = stub.recorded();
        assert_eq!(recorded[0].1, "/api/status");
    }
}
