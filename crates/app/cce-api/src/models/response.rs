//! Common API response and error types shared between CLI and Server

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// One entity a seed could have meant.
///
/// Sent as the `details` payload of an `AMBIGUOUS_SYMBOL` rejection, so a
/// caller can offer the candidates instead of asking the user to guess. The
/// shape is the wire contract; the server's domain type converts into it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct SymbolCandidate {
    /// Stable symbol id to pass as the seed.
    pub stable_id: String,
    /// File the entity is declared in.
    pub file_path: String,
    /// Qualified name, when the index knows one.
    pub scoped_name: String,
    /// Entity kind (function, class, method, ...).
    pub kind: String,
}

/// Common error response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
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
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ErrorDetail {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}
