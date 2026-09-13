//! Cross-file type propagation.
//!
//! Caches function return types with `High`/`Medium` confidence so that
//! callers in other files can infer variable types from `x = foo()` patterns
//! where `foo` is defined in a different file.
//!
//! The propagator is stored inside [`crate::symbol_table::ProjectSymbolTable`]
//! and is populated by [`crate::index::builder::symbol_table::SymbolTableBuilder`]
//! after per-file type inference. The resolver queries it when the
//! [`crate::symbol_table::TypeMemberIndex`] cannot determine an owner type.

mod arg_inference;
mod call_parsing;
mod propagation;
mod resolution;

pub use super::call_utils::{split_call_args, split_call_target};
pub use super::generics::{shape_contains_param, substitute_call_return_type};
pub use super::propagator::CrossFilePropagator;
pub use arg_inference::infer_arg_shape;
pub use call_parsing::{CallStep, parse_call_chain};
pub(crate) use propagation::collection_element_access;
pub use propagation::propagate_variable_types;
pub use resolution::{
    candidate_downgrades_existing, refine_generic_call, resolve_single_call_binding,
};

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::Span;
    use cce_types::entity::{Entity, EntityId, EntityKind};
    use cce_types::language::Language;

    use crate::type_inference::types::{ScopedTypeContext, TypeBinding, TypeShape};

    fn dummy_span() -> Span {
        Span::default()
    }

    #[test]
    fn test_propagator_insert_and_lookup() {
        let propagator = CrossFilePropagator::new();
        let mut ctx = ScopedTypeContext::new(Language::Python);
        let binding = TypeBinding {
            type_name: "MyClass".to_string(),
            type_entity_id: None,
            span: dummy_span(),
            origin: None,
            shape: None,
        };
        ctx.add_return_type(EntityId(1), binding.clone());

        let entities = vec![
            Entity::new(
                EntityId(1),
                EntityKind::Function,
                "create_user".to_string(),
                dummy_span(),
            )
            .with_return_type(Some("MyClass".to_string())),
        ];

        propagator.insert_file("a.py", &ctx, &entities);
        assert_eq!(propagator.len(), 1);
        assert!(propagator.get_return_type(EntityId(1)).is_some());
        assert!(propagator.get_return_type_by_name("create_user").is_some());
        assert_eq!(
            propagator
                .get_return_type_by_name("create_user")
                .unwrap()
                .type_name,
            "MyClass"
        );
    }

    #[test]
    fn test_propagator_remove_file() {
        let propagator = CrossFilePropagator::new();
        let mut ctx = ScopedTypeContext::new(Language::Rust);
        ctx.add_return_type(
            EntityId(1),
            TypeBinding {
                type_name: "String".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );
        let entities = vec![Entity::new(
            EntityId(1),
            EntityKind::Function,
            "get_name".to_string(),
            dummy_span(),
        )];
        propagator.insert_file("b.rs", &ctx, &entities);
        assert_eq!(propagator.len(), 1);
        propagator.remove_file("b.rs");
        assert_eq!(propagator.len(), 0);
        assert!(propagator.get_return_type_by_name("get_name").is_none());
    }

    fn test_binding(type_name: &str) -> TypeBinding {
        TypeBinding {
            type_name: type_name.to_string(),
            type_entity_id: None,
            span: dummy_span(),
            origin: None,
            shape: Some(TypeShape::Named(type_name.to_string())),
        }
    }

    /// Cross-file overloads dispatch on call-site shapes: `combine(1, 2)`
    /// picks the `(Int, Int)` overload even when it was inserted first and
    /// the collapsed name slot holds another overload's return.
    #[test]
    fn test_resolve_single_call_binding_dispatches_overloads() {
        let propagator = CrossFilePropagator::new();
        let mut ctx_a = ScopedTypeContext::new(Language::Kotlin);
        ctx_a.add_return_type(EntityId(1), test_binding("Int"));
        ctx_a.add_parameter_types(EntityId(1), vec![test_binding("Int"), test_binding("Int")]);
        propagator.insert_file(
            "a.kt",
            &ctx_a,
            &[Entity::new(
                EntityId(1),
                EntityKind::Function,
                "combine".to_string(),
                dummy_span(),
            )],
        );
        let mut ctx_b = ScopedTypeContext::new(Language::Kotlin);
        ctx_b.add_return_type(EntityId(2), test_binding("String"));
        ctx_b.add_parameter_types(
            EntityId(2),
            vec![test_binding("String"), test_binding("String")],
        );
        propagator.insert_file(
            "b.kt",
            &ctx_b,
            &[Entity::new(
                EntityId(2),
                EntityKind::Function,
                "combine".to_string(),
                dummy_span(),
            )],
        );

        let empty_ctx = ScopedTypeContext::new(Language::Kotlin);
        let mut resolve_arg = |arg: &str| infer_arg_shape(&empty_ctx, Language::Kotlin, arg);
        let (type_name, _, _) = resolve_single_call_binding(
            &propagator,
            Language::Kotlin,
            "combine(1, 2)",
            &mut resolve_arg,
        )
        .expect("overload should resolve");
        assert_eq!(type_name, "Int");
        let (type_name, _, _) = resolve_single_call_binding(
            &propagator,
            Language::Kotlin,
            "combine(\"a\", \"b\")",
            &mut resolve_arg,
        )
        .expect("overload should resolve");
        assert_eq!(type_name, "String");
        // Unknown shapes keep the legacy collapsed-slot lookup.
        let (type_name, _, _) = resolve_single_call_binding(
            &propagator,
            Language::Kotlin,
            "combine(x, y)",
            &mut |_| None,
        )
        .expect("legacy lookup should apply");
        assert_eq!(type_name, "String");
    }

    #[test]
    fn test_propagator_medium_confidence_cached() {
        let propagator = CrossFilePropagator::new();
        let mut ctx = ScopedTypeContext::new(Language::Python);
        ctx.add_return_type(
            EntityId(1),
            TypeBinding {
                type_name: "MyType".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );
        let entities = vec![Entity::new(
            EntityId(1),
            EntityKind::Function,
            "foo".to_string(),
            dummy_span(),
        )];
        propagator.insert_file("c.py", &ctx, &entities);
        assert_eq!(propagator.len(), 1);
    }

    #[test]
    fn test_param_propagation_insert_and_lookup() {
        let propagator = CrossFilePropagator::new();
        let mut ctx = ScopedTypeContext::new(Language::Java);
        ctx.add_parameter_types(
            EntityId(10),
            vec![TypeBinding {
                type_name: "String".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            }],
        );
        let entities = vec![Entity::new(
            EntityId(10),
            EntityKind::Method,
            "doSomething".to_string(),
            dummy_span(),
        )];
        propagator.insert_file("A.java", &ctx, &entities);
        assert!(propagator.get_parameter_types(EntityId(10)).is_some());
        assert!(
            propagator
                .get_parameter_types_by_name("doSomething")
                .is_some()
        );
        assert_eq!(
            propagator
                .get_parameter_types_by_name("doSomething")
                .unwrap()[0]
                .type_name,
            "String"
        );
    }

    #[test]
    fn test_field_propagation_insert_and_lookup() {
        let propagator = CrossFilePropagator::new();
        let mut ctx = ScopedTypeContext::new(Language::Java);
        ctx.add_variable_type(
            "myField".to_string(),
            TypeBinding {
                type_name: "int".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );
        let entities = vec![Entity::new(
            EntityId(20),
            EntityKind::Field,
            "myField".to_string(),
            dummy_span(),
        )];
        propagator.insert_file("B.java", &ctx, &entities);
        assert!(propagator.get_field_type(EntityId(20)).is_some());
        assert!(propagator.get_field_type_by_name("myField").is_some());
        assert_eq!(
            propagator
                .get_field_type_by_name("myField")
                .unwrap()
                .type_name,
            "int"
        );
    }

    #[test]
    fn test_propagator_remove_file_clears_params_and_fields() {
        let propagator = CrossFilePropagator::new();
        let mut ctx = ScopedTypeContext::new(Language::Java);
        ctx.add_return_type(
            EntityId(1),
            TypeBinding {
                type_name: "String".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );
        ctx.add_parameter_types(
            EntityId(1),
            vec![TypeBinding {
                type_name: "int".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            }],
        );
        ctx.add_variable_type(
            "myField".to_string(),
            TypeBinding {
                type_name: "bool".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );
        let entities = vec![
            Entity::new(
                EntityId(1),
                EntityKind::Function,
                "foo".to_string(),
                dummy_span(),
            ),
            Entity::new(
                EntityId(2),
                EntityKind::Field,
                "myField".to_string(),
                dummy_span(),
            ),
        ];
        propagator.insert_file("C.java", &ctx, &entities);
        assert!(!propagator.is_empty());
        propagator.remove_file("C.java");
        assert!(propagator.is_empty());
        assert!(propagator.get_return_type_by_name("foo").is_none());
        assert!(propagator.get_parameter_types_by_name("foo").is_none());
        assert!(propagator.get_field_type_by_name("myField").is_none());
    }

    // ==================== lookup_member_type tests ====================

    #[test]
    fn test_lookup_member_type() {
        let propagator = CrossFilePropagator::new();
        let mut ctx = ScopedTypeContext::new(Language::Java);
        ctx.add_variable_type(
            "MyType::name".to_string(),
            TypeBinding {
                type_name: "String".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );
        let entities = vec![Entity::new(
            EntityId(20),
            EntityKind::Field,
            "MyType::name".to_string(),
            dummy_span(),
        )];
        propagator.insert_file("A.java", &ctx, &entities);
        let result = propagator.lookup_member_type("MyType", "name");
        assert!(result.is_some());
        assert_eq!(result.unwrap().type_name, "String");
    }

    #[test]
    fn test_lookup_member_type_fallback() {
        let propagator = CrossFilePropagator::new();
        let mut ctx = ScopedTypeContext::new(Language::Java);
        ctx.add_variable_type(
            "name".to_string(),
            TypeBinding {
                type_name: "String".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );
        let entities = vec![Entity::new(
            EntityId(20),
            EntityKind::Field,
            "name".to_string(),
            dummy_span(),
        )];
        propagator.insert_file("A.java", &ctx, &entities);
        let result = propagator.lookup_member_type("MyType", "name");
        assert!(result.is_some());
        assert_eq!(result.unwrap().type_name, "String");
    }

    #[test]
    fn test_lookup_member_type_not_found() {
        let propagator = CrossFilePropagator::new();
        let result = propagator.lookup_member_type("MyType", "name");
        assert!(result.is_none());
    }

    // ==================== is_empty / len / total_len tests ====================

    #[test]
    fn test_propagator_is_empty() {
        let propagator = CrossFilePropagator::new();
        assert!(propagator.is_empty());
        assert_eq!(propagator.len(), 0);
        assert_eq!(propagator.total_len(), 0);
    }

    #[test]
    fn test_propagator_len_and_total_len() {
        let propagator = CrossFilePropagator::new();
        let mut ctx = ScopedTypeContext::new(Language::Java);
        ctx.add_return_type(
            EntityId(1),
            TypeBinding {
                type_name: "String".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );
        ctx.add_variable_type(
            "bar".to_string(),
            TypeBinding {
                type_name: "int".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );
        let entities = vec![
            Entity::new(
                EntityId(1),
                EntityKind::Function,
                "foo".to_string(),
                dummy_span(),
            ),
            Entity::new(
                EntityId(2),
                EntityKind::Field,
                "bar".to_string(),
                dummy_span(),
            ),
        ];
        propagator.insert_file("A.java", &ctx, &entities);
        assert_eq!(propagator.len(), 1);
        assert!(propagator.total_len() >= 1);
    }

    // ==================== clear tests ====================

    #[test]
    fn test_propagator_clear() {
        let propagator = CrossFilePropagator::new();
        let mut ctx = ScopedTypeContext::new(Language::Java);
        ctx.add_return_type(
            EntityId(1),
            TypeBinding {
                type_name: "String".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );
        let entities = vec![Entity::new(
            EntityId(1),
            EntityKind::Function,
            "foo".to_string(),
            dummy_span(),
        )];
        propagator.insert_file("A.java", &ctx, &entities);
        assert!(!propagator.is_empty());
        propagator.clear();
        assert!(propagator.is_empty());
    }
}
