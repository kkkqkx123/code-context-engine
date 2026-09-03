//! Function detail handler
//!
//! Provides function detail query functionality.

use axum::extract::{Path, State};

use cce_relation::index::snapshot_query::{SnapshotEntityQueryOps, SnapshotSymbolQueryOps};

use cce_api::models::{ErrorResponse, FunctionDetailResponse, FunctionInfo, error_codes};

use crate::api::response::ApiResult;

/// Unified response type for detail handler
pub type DetailApiResponse = ApiResult<FunctionDetailResponse>;

/// Handle function detail request
///
/// The `id` parameter is a stable symbol ID (string), consistent with
/// the calls/callers/call-chain/class endpoints.
pub async fn handle_function_detail(
    State(state): State<crate::api::state::AppState>,
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

    // Resolve stable symbol ID to entity_id
    // This is the same resolution path used by calls.rs
    let entity_id = match snapshot.index.get_entity_id_by_stable_symbol_id(&id) {
        Some(eid) => eid,
        None => {
            // Try parsing as numeric ID for backwards compatibility
            if let Ok(numeric_id) = id.parse::<u64>() {
                cce_types::EntityId(numeric_id)
            } else {
                return DetailApiResponse::Error(ErrorResponse::new(
                    error_codes::INVALID_REQUEST,
                    "Unknown stable symbol ID".to_string(),
                ));
            }
        }
    };

    // Get entity from function index (zero-copy read of the shared snapshot)
    let function_info = snapshot
        .index
        .get_function_by_entity_id(entity_id)
        .map(|entity| {
            let file_path = snapshot
                .index
                .get_file_path_by_entity(entity_id)
                .unwrap_or_default();

            FunctionInfo {
                id: id.clone(),
                name: entity.name.clone(),
                signature: entity.signature.clone(),
                parameters: Vec::new(), // Would need to extract from metadata
                return_type: None,      // Would need to extract from metadata
                file_path,
                start_line: entity.span.start_position.row as u32,
                end_line: entity.span.end_position.row as u32,
                doc_comment: entity.doc_comment.clone(),
            }
        })
        .unwrap_or_else(|| {
            // Entity not found in relation index, try SQLite
            if let Some(client) = state.metadata_store.as_deref()
                && let Ok(project) = client.for_project(project_id)
                && let Ok(numeric_id) = id.parse::<i64>()
            {
                use cce_storage_sqlite::EntityRepository;
                match project.with_transaction(|tx| EntityRepository::get_by_id(tx, numeric_id)) {
                    Ok(Some(record)) => FunctionInfo {
                        id: id.clone(),
                        name: record.name,
                        signature: record.signature.unwrap_or_default(),
                        parameters: Vec::new(),
                        return_type: None,
                        file_path: String::new(), // Would need to join with files table
                        start_line: record.span_start_row.unwrap_or(0) as u32,
                        end_line: record.span_end_row.unwrap_or(0) as u32,
                        doc_comment: None,
                    },
                    _ => FunctionInfo {
                        id: id.clone(),
                        name: "Unknown".to_string(),
                        signature: String::new(),
                        parameters: Vec::new(),
                        return_type: None,
                        file_path: String::new(),
                        start_line: 0,
                        end_line: 0,
                        doc_comment: None,
                    },
                }
            } else {
                FunctionInfo {
                    id: id.clone(),
                    name: "Unknown".to_string(),
                    signature: String::new(),
                    parameters: Vec::new(),
                    return_type: None,
                    file_path: String::new(),
                    start_line: 0,
                    end_line: 0,
                    doc_comment: None,
                }
            }
        });

    let response = FunctionDetailResponse {
        success: true,
        function: function_info,
        relation_info: stale_relation_info(&runtime).await,
    };

    DetailApiResponse::Success(response)
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
