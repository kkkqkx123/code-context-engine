//! Symbol lookup tool handlers
//!
//! Provides LSP-like functionality using the internal relation index:
//! - Find all references to a symbol
//! - Get all symbols in a file
//! - Jump to the definition of a symbol
//!
//! Responses use the shared cce-api wire models: the orchestrator result is
//! carried in the `result` field and `relation_info` reports the relation
//! capability state, keeping the response shape consistent with the frontend.

use axum::{Json, extract::State};

use super::to_api_model;
use cce_orchestrator::{
    FindReferencesResponse as OrchFindReferencesResponse, FindReferencesTool,
    GetSymbolsResponse as OrchGetSymbolsResponse, GetSymbolsTool,
    GotoDefinitionResponse as OrchGotoDefinitionResponse, GotoDefinitionTool,
};

use crate::api::AppState;

// ============================================================================
// Find References
// ============================================================================

/// Handle find references
///
/// # Endpoint
///
/// `POST /api/tools/references`
#[utoipa::path(
    post, path = "/api/tools/references", tag = "Tools",
    request_body = cce_api::models::FindReferencesRequest,
    responses(
        (status = 200, body = cce_api::models::FindReferencesResponse, description = "References result, errors reported in-band")
    )
)]
pub async fn handle_find_references(
    State(state): State<AppState>,
    Json(request): Json<cce_api::models::FindReferencesRequest>,
) -> Json<cce_api::models::FindReferencesResponse> {
    // Validate project_id
    if request.project_id <= 0 {
        return Json(cce_api::models::FindReferencesResponse {
            success: false,
            result: None,
            error: Some("Invalid project_id".to_string()),
            relation_info: None,
        });
    }

    // Get relation runtime for this project
    let runtime = match state.engine.get_relation_runtime(request.project_id).await {
        Ok(rt) => rt,
        Err(e) => {
            return Json(cce_api::models::FindReferencesResponse {
                success: false,
                result: None,
                error: Some(format!("Failed to get relation runtime: {}", e)),
                relation_info: None,
            });
        }
    };

    // Check if runtime can serve queries
    if !runtime.can_serve_queries().await {
        let info = runtime.get_capability_info().await;
        return Json(cce_api::models::FindReferencesResponse {
            success: false,
            result: None,
            error: Some(format!("Relation index not available: {:?}", info.state)),
            relation_info: Some(info.to_json_map()),
        });
    }

    // Get snapshot
    let snapshot = match runtime.get_snapshot().await {
        Some(s) => s,
        None => {
            return Json(cce_api::models::FindReferencesResponse {
                success: false,
                result: None,
                error: Some("No relation snapshot available".to_string()),
                relation_info: None,
            });
        }
    };

    // Zero-copy: the tool shares the published snapshot's maps.
    let index = snapshot.index.clone();
    let mut tool = FindReferencesTool::new(index, request.project_id);
    if let Some(sqlite) = state.metadata_store.as_ref()
        && let Ok(project) = sqlite.for_project(request.project_id)
    {
        tool = tool.with_sqlite(project);
    }
    let req = cce_orchestrator::FindReferencesRequest {
        path: request.path,
        line: request.line,
        column: request.column,
        symbol: request.symbol,
        context_lines: request.context_lines,
        include_snippet: request.include_snippet,
        include_entity_info: request.include_entity_info,
    };

    let capability_info = runtime.get_capability_info().await;

    match tool.find_references(req) {
        Ok(response) => {
            let result = match to_api_model::<OrchFindReferencesResponse, _>(response) {
                Ok(model) => Some(model),
                Err(e) => {
                    return Json(cce_api::models::FindReferencesResponse {
                        success: false,
                        result: None,
                        error: Some(format!("Failed to serialize response: {}", e)),
                        relation_info: Some(capability_info.to_json_map()),
                    });
                }
            };
            Json(cce_api::models::FindReferencesResponse {
                success: true,
                result,
                error: None,
                relation_info: Some(capability_info.to_json_map()),
            })
        }
        Err(e) => Json(cce_api::models::FindReferencesResponse {
            success: false,
            result: None,
            error: Some(e.to_string()),
            relation_info: Some(capability_info.to_json_map()),
        }),
    }
}

// ============================================================================
// Get Symbols
// ============================================================================

