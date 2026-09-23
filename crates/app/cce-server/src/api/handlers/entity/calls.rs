//! Function calls handlers
//!
//! Provides function callees and callers query functionality.

use axum::{
    Json,
    extract::{Path, Query as QueryParams, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};

use cce_orchestrator::query::RelationQueryOptions;
use cce_relation::index::snapshot_query::{SnapshotEntityQueryOps, SnapshotSymbolQueryOps};

use cce_api::models::{
    CallChainNode, ErrorResponse, FunctionCallersResponse, FunctionCallsResponse, error_codes,
};

/// Unified response enum for calls handlers
#[derive(Serialize)]
#[serde(untagged)]
pub enum CallsApiResponse {
    Error(ErrorResponse),
    Calls(FunctionCallsResponse),
    Callers(FunctionCallersResponse),
}

impl IntoResponse for CallsApiResponse {
    fn into_response(self) -> axum::response::Response {
        match self {
            CallsApiResponse::Error(err) => {
                let status = if err.error.code == error_codes::INVALID_REQUEST {
                    StatusCode::BAD_REQUEST
                } else {
                    StatusCode::INTERNAL_SERVER_ERROR
                };
                (status, Json(err)).into_response()
            }
            CallsApiResponse::Calls(resp) => (StatusCode::OK, Json(resp)).into_response(),
            CallsApiResponse::Callers(resp) => (StatusCode::OK, Json(resp)).into_response(),
        }
    }
}

/// File-scope filter parameters shared by relation query endpoints.
/// Flattened into each query params struct.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RelationFilterParams {
    /// Exclude entities located in test files
    #[serde(default)]
    pub exclude_tests: Option<bool>,
    /// Keep only entities under this directory prefix
    #[serde(default)]
    pub directory_prefix: Option<String>,
    /// Exact file paths to exclude
    #[serde(default)]
    pub excluded_files: Option<Vec<String>>,
}

impl RelationFilterParams {
    /// Apply the HTTP filter parameters onto query options.
    pub fn apply(self, options: RelationQueryOptions) -> RelationQueryOptions {
        let mut options = options.with_exclude_tests(self.exclude_tests.unwrap_or(false));
        if let Some(prefix) = self.directory_prefix {
            options = options.with_directory_prefix(prefix);
        }
        if let Some(files) = self.excluded_files {
            options = options.with_excluded_files(files);
        }
        options
    }
}

/// Query parameters for call chain queries
#[derive(Debug, Deserialize)]
pub struct CallChainQueryParams {
    #[serde(default = "default_max_depth")]
    pub max_depth: usize,
    #[serde(default)]
    pub offset: Option<usize>,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(flatten)]
    pub filter: RelationFilterParams,
}

fn default_max_depth() -> usize {
    3
}

fn default_limit() -> usize {
    20
}

/// Return the relation capability info as a JSON map when the served
/// snapshot is stale (runtime degraded or updating); `None` when the
/// snapshot is fresh. Lets the API layer hint that the answered data may
/// lag the latest published epoch.
async fn stale_relation_info(
    runtime: &crate::runtime::RelationRuntime,
) -> Option<serde_json::Value> {
    let info = runtime.get_capability_info().await;
    info.stale.then(|| info.to_json_map())
}

impl From<CallChainQueryParams> for RelationQueryOptions {
    fn from(params: CallChainQueryParams) -> Self {
        params.filter.apply(
            RelationQueryOptions::new()
                .with_max_depth(params.max_depth)
                .with_offset(params.offset.unwrap_or(0))
                .with_limit(params.limit),
        )
    }
}

