//! Shared extraction utilities for type inference.
//!
//! Functions here parse entity struct fields and metadata to produce
//! `TypeBinding` entries. They are called by per-language inferers.

use cce_types::entity::Entity;

use super::types::{InferenceOrigin, ScopedTypeContext, TypeBinding, parse_type_shape};

use super::generics::{GenericTypeArg, parse_generic_type};

/// Extract type information from a function entity's struct fields.
///
/// Reads return type from `entity.return_type` and parameter types
/// from `entity.parameters` (filtering entries with type annotations).
pub fn extract_function_types(entity: &Entity, ctx: &mut ScopedTypeContext) {
    if let Some(ref return_type) = entity.return_type {
        let shape = parse_type_shape(return_type, ctx.language());
        let binding = TypeBinding {
            type_name: return_type.clone(),
            type_entity_id: None,
            span: entity.span,
            origin: Some(InferenceOrigin::TypeAnnotation),
            shape,
        };
        ctx.add_return_type(entity.id, binding);
    }

    let param_bindings: Vec<TypeBinding> = entity
        .parameters
        .iter()
        .filter_map(|(_name, ty)| {
            ty.as_ref().map(|type_name| TypeBinding {
                type_name: type_name.clone(),
                type_entity_id: None,
                span: entity.span,
                origin: Some(InferenceOrigin::TypeAnnotation),
                shape: parse_type_shape(type_name, ctx.language()),
            })
        })
        .collect();
    if !param_bindings.is_empty() {
        ctx.add_parameter_types(entity.id, param_bindings);
    }
}

/// Extract type information from a variable entity's metadata.
///
/// Checks metadata keys populated by the parser in priority order:
/// 1. `type_annotation` / `variable_type` — explicit type annotation (High)
/// 2. `constructor_type` — constructor call like `x = MyClass()` (Medium)
/// 3. `literal_type` — literal assignment like `x = 42` (Medium)
pub fn extract_variable_type(entity: &Entity, ctx: &mut ScopedTypeContext) {
    if let Some(type_name) = entity
        .metadata
        .get("type_annotation")
        .or_else(|| entity.metadata.get("variable_type"))
    {
        let binding = TypeBinding {
            type_name: type_name.clone(),
            type_entity_id: None,
            span: entity.span,
            origin: Some(InferenceOrigin::TypeAnnotation),
            shape: parse_type_shape(type_name, ctx.language()),
        };
        ctx.add_variable_type(entity.name.clone(), binding);
        try_bind_generic(ctx, type_name);
        return;
    }

    if let Some(init_type) = entity.metadata.get("constructor_type") {
        let binding = TypeBinding {
            type_name: init_type.clone(),
            type_entity_id: None,
            span: entity.span,
            origin: Some(InferenceOrigin::ConstructorCall),
            shape: parse_type_shape(init_type, ctx.language()),
        };
        ctx.add_variable_type(entity.name.clone(), binding);
        try_bind_generic(ctx, init_type);
        return;
    }

    if let Some(lit_type) = entity.metadata.get("literal_type") {
        let binding = TypeBinding {
            type_name: lit_type.clone(),
            type_entity_id: None,
            span: entity.span,
            origin: Some(InferenceOrigin::LiteralType),
            shape: parse_type_shape(lit_type, ctx.language()),
        };
        ctx.add_variable_type(entity.name.clone(), binding);
        try_bind_generic(ctx, lit_type);
    }
}

/// Extract type information from a field/property entity.
///
/// Checks metadata keys in priority order:
/// 1. `type_annotation` / `variable_type` — explicit type annotation (High)
/// 2. `field_type` — parser-inferred field type (Medium)
pub fn extract_field_type(entity: &Entity, ctx: &mut ScopedTypeContext) {
    if let Some(type_name) = entity
        .metadata
        .get("type_annotation")
        .or_else(|| entity.metadata.get("variable_type"))
    {
        let binding = TypeBinding {
            type_name: type_name.clone(),
            type_entity_id: None,
            span: entity.span,
            origin: Some(InferenceOrigin::TypeAnnotation),
            shape: parse_type_shape(type_name, ctx.language()),
        };
        ctx.add_variable_type(entity.name.clone(), binding);
        return;
    }

    if let Some(field_type) = entity.metadata.get("field_type") {
        let binding = TypeBinding {
            type_name: field_type.clone(),
            type_entity_id: None,
            span: entity.span,
            origin: Some(InferenceOrigin::TypeAnnotation),
            shape: parse_type_shape(field_type, ctx.language()),
        };
        ctx.add_variable_type(entity.name.clone(), binding);
        try_bind_generic(ctx, field_type);
    }
}

