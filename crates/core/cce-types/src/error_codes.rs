//! Stable wire error codes shared by the HTTP, MCP and CLI surfaces.
//!
//! This module is the single source of truth for error code values. Transport
//! crates re-export or map these constants but must not redefine them: the
//! values are part of the wire contract observed by clients and generated
//! frontends.

/// Request rejected as a whole (malformed or not acceptable).
pub const INVALID_REQUEST: &str = "INVALID_REQUEST";
/// A parameter carried an invalid value.
pub const INVALID_INPUT: &str = "INVALID_INPUT";
/// Requested entity or project does not exist.
pub const ENTITY_NOT_FOUND: &str = "ENTITY_NOT_FOUND";
/// Index has not been initialized for the request scope.
pub const INDEX_NOT_INITIALIZED: &str = "INDEX_NOT_INITIALIZED";
/// Parsing failed.
pub const PARSE_ERROR: &str = "PARSE_ERROR";
/// Storage backend failure.
pub const STORAGE_ERROR: &str = "STORAGE_ERROR";
/// Query execution failure.
pub const QUERY_ERROR: &str = "QUERY_ERROR";
/// Internal error not classified otherwise.
pub const INTERNAL_ERROR: &str = "INTERNAL_ERROR";
/// A required backend is unavailable.
pub const SERVICE_UNAVAILABLE: &str = "SERVICE_UNAVAILABLE";
/// Resource state conflicts with the request.
pub const CONFLICT: &str = "CONFLICT";
/// Capability is not implemented.
pub const NOT_IMPLEMENTED: &str = "NOT_IMPLEMENTED";
/// A symbol-name seed matched multiple entities; candidates are attached as details.
pub const AMBIGUOUS_SYMBOL: &str = "AMBIGUOUS_SYMBOL";
/// A tool is outside the active capability profile or explicit allow-list.
pub const TOOL_DISABLED: &str = "TOOL_DISABLED";
/// A read-only run exhausted its call budget; downstream must fall back.
pub const BUDGET_EXHAUSTED: &str = "BUDGET_EXHAUSTED";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_code_values_stay_stable() {
        assert_eq!(INVALID_REQUEST, "INVALID_REQUEST");
        assert_eq!(INVALID_INPUT, "INVALID_INPUT");
        assert_eq!(ENTITY_NOT_FOUND, "ENTITY_NOT_FOUND");
        assert_eq!(INDEX_NOT_INITIALIZED, "INDEX_NOT_INITIALIZED");
        assert_eq!(PARSE_ERROR, "PARSE_ERROR");
        assert_eq!(STORAGE_ERROR, "STORAGE_ERROR");
        assert_eq!(QUERY_ERROR, "QUERY_ERROR");
        assert_eq!(INTERNAL_ERROR, "INTERNAL_ERROR");
        assert_eq!(SERVICE_UNAVAILABLE, "SERVICE_UNAVAILABLE");
        assert_eq!(CONFLICT, "CONFLICT");
        assert_eq!(NOT_IMPLEMENTED, "NOT_IMPLEMENTED");
        assert_eq!(AMBIGUOUS_SYMBOL, "AMBIGUOUS_SYMBOL");
        assert_eq!(TOOL_DISABLED, "TOOL_DISABLED");
        assert_eq!(BUDGET_EXHAUSTED, "BUDGET_EXHAUSTED");
    }
}
