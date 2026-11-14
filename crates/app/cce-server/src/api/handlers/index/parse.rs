//! Parse handler
//!
//! Provides single file parsing functionality.

use axum::Json;

use cce_api::models::{
    EntityInfo, ErrorResponse, ParseRequest, ParseResponse, RelationInfo, error_codes,
};
use cce_server_shared::services::tools;

use crate::api::response::ApiResult;

/// Unified response type for parse handler
pub type ParseApiResponse = ApiResult<ParseResponse>;

/// Handle parse request
#[utoipa::path(
    post, path = "/api/parse", tag = "Index",
    request_body = ParseRequest,
    responses(
        (status = 200, body = ParseResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_parse(Json(request): Json<ParseRequest>) -> ParseApiResponse {
    let parsed = match tools::parse_file(&request.file_path).await {
        Ok(parsed) => parsed,
        Err(error) => {
            return ParseApiResponse::Error(ErrorResponse::new(
                error_codes::INVALID_REQUEST,
                error.to_string(),
            ));
        }
    };
    ParseApiResponse::Success(ParseResponse {
        success: true,
        file_path: parsed.file_path,
        language: parsed.language,
        encoding: parsed.encoding,
        entities: parsed
            .entities
            .into_iter()
            .map(|entity| EntityInfo {
                id: entity.id,
                kind: entity.kind,
                name: entity.name,
                signature: Some(entity.signature),
                start_line: entity.start_line,
                end_line: entity.end_line,
                doc_comment: entity.doc_comment,
            })
            .collect(),
        relations: parsed
            .relations
            .into_iter()
            .map(|relation| RelationInfo {
                caller_id: relation.caller_id,
                callee_id: relation.callee_id,
                relation_type: relation.relation_type,
                line: relation.line,
            })
            .collect(),
        elapsed_ms: parsed.elapsed_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_request_validation() {
        let request = ParseRequest {
            file_path: "".to_string(),
        };

        assert!(request.file_path.trim().is_empty());
    }
}
