//! Drop JavaScript/TypeScript variables whose initializer is a `require()` call.
//!
//! Import-like entities are excluded from retrieval conversion. For Rust `use`
//! and C `#include` the import node is the whole statement. CommonJS binds the
//! module to a variable (`var express = require('../..')`); keeping the
//! Variable while dropping the Require leaves a name-only fragment. Removing
//! the Variable keeps the Require for the relation index and keeps the whole
//! statement out of NL conversion.

use cce_types::entity::{Entity, EntityKind};
use cce_types::language::Language;

/// Whether `language` uses CommonJS `require()` bindings that share a variable span.
fn is_js_family(language: &Language) -> bool {
    matches!(
        language,
        Language::JavaScript | Language::TypeScript | Language::Jsx | Language::Tsx
    )
}

/// Remove variables whose source span fully contains a `require()` entity.
///
/// The Require entity is kept so dependency extraction can still emit an
/// import edge. Import-only grouping later drops the Require from retrieval.
pub fn drop_js_require_bound_variables(entities: &mut Vec<Entity>, language: &Language) {
    if !is_js_family(language) {
        return;
    }

    let require_ranges: Vec<(usize, usize)> = entities
        .iter()
        .filter(|entity| entity.kind == EntityKind::Require)
        .map(|entity| (entity.span.start_byte, entity.span.end_byte))
        .collect();

    if require_ranges.is_empty() {
        return;
    }

    entities.retain(|entity| {
        if entity.kind != EntityKind::Variable {
            return true;
        }
        let start = entity.span.start_byte;
        let end = entity.span.end_byte;
        !require_ranges
            .iter()
            .any(|&(req_start, req_end)| start <= req_start && req_end <= end)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::{EntityId, Span};

    fn entity(id: u64, kind: EntityKind, name: &str, start: usize, end: usize) -> Entity {
        Entity::new(
            EntityId(id),
            kind,
            name.to_string(),
            Span::new(start, end, 0, 0, 0, 0),
        )
    }

    #[test]
    fn drops_variable_that_wraps_require() {
        let mut entities = vec![
            entity(1, EntityKind::Variable, "express", 0, 40),
            entity(2, EntityKind::Require, "'../..'", 16, 38),
            entity(3, EntityKind::Variable, "app", 42, 70),
        ];
        drop_js_require_bound_variables(&mut entities, &Language::JavaScript);
        let names: Vec<&str> = entities.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["'../..'", "app"]);
    }

    #[test]
    fn keeps_require_only_file() {
        let mut entities = vec![entity(1, EntityKind::Require, "'fs'", 0, 14)];
        drop_js_require_bound_variables(&mut entities, &Language::JavaScript);
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].kind, EntityKind::Require);
    }

    #[test]
    fn rust_variables_are_untouched() {
        let mut entities = vec![
            entity(1, EntityKind::Variable, "x", 0, 40),
            entity(2, EntityKind::Require, "'../..'", 16, 38),
        ];
        drop_js_require_bound_variables(&mut entities, &Language::Rust);
        assert_eq!(entities.len(), 2);
    }
}
