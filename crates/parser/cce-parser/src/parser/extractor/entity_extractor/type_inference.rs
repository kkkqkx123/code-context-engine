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

/// Check if a string looks like a valid call-target callee path.
///
/// Extends the type-name vocabulary with receiver qualification used by
/// real call expressions: `$` (PHP variables), `->` (PHP `$this->m`,
/// C++ `ptr->m`), `?.`/`?->` (Kotlin safe-call, PHP nullsafe) and `/`
/// (qualified paths). Callers strip the receiver down to the trailing
/// identifier before name lookups; this check only rejects literals and
/// operator noise (`42(`, `a + b(`).
pub(crate) fn is_valid_call_target_name(name: &str) -> bool {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return false;
    }
    if !trimmed.chars().all(|c| {
        c.is_alphanumeric() || matches!(c, '_' | '.' | ':' | '/' | '$' | '-' | '>' | '?' | '<')
    }) {
        return false;
    }
    // The trailing identifier after the last receiver separator must look
    // like a name; a path ending in an operator (`a->`, `a?`) is noise.
    let mut tail = trimmed;
    for sep in ["?->", "?.", "->", "::", ".", ":", "/"] {
        if let Some(pos) = tail.rfind(sep) {
            let after = tail[pos + sep.len()..].trim();
            // Only cut when the separator is not the whole remainder
            // (keeps `a.b->c` reducing stepwise to `c`).
            if !after.is_empty() {
                tail = after;
            }
        }
    }
    let tail = tail.trim().trim_start_matches('$').trim();
    !tail.is_empty()
        && tail
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '<' || c == '>')
        && tail
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$')
}
