//! PHP modifier extraction.
//!
//! Reads the modifier captures the PHP entity query binds
//! (`@entity.*.visibility`, `.static`, `.abstract`, `.final`, `.readonly`)
//! and records their source text. Visibility words flow into the generic
//! visibility fallback; the rest is informational for conversion text.

use crate::tree_sitter_query::executor::QueryMatch;
use cce_types::entity::Entity;

/// Capture-name suffixes that carry PHP modifiers.
const MODIFIER_SUFFIXES: &[&str] = &[".visibility", ".static", ".abstract", ".final", ".readonly"];

pub fn extract_php_modifiers(mat: &QueryMatch, entity: &mut Entity) {
    let mut found: Vec<String> = Vec::new();
    for capture in &mat.captures {
        if !MODIFIER_SUFFIXES
            .iter()
            .any(|suffix| capture.name.ends_with(suffix))
        {
            continue;
        }
        let modifier = capture.text.trim().to_lowercase();
        if !modifier.is_empty() && !found.contains(&modifier) {
            found.push(modifier);
        }
    }
    entity.modifiers = found;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree_sitter_query::executor::Capture;
    use cce_types::Span;
    use cce_types::entity::{EntityId, EntityKind};
    use std::collections::HashMap;

    fn make_entity() -> Entity {
        Entity {
            id: EntityId(1),
            kind: EntityKind::Method,
            name: "save".to_string(),
            signature: String::new(),
            parameters: Vec::new(),
            return_type: None,
            span: Span::default(),
            depth: 0,
            parent: None,
            children: Vec::new(),
            doc_comment: None,
            modifiers: Vec::new(),
            attributes: HashMap::new(),
            metadata: HashMap::new(),
            is_stdlib: false,
            subtype: None,
            stdlib_category: None,
        }
    }

    fn make_capture(name: &str, text: &str) -> Capture {
        Capture {
            name: name.to_string(),
            text: text.to_string(),
            start_byte: 0,
            end_byte: text.len(),
            start_point: (0, 0),
            end_point: (0, 0),
        }
    }

    #[test]
    fn php_visibility_and_static_captured() {
        let mut entity = make_entity();
        let mat = QueryMatch {
            captures: vec![
                make_capture("entity.method", "public static function save() {}"),
                make_capture("entity.method.visibility", "public"),
                make_capture("entity.method.static", "static"),
            ],
            pattern_index: 0,
            index: 0,
        };
        extract_php_modifiers(&mat, &mut entity);
        assert_eq!(entity.modifiers, vec!["public", "static"]);
    }

    #[test]
    fn php_abstract_final_captured_once() {
        let mut entity = make_entity();
        let mat = QueryMatch {
            captures: vec![
                make_capture("entity.method.abstract", "abstract"),
                make_capture("entity.method.final", "final"),
                make_capture("entity.method.abstract", "abstract"),
            ],
            pattern_index: 0,
            index: 0,
        };
        extract_php_modifiers(&mat, &mut entity);
        assert_eq!(entity.modifiers, vec!["abstract", "final"]);
    }

    #[test]
    fn php_unrelated_captures_ignored() {
        let mut entity = make_entity();
        let mat = QueryMatch {
            captures: vec![make_capture("entity.method.name", "save")],
            pattern_index: 0,
            index: 0,
        };
        extract_php_modifiers(&mat, &mut entity);
        assert!(entity.modifiers.is_empty());
    }
}