/// Handle function calls request (get callees)
pub async fn handle_function_calls(
    State(state): State<crate::api::state::AppState>,
    Path((project_id, id)): Path<(i64, String)>,
    QueryParams(params): QueryParams<CallChainQueryParams>,
) -> CallsApiResponse {
    // Validate project_id
    if project_id <= 0 {
        return CallsApiResponse::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "Invalid project_id".to_string(),
        ));
    }

    let options: RelationQueryOptions = params.into();

    // Get relation runtime for this project
    let runtime = match state.engine.get_relation_runtime(project_id).await {
        Ok(rt) => rt,
        Err(e) => {
            return CallsApiResponse::Error(ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                format!("Failed to get relation runtime: {}", e),
            ));
        }
    };

    // Check if runtime can serve queries
    if !runtime.can_serve_queries().await {
        let info = runtime.get_capability_info().await;
        return CallsApiResponse::Error(ErrorResponse::new(
            error_codes::SERVICE_UNAVAILABLE,
            format!(
                "Relation index not available: {:?}, epoch: {}",
                info.state, info.relation_epoch
            ),
        ));
    }

    // Get snapshot for epoch / symbol lookup
    let snapshot = match runtime.get_snapshot().await {
        Some(s) => s,
        None => {
            return CallsApiResponse::Error(ErrorResponse::new(
                error_codes::SERVICE_UNAVAILABLE,
                "No relation snapshot available".to_string(),
            ));
        }
    };

    let entity_id = match snapshot.index.get_entity_id_by_stable_symbol_id(&id) {
        Some(eid) => eid,
        None => {
            // Try parsing as numeric ID for backwards compatibility
            if let Ok(numeric_id) = id.parse::<u64>() {
                cce_types::EntityId(numeric_id)
            } else {
                return CallsApiResponse::Error(ErrorResponse::new(
                    error_codes::INVALID_REQUEST,
                    "Unknown stable symbol ID".to_string(),
                ));
            }
        }
    };

    // Use cached RelationSearcher (LRU) instead of per-request CallChainQuery
    let searcher = match state.get_relation_searcher(project_id).await {
        Ok(s) => s,
        Err(e) => {
            return CallsApiResponse::Error(ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                format!("Failed to get relation searcher: {}", e),
            ));
        }
    };
    let filtered_callees: Vec<cce_types::ResolvedRelation> =
        searcher.filter_callees(entity_id, &options);
    // Total after filtering, before pagination
    let total = filtered_callees.len();
    let resolved_callees: Vec<cce_types::ResolvedRelation> = filtered_callees
        .into_iter()
        .skip(options.offset)
        .take(options.limit)
        .collect();
    // Convert ResolvedRelation to CallChainNode
    let callees: Vec<CallChainNode> = resolved_callees
        .into_iter()
        .map(|r| {
            // Try to get callee info from relation index
            let (function_name, file_path) = if let Some(callee_id) = r.callee_id {
                searcher
                    .query()
                    .index()
                    .get_function_by_entity_id(callee_id)
                    .map(|entity| {
                        let path = searcher
                            .query()
                            .index()
                            .get_file_path_by_entity(callee_id)
                            .unwrap_or_else(|| "Unknown".to_string());
                        (entity.name.clone(), path)
                    })
                    .unwrap_or_else(|| ("Unknown".to_string(), "Unknown".to_string()))
            } else {
                ("Unknown".to_string(), "Unknown".to_string())
            };

            CallChainNode {
                function_id: r
                    .callee_id
                    .and_then(|callee_id| {
                        searcher
                            .query()
                            .index()
                            .get_symbol_key_by_entity_id(callee_id)
                    })
                    .map(|key| key.stable_id().0)
                    .unwrap_or_default(),
                function_name,
                file_path,
                depth: 0,
                relation_type: format!("{:?}", r.relation_type),
                call_line: None,
            }
        })
        .collect();

    // Get caller function name
    let function_name = searcher
        .query()
        .index()
        .get_function_by_entity_id(entity_id)
        .map(|entity| entity.name.clone())
        .unwrap_or_else(|| "Unknown".to_string());

    let response = FunctionCallsResponse {
        success: true,
        relation_epoch: snapshot.relation_epoch,
        function_id: id,
        function_name,
        callees,
        total_callees: total,
        relation_info: stale_relation_info(&runtime).await,
    };

    CallsApiResponse::Calls(response)
}

