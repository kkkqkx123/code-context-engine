//! Unified API response envelope
//!
//! Replaces the per-handler `XxxApiResponse` untagged enums with a single
//! generic envelope: `ApiResult::Error(ErrorResponse)` on failure and
//! `ApiResult::Success(payload)` on success. Error codes are mapped to HTTP
//! status codes in one place instead of being duplicated in every handler.

use axum::{Json, http::StatusCode, response::IntoResponse};
use cce_api::models::ErrorResponse;
use cce_server_shared::services::relation::{RelationServiceError, relation_error_parts};
use cce_types::error_codes;
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
    let (code, message, details) = relation_error_parts(&error);
    match details {
        Some(details) => ErrorResponse::with_details(code, message, details),
        None => ErrorResponse::new(code, message),
    }
}

/// Map an API error code to an HTTP status code.
fn status_for_code(code: &str) -> StatusCode {
    match code {
        error_codes::INVALID_REQUEST
        | error_codes::INVALID_INPUT
        | error_codes::AMBIGUOUS_SYMBOL => StatusCode::BAD_REQUEST,
        error_codes::ENTITY_NOT_FOUND => StatusCode::NOT_FOUND,
        error_codes::SERVICE_UNAVAILABLE => StatusCode::SERVICE_UNAVAILABLE,
        error_codes::CONFLICT => StatusCode::CONFLICT,
        error_codes::NOT_IMPLEMENTED => StatusCode::NOT_IMPLEMENTED,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ambiguous_seed_is_a_client_rejection() {
        let response = relation_error(RelationServiceError::AmbiguousSeed {
            seed: "run".to_string(),
            candidates: Vec::new(),
        });
        assert_eq!(response.error.code, error_codes::AMBIGUOUS_SYMBOL);
        assert_eq!(
            status_for_code(&response.error.code),
            StatusCode::BAD_REQUEST
        );
    }
}
