//! Type-name validation for entity metadata extraction.
//!
//! Explicit type annotations come from deterministic tree-sitter captures
//! (see the per-language query schemes) and from AST field access via
//! `cce_parser_core::ast_accessor`. Source-text guessing is intentionally
//! absent: type names are only recorded when a structured capture or AST
//! node provides them.

/// Check if a string looks like a valid type name.
pub(crate) fn is_valid_type_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == ':')
        && name
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_')
}
