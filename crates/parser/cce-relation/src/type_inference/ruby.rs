//! Ruby-specific type inference.
//!
//! Handles Ruby-specific patterns:
//! - Constructor calls via `ClassName.new()`
//! - Type annotations via `@type` metadata or YARD-style `@return [Type]`
//! - Method return type annotations
//! - Field/instance variable type inference from assignment

use cce_types::entity::{Entity, EntityKind};
use cce_types::language::Language;

use super::extractors::{extract_field_type, extract_function_types, extract_variable_type};
use super::traits::LanguageTypeInferer;
use super::types::{ScopedTypeContext, TypeBinding, parse_type_shape};

/// Ruby type inference implementation.
pub struct RubyTypeInferer;

impl LanguageTypeInferer for RubyTypeInferer {
    fn infer_declarations(&self, entities: &[Entity], ctx: &mut ScopedTypeContext) {
        for entity in entities {
            match entity.kind {
                EntityKind::Function | EntityKind::Method => {
                    extract_function_types(entity, ctx);

                    if let Some(return_type) = entity.metadata.get("yard_return_type") {
                        let binding = TypeBinding {
                            type_name: return_type.clone(),
                            type_entity_id: None,
                            span: entity.span,
                            origin: Some(super::types::InferenceOrigin::GenericInference),
                            shape: parse_type_shape(return_type, Language::Ruby),
                        };
                        ctx.add_return_type(entity.id, binding);
                    }
                }
                EntityKind::Variable => {
                    extract_variable_type(entity, ctx);
                    // Constructor calls (`ClassName.new()`) are handled by the shared
                    // `extract_variable_type`, which binds `constructor_type` with a
                    // resolved shape only when no concrete annotation is present.
                    // No duplicate handling here so explicit annotations keep priority.
                    if let Some(type_annotation) = entity.metadata.get("ruby_type") {
                        let binding = TypeBinding {
                            type_name: type_annotation.clone(),
                            type_entity_id: None,
                            span: entity.span,
                            origin: Some(super::types::InferenceOrigin::GenericInference),
                            shape: parse_type_shape(type_annotation, Language::Ruby),
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
    use cce_types::Span;
    use cce_types::entity::EntityId;
    use cce_types::language::Language;

    fn dummy_span() -> Span {
        Span::default()
    }

    #[test]
    fn test_ruby_method_signature() {
        let mut ctx = ScopedTypeContext::new(Language::Ruby);
        let entities = vec![
            Entity::new(
                EntityId(1),
                EntityKind::Method,
                "get_value".to_string(),
                dummy_span(),
            )
            .with_return_type(Some("String".to_string())),
        ];

        RubyTypeInferer.infer_declarations(&entities, &mut ctx);
        let rt = ctx.get_return_type(EntityId(1)).unwrap();
        assert_eq!(rt.type_name, "String");
    }

    #[test]
    fn test_ruby_constructor_call() {
        let mut ctx = ScopedTypeContext::new(Language::Ruby);
        let entities = vec![
            Entity::new(
                EntityId(2),
                EntityKind::Variable,
                "user".to_string(),
                dummy_span(),
            )
            .with_metadata("constructor_type", "User"),
        ];

        RubyTypeInferer.infer_declarations(&entities, &mut ctx);
        let vt = ctx.get_variable_type("user").unwrap();
        assert_eq!(vt.type_name, "User");
        assert!(vt.origin.is_some());
    }

    #[test]
    fn test_ruby_type_annotation() {
        let mut ctx = ScopedTypeContext::new(Language::Ruby);
        let entities = vec![
            Entity::new(
                EntityId(3),
                EntityKind::Variable,
                "count".to_string(),
                dummy_span(),
            )
            .with_metadata("ruby_type", "Integer"),
        ];

        RubyTypeInferer.infer_declarations(&entities, &mut ctx);
        let vt = ctx.get_variable_type("count").unwrap();
        assert_eq!(vt.type_name, "Integer");
        assert!(vt.origin.is_some());
    }

    #[test]
    fn test_ruby_yard_return_type() {
        let mut ctx = ScopedTypeContext::new(Language::Ruby);
        let entities = vec![
            Entity::new(
                EntityId(4),
                EntityKind::Method,
                "fetch".to_string(),
                dummy_span(),
            )
            .with_metadata("yard_return_type", "Hash"),
        ];

        RubyTypeInferer.infer_declarations(&entities, &mut ctx);
        let rt = ctx.get_return_type(EntityId(4)).unwrap();
        assert_eq!(rt.type_name, "Hash");
        assert!(rt.origin.is_some());
    }

    #[test]
    fn test_ruby_field_type() {
        let mut ctx = ScopedTypeContext::new(Language::Ruby);
        let entities = vec![
            Entity::new(
                EntityId(5),
                EntityKind::Field,
                "name".to_string(),
                dummy_span(),
            )
            .with_metadata("type_annotation", "String"),
        ];

        RubyTypeInferer.infer_declarations(&entities, &mut ctx);
        let ft = ctx.get_variable_type("name").unwrap();
        assert_eq!(ft.type_name, "String");
        assert!(ft.origin.is_some());
    }
}