/// Handle function callers request
pub async fn handle_function_callers(
    State(state): State<crate::api::state::AppState>,
    Path((project_id, id)): Path<(i64, String)>,
    QueryParams(params): QueryParams<CallChainQueryParams>,
) -> CallsApiResponse {
    // Validate project_id
    if project_id <= 0 {
        return CallsApiResponse::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "Invalid project_id".to_string(),
        ));
    }

    let options: RelationQueryOptions = params.into();

    // Get relation runtime for this project
    let runtime = match state.engine.get_relation_runtime(project_id).await {
        Ok(rt) => rt,
        Err(e) => {
            return CallsApiResponse::Error(ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                format!("Failed to get relation runtime: {}", e),
            ));
        }
    };

    // Check if runtime can serve queries
    if !runtime.can_serve_queries().await {
        let info = runtime.get_capability_info().await;
        return CallsApiResponse::Error(ErrorResponse::new(
            error_codes::SERVICE_UNAVAILABLE,
            format!(
                "Relation index not available: {:?}, epoch: {}",
                info.state, info.relation_epoch
            ),
        ));
    }

    // Get snapshot for stable-id lookup and epoch
    let snapshot = match runtime.get_snapshot().await {
        Some(s) => s,
        None => {
            return CallsApiResponse::Error(ErrorResponse::new(
                error_codes::SERVICE_UNAVAILABLE,
                "No relation snapshot available".to_string(),
            ));
        }
    };

    let entity_id = match snapshot.index.get_entity_id_by_stable_symbol_id(&id) {
        Some(eid) => eid,
        None => {
            // Try parsing as numeric ID for backwards compatibility
            if let Ok(numeric_id) = id.parse::<u64>() {
                cce_types::EntityId(numeric_id)
            } else {
                return CallsApiResponse::Error(ErrorResponse::new(
                    error_codes::INVALID_REQUEST,
                    "Unknown stable symbol ID".to_string(),
                ));
            }
        }
    };
    let searcher = match state.get_relation_searcher(project_id).await {
        Ok(s) => s,
        Err(e) => {
            return CallsApiResponse::Error(ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                format!("Failed to get relation searcher: {}", e),
            ));
        }
    };
    let filtered_callers: Vec<cce_types::EntityId> = searcher.filter_callers(entity_id, &options);
    let total_callers = filtered_callers.len();
    let caller_ids: Vec<cce_types::EntityId> = filtered_callers
        .into_iter()
        .skip(options.offset)
        .take(options.limit)
        .collect();

    // Convert EntityId to CallChainNode
    let callers: Vec<CallChainNode> = caller_ids
        .into_iter()
        .map(|id| {
            let (function_name, file_path) = searcher
                .query()
                .index()
                .get_function_by_entity_id(id)
                .map(|entity| {
                    let path = searcher
                        .query()
                        .index()
                        .get_file_path_by_entity(id)
                        .unwrap_or_else(|| "Unknown".to_string());
                    (entity.name.clone(), path)
                })
                .unwrap_or_else(|| ("Unknown".to_string(), "Unknown".to_string()));

            CallChainNode {
                function_id: searcher
                    .query()
                    .index()
                    .get_symbol_key_by_entity_id(id)
                    .map(|key| key.stable_id().0)
                    .unwrap_or_default(),
                function_name,
                file_path,
                depth: 0,
                relation_type: "caller".to_string(),
                call_line: None,
            }
        })
        .collect();

    // Get callee function name
    let function_name = searcher
        .query()
        .index()
        .get_function_by_entity_id(entity_id)
        .map(|entity| entity.name.clone())
        .unwrap_or_else(|| "Unknown".to_string());

    let response = FunctionCallersResponse {
        success: true,
        relation_epoch: snapshot.relation_epoch,
        function_id: id,
        function_name,
        callers,
        total_callers,
        relation_info: stale_relation_info(&runtime).await,
    };

    CallsApiResponse::Callers(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_max_depth() {
        assert_eq!(default_max_depth(), 3);
    }

    #[test]
    fn test_params_to_options() {
        let params = CallChainQueryParams {
            max_depth: 5,
            offset: Some(10),
            limit: 50,
            filter: RelationFilterParams {
                exclude_tests: Some(true),
                directory_prefix: Some("src/flask".to_string()),
                excluded_files: Some(vec!["src/flask/app.py".to_string()]),
            },
        };
        let opts: RelationQueryOptions = params.into();
        assert_eq!(opts.max_depth, 5);
        assert_eq!(opts.offset, 10);
        assert_eq!(opts.limit, 50);
        assert!(
            opts.exclude_content_types
                .contains(&cce_orchestrator::query::ExcludableContentType::Test)
        );
        assert_eq!(opts.directory_prefix.as_deref(), Some("src/flask"));
        assert_eq!(opts.excluded_files.len(), 1);
    }
}
