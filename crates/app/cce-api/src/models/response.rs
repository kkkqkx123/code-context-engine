//! Common API response and error types shared between CLI and Server

use serde::Serialize;

/// Common error response
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub success: bool,
    pub error: ErrorDetail,
}

impl ErrorResponse {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            success: false,
            error: ErrorDetail {
                code: code.into(),
                message: message.into(),
                details: None,
            },
        }
    }

    pub fn with_details(
        code: impl Into<String>,
        message: impl Into<String>,
        details: impl Into<String>,
    ) -> Self {
        Self {
            success: false,
            error: ErrorDetail {
                code: code.into(),
                message: message.into(),
                details: Some(details.into()),
            },
        }
    }
}

/// Error detail structure
#[derive(Debug, Serialize)]
pub struct ErrorDetail {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

/// Error codes
pub mod error_codes {
    pub const INVALID_REQUEST: &str = "INVALID_REQUEST";
    pub const INVALID_INPUT: &str = "INVALID_INPUT";
    pub const ENTITY_NOT_FOUND: &str = "ENTITY_NOT_FOUND";
    pub const INDEX_NOT_INITIALIZED: &str = "INDEX_NOT_INITIALIZED";
    pub const PARSE_ERROR: &str = "PARSE_ERROR";
    pub const STORAGE_ERROR: &str = "STORAGE_ERROR";
    pub const QUERY_ERROR: &str = "QUERY_ERROR";
    pub const INTERNAL_ERROR: &str = "INTERNAL_ERROR";
    pub const SERVICE_UNAVAILABLE: &str = "SERVICE_UNAVAILABLE";
}
