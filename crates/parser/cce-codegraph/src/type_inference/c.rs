//! C-specific type inference.
//!
//! Handles C-specific patterns:
//! - Explicit variable declarations (`int x`, `struct Point p`)
//! - Function prototypes and definitions (return types)
//! - Field declarations inside structs and unions
//! - Initializer-based bindings (literals, constructor-style calls)
//!
//! The inference reuses the shared extractors over parser-provided
//! metadata (`type_annotation`, `constructor_type`, `literal_type`).
//! Typedef alias expansion is intentionally out of scope: the shared
//! type context has no alias table, and guessing would violate the
//! high-confidence-only rule.

use cce_types::entity::{Entity, EntityKind};

use super::extractors::{extract_field_type, extract_function_types, extract_variable_type};
use super::traits::LanguageTypeInferer;
use super::types::ScopedTypeContext;

/// C type inference implementation.
pub struct CTypeInferer;

impl LanguageTypeInferer for CTypeInferer {
    fn infer_declarations(&self, entities: &[Entity], ctx: &mut ScopedTypeContext) {
        for entity in entities {
            match entity.kind {
                EntityKind::Function => {
                    extract_function_types(entity, ctx);
                }
                EntityKind::Variable => {
                    extract_variable_type(entity, ctx);
                }
                EntityKind::Field | EntityKind::Property => {
                    extract_field_type(entity, ctx);
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::Position;
    use cce_types::Span;
    use cce_types::entity::EntityId;
    use cce_types::language::Language;

    fn make_entity(kind: EntityKind, name: &str, metadata: Vec<(&str, &str)>) -> Entity {
        Entity {
            id: EntityId(1),
            kind,
            name: name.to_string(),
            signature: String::new(),
            parameters: Vec::new(),
            return_type: None,
            span: Span {
                start_position: Position { row: 0, column: 0 },
                end_position: Position { row: 0, column: 10 },
                start_byte: 0,
                end_byte: 10,
            },
            depth: 0,
            parent: None,
            children: Vec::new(),
            doc_comment: None,
            modifiers: Vec::new(),
            attributes: Default::default(),
            metadata: metadata
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            is_stdlib: false,
            stdlib_category: None,
            subtype: None,
        }
    }

    #[test]
    fn test_explicit_variable_type() {
        let inferer = CTypeInferer;
        let mut ctx = ScopedTypeContext::new(Language::C);

        let entities = vec![make_entity(
            EntityKind::Variable,
            "count",
            vec![("type_annotation", "int")],
        )];
        inferer.infer_declarations(&entities, &mut ctx);

        let binding = ctx
            .get_variable_type("count")
            .expect("explicit type should be inferred");
        assert_eq!(binding.type_name, "int");
    }

    #[test]
    fn test_struct_variable_type() {
        let inferer = CTypeInferer;
        let mut ctx = ScopedTypeContext::new(Language::C);

        let entities = vec![make_entity(
            EntityKind::Variable,
            "origin",
            vec![("type_annotation", "struct Point")],
        )];
        inferer.infer_declarations(&entities, &mut ctx);

        let binding = ctx
            .get_variable_type("origin")
            .expect("struct type should be inferred");
        assert_eq!(binding.type_name, "struct Point");
    }

    #[test]
    fn test_literal_variable_type() {
        let inferer = CTypeInferer;
        let mut ctx = ScopedTypeContext::new(Language::C);

        let entities = vec![make_entity(
            EntityKind::Variable,
            "flag",
            vec![("literal_type", "int")],
        )];
        inferer.infer_declarations(&entities, &mut ctx);

        let binding = ctx
            .get_variable_type("flag")
            .expect("literal type should be inferred");
        assert_eq!(binding.type_name, "int");
    }

    #[test]
    fn test_function_return_type() {
        let inferer = CTypeInferer;
        let mut ctx = ScopedTypeContext::new(Language::C);

        let mut entity = make_entity(EntityKind::Function, "distance", vec![]);
        entity.return_type = Some("double".to_string());
        let eid = entity.id;
        inferer.infer_declarations(&[entity], &mut ctx);

        let binding = ctx
            .get_return_type(eid)
            .expect("return type should be inferred");
        assert_eq!(binding.type_name, "double");
    }

    #[test]
    fn test_field_type() {
        let inferer = CTypeInferer;
        let mut ctx = ScopedTypeContext::new(Language::C);

        let entities = vec![make_entity(
            EntityKind::Field,
            "x",
            vec![("type_annotation", "int")],
        )];
        inferer.infer_declarations(&entities, &mut ctx);

        let binding = ctx
            .get_variable_type("x")
            .expect("field type should be inferred");
        assert_eq!(binding.type_name, "int");
    }

    #[test]
    fn test_unrelated_kinds_produce_no_bindings() {
        let inferer = CTypeInferer;
        let mut ctx = ScopedTypeContext::new(Language::C);

        let entities = vec![make_entity(EntityKind::Struct, "Point", vec![])];
        inferer.infer_declarations(&entities, &mut ctx);

        assert!(ctx.is_empty());
    }
}
