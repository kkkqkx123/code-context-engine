//! Classification query handlers
//!
//! Provides endpoints for querying resolved relations by external call
//! classification (stdlib, external, dev, local) and classification statistics.

use axum::extract::{Path, Query, State};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use cce_relation::index::snapshot_query::SnapshotRelationQueryOps;
use cce_types::ExternalCallType;

use cce_api::models::{ErrorResponse, error_codes};

use crate::api::response::ApiResult;
use crate::runtime::PublishedSnapshot;

/// Query parameters for classification filtering
#[derive(Debug, Deserialize)]
pub struct ClassificationQueryParams {
    /// Maximum number of results (default 1000)
    pub limit: Option<usize>,
}

/// Response for classification statistics
#[derive(Debug, Serialize)]
pub struct ClassificationStatsResponse {
    /// Per-classification counts
    pub stats: HashMap<String, usize>,
    /// Total number of classified relations
    pub total: usize,
}

/// Get a relation snapshot, or return an `ErrorResponse`.
async fn get_snapshot_or_error(
    state: &crate::api::state::AppState,
    project_id: i64,
) -> Result<Arc<PublishedSnapshot>, ErrorResponse> {
    if project_id <= 0 {
        return Err(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "Invalid project_id".to_string(),
        ));
    }

    let runtime = state
        .engine
        .get_relation_runtime(project_id)
        .await
        .map_err(|e| {
            ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                format!("Failed to get relation runtime: {}", e),
            )
        })?;

    if !runtime.can_serve_queries().await {
        let info = runtime.get_capability_info().await;
        return Err(ErrorResponse::new(
            error_codes::SERVICE_UNAVAILABLE,
            format!(
                "Relation index not available: {:?}, epoch: {}",
                info.state, info.relation_epoch
            ),
        ));
    }

    runtime.get_snapshot().await.ok_or_else(|| {
        ErrorResponse::new(
            error_codes::SERVICE_UNAVAILABLE,
            "No relation snapshot available".to_string(),
        )
    })
}

/// Parse an `ExternalCallType` label from a URL path segment.
///
/// The match is case-insensitive. Returns `Err` for unrecognized labels.
fn classify_label(s: &str) -> Result<ExternalCallType, String> {
    match s.to_ascii_lowercase().as_str() {
        "stdlib" | "std" => Ok(ExternalCallType::StandardLibrary {
            library: String::new(),
        }),
        "external" | "ext" | "third_party" => Ok(ExternalCallType::ExternalLibrary {
            package: String::new(),
        }),
        "dev" | "development" => Ok(ExternalCallType::DevDependency {
            package: String::new(),
        }),
        "local" | "path" => Ok(ExternalCallType::LocalDependency {
            package: String::new(),
        }),
        "unknown" | "other" => Ok(ExternalCallType::Unknown {
            raw_target: String::new(),
        }),
        _ => Err(format!(
            "Unknown classification '{}'. Valid: stdlib, external, dev, local, unknown",
            s
        )),
    }
}

/// Returns true when `relation.external_type` matches the given classification
/// category, ignoring the inner data fields.
fn matches_classification(
    relation_external: Option<&ExternalCallType>,
    target: &ExternalCallType,
) -> bool {
    use std::mem::discriminant;
    match (relation_external, target) {
        (Some(a), b) => discriminant(a) == discriminant(b),
        _ => false,
    }
}

/// GET /api/project/{project_id}/relations/classification/{classification}
///
/// Returns all resolved relations whose `external_type` matches the given
/// classification category (stdlib, external, dev, local, unknown).
pub async fn get_relations_by_classification(
    State(state): State<crate::api::state::AppState>,
    Path((project_id, classification)): Path<(i64, String)>,
    Query(params): Query<ClassificationQueryParams>,
) -> ApiResult<Vec<serde_json::Value>> {
    let snapshot = match get_snapshot_or_error(&state, project_id).await {
        Ok(s) => s,
        Err(e) => return ApiResult::Error(e),
    };

    let target = match classify_label(&classification) {
        Ok(t) => t,
        Err(msg) => {
            return ApiResult::Error(ErrorResponse::new(error_codes::INVALID_REQUEST, msg));
        }
    };

    let limit = params.limit.unwrap_or(1000);
    let relations = snapshot.index.get_relations_by_classification(&target);

    let results: Vec<serde_json::Value> = relations
        .into_iter()
        .filter(|r| matches_classification(r.external_type.as_ref(), &target))
        .take(limit)
        .filter_map(|r| serde_json::to_value(&r).ok())
        .collect();

    ApiResult::Success(results)
}

/// GET /api/project/{project_id}/relations/classification/stats
///
/// Returns counts of resolved relations grouped by external call classification.
pub async fn get_classification_stats(
    State(state): State<crate::api::state::AppState>,
    Path(project_id): Path<i64>,
) -> ApiResult<ClassificationStatsResponse> {
    let snapshot = match get_snapshot_or_error(&state, project_id).await {
        Ok(s) => s,
        Err(e) => return ApiResult::Error(e),
    };

    let stats_map = snapshot.index.get_classification_stats();
    let total: usize = stats_map.values().sum();

    let stats: HashMap<String, usize> = stats_map
        .into_iter()
        .map(|(k, v)| {
            let label = match k {
                ExternalCallType::StandardLibrary { .. } => "stdlib",
                ExternalCallType::ExternalLibrary { .. } => "external",
                ExternalCallType::DevDependency { .. } => "dev",
                ExternalCallType::LocalDependency { .. } => "local",
                ExternalCallType::Unknown { .. } => "unknown",
            };
            (label.to_string(), v)
        })
        .collect();

    ApiResult::Success(ClassificationStatsResponse { stats, total })
}
