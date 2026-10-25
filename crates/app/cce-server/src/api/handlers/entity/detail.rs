//! Function detail handler
//!
//! Provides function detail query functionality.

use axum::extract::{Path, State};

use cce_codegraph::index::snapshot_query::SnapshotEntityQueryOps;

use cce_api::models::{ErrorResponse, FunctionDetailResponse, FunctionInfo, error_codes};

use super::seed::resolve_symbol_seed;
use crate::api::response::ApiResult;

/// Unified response type for detail handler
pub type DetailApiResponse = ApiResult<FunctionDetailResponse>;

/// Handle function detail request
///
/// The `id` parameter is a stable symbol ID (string), consistent with
/// the calls/callers/call-chain/class endpoints.
#[utoipa::path(
    get, path = "/api/project/{project_id}/function/{id}", tag = "Entity",
    params(("project_id" = i64, Path, description = "Project id"), ("id" = String, Path, description = "Function id")),
    responses(
        (status = 200, body = FunctionDetailResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 503, body = ErrorResponse, description = "Index unavailable"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_function_detail(
    State(state): State<cce_server_shared::context::SharedContext>,
    Path((project_id, id)): Path<(i64, String)>,
) -> DetailApiResponse {
    // Validate project_id
    if project_id <= 0 {
        return DetailApiResponse::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "Invalid project_id".to_string(),
        ));
    }

    // Get relation runtime for this project
    let runtime = match state.engine.get_relation_runtime(project_id).await {
        Ok(rt) => rt,
        Err(e) => {
            return DetailApiResponse::Error(ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                format!("Failed to get relation runtime: {}", e),
            ));
        }
    };

    // Check if runtime can serve queries
    if !runtime.can_serve_queries().await {
        let info = runtime.get_capability_info().await;
        return DetailApiResponse::Error(ErrorResponse::new(
            error_codes::SERVICE_UNAVAILABLE,
            format!(
                "Relation index not available: {:?}, epoch: {}",
                info.state, info.relation_epoch
            ),
        ));
    }

    // Get snapshot
    let snapshot = match runtime.get_snapshot().await {
        Some(s) => s,
        None => {
            return DetailApiResponse::Error(ErrorResponse::new(
                error_codes::SERVICE_UNAVAILABLE,
                "No relation snapshot available".to_string(),
            ));
        }
    };

    let entity_id = match resolve_symbol_seed(snapshot.index.as_ref(), &id) {
        Ok(eid) => eid,
        Err(error) => return DetailApiResponse::Error(error),
    };

    // Get entity from function index (zero-copy read of the shared snapshot)
    if let Some(entity) = snapshot.index.get_function_by_entity_id(entity_id) {
        let file_path = snapshot
            .index
            .get_file_path_by_entity(entity_id)
            .unwrap_or_default();
        let function_info = FunctionInfo {
            id: id.clone(),
            name: entity.name.clone(),
            signature: entity.signature.clone(),
            parameters: Vec::new(),
            return_type: None,
            file_path,
            start_line: entity.span.start_position.row as u32,
            end_line: entity.span.end_position.row as u32,
            doc_comment: entity.doc_comment.clone(),
        };
        let response = FunctionDetailResponse {
            success: true,
            function: function_info,
            relation_info: stale_relation_info(&runtime).await,
        };
        return DetailApiResponse::Success(response);
    }
    // Snapshot miss: missing entities are reported as not found instead of a
    // placeholder success so callers can distinguish absence from data.
    DetailApiResponse::Error(ErrorResponse::new(
        error_codes::ENTITY_NOT_FOUND,
        format!("Unknown symbol seed: {id}"),
    ))
}

/// Return the relation capability info as a JSON map when the served
/// snapshot is stale (runtime degraded or updating); `None` when the
/// snapshot is fresh. Lets the API layer hint that the answered data may
/// lag the latest published epoch.
async fn stale_relation_info(
    runtime: &cce_server_shared::runtime::RelationRuntime,
) -> Option<serde_json::Value> {
    let info = runtime.get_capability_info().await;
    info.stale.then(|| info.to_json_map())
}

#[cfg(test)]
mod tests {
    use cce_types::EntityId;

    #[test]
    fn test_entity_id_from_str_with_prefix() {
        assert_eq!(EntityId::from_str_with_prefix("123"), Ok(EntityId(123)));
        assert_eq!(
            EntityId::from_str_with_prefix("entity:456"),
            Ok(EntityId(456))
        );
        assert!(EntityId::from_str_with_prefix("invalid").is_err());
    }

    #[test]
    fn test_stable_symbol_id_parsing() {
        // Numeric string should parse as fallback
        assert_eq!("123".parse::<u64>(), Ok(123));
        // Non-numeric strings should fail numeric parse
        assert!("snapshot-local:123".parse::<u64>().is_err());
    }
}
