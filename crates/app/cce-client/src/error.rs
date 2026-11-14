//! Failure model of a single client call.
//!
//! Classification uses the HTTP status and whether the response body could be
//! decoded. Error codes travel as opaque strings; the client never maps them
//! to classes because the server already derives the status from the code.

use std::fmt;

use cce_api::models::ErrorResponse;

/// HTTP verb of the call that failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiMethod {
    Get,
    Post,
    Put,
    Delete,
}

impl fmt::Display for ApiMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ApiMethod::Get => "GET",
            ApiMethod::Post => "POST",
            ApiMethod::Put => "PUT",
            ApiMethod::Delete => "DELETE",
        })
    }
}

/// One failed client call.
#[derive(Debug)]
pub enum ClientError {
    /// The client itself could not be built: invalid base URL or an HTTP stack
    /// that refuses to initialize.
    Config { detail: String },
    /// The call produced no usable answer: connection failure, timeout, or a
    /// status in the retryable set (5xx, 429, 408). Retrying later may succeed.
    Unavailable {
        method: ApiMethod,
        path: String,
        /// Response status, absent when no response was received.
        status: Option<u16>,
        detail: String,
    },
    /// The server rejected the credentials (401/403).
    Unauthorized {
        method: ApiMethod,
        path: String,
        status: u16,
        code: Option<String>,
        message: String,
    },
    /// The server refused the request (other 4xx). Repeating it unchanged
    /// cannot succeed.
    Rejected {
        method: ApiMethod,
        path: String,
        status: u16,
        code: Option<String>,
        message: String,
    },
    /// A 2xx response whose body does not match the declared response type.
    Protocol {
        method: ApiMethod,
        path: String,
        detail: String,
    },
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ClientError::Config { detail } => {
                write!(f, "client configuration invalid: {detail}")
            }
            ClientError::Unavailable {
                method,
                path,
                status,
                detail,
            } => {
                write!(f, "{method} {path} unavailable")?;
                if let Some(status) = status {
                    write!(f, " with {status}")?;
                }
                write!(f, ": {detail}")
            }
            ClientError::Unauthorized {
                method,
                path,
                status,
                code,
                message,
            } => write_failure(f, "unauthorized", method, path, *status, code, message),
            ClientError::Rejected {
                method,
                path,
                status,
                code,
                message,
            } => write_failure(f, "rejected", method, path, *status, code, message),
            ClientError::Protocol {
                method,
                path,
                detail,
            } => write!(f, "{method} {path} protocol mismatch: {detail}"),
        }
    }
}

impl std::error::Error for ClientError {}

fn write_failure(
    f: &mut fmt::Formatter<'_>,
    class: &str,
    method: &ApiMethod,
    path: &str,
    status: u16,
    code: &Option<String>,
    message: &str,
) -> fmt::Result {
    write!(f, "{method} {path} {class}: {status} {message}")?;
    match code {
        Some(code) => write!(f, " [{code}]"),
        None => Ok(()),
    }
}

impl ClientError {
    /// Response status when the failure carried one.
    pub fn status(&self) -> Option<u16> {
        match self {
            ClientError::Config { .. } | ClientError::Protocol { .. } => None,
            ClientError::Unavailable { status, .. } => *status,
            ClientError::Unauthorized { status, .. } | ClientError::Rejected { status, .. } => {
                Some(*status)
            }
        }
    }

    /// Server error code when the failure body carried one.
    pub fn code(&self) -> Option<&str> {
        match self {
            ClientError::Unauthorized { code, .. } | ClientError::Rejected { code, .. } => {
                code.as_deref()
            }
            _ => None,
        }
    }

    /// Request path of the failed call; empty when the client never formed a
    /// request.
    pub fn path(&self) -> &str {
        match self {
            ClientError::Config { .. } => "",
            ClientError::Unavailable { path, .. }
            | ClientError::Unauthorized { path, .. }
            | ClientError::Rejected { path, .. }
            | ClientError::Protocol { path, .. } => path,
        }
    }

    /// Classify a non-2xx response.
    pub(crate) fn from_failure(
        method: ApiMethod,
        path: &str,
        status: u16,
        body: &str,
    ) -> ClientError {
        let (code, message) = parse_error_body(body);
        match status {
            401 | 403 => ClientError::Unauthorized {
                method,
                path: path.to_string(),
                status,
                code,
                message,
            },
            500..=599 | 408 | 429 => ClientError::Unavailable {
                method,
                path: path.to_string(),
                status: Some(status),
                detail: match code {
                    Some(code) => format!("{code}: {message}"),
                    None => message,
                },
            },
            _ => ClientError::Rejected {
                method,
                path: path.to_string(),
                status,
                code,
                message,
            },
        }
    }
}

/// Extract the server error code and message from an error response body.
///
/// Bodies that are not the contract error envelope stay as raw text so the
/// detail is never silently dropped.
fn parse_error_body(body: &str) -> (Option<String>, String) {
    match serde_json::from_str::<ErrorResponse>(body) {
        Ok(error) => {
            let message = match error.error.details {
                Some(details) if !details.is_empty() => {
                    format!("{} ({details})", error.error.message)
                }
                _ => error.error.message,
            };
            (Some(error.error.code), message)
        }
        Err(_) => {
            let trimmed = body.trim();
            if trimmed.is_empty() {
                (None, "empty response body".to_string())
            } else {
                (None, trimmed.to_string())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(code: &str, message: &str) -> String {
        serde_json::json!({
            "success": false,
            "error": {"code": code, "message": message}
        })
        .to_string()
    }

    #[test]
    fn unauthorized_statuses_carry_code() {
        for status in [401, 403] {
            let error = ClientError::from_failure(
                ApiMethod::Get,
                "/api/health",
                status,
                &body("INVALID_INPUT", "bad token"),
            );
            assert!(matches!(error, ClientError::Unauthorized { .. }));
            assert_eq!(error.status(), Some(status));
            assert_eq!(error.code(), Some("INVALID_INPUT"));
        }
    }

    #[test]
    fn server_failures_are_unavailable_and_keep_status() {
        for status in [408, 429, 500, 503] {
            let error = ClientError::from_failure(
                ApiMethod::Post,
                "/api/index",
                status,
                &body("SERVICE_UNAVAILABLE", "backend down"),
            );
            assert!(matches!(error, ClientError::Unavailable { .. }));
            assert_eq!(error.status(), Some(status));
            let text = error.to_string();
            assert!(text.contains("backend down"), "detail lost: {text}");
            assert!(text.contains("SERVICE_UNAVAILABLE"), "code lost: {text}");
        }
    }

    #[test]
    fn client_errors_are_rejected() {
        for status in [400, 404, 409, 422] {
            let error = ClientError::from_failure(
                ApiMethod::Get,
                "/api/project/7",
                status,
                &body("ENTITY_NOT_FOUND", "no such project"),
            );
            assert!(matches!(error, ClientError::Rejected { .. }));
            assert_eq!(error.code(), Some("ENTITY_NOT_FOUND"));
        }
    }

    #[test]
    fn non_contract_error_body_is_preserved() {
        let error = ClientError::from_failure(ApiMethod::Get, "/api/health", 502, "<html>bad");
        assert!(matches!(error, ClientError::Unavailable { .. }));
        assert!(error.to_string().contains("<html>bad"));
    }

    #[test]
    fn empty_error_body_reports_absence() {
        let error = ClientError::from_failure(ApiMethod::Delete, "/api/index", 500, "   ");
        assert!(error.to_string().contains("empty response body"));
    }
}
