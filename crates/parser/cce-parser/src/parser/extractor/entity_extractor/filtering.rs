//! Low-value entity filtering
//!
//! Removes entities that provide negligible semantic value for retrieval.

use cce_types::{Entity, EntityKind};

/// Filter out entities that provide negligible semantic value for retrieval.
///
/// Removes:
/// 1. **Short-name entities**: Single-character names that are typically
///    generic type parameters (T, F, E), lifetime parameters ('a, 'b),
///    or local bindings. These names carry no meaningful semantics.
/// 2. **Tiny-span entities**: Fragments covering fewer than 3 source lines
///    without doc comments, which are usually parameter-level or placeholder
///    nodes.
///
/// Typed variables are always kept regardless of name length: they feed
/// type inference (see `deduplicate_contained_entities`), and the grouper
/// already skips variable entities when forming standalone groups, so
/// retention does not leak into retrieval.
pub(crate) fn filter_low_value_entities(entities: &mut Vec<Entity>) {
    let before = entities.len();
    entities.retain(|entity| {
        // Always keep impl blocks — they anchor structural relations
        // (Implementation/ImplAssociation) derived from entity metadata.
        if entity.kind.is_impl_block() {
            return true;
        }

        // Always keep type-bearing variables, fields, and properties for
        // inference, regardless of name length.
        if matches!(
            entity.kind,
            EntityKind::Variable | EntityKind::Field | EntityKind::Property
        ) && super::deduplication::variable_carries_type_info(entity)
        {
            return true;
        }

        // Always keep entities with doc comments — they carry meaningful semantics
        if entity.doc_comment.is_some() {
            return true;
        }

        // Always keep entities with significant span (> 5 lines)
        let line_count = entity
            .span
            .end_position
            .row
            .saturating_sub(entity.span.start_position.row)
            + 1;
        if line_count > 5 {
            return true;
        }

        // Filter out single-character name entities
        // These are almost always generic type params (T, F, E), lifetime params
        // ('a), or local short bindings — none carry retrievable semantics
        if entity.name.len() == 1 {
            return false;
        }

        // Filter out two-character name entities that are common Rust type params
        // K, V, Ok, Err are typical generic/placeholder names
        if entity.name.len() == 2 && matches!(entity.kind, EntityKind::Variable | EntityKind::Field)
        {
            return false;
        }

        true
    });
    let _removed = before - entities.len();
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::{EntityId, Span};

    #[test]
    fn test_filter_low_value_entities_removes_single_char_names() {
        let mut entities = vec![
            Entity::new(
                EntityId(0),
                EntityKind::Struct,
                "OnceCell".to_string(),
                Span::new(0, 50, 0, 0, 3, 10),
            ),
            Entity::new(
                EntityId(1),
                EntityKind::Variable,
                "r".to_string(),
                Span::new(60, 61, 2, 5, 2, 6),
            ),
            Entity::new(
                EntityId(2),
                EntityKind::Function,
                "initialize".to_string(),
                Span::new(100, 200, 5, 0, 15, 10),
            ),
        ];

        filter_low_value_entities(&mut entities);

        let names: Vec<_> = entities.iter().map(|e| e.name.as_str()).collect();
        assert!(
            !names.contains(&"r"),
            "Single-char variable r should be filtered"
        );
        assert!(
            names.contains(&"OnceCell"),
            "Struct OnceCell should be kept"
        );
        assert!(
            names.contains(&"initialize"),
            "Function initialize should be kept"
        );
    }

    #[test]
    fn test_filter_low_value_entities_keeps_doc_comment_entities() {
        let mut entities = vec![
            Entity {
                id: EntityId(0),
                kind: EntityKind::Variable,
                name: "x".to_string(),
                span: Span::new(10, 11, 0, 10, 0, 11),
                doc_comment: Some("A local variable".to_string()),
                ..Default::default()
            },
            Entity::new(
                EntityId(1),
                EntityKind::Variable,
                "y".to_string(),
                Span::new(20, 21, 0, 20, 0, 21),
            ),
        ];

        filter_low_value_entities(&mut entities);

        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].name, "x");
    }

    #[test]
    fn test_filter_low_value_entities_keeps_large_entities() {
        let mut entities = vec![
            Entity::new(
                EntityId(0),
                EntityKind::Struct,
                "LargeStruct".to_string(),
                Span::new(0, 500, 0, 0, 20, 5),
            ),
            Entity::new(
                EntityId(1),
                EntityKind::Variable,
                "x".to_string(),
                Span::new(10, 11, 0, 10, 0, 11),
            ),
        ];

        filter_low_value_entities(&mut entities);

        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].name, "LargeStruct");
    }
}
