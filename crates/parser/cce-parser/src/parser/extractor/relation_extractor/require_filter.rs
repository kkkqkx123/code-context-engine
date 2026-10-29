//! Filter for shadowed `require()` calls in the JavaScript family.
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
    if !matches!(
        language,
        Language::JavaScript | Language::TypeScript | Language::Jsx | Language::Tsx
    ) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::Span;
    use cce_types::entity::{EntityId, EntityKind};

    fn shadowed_fixture() -> (QueryMatch, Capture, Vec<Entity>) {
        let dep = Capture {
            name: "dependency.require".to_string(),
            text: "require".to_string(),
            start_byte: 50,
            end_byte: 57,
            start_point: (0, 50),
            end_point: (0, 57),
        };
        let func = Capture {
            name: "dependency.require.function".to_string(),
            text: "require".to_string(),
            start_byte: 50,
            end_byte: 57,
            start_point: (0, 50),
            end_point: (0, 57),
        };
        let mat = QueryMatch {
            index: 0,
            pattern_index: 0,
            captures: vec![dep.clone(), func],
        };
        let mut enclosing = Entity::new(
            EntityId(1),
            EntityKind::Function,
            "outer".to_string(),
            Span::new(0, 100, 0, 0, 0, 100),
        );
        enclosing.parameters = vec![("require".to_string(), None)];
        (mat, dep, vec![enclosing])
    }

    #[test]
    fn test_shadowed_require_js_family_parity() {
        let (mat, dep, entities) = shadowed_fixture();
        for language in [
            Language::JavaScript,
            Language::TypeScript,
            Language::Jsx,
            Language::Tsx,
        ] {
            assert!(
                is_shadowed_require(&mat, &dep, &entities, &language),
                "shadowed require must filter for {language:?}"
            );
        }
        assert!(!is_shadowed_require(&mat, &dep, &entities, &Language::Rust));
    }
}
