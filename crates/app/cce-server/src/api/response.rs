//! Unified API response envelope
//!
//! Replaces the per-handler `XxxApiResponse` untagged enums with a single
//! generic envelope: `ApiResult::Error(ErrorResponse)` on failure and
//! `ApiResult::Success(payload)` on success. Error codes are mapped to HTTP
//! status codes in one place instead of being duplicated in every handler.

use axum::{Json, http::StatusCode, response::IntoResponse};
use cce_api::models::{ErrorResponse, error_codes};
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

/// Map an API error code to an HTTP status code.
fn status_for_code(code: &str) -> StatusCode {
    match code {
        error_codes::INVALID_REQUEST | error_codes::INVALID_INPUT => StatusCode::BAD_REQUEST,
        error_codes::ENTITY_NOT_FOUND => StatusCode::NOT_FOUND,
        error_codes::SERVICE_UNAVAILABLE => StatusCode::SERVICE_UNAVAILABLE,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}
