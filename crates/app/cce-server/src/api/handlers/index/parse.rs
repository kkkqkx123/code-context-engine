//! Parse handler
//!
//! Provides single file parsing functionality.

use axum::Json;
use std::path::Path;

use cce_api::models::{
    EntityInfo, ErrorResponse, ParseRequest, ParseResponse, RelationInfo, error_codes,
};

use cce_parser::parser::ParseCoordinator;

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
pub async fn handle_parse(
    Json(request): Json<ParseRequest>,
) -> ParseApiResponse {
    let start = std::time::Instant::now();

    // Validate file path
    if request.file_path.trim().is_empty() {
        return ParseApiResponse::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "File path cannot be empty",
        ));
    }

    // Read file content with automatic encoding detection
    let (content, detected_encoding) =
        match cce_utils::file::read_file_to_utf8_with_encoding_async(Path::new(&request.file_path))
            .await
        {
            Ok(result) => result,
            Err(e) => {
                return ParseApiResponse::Error(ErrorResponse::new(
                    error_codes::INVALID_REQUEST,
                    format!("Unable to read file: {}", e),
                ));
            }
        };

    // Parse file
    let mut parser = ParseCoordinator::new();
    let parsed = match parser.parse(&request.file_path, &content) {
        Ok(parsed) => parsed,
        Err(e) => {
            return ParseApiResponse::Error(ErrorResponse::new(
                error_codes::PARSE_ERROR,
                e.to_string(),
            ));
        }
    };

    // Convert entities
    let entities: Vec<EntityInfo> = parsed
        .entities
        .iter()
        .map(|entity| EntityInfo {
            id: entity.id.0,
            kind: format!("{:?}", entity.kind),
            name: entity.name.clone(),
            signature: Some(entity.signature.clone()),
            start_line: entity.span.start_position.row as u32 + 1,
            end_line: entity.span.end_position.row as u32 + 1,
            doc_comment: entity.doc_comment.clone(),
        })
        .collect();

    // Build entity name -> id map for intra-file resolution
    let entity_by_name: std::collections::HashMap<&str, u64> = parsed
        .entities
        .iter()
        .map(|e| (e.name.as_str(), e.id.0))
        .collect();

    // Convert relations (callee_id resolved for intra-file calls)
    let relations: Vec<RelationInfo> = parsed
        .raw_relations
        .iter()
        .map(|relation| RelationInfo {
            caller_id: relation.src.0,
            callee_id: entity_by_name
                .get(relation.dst_name.as_str())
                .copied()
                .unwrap_or(0),
            relation_type: relation.relation_type.to_string(),
            line: relation.span.start_position.row as u32 + 1,
        })
        .collect();

    let response = ParseResponse {
        success: true,
        file_path: request.file_path,
        language: format!("{}", parsed.language),
        encoding: detected_encoding,
        entities,
        relations,
        elapsed_ms: start.elapsed().as_millis() as u64,
    };

    ParseApiResponse::Success(response)
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
