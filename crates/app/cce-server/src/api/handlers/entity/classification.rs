//! Classification query handlers
//!
//! Provides endpoints for querying resolved relations by external call
//! classification (stdlib, external, dev, local) and classification statistics.

use axum::extract::{Path, Query, State};

use cce_api::models::{
    ClassificationQueryParams, ClassificationStatsResponse, ErrorResponse, error_codes,
};

use crate::api::response::{ApiResult, relation_error};
use cce_server_shared::services::relation::{self, ClassificationCategory};

/// GET /api/project/{project_id}/relations/classification/{classification}
///
/// Returns all resolved relations whose `external_type` matches the given
/// classification category (stdlib, external, dev, local, unknown).
#[utoipa::path(
    get, path = "/api/project/{project_id}/relations/classification/{classification}", tag = "Entity",
    params(ClassificationQueryParams, ("project_id" = i64, Path, description = "Project id"), ("classification" = String, Path, description = "Relation classification")),
    responses(
        (status = 200, description = "Matched relations as untyped JSON array"), (status = 400, body = ErrorResponse, description = "Invalid request"), (status = 404, body = ErrorResponse, description = "Resource not found"), (status = 503, body = ErrorResponse, description = "Index unavailable"), (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn get_relations_by_classification(
    State(state): State<cce_server_shared::context::SharedContext>,
    Path((project_id, classification)): Path<(i64, String)>,
    Query(params): Query<ClassificationQueryParams>,
) -> ApiResult<Vec<serde_json::Value>> {
    let Some(category) = ClassificationCategory::parse(&classification) else {
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            format!(
                "Unknown classification '{}'. Valid: stdlib, external, dev, local, unknown",
                classification
            ),
        ));
    };
    let limit = params.limit.unwrap_or(1000);
    match relation::relations_by_classification(&state, project_id, category, limit).await {
        Ok(relations) => ApiResult::Success(
            relations
                .into_iter()
                .filter_map(|relation| serde_json::to_value(&relation).ok())
                .collect(),
        ),
        Err(error) => ApiResult::Error(relation_error(error)),
    }
}

/// GET /api/project/{project_id}/relations/classification/stats
///
/// Returns counts of resolved relations grouped by external call classification.
#[utoipa::path(
    get, path = "/api/project/{project_id}/relations/classification/stats", tag = "Entity",
    params(("project_id" = i64, Path, description = "Project id")),
    responses(
        (status = 200, body = ClassificationStatsResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 503, body = ErrorResponse, description = "Index unavailable"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn get_classification_stats(
    State(state): State<cce_server_shared::context::SharedContext>,
    Path(project_id): Path<i64>,
) -> ApiResult<ClassificationStatsResponse> {
    match relation::classification_stats(&state, project_id).await {
        Ok(stats) => ApiResult::Success(ClassificationStatsResponse {
            stats: stats.stats,
            total: stats.total,
        }),
        Err(error) => ApiResult::Error(relation_error(error)),
    }
}
