//! Failure model for the CLI surface.
//!
//! A command reports a failure by returning `CliError` through `anyhow`.
//! The entrypoint is the only place that renders it: plain text on stderr for
//! human readers, or a structured envelope on stderr when `--format json` is
//! active (stdout stays reserved for successful payloads). The process exit
//! status is derived from the wire code so scripts and agents can branch on it.

use std::fmt;

use cce_client::ClientError;
use cce_types::error_codes;

/// A CLI command failure carrying a stable wire code.
#[derive(Debug)]
pub struct CliError {
    pub code: &'static str,
    pub message: String,
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CliError {}

impl CliError {
    /// Build a failure with an explicit wire code.
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    /// Invalid user input (exit status 2).
    pub fn input(message: impl Into<String>) -> Self {
        Self::new(error_codes::INVALID_INPUT, message)
    }

    /// A query or command execution failure (exit status 1).
    pub fn query(message: impl Into<String>) -> Self {
        Self::new(error_codes::QUERY_ERROR, message)
    }

    /// Map a transport failure to a wire code, preserving the server code when
    /// it is one of the shared constants. The context prefixes the message so
    /// the rendered text stays as specific as the previous inline formatting.
    pub fn from_client(context: &str, error: &ClientError) -> Self {
        let code = match error {
            ClientError::Unavailable { .. } => error_codes::SERVICE_UNAVAILABLE,
            ClientError::Rejected {
                code: Some(code), ..
            }
            | ClientError::Unauthorized {
                code: Some(code), ..
            } => match code.as_str() {
                value if value == error_codes::ENTITY_NOT_FOUND => error_codes::ENTITY_NOT_FOUND,
                value if value == error_codes::INVALID_INPUT => error_codes::INVALID_INPUT,
                value if value == error_codes::INVALID_REQUEST => error_codes::INVALID_REQUEST,
                value if value == error_codes::AMBIGUOUS_SYMBOL => error_codes::AMBIGUOUS_SYMBOL,
                value if value == error_codes::SERVICE_UNAVAILABLE => {
                    error_codes::SERVICE_UNAVAILABLE
                }
                _ => error_codes::INTERNAL_ERROR,
            },
            ClientError::Rejected { .. }
            | ClientError::Unauthorized { .. }
            | ClientError::Config { .. }
            | ClientError::Protocol { .. } => error_codes::INTERNAL_ERROR,
        };
        Self::new(code, format!("{context}: {error}"))
    }

    /// Process exit status for this failure.
    ///
    /// Input problems map to 2, missing entities to 3, unavailable backends to
    /// 4, and everything else to 1. These values are the shared CLI contract.
    pub fn exit_code(&self) -> i32 {
        match self.code {
            error_codes::INVALID_REQUEST
            | error_codes::INVALID_INPUT
            | error_codes::AMBIGUOUS_SYMBOL => 2,
            error_codes::ENTITY_NOT_FOUND => 3,
            error_codes::SERVICE_UNAVAILABLE | error_codes::INDEX_NOT_INITIALIZED => 4,
            _ => 1,
        }
    }
}

/// Build a query failure via `anyhow`, ready to return from a command handler.
pub fn fail(message: impl Into<String>) -> anyhow::Error {
    CliError::query(message).into()
}

/// Build an input validation failure via `anyhow`.
pub fn fail_input(message: impl Into<String>) -> anyhow::Error {
    CliError::input(message).into()
}

/// Build a transport failure via `anyhow`, mapping the client error code.
pub fn fail_client(context: &str, error: &ClientError) -> anyhow::Error {
    CliError::from_client(context, error).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_client::ApiMethod;

    fn rejected(code: &str, status: u16) -> ClientError {
        ClientError::Rejected {
            method: ApiMethod::Get,
            path: "/api/project/7".to_string(),
            status,
            code: Some(code.to_string()),
            message: "denied".to_string(),
            details: None,
        }
    }

    #[test]
    fn exit_codes_follow_the_shared_contract() {
        assert_eq!(CliError::input("bad").exit_code(), 2);
        assert_eq!(
            CliError::new(error_codes::AMBIGUOUS_SYMBOL, "match").exit_code(),
            2
        );
        assert_eq!(
            CliError::new(error_codes::ENTITY_NOT_FOUND, "missing").exit_code(),
            3
        );
        assert_eq!(
            CliError::new(error_codes::SERVICE_UNAVAILABLE, "down").exit_code(),
            4
        );
        assert_eq!(CliError::query("failed").exit_code(), 1);
    }

    #[test]
    fn client_error_codes_map_to_expected_classes() {
        let not_found =
            CliError::from_client("load", &rejected(error_codes::ENTITY_NOT_FOUND, 404));
        assert_eq!(not_found.code, error_codes::ENTITY_NOT_FOUND);
        assert_eq!(not_found.exit_code(), 3);
        assert!(not_found.message.starts_with("load: "));

        let invalid = CliError::from_client("load", &rejected(error_codes::INVALID_INPUT, 400));
        assert_eq!(invalid.exit_code(), 2);

        let unavailable = ClientError::Unavailable {
            method: ApiMethod::Get,
            path: "/api/health".to_string(),
            status: Some(503),
            detail: "down".to_string(),
        };
        let mapped = CliError::from_client("health", &unavailable);
        assert_eq!(mapped.code, error_codes::SERVICE_UNAVAILABLE);
        assert_eq!(mapped.exit_code(), 4);
    }
}
