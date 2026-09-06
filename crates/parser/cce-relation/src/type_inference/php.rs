//! PHP-specific type inference.
//!
//! Handles PHP-specific patterns:
//! - PHPDoc `@var Type $var` and `@return Type` annotations
//! - Constructor calls via `new ClassName()`
//! - Type declarations on function parameters and return types
//! - Property type declarations

use cce_types::entity::{Entity, EntityKind};
use cce_types::language::Language;

use super::extractors::{extract_field_type, extract_function_types, extract_variable_type};
use super::traits::LanguageTypeInferer;
use super::types::{ScopedTypeContext, TypeBinding, parse_type_shape};

/// PHP type inference implementation.
pub struct PhpTypeInferer;

impl LanguageTypeInferer for PhpTypeInferer {
    fn infer_declarations(&self, entities: &[Entity], ctx: &mut ScopedTypeContext) {
        for entity in entities {
            match entity.kind {
                EntityKind::Function | EntityKind::Method => {
                    extract_function_types(entity, ctx);

                    if let Some(return_type) = entity.metadata.get("phpdoc_return_type") {
                        let binding = TypeBinding {
                            type_name: return_type.clone(),
                            type_entity_id: None,
                            span: entity.span,
                            origin: Some(super::types::InferenceOrigin::FunctionReturn),
                            shape: parse_type_shape(return_type, Language::Php),
                        };
                        ctx.add_return_type(entity.id, binding);
                    }
                }
                EntityKind::Variable => {
                    extract_variable_type(entity, ctx);
                    // Constructor calls (`new ClassName()`) are handled by the shared
                    // `extract_variable_type`, which binds `constructor_type` with a
                    // resolved shape only when no concrete annotation is present.
                    // No duplicate handling here so explicit annotations keep priority.
                    if let Some(phpdoc_type) = entity.metadata.get("phpdoc_var_type") {
                        let binding = TypeBinding {
                            type_name: phpdoc_type.clone(),
                            type_entity_id: None,
                            span: entity.span,
                            origin: Some(super::types::InferenceOrigin::TypeAnnotation),
                            shape: parse_type_shape(phpdoc_type, Language::Php),
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
    fn test_php_method_signature() {
        let mut ctx = ScopedTypeContext::new(Language::Php);
        let entities = vec![
            Entity::new(
                EntityId(1),
                EntityKind::Method,
                "getValue".to_string(),
                dummy_span(),
            )
            .with_return_type(Some("string".to_string())),
        ];

        PhpTypeInferer.infer_declarations(&entities, &mut ctx);
        let rt = ctx.get_return_type(EntityId(1)).unwrap();
        assert_eq!(rt.type_name, "string");
    }

    #[test]
    fn test_php_constructor_call() {
        let mut ctx = ScopedTypeContext::new(Language::Php);
        let entities = vec![
            Entity::new(
                EntityId(2),
                EntityKind::Variable,
                "user".to_string(),
                dummy_span(),
            )
            .with_metadata("constructor_type", "User"),
        ];

        PhpTypeInferer.infer_declarations(&entities, &mut ctx);
        let vt = ctx.get_variable_type("user").unwrap();
        assert_eq!(vt.type_name, "User");
        assert!(vt.origin.is_some());
    }

    #[test]
    fn test_php_phpdoc_return_type() {
        let mut ctx = ScopedTypeContext::new(Language::Php);
        let entities = vec![
            Entity::new(
                EntityId(3),
                EntityKind::Method,
                "fetch".to_string(),
                dummy_span(),
            )
            .with_metadata("phpdoc_return_type", "array"),
        ];

        PhpTypeInferer.infer_declarations(&entities, &mut ctx);
        let rt = ctx.get_return_type(EntityId(3)).unwrap();
        assert_eq!(rt.type_name, "array");
        assert!(rt.origin.is_some());
    }

    #[test]
    fn test_php_phpdoc_var_type() {
        let mut ctx = ScopedTypeContext::new(Language::Php);
        let entities = vec![
            Entity::new(
                EntityId(4),
                EntityKind::Variable,
                "name".to_string(),
                dummy_span(),
            )
            .with_metadata("phpdoc_var_type", "string"),
        ];

        PhpTypeInferer.infer_declarations(&entities, &mut ctx);
        let vt = ctx.get_variable_type("name").unwrap();
        assert_eq!(vt.type_name, "string");
        assert!(vt.origin.is_some());
    }

    #[test]
    fn test_php_field_type() {
        let mut ctx = ScopedTypeContext::new(Language::Php);
        let entities = vec![
            Entity::new(
                EntityId(5),
                EntityKind::Field,
                "count".to_string(),
                dummy_span(),
            )
            .with_metadata("type_annotation", "int"),
        ];

        PhpTypeInferer.infer_declarations(&entities, &mut ctx);
        let ft = ctx.get_variable_type("count").unwrap();
        assert_eq!(ft.type_name, "int");
        assert!(ft.origin.is_some());
    }
}