/// Handle get symbols
///
/// # Endpoint
///
/// `POST /api/tools/symbols`
#[utoipa::path(
    post, path = "/api/tools/symbols", tag = "Tools",
    request_body = cce_api::models::GetSymbolsRequest,
    responses(
        (status = 200, body = cce_api::models::GetSymbolsResponse, description = "Symbols result, errors reported in-band")
    )
)]
pub async fn handle_get_symbols(
    State(state): State<AppState>,
    Json(request): Json<cce_api::models::GetSymbolsRequest>,
) -> Json<cce_api::models::GetSymbolsResponse> {
    // Validate project_id
    if request.project_id <= 0 {
        return Json(cce_api::models::GetSymbolsResponse {
            success: false,
            result: None,
            error: Some("Invalid project_id".to_string()),
            relation_info: None,
        });
    }

    // Get relation runtime for this project
    let runtime = match state.engine.get_relation_runtime(request.project_id).await {
        Ok(rt) => rt,
        Err(e) => {
            return Json(cce_api::models::GetSymbolsResponse {
                success: false,
                result: None,
                error: Some(format!("Failed to get relation runtime: {}", e)),
                relation_info: None,
            });
        }
    };

    // Check if runtime can serve queries
    if !runtime.can_serve_queries().await {
        let info = runtime.get_capability_info().await;
        return Json(cce_api::models::GetSymbolsResponse {
            success: false,
            result: None,
            error: Some(format!("Relation index not available: {:?}", info.state)),
            relation_info: Some(info.to_json_map()),
        });
    }

    // Get snapshot
    let snapshot = match runtime.get_snapshot().await {
        Some(s) => s,
        None => {
            return Json(cce_api::models::GetSymbolsResponse {
                success: false,
                result: None,
                error: Some("No relation snapshot available".to_string()),
                relation_info: None,
            });
        }
    };

    // Zero-copy: the tool shares the published snapshot's maps.
    let index = snapshot.index.clone();
    let tool = GetSymbolsTool::new(index);
    let req = cce_orchestrator::GetSymbolsRequest {
        paths: request.paths,
    };

    let capability_info = runtime.get_capability_info().await;

    match tool.get_symbols(req) {
        Ok(response) => {
            let result = match to_api_model::<OrchGetSymbolsResponse, _>(response) {
                Ok(model) => Some(model),
                Err(e) => {
                    return Json(cce_api::models::GetSymbolsResponse {
                        success: false,
                        result: None,
                        error: Some(format!("Failed to serialize response: {}", e)),
                        relation_info: Some(capability_info.to_json_map()),
                    });
                }
            };
            Json(cce_api::models::GetSymbolsResponse {
                success: true,
                result,
                error: None,
                relation_info: Some(capability_info.to_json_map()),
            })
        }
        Err(e) => Json(cce_api::models::GetSymbolsResponse {
            success: false,
            result: None,
            error: Some(e.to_string()),
            relation_info: Some(capability_info.to_json_map()),
        }),
    }
}

// ============================================================================
// Goto Definition
// ============================================================================

/// Handle goto definition
///
/// # Endpoint
///
/// `POST /api/tools/definition`
#[utoipa::path(
    post, path = "/api/tools/definition", tag = "Tools",
    request_body = cce_api::models::GotoDefinitionRequest,
    responses(
        (status = 200, body = cce_api::models::GotoDefinitionResponse, description = "Definition result, errors reported in-band")
    )
)]
pub async fn handle_goto_definition(
    State(state): State<AppState>,
    Json(request): Json<cce_api::models::GotoDefinitionRequest>,
) -> Json<cce_api::models::GotoDefinitionResponse> {
    // Validate project_id
    if request.project_id <= 0 {
        return Json(cce_api::models::GotoDefinitionResponse {
            success: false,
            result: None,
            error: Some("Invalid project_id".to_string()),
            relation_info: None,
        });
    }

    // Get relation runtime for this project
    let runtime = match state.engine.get_relation_runtime(request.project_id).await {
        Ok(rt) => rt,
        Err(e) => {
            return Json(cce_api::models::GotoDefinitionResponse {
                success: false,
                result: None,
                error: Some(format!("Failed to get relation runtime: {}", e)),
                relation_info: None,
            });
        }
    };

    // Check if runtime can serve queries
    if !runtime.can_serve_queries().await {
        let info = runtime.get_capability_info().await;
        return Json(cce_api::models::GotoDefinitionResponse {
            success: false,
            result: None,
            error: Some(format!("Relation index not available: {:?}", info.state)),
            relation_info: Some(info.to_json_map()),
        });
    }

    // Get snapshot
    let snapshot = match runtime.get_snapshot().await {
        Some(s) => s,
        None => {
            return Json(cce_api::models::GotoDefinitionResponse {
                success: false,
                result: None,
                error: Some("No relation snapshot available".to_string()),
                relation_info: None,
            });
        }
    };

    // Zero-copy: the tool shares the published snapshot's maps.
    let index = snapshot.index.clone();
    let mut tool = GotoDefinitionTool::new(index, request.project_id);
    if let Some(sqlite) = state.metadata_store.as_ref()
        && let Ok(project) = sqlite.for_project(request.project_id)
    {
        tool = tool.with_sqlite(project);
    }
    let req = cce_orchestrator::GotoDefinitionRequest {
        path: request.path,
        line: request.line,
        column: request.column,
        symbol: request.symbol,
        include_body: request.include_body,
    };

    let capability_info = runtime.get_capability_info().await;

    match tool.goto_definition(req) {
        Ok(response) => {
            let result = match to_api_model::<OrchGotoDefinitionResponse, _>(response) {
                Ok(model) => Some(model),
                Err(e) => {
                    return Json(cce_api::models::GotoDefinitionResponse {
                        success: false,
                        result: None,
                        error: Some(format!("Failed to serialize response: {}", e)),
                        relation_info: Some(capability_info.to_json_map()),
                    });
                }
            };
            Json(cce_api::models::GotoDefinitionResponse {
                success: true,
                result,
                error: None,
                relation_info: Some(capability_info.to_json_map()),
            })
        }
        Err(e) => Json(cce_api::models::GotoDefinitionResponse {
            success: false,
            result: None,
            error: Some(e.to_string()),
            relation_info: Some(capability_info.to_json_map()),
        }),
    }
}
