//! Failure model of a single client call.
//!
//! Classification uses the HTTP status and whether the response body could be
//! decoded. Error codes travel as opaque strings; the client never maps them
//! to classes because the server already derives the status from the code.

use std::fmt;

use cce_api::models::{ErrorResponse, SymbolCandidate};

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
        /// Machine-readable payload the server attached, verbatim.
        ///
        /// It used to be folded into `message`, which made an ambiguous-symbol
        /// answer ("pass one of the candidate stable IDs") impossible to act
        /// on: the ids were inside a sentence. Keeping it separate lets a
        /// caller render the candidates without re-parsing prose.
        details: Option<String>,
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
                details,
            } => {
                write_failure(f, "rejected", method, path, *status, code, message)?;
                match details {
                    Some(details) if !details.is_empty() => write!(f, " ({details})"),
                    _ => Ok(()),
                }
            }
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
    /// Whether repeating the same call later may succeed without changes.
    ///
    /// This restates the classification: only calls without a usable answer
    /// qualify. Callers still own retry timing and limits.
    pub fn is_retryable(&self) -> bool {
        matches!(self, ClientError::Unavailable { .. })
    }

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

    /// Candidate entities when the server rejected an ambiguous seed.
    ///
    /// Empty for every other failure, and for a `details` payload that is not
    /// the candidate array: a caller must never render a malformed payload as
    /// if it were a list of choices.
    pub fn symbol_candidates(&self) -> Vec<SymbolCandidate> {
        let Some(details) = self.details() else {
            return Vec::new();
        };
        serde_json::from_str::<Vec<SymbolCandidate>>(details).unwrap_or_default()
    }

    /// Structured detail the server attached, when it sent one.
    pub fn details(&self) -> Option<&str> {
        match self {
            ClientError::Rejected { details, .. } => details.as_deref(),
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
        let (code, message, details) = parse_error_body(body);
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
                details,
            },
        }
    }
}

/// Extract the server error code, message and details from a body.
///
/// Bodies that are not the contract error envelope stay as raw text so the
/// detail is never silently dropped. `details` is kept apart from `message`
/// because callers act on it: an ambiguous-symbol answer carries the candidate
/// ids there.
fn parse_error_body(body: &str) -> (Option<String>, String, Option<String>) {
    match serde_json::from_str::<ErrorResponse>(body) {
        Ok(error) => {
            let details = error.error.details.filter(|details| !details.is_empty());
            (Some(error.error.code), error.error.message, details)
        }
        Err(_) => {
            let trimmed = body.trim();
            if trimmed.is_empty() {
                (None, "empty response body".to_string(), None)
            } else {
                (None, trimmed.to_string(), None)
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
    fn details_stay_out_of_the_message() {
        let raw = serde_json::json!({
            "success": false,
            "error": {
                "code": "AMBIGUOUS_SYMBOL",
                "message": "3 entities match",
                "details": "[{\"stable_id\":\"sym_a\",\"file_path\":\"a.rs\",\"scoped_name\":\"a::run\",\"kind\":\"function\"}]",
            },
        })
        .to_string();
        let error = ClientError::from_failure(ApiMethod::Post, "/api/graph/ego", 400, &raw);
        let ClientError::Rejected {
            message, details, ..
        } = &error
        else {
            panic!("expected a rejection");
        };
        assert_eq!(message, "3 entities match");
        assert!(details.as_deref().is_some_and(|d| d.contains("sym_a")));
        let candidates = error.symbol_candidates();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].stable_id, "sym_a");
        assert!(error.to_string().contains("sym_a"), "display keeps details");
    }

    #[test]
    fn a_details_payload_that_is_not_a_candidate_list_yields_none() {
        let plain = serde_json::json!({
            "success": false,
            "error": {
                "code": "ENTITY_NOT_FOUND",
                "message": "unknown seed",
                "details": "Seed must be a stable symbol ID",
            },
        })
        .to_string();
        let error = ClientError::from_failure(ApiMethod::Post, "/api/graph/ego", 404, &plain);
        assert_eq!(error.details(), Some("Seed must be a stable symbol ID"));
        assert!(error.symbol_candidates().is_empty());
    }

    #[test]
    fn a_failure_without_a_body_has_no_details() {
        let error = ClientError::from_failure(ApiMethod::Post, "/api/graph/ego", 400, "<html>bad");
        assert_eq!(error.details(), None);
        assert!(error.symbol_candidates().is_empty());
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

    #[test]
    fn only_calls_without_usable_answer_are_retryable() {
        let unavailable = ClientError::from_failure(
            ApiMethod::Get,
            "/api/health",
            503,
            &body("SERVICE_UNAVAILABLE", "backend down"),
        );
        assert!(unavailable.is_retryable());

        let unauthorized =
            ClientError::from_failure(ApiMethod::Get, "/api/health", 401, &body("DENIED", "no"));
        assert!(!unauthorized.is_retryable());

        let rejected = ClientError::from_failure(
            ApiMethod::Get,
            "/api/project/7",
            404,
            &body("ENTITY_NOT_FOUND", "no such project"),
        );
        assert!(!rejected.is_retryable());

        let config = ClientError::Config {
            detail: "bad url".to_string(),
        };
        assert!(!config.is_retryable());

        let protocol = ClientError::Protocol {
            method: ApiMethod::Get,
            path: "/api/health".to_string(),
            detail: "bad shape".to_string(),
        };
        assert!(!protocol.is_retryable());
    }
}
