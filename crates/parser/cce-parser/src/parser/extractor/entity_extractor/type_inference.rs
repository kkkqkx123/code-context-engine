//! Fallback type inference for fields, properties and variables
//!
//! Handles type annotation extraction via source heuristics when tree-sitter
//! type captures are missing. Covers postfix `name: Type` and prefix `Type name`
//! forms and language-specific modifier stripping.

use cce_types::Entity;
use cce_types::language::Language;

use crate::tree_sitter_query::executor::QueryMatch;

/// Fallback type annotation extraction for languages without explicit tree-sitter type captures.
///
/// The previous heuristic scanned source text around `:` and `=` to guess a
/// type name. That source-text heuristic has been thoroughly removed: the
/// function now returns `None` unless a structured capture is available.
///
/// TODO: Wire in `cce_parser_core::ast_accessor::extract_type_annotation` as a
/// fallback when a `tree_sitter::Node` can be retrieved from the query match
/// captures. This would replace the current `None` return with AST-based type
/// inference for fields, properties, and variables.
pub(crate) fn extract_fallback_type_annotation(
    _mat: &QueryMatch,
    _entity: &Entity,
    _language: &Language,
    _source: &str,
) -> Option<String> {
    // Deterministic path requires a structured AST node; source-text guessing
    // is intentionally disabled to avoid non-deterministic fallback.
    None
}

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