fn try_bind_generic(ctx: &mut ScopedTypeContext, type_name: &str) {
    if let Some(gt) = parse_generic_type(type_name) {
        match gt.args.len() {
            1 => {
                if let GenericTypeArg::Concrete(concrete) = &gt.args[0] {
                    for param in &["T", "E", "U", "Value"] {
                        if ctx.get_type_param_for_owner(&gt.base, param).is_none() {
                            ctx.bind_type_param_owned(
                                &gt.base,
                                (*param).to_string(),
                                concrete.clone(),
                            );
                        }
                    }
                }
            }
            2 => {
                let first = match &gt.args[0] {
                    GenericTypeArg::Concrete(s) => Some(s.clone()),
                    _ => None,
                };
                let second = match &gt.args[1] {
                    GenericTypeArg::Concrete(s) => Some(s.clone()),
                    _ => None,
                };
                if let Some(k) = first {
                    for param in &["K", "Key"] {
                        if ctx.get_type_param_for_owner(&gt.base, param).is_none() {
                            ctx.bind_type_param_owned(&gt.base, (*param).to_string(), k.clone());
                        }
                    }
                    if ctx.get_type_param_for_owner(&gt.base, "T").is_none() {
                        ctx.bind_type_param_owned(&gt.base, "T".to_string(), k);
                    }
                }
                if let Some(v) = second {
                    for param in &["V", "Value"] {
                        if ctx.get_type_param_for_owner(&gt.base, param).is_none() {
                            ctx.bind_type_param_owned(&gt.base, (*param).to_string(), v.clone());
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::language::Language;

    fn make_entity(
        id: u64,
        name: &str,
        return_type: Option<&str>,
        parameters: Vec<(&str, Option<&str>)>,
    ) -> Entity {
        Entity {
            id: cce_types::EntityId(id),
            name: name.to_string(),
            kind: cce_types::entity::EntityKind::Function,
            return_type: return_type.map(|s| s.to_string()),
            parameters: parameters
                .into_iter()
                .map(|(n, t)| (n.to_string(), t.map(|s| s.to_string())))
                .collect(),
            metadata: std::collections::HashMap::new(),
            span: cce_types::Span::default(),
            ..Default::default()
        }
    }

    fn make_variable_entity(id: u64, name: &str, metadata: Vec<(&str, &str)>) -> Entity {
        Entity {
            id: cce_types::EntityId(id),
            name: name.to_string(),
            kind: cce_types::entity::EntityKind::Variable,
            return_type: None,
            parameters: Vec::new(),
            metadata: metadata
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            span: cce_types::Span::default(),
            ..Default::default()
        }
    }

    // ==================== extract_function_types tests ====================

    #[test]
    fn test_extract_function_types_with_return_type() {
        let entity = make_entity(1, "foo", Some("String"), vec![]);
        let mut ctx = ScopedTypeContext::new(Language::Python);
        extract_function_types(&entity, &mut ctx);
        let rt = ctx.get_return_type(cce_types::EntityId(1)).unwrap();
        assert_eq!(rt.type_name, "String");
        assert_eq!(rt.origin, Some(InferenceOrigin::TypeAnnotation));
    }

    #[test]
    fn test_extract_function_types_without_return_type() {
        let entity = make_entity(1, "foo", None, vec![]);
        let mut ctx = ScopedTypeContext::new(Language::Python);
        extract_function_types(&entity, &mut ctx);
        assert!(ctx.get_return_type(cce_types::EntityId(1)).is_none());
    }

    #[test]
    fn test_extract_function_types_with_typed_parameters() {
        let entity = make_entity(
            1,
            "foo",
            None,
            vec![("x", Some("int")), ("y", Some("String"))],
        );
        let mut ctx = ScopedTypeContext::new(Language::Python);
        extract_function_types(&entity, &mut ctx);
        let params = ctx.get_parameter_types(cce_types::EntityId(1)).unwrap();
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].type_name, "int");
        assert_eq!(params[1].type_name, "String");
    }

    #[test]
    fn test_extract_function_types_with_untyped_parameters() {
        let entity = make_entity(1, "foo", None, vec![("x", None), ("y", None)]);
        let mut ctx = ScopedTypeContext::new(Language::Python);
        extract_function_types(&entity, &mut ctx);
        assert!(ctx.get_parameter_types(cce_types::EntityId(1)).is_none());
    }

    #[test]
    fn test_extract_function_types_mixed_parameters() {
        let entity = make_entity(1, "foo", None, vec![("x", Some("int")), ("y", None)]);
        let mut ctx = ScopedTypeContext::new(Language::Python);
        extract_function_types(&entity, &mut ctx);
        let params = ctx.get_parameter_types(cce_types::EntityId(1)).unwrap();
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].type_name, "int");
    }

    #[test]
    fn test_extract_function_types_with_shape() {
        let entity = make_entity(1, "foo", Some("List<String>"), vec![]);
        let mut ctx = ScopedTypeContext::new(Language::Python);
        extract_function_types(&entity, &mut ctx);
        let rt = ctx.get_return_type(cce_types::EntityId(1)).unwrap();
        assert!(rt.shape.is_some());
    }

    // ==================== extract_variable_type tests ====================

    #[test]
    fn test_extract_variable_type_type_annotation() {
        let entity = make_variable_entity(1, "x", vec![("type_annotation", "String")]);
        let mut ctx = ScopedTypeContext::new(Language::Python);
        extract_variable_type(&entity, &mut ctx);
        let binding = ctx.get_variable_type("x").unwrap();
        assert_eq!(binding.type_name, "String");
        assert_eq!(binding.origin, Some(InferenceOrigin::TypeAnnotation));
    }

    #[test]
    fn test_extract_variable_type_variable_type() {
        let entity = make_variable_entity(1, "x", vec![("variable_type", "int")]);
        let mut ctx = ScopedTypeContext::new(Language::Python);
        extract_variable_type(&entity, &mut ctx);
        let binding = ctx.get_variable_type("x").unwrap();
        assert_eq!(binding.type_name, "int");
        assert_eq!(binding.origin, Some(InferenceOrigin::TypeAnnotation));
    }

    #[test]
    fn test_extract_variable_type_constructor_call() {
        let entity = make_variable_entity(1, "x", vec![("constructor_type", "MyClass")]);
        let mut ctx = ScopedTypeContext::new(Language::Python);
        extract_variable_type(&entity, &mut ctx);
        let binding = ctx.get_variable_type("x").unwrap();
        assert_eq!(binding.type_name, "MyClass");
        assert_eq!(binding.origin, Some(InferenceOrigin::ConstructorCall));
    }

    #[test]
    fn test_extract_variable_type_literal() {
        let entity = make_variable_entity(1, "x", vec![("literal_type", "int")]);
        let mut ctx = ScopedTypeContext::new(Language::Python);
        extract_variable_type(&entity, &mut ctx);
        let binding = ctx.get_variable_type("x").unwrap();
        assert_eq!(binding.type_name, "int");
        assert_eq!(binding.origin, Some(InferenceOrigin::LiteralType));
    }

    #[test]
    fn test_extract_variable_type_no_metadata() {
        let entity = make_variable_entity(1, "x", vec![]);
        let mut ctx = ScopedTypeContext::new(Language::Python);
        extract_variable_type(&entity, &mut ctx);
        assert!(ctx.get_variable_type("x").is_none());
    }

    #[test]
    fn test_extract_variable_type_priority_type_annotation_over_constructor() {
        let entity = make_variable_entity(
            1,
            "x",
            vec![
                ("type_annotation", "String"),
                ("constructor_type", "MyClass"),
            ],
        );
        let mut ctx = ScopedTypeContext::new(Language::Python);
        extract_variable_type(&entity, &mut ctx);
        let binding = ctx.get_variable_type("x").unwrap();
        assert_eq!(binding.type_name, "String");
        assert_eq!(binding.origin, Some(InferenceOrigin::TypeAnnotation));
    }

    #[test]
    fn test_extract_variable_type_priority_constructor_over_literal() {
        let entity = make_variable_entity(
            1,
            "x",
            vec![("constructor_type", "MyClass"), ("literal_type", "int")],
        );
        let mut ctx = ScopedTypeContext::new(Language::Python);
        extract_variable_type(&entity, &mut ctx);
        let binding = ctx.get_variable_type("x").unwrap();
        assert_eq!(binding.type_name, "MyClass");
        assert_eq!(binding.origin, Some(InferenceOrigin::ConstructorCall));
    }

    // ==================== extract_field_type tests ====================

    #[test]
    fn test_extract_field_type_type_annotation() {
        let entity = make_variable_entity(1, "name", vec![("type_annotation", "String")]);
        let mut ctx = ScopedTypeContext::new(Language::Python);
        extract_field_type(&entity, &mut ctx);
        let binding = ctx.get_variable_type("name").unwrap();
        assert_eq!(binding.type_name, "String");
        assert_eq!(binding.origin, Some(InferenceOrigin::TypeAnnotation));
    }

    #[test]
    fn test_extract_field_type_field_type() {
        let entity = make_variable_entity(1, "name", vec![("field_type", "String")]);
        let mut ctx = ScopedTypeContext::new(Language::Python);
        extract_field_type(&entity, &mut ctx);
        let binding = ctx.get_variable_type("name").unwrap();
        assert_eq!(binding.type_name, "String");
    }

    #[test]
    fn test_extract_field_type_no_metadata() {
        let entity = make_variable_entity(1, "name", vec![]);
        let mut ctx = ScopedTypeContext::new(Language::Python);
        extract_field_type(&entity, &mut ctx);
        assert!(ctx.get_variable_type("name").is_none());
    }

    #[test]
    fn test_extract_field_type_priority_type_annotation_over_field() {
        let entity = make_variable_entity(
            1,
            "name",
            vec![("type_annotation", "String"), ("field_type", "int")],
        );
        let mut ctx = ScopedTypeContext::new(Language::Python);
        extract_field_type(&entity, &mut ctx);
        let binding = ctx.get_variable_type("name").unwrap();
        assert_eq!(binding.type_name, "String");
    }

    // ==================== try_bind_generic tests ====================

    #[test]
    fn test_try_bind_generic_single_arg() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        try_bind_generic(&mut ctx, "List<String>");
        assert_eq!(ctx.get_type_param_for_owner("List", "T"), Some("String"));
    }

    #[test]
    fn test_try_bind_generic_two_args() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        try_bind_generic(&mut ctx, "HashMap<String, Integer>");
        assert_eq!(ctx.get_type_param_for_owner("HashMap", "K"), Some("String"));
        assert_eq!(
            ctx.get_type_param_for_owner("HashMap", "V"),
            Some("Integer")
        );
    }

    #[test]
    fn test_try_bind_generic_no_generic() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        try_bind_generic(&mut ctx, "String");
    }

    #[test]
    fn test_try_bind_generic_existing_binding_not_overridden() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        ctx.bind_type_param_owned("List", "T".to_string(), "int".to_string());
        try_bind_generic(&mut ctx, "List<String>");
        assert_eq!(ctx.get_type_param_for_owner("List", "T"), Some("int"));
    }

    // ==================== extract_variable_type with generic binding ====================

    #[test]
    fn test_extract_variable_type_binds_generic() {
        let entity = make_variable_entity(1, "items", vec![("type_annotation", "List<String>")]);
        let mut ctx = ScopedTypeContext::new(Language::Python);
        extract_variable_type(&entity, &mut ctx);
        let binding = ctx.get_variable_type("items").unwrap();
        assert_eq!(binding.type_name, "List<String>");
        assert_eq!(ctx.get_type_param_for_owner("List", "T"), Some("String"));
    }
}
