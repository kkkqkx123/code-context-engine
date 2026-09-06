//! Filter for shadowed `require()` calls in JavaScript/TypeScript.
//!
//! The tree-sitter query matches on identifier text only, so a locally bound
//! `require` (parameter or variable) still fires the dependency pattern.
//! Matches shadowed by a local binding are dropped; genuine top-level
//! `require()` calls are preserved.

use crate::tree_sitter_query::executor::{Capture, QueryMatch};
use cce_types::Entity;
use cce_types::language::Language;

/// Whether a `require()` dependency match is shadowed by a local binding.
///
/// Drops the edge when the call sits inside a function whose parameters bind
/// `require`, or when a `require` variable in the same scope precedes the
/// call. TS `import x = require()` has no function capture and never filters.
pub(crate) fn is_shadowed_require(
    mat: &QueryMatch,
    dep_capture: &Capture,
    entities: &[Entity],
    language: &Language,
) -> bool {
    if !matches!(language, Language::JavaScript | Language::TypeScript) {
        return false;
    }
    if !dep_capture.name.contains("dependency.require") {
        return false;
    }
    let Some(func) = mat
        .captures
        .iter()
        .find(|c| c.name.ends_with("dependency.require.function"))
    else {
        return false;
    };
    if func.text != "require" {
        return false;
    }
    let call_byte = func.start_byte;
    let enclosing = entities
        .iter()
        .filter(|e| {
            e.kind.is_function_like()
                && e.span.start_byte <= call_byte
                && call_byte <= e.span.end_byte
        })
        .min_by_key(|e| e.span.end_byte - e.span.start_byte);
    if let Some(func_entity) = enclosing {
        if func_entity
            .parameters
            .iter()
            .any(|(n, _)| n == "require" || n.ends_with(" require"))
        {
            return true;
        }
        for e in entities {
            if e.name == "require"
                && e.span.start_byte < call_byte
                && e.span.start_byte >= func_entity.span.start_byte
                && e.span.end_byte <= func_entity.span.end_byte
            {
                return true;
            }
        }
        return false;
    }
    entities
        .iter()
        .any(|e| e.name == "require" && e.span.start_byte < call_byte)
}
