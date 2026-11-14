//! Unified API response envelope
//!
//! Replaces the per-handler `XxxApiResponse` untagged enums with a single
//! generic envelope: `ApiResult::Error(ErrorResponse)` on failure and
//! `ApiResult::Success(payload)` on success. Error codes are mapped to HTTP
//! status codes in one place instead of being duplicated in every handler.

use axum::{Json, http::StatusCode, response::IntoResponse};
use cce_api::models::{ErrorResponse, error_codes};
use cce_server_shared::services::relation::RelationServiceError;
use serde::Serialize;

/// Unified API result: either an error payload or a success payload.
///
/// Serializes as `ErrorResponse` or the success type directly (untagged),
/// and maps error codes to HTTP status codes.
#[derive(Serialize)]
#[serde(untagged)]
pub enum ApiResult<T: Serialize> {
    /// Error payload with an API error code
    Error(ErrorResponse),
    /// Successful response payload
    Success(T),
}

impl<T: Serialize> IntoResponse for ApiResult<T> {
    fn into_response(self) -> axum::response::Response {
        match self {
            ApiResult::Error(err) => {
                let status = status_for_code(&err.error.code);
                (status, Json(err)).into_response()
            }
            ApiResult::Success(resp) => (StatusCode::OK, Json(resp)).into_response(),
        }
    }
}

/// Map a relation service failure onto the HTTP error contract.
///
/// Availability stays a transport concern: the service reports the runtime
/// state, the HTTP surface renders it as `503`.
pub fn relation_error(error: RelationServiceError) -> ErrorResponse {
    match error {
        RelationServiceError::InvalidInput(message) => {
            ErrorResponse::new(error_codes::INVALID_REQUEST, message)
        }
        RelationServiceError::Project(message) | RelationServiceError::Execution(message) => {
            ErrorResponse::new(error_codes::INTERNAL_ERROR, message)
        }
        RelationServiceError::UnknownSeed(seed) => ErrorResponse::with_details(
            error_codes::ENTITY_NOT_FOUND,
            format!("Unknown symbol seed: {seed}"),
            "Seed must be a stable symbol ID (sym_…), 'path/to/file#name', '#name', or a bare symbol name.",
        ),
        RelationServiceError::AmbiguousSeed { seed, candidates } => ErrorResponse::with_details(
            error_codes::AMBIGUOUS_SYMBOL,
            format!(
                "Symbol seed '{seed}' matches {} entities; pass one of the candidate stable IDs",
                candidates.len()
            ),
            serde_json::to_string(&candidates).unwrap_or_else(|_| "[]".to_string()),
        ),
        RelationServiceError::Unavailable { state, epoch } => ErrorResponse::new(
            error_codes::SERVICE_UNAVAILABLE,
            format!("Relation index not available: {state:?}, epoch: {epoch}"),
        ),
    }
}

/// Map an API error code to an HTTP status code.
fn status_for_code(code: &str) -> StatusCode {
    match code {
        error_codes::INVALID_REQUEST | error_codes::INVALID_INPUT => StatusCode::BAD_REQUEST,
        error_codes::ENTITY_NOT_FOUND => StatusCode::NOT_FOUND,
        error_codes::SERVICE_UNAVAILABLE => StatusCode::SERVICE_UNAVAILABLE,
        error_codes::CONFLICT => StatusCode::CONFLICT,
        error_codes::NOT_IMPLEMENTED => StatusCode::NOT_IMPLEMENTED,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}
