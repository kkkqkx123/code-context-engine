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

use super::{
    find_refs_result_from_orch, get_symbols_result_from_orch, goto_definition_result_from_orch,
};
use cce_server_shared::services::tools;
use cce_server_shared::services::tools::ToolAnswer;

use crate::api::SharedContext;

/// Map a tool answer onto one of the wire response envelopes.
pub(crate) fn answer_fields<T, U>(
    answer: ToolAnswer<T>,
    map: impl FnOnce(T) -> U,
) -> (bool, Option<U>, Option<String>, Option<serde_json::Value>) {
    let ToolAnswer {
        result,
        relation_info,
    } = answer;
    match result {
        Ok(value) => (true, Some(map(value)), None, relation_info),
        Err(error) => (false, None, Some(error), relation_info),
    }
}

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
    State(state): State<SharedContext>,
    Json(request): Json<cce_api::models::FindReferencesRequest>,
) -> Json<cce_api::models::FindReferencesResponse> {
    let answer = tools::find_references(
        &state,
        request.project_id,
        cce_orchestrator::FindReferencesRequest {
            path: request.path,
            line: request.line,
            column: request.column,
            symbol: request.symbol,
            context_lines: request.context_lines,
            include_snippet: request.include_snippet,
            include_entity_info: request.include_entity_info,
        },
    )
    .await;
    let (success, result, error, relation_info) = answer_fields(answer, find_refs_result_from_orch);
    Json(cce_api::models::FindReferencesResponse {
        success,
        result,
        error,
        relation_info,
    })
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
    State(state): State<SharedContext>,
    Json(request): Json<cce_api::models::GetSymbolsRequest>,
) -> Json<cce_api::models::GetSymbolsResponse> {
    let answer = tools::get_symbols(
        &state,
        request.project_id,
        cce_orchestrator::GetSymbolsRequest {
            paths: request.paths,
        },
    )
    .await;
    let (success, result, error, relation_info) =
        answer_fields(answer, get_symbols_result_from_orch);
    Json(cce_api::models::GetSymbolsResponse {
        success,
        result,
        error,
        relation_info,
    })
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
    State(state): State<SharedContext>,
    Json(request): Json<cce_api::models::GotoDefinitionRequest>,
) -> Json<cce_api::models::GotoDefinitionResponse> {
    let answer = tools::goto_definition(
        &state,
        request.project_id,
        cce_orchestrator::GotoDefinitionRequest {
            path: request.path,
            line: request.line,
            column: request.column,
            symbol: request.symbol,
            include_body: request.include_body,
        },
    )
    .await;
    let (success, result, error, relation_info) =
        answer_fields(answer, goto_definition_result_from_orch);
    Json(cce_api::models::GotoDefinitionResponse {
        success,
        result,
        error,
        relation_info,
    })
}
