//! C++-specific type inference.
//!
//! Handles C++-specific patterns:
//! - `auto` type inference
//! - Template type parameter inference
//! - `decltype` type inference
//! - Range-for loop type inference
//! - Constructor call type binding

use cce_types::entity::{Entity, EntityKind};
use cce_types::language::Language;

use super::extractors::{extract_field_type, extract_function_types, extract_variable_type};
use super::traits::LanguageTypeInferer;
use super::types::{InferenceOrigin, ScopedTypeContext, TypeBinding, parse_type_shape};

/// C++ type inference implementation.
pub struct CppTypeInferer;

impl LanguageTypeInferer for CppTypeInferer {
    fn infer_declarations(&self, entities: &[Entity], ctx: &mut ScopedTypeContext) {
        for entity in entities {
            match entity.kind {
                EntityKind::Function | EntityKind::Method => {
                    extract_function_types(entity, ctx);
                }
                EntityKind::Variable => {
                    extract_variable_type(entity, ctx);

                    // auto type inference: `auto x = expr;`
                    if let Some(auto_type) = entity.metadata.get("auto_type") {
                        let binding = TypeBinding {
                            type_name: auto_type.clone(),
                            type_entity_id: None,
                            span: entity.span,
                            origin: Some(InferenceOrigin::TypeAnnotation),
                            shape: parse_type_shape(auto_type, Language::Cpp),
                        };
                        ctx.add_variable_type(entity.name.clone(), binding);
                    }

                    // decltype inference: `decltype(expr) x;`
                    if let Some(decltype_type) = entity.metadata.get("decltype_type") {
                        let binding = TypeBinding {
                            type_name: decltype_type.clone(),
                            type_entity_id: None,
                            span: entity.span,
                            origin: Some(InferenceOrigin::TypeAnnotation),
                            shape: parse_type_shape(decltype_type, Language::Cpp),
                        };
                        ctx.add_variable_type(entity.name.clone(), binding);
                    }

                    // Constructor calls (`Type x = Type(args);`) are handled by
                    // the shared `extract_variable_type`, which binds
                    // `constructor_type` only when no concrete annotation is
                    // present. No duplicate handling here so explicit
                    // annotations keep priority.
                    // Explicit type declaration: `Type x = ...;`
                    if let Some(explicit_type) = entity.metadata.get("explicit_type") {
                        let binding = TypeBinding {
                            type_name: explicit_type.clone(),
                            type_entity_id: None,
                            span: entity.span,
                            origin: Some(InferenceOrigin::TypeAnnotation),
                            shape: parse_type_shape(explicit_type, Language::Cpp),
                        };
                        ctx.add_variable_type(entity.name.clone(), binding);
                    }

                    // Range-for type: `for (auto& x : container)` where x's type
                    // is inferred from the container's value type.
                    if let Some(range_for_type) = entity.metadata.get("range_for_type") {
                        let binding = TypeBinding {
                            type_name: range_for_type.clone(),
                            type_entity_id: None,
                            span: entity.span,
                            origin: Some(InferenceOrigin::TypeAnnotation),
                            shape: parse_type_shape(range_for_type, Language::Cpp),
                        };
                        ctx.add_variable_type(entity.name.clone(), binding);
                    }
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
    fn test_auto_type_inference() {
        let inferer = CppTypeInferer;
        let mut ctx = ScopedTypeContext::new(cce_types::language::Language::Cpp);

        let entities = vec![make_entity(
            EntityKind::Variable,
            "x",
            vec![("auto_type", "int")],
        )];
        inferer.infer_declarations(&entities, &mut ctx);

        let binding = ctx
            .get_variable_type("x")
            .expect("auto type should be inferred");
        assert_eq!(binding.type_name, "int");
    }

    #[test]
    fn test_decltype_inference() {
        let inferer = CppTypeInferer;
        let mut ctx = ScopedTypeContext::new(cce_types::language::Language::Cpp);

        let entities = vec![make_entity(
            EntityKind::Variable,
            "y",
            vec![("decltype_type", "double")],
        )];
        inferer.infer_declarations(&entities, &mut ctx);

        let binding = ctx
            .get_variable_type("y")
            .expect("decltype type should be inferred");
        assert_eq!(binding.type_name, "double");
    }

    #[test]
    fn test_constructor_type_inference() {
        let inferer = CppTypeInferer;
        let mut ctx = ScopedTypeContext::new(cce_types::language::Language::Cpp);

        let entities = vec![make_entity(
            EntityKind::Variable,
            "obj",
            vec![("constructor_type", "MyClass")],
        )];
        inferer.infer_declarations(&entities, &mut ctx);

        let binding = ctx
            .get_variable_type("obj")
            .expect("constructor type should be inferred");
        assert_eq!(binding.type_name, "MyClass");
    }

    #[test]
    fn test_range_for_type_inference() {
        let inferer = CppTypeInferer;
        let mut ctx = ScopedTypeContext::new(cce_types::language::Language::Cpp);

        let entities = vec![make_entity(
            EntityKind::Variable,
            "elem",
            vec![("range_for_type", "int")],
        )];
        inferer.infer_declarations(&entities, &mut ctx);

        let binding = ctx
            .get_variable_type("elem")
            .expect("range-for type should be inferred");
        assert_eq!(binding.type_name, "int");
    }

    #[test]
    fn test_explicit_type_inference() {
        let inferer = CppTypeInferer;
        let mut ctx = ScopedTypeContext::new(cce_types::language::Language::Cpp);

        let entities = vec![make_entity(
            EntityKind::Variable,
            "val",
            vec![("explicit_type", "std::string")],
        )];
        inferer.infer_declarations(&entities, &mut ctx);

        let binding = ctx
            .get_variable_type("val")
            .expect("explicit type should be inferred");
        assert_eq!(binding.type_name, "std::string");
    }

    #[test]
    fn test_function_return_type() {
        let inferer = CppTypeInferer;
        let mut ctx = ScopedTypeContext::new(cce_types::language::Language::Cpp);

        let mut entity = make_entity(EntityKind::Function, "compute", vec![]);
        entity.return_type = Some("int".to_string());
        let eid = entity.id;
        inferer.infer_declarations(&[entity], &mut ctx);

        let binding = ctx
            .get_return_type(eid)
            .expect("return type should be inferred");
        assert_eq!(binding.type_name, "int");
    }
}
