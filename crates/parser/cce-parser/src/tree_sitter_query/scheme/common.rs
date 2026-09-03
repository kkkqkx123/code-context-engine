//! Common query schemes shared across all languages
//!
//! Provides Tree-sitter query patterns that are language-agnostic
//! and can be reused across different language implementations.

/// Get comment query
///
/// Returns Tree-sitter query patterns for identifying comments.
/// This query is language-agnostic and works for:
/// - C/C++: Line comments (// ...) and block comments (/* ... */)
/// - Most other languages with similar comment syntax
///
/// Comments are meta-information attached to code elements,
/// distinct from entities, calls, and dependencies.
pub fn comment_query() -> &'static str {
    r#"
; ============================================
; Comments (Meta-information)
; ============================================

; All comments (both line and block comments)
(comment) @comment
"#
}

/// Get shared bitwise shift operator query fragments using `(#eq?)` predicate
/// on the `operator` field.
///
/// The `node_kind` argument should point to the full expression node
/// (e.g., `binary_expression` or `binary_operator`) that has an `operator`
/// named field. This is more precise than the fallback version because
/// `(#eq?)` is evaluated by tree-sitter, unlike `(#match?)` which is not
/// evaluated in version 0.26.
pub fn bitwise_shift_operator_query(node_kind: &str) -> String {
    format!(
        r#"
; ============================================
; Bitwise Shift Operators
; ============================================

; Shift left
({node_kind}
  operator: _ @_op_shift_left
  (#eq? @_op_shift_left "<<")) @behavior.op.shift_left

; Shift right
({node_kind}
  operator: _ @_op_shift_right
  (#eq? @_op_shift_right ">>")) @behavior.op.shift_right
"#
    )
}

/// Fallback for grammars that don't have an `operator` named field
/// (e.g., Dart's `shift_expression` and `binary_operator` use anonymous
/// child tokens instead of named fields).
///
/// Uses literal matching on the anonymous operator token to distinguish
/// `<<` from `>>`, which is more precise than matching the whole node.
pub fn bitwise_shift_operator_query_fallback(node_kind: &str) -> String {
    format!(
        r#"
; ============================================
; Bitwise Shift Operators (fallback - field-less grammar)
; ============================================

; Shift left - match anonymous "<<" token
({node_kind}
  "<<" @behavior.op.shift_left)

; Shift right - match anonymous ">>" token
({node_kind}
  ">>" @behavior.op.shift_right)
"#
    )
}
