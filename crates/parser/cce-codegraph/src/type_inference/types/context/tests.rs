use super::*;

use cce_types::language::Language;

// ==================== ScopedTypeContext tests ====================

#[test]
fn test_scoped_type_context_new() {
    let ctx = ScopedTypeContext::new(Language::Python);
    assert_eq!(ctx.language(), Language::Python);
    assert!(ctx.is_empty());
    assert_eq!(ctx.scope_depth(), 1);
}

#[test]
fn test_scoped_type_context_default() {
    let ctx = ScopedTypeContext::default();
    assert_eq!(ctx.language(), Language::Unknown);
    assert!(ctx.is_empty());
}

#[test]
fn test_scoped_type_context_push_pop_scope() {
    let mut ctx = ScopedTypeContext::new(Language::Python);
    assert_eq!(ctx.scope_depth(), 1);
    ctx.push_scope();
    assert_eq!(ctx.scope_depth(), 2);
    ctx.pop_scope();
    assert_eq!(ctx.scope_depth(), 1);
}

#[test]
#[should_panic(expected = "Cannot pop the root scope")]
fn test_scoped_type_context_pop_root_scope_panics() {
    let mut ctx = ScopedTypeContext::new(Language::Python);
    ctx.pop_scope();
}

#[test]
fn test_scoped_type_context_shadowing() {
    let mut ctx = ScopedTypeContext::new(Language::Python);
    ctx.add_variable_type(
        "x".to_string(),
        TypeBinding {
            type_name: "int".to_string(),
            ..Default::default()
        },
    );
    assert_eq!(ctx.get_variable_type("x").unwrap().type_name, "int");
    ctx.push_scope();
    ctx.add_variable_type(
        "x".to_string(),
        TypeBinding {
            type_name: "String".to_string(),
            ..Default::default()
        },
    );
    assert_eq!(ctx.get_variable_type("x").unwrap().type_name, "String");
    ctx.pop_scope();
    assert_eq!(ctx.get_variable_type("x").unwrap().type_name, "int");
}

#[test]
fn test_scoped_type_context_get_variable_type_current_scope() {
    let mut ctx = ScopedTypeContext::new(Language::Python);
    ctx.add_variable_type(
        "x".to_string(),
        TypeBinding {
            type_name: "int".to_string(),
            ..Default::default()
        },
    );
    assert!(ctx.get_variable_type_current_scope("x").is_some());
    ctx.push_scope();
    ctx.add_variable_type(
        "x".to_string(),
        TypeBinding {
            type_name: "String".to_string(),
            ..Default::default()
        },
    );
    assert_eq!(
        ctx.get_variable_type_current_scope("x").unwrap().type_name,
        "String"
    );
    ctx.pop_scope();
    assert_eq!(
        ctx.get_variable_type_current_scope("x").unwrap().type_name,
        "int"
    );
}

#[test]
fn test_scoped_type_context_return_type() {
    let mut ctx = ScopedTypeContext::new(Language::Python);
    let entity_id = cce_types::EntityId(1);
    ctx.add_return_type(
        entity_id,
        TypeBinding {
            type_name: "String".to_string(),
            ..Default::default()
        },
    );
    assert_eq!(ctx.get_return_type(entity_id).unwrap().type_name, "String");
    assert!(ctx.get_return_type(cce_types::EntityId(999)).is_none());
}

#[test]
fn test_scoped_type_context_parameter_types() {
    let mut ctx = ScopedTypeContext::new(Language::Python);
    let entity_id = cce_types::EntityId(1);
    ctx.add_parameter_types(
        entity_id,
        vec![
            TypeBinding {
                type_name: "int".to_string(),
                ..Default::default()
            },
            TypeBinding {
                type_name: "String".to_string(),
                ..Default::default()
            },
        ],
    );
    let params = ctx.get_parameter_types(entity_id).unwrap();
    assert_eq!(params.len(), 2);
    assert_eq!(params[0].type_name, "int");
    assert_eq!(params[1].type_name, "String");
    assert!(ctx.get_parameter_types(cce_types::EntityId(999)).is_none());
}

#[test]
fn test_scoped_type_context_bind_type_param() {
    let mut ctx = ScopedTypeContext::new(Language::Rust);
    ctx.bind_type_param("T".to_string(), "String".to_string());
    assert_eq!(ctx.get_type_param("T"), Some("String"));
    assert_eq!(ctx.get_type_param("U"), None);
}

#[test]
fn test_scoped_type_context_type_param_scope() {
    let mut ctx = ScopedTypeContext::new(Language::Rust);
    ctx.bind_type_param("T".to_string(), "int".to_string());
    ctx.push_scope();
    ctx.bind_type_param("T".to_string(), "String".to_string());
    assert_eq!(ctx.get_type_param("T"), Some("String"));
    ctx.pop_scope();
    assert_eq!(ctx.get_type_param("T"), Some("int"));
}

#[test]
fn test_scoped_type_context_bind_type_param_owned() {
    let mut ctx = ScopedTypeContext::new(Language::Rust);
    ctx.bind_type_param_owned("Vec", "T".to_string(), "String".to_string());
    assert_eq!(ctx.get_type_param_for_owner("Vec", "T"), Some("String"));
    // Falls back to global get_type_param since no ownership record for HashMap
    assert_eq!(ctx.get_type_param_for_owner("HashMap", "T"), Some("String"));
}

#[test]
fn test_scoped_type_context_resolve_type_simple() {
    let mut ctx = ScopedTypeContext::new(Language::Rust);
    ctx.bind_type_param("T".to_string(), "String".to_string());
    assert_eq!(ctx.resolve_type("T"), "String");
    assert_eq!(ctx.resolve_type("int"), "int");
}

#[test]
fn test_scoped_type_context_resolve_type_generic() {
    let mut ctx = ScopedTypeContext::new(Language::Rust);
    ctx.bind_type_param("T".to_string(), "String".to_string());
    assert_eq!(ctx.resolve_type("Vec<T>"), "Vec<String>");
}

#[test]
fn test_scoped_type_context_narrow_union() {
    let mut ctx = ScopedTypeContext::new(Language::Python);
    let shape = TypeShape::Union(vec![
        TypeShape::Named("String".to_string()),
        TypeShape::Named("None".to_string()),
    ]);
    ctx.add_variable_type(
        "x".to_string(),
        TypeBinding {
            type_name: type_shape_to_string(&shape),
            shape: Some(shape),
            ..Default::default()
        },
    );
    ctx.narrow_union("x", &TypeShape::Named("None".to_string()));
    let binding = ctx.get_variable_type("x").unwrap();
    assert_eq!(binding.type_name, "String");
}

#[test]
fn test_scoped_type_context_merge_from() {
    let mut ctx1 = ScopedTypeContext::new(Language::Python);
    ctx1.add_variable_type(
        "x".to_string(),
        TypeBinding {
            type_name: "int".to_string(),
            origin: Some(InferenceOrigin::LiteralType),
            ..Default::default()
        },
    );
    let mut ctx2 = ScopedTypeContext::new(Language::Python);
    ctx2.add_variable_type(
        "x".to_string(),
        TypeBinding {
            type_name: "String".to_string(),
            origin: Some(InferenceOrigin::TypeAnnotation),
            ..Default::default()
        },
    );
    ctx1.merge_from(&ctx2);
    assert_eq!(ctx1.get_variable_type("x").unwrap().type_name, "String");
}

#[test]
fn test_scoped_type_context_merge_from_lower_priority() {
    let mut ctx1 = ScopedTypeContext::new(Language::Python);
    ctx1.add_variable_type(
        "x".to_string(),
        TypeBinding {
            type_name: "int".to_string(),
            origin: Some(InferenceOrigin::TypeAnnotation),
            ..Default::default()
        },
    );
    let mut ctx2 = ScopedTypeContext::new(Language::Python);
    ctx2.add_variable_type(
        "x".to_string(),
        TypeBinding {
            type_name: "String".to_string(),
            origin: Some(InferenceOrigin::LiteralType),
            ..Default::default()
        },
    );
    ctx1.merge_from(&ctx2);
    assert_eq!(ctx1.get_variable_type("x").unwrap().type_name, "int");
}

#[test]
fn test_scoped_type_context_add_pattern_match_binding_identifier() {
    let mut ctx = ScopedTypeContext::new(Language::Rust);
    let pattern = Pattern::Identifier("x".to_string());
    let source_type = TypeShape::Named("String".to_string());
    ctx.add_pattern_match_binding(&pattern, &source_type, cce_types::Span::default());
    assert_eq!(ctx.get_variable_type("x").unwrap().type_name, "String");
}

#[test]
fn test_scoped_type_context_add_pattern_match_binding_tuple() {
    let mut ctx = ScopedTypeContext::new(Language::Rust);
    let pattern = Pattern::Tuple(vec!["a".to_string(), "b".to_string()]);
    let source_type = TypeShape::Generic {
        base: "Tuple".to_string(),
        args: vec![
            TypeShape::Named("int".to_string()),
            TypeShape::Named("String".to_string()),
        ],
    };
    ctx.add_pattern_match_binding(&pattern, &source_type, cce_types::Span::default());
    assert_eq!(ctx.get_variable_type("a").unwrap().type_name, "int");
    assert_eq!(ctx.get_variable_type("b").unwrap().type_name, "String");
}

#[test]
fn test_scoped_type_context_add_pattern_match_binding_struct() {
    let mut ctx = ScopedTypeContext::new(Language::Rust);
    let pattern = Pattern::Struct(vec!["x".to_string(), "y".to_string()]);
    let source_type = TypeShape::Named("Point".to_string());
    ctx.add_pattern_match_binding(&pattern, &source_type, cce_types::Span::default());
    assert_eq!(ctx.get_variable_type("x").unwrap().type_name, "unknown");
    assert_eq!(ctx.get_variable_type("y").unwrap().type_name, "unknown");
}

#[test]
fn test_scoped_type_context_add_pattern_match_binding_wildcard() {
    let mut ctx = ScopedTypeContext::new(Language::Rust);
    let pattern = Pattern::Wildcard;
    let source_type = TypeShape::Named("Point".to_string());
    ctx.add_pattern_match_binding(&pattern, &source_type, cce_types::Span::default());
}

#[test]
fn test_scoped_type_context_add_destructuring_binding_tuple() {
    let mut ctx = ScopedTypeContext::new(Language::Python);
    let source_type = TypeShape::Generic {
        base: "tuple".to_string(),
        args: vec![
            TypeShape::Named("int".to_string()),
            TypeShape::Named("String".to_string()),
        ],
    };
    ctx.add_destructuring_binding("a", &source_type, Some(0), cce_types::Span::default());
    assert_eq!(ctx.get_variable_type("a").unwrap().type_name, "int");
    ctx.add_destructuring_binding("b", &source_type, Some(1), cce_types::Span::default());
    assert_eq!(ctx.get_variable_type("b").unwrap().type_name, "String");
}

#[test]
fn test_scoped_type_context_add_destructuring_binding_array() {
    let mut ctx = ScopedTypeContext::new(Language::Python);
    let source_type = TypeShape::Array(Box::new(TypeShape::Named("int".to_string())));
    ctx.add_destructuring_binding("elem", &source_type, None, cce_types::Span::default());
    assert_eq!(ctx.get_variable_type("elem").unwrap().type_name, "int");
}

#[test]
fn test_scoped_type_context_is_empty() {
    let mut ctx = ScopedTypeContext::new(Language::Python);
    assert!(ctx.is_empty());
    ctx.add_variable_type(
        "x".to_string(),
        TypeBinding {
            type_name: "int".to_string(),
            ..Default::default()
        },
    );
    assert!(!ctx.is_empty());
}

// ==================== Nested destructuring pattern tests ====================

#[test]
fn test_pattern_match_nested_tuple() {
    let mut ctx = ScopedTypeContext::new(Language::Rust);
    let inner_tuple = Pattern::Tuple(vec!["x".to_string(), "y".to_string()]);
    let source_type = TypeShape::Generic {
        base: "Tuple".to_string(),
        args: vec![
            TypeShape::Named("i32".to_string()),
            TypeShape::Named("String".to_string()),
        ],
    };
    ctx.add_pattern_match_binding(&inner_tuple, &source_type, cce_types::Span::default());
    assert_eq!(ctx.get_variable_type("x").unwrap().type_name, "i32");
    assert_eq!(ctx.get_variable_type("y").unwrap().type_name, "String");
}

#[test]
fn test_pattern_match_struct_multiple_fields() {
    let mut ctx = ScopedTypeContext::new(Language::Rust);
    let pattern = Pattern::Struct(vec![
        "name".to_string(),
        "age".to_string(),
        "email".to_string(),
    ]);
    let source_type = TypeShape::Named("User".to_string());
    ctx.add_pattern_match_binding(&pattern, &source_type, cce_types::Span::default());
    assert_eq!(ctx.get_variable_type("name").unwrap().type_name, "unknown");
    assert_eq!(ctx.get_variable_type("age").unwrap().type_name, "unknown");
    assert_eq!(ctx.get_variable_type("email").unwrap().type_name, "unknown");
}

#[test]
fn test_pattern_match_tuple_three_elements() {
    let mut ctx = ScopedTypeContext::new(Language::Python);
    let pattern = Pattern::Tuple(vec!["a".to_string(), "b".to_string(), "c".to_string()]);
    let source_type = TypeShape::Generic {
        base: "Tuple".to_string(),
        args: vec![
            TypeShape::Named("int".to_string()),
            TypeShape::Named("String".to_string()),
            TypeShape::Named("bool".to_string()),
        ],
    };
    ctx.add_pattern_match_binding(&pattern, &source_type, cce_types::Span::default());
    assert_eq!(ctx.get_variable_type("a").unwrap().type_name, "int");
    assert_eq!(ctx.get_variable_type("b").unwrap().type_name, "String");
    assert_eq!(ctx.get_variable_type("c").unwrap().type_name, "bool");
}

#[test]
fn test_pattern_match_tuple_fewer_args_than_source() {
    let mut ctx = ScopedTypeContext::new(Language::Rust);
    let pattern = Pattern::Tuple(vec!["a".to_string()]);
    let source_type = TypeShape::Generic {
        base: "Tuple".to_string(),
        args: vec![
            TypeShape::Named("i32".to_string()),
            TypeShape::Named("String".to_string()),
        ],
    };
    ctx.add_pattern_match_binding(&pattern, &source_type, cce_types::Span::default());
    assert_eq!(ctx.get_variable_type("a").unwrap().type_name, "i32");
    assert!(ctx.get_variable_type("b").is_none());
}

#[test]
fn test_pattern_match_identifier_with_generic_source() {
    let mut ctx = ScopedTypeContext::new(Language::Python);
    let pattern = Pattern::Identifier("items".to_string());
    let source_type = TypeShape::Generic {
        base: "List".to_string(),
        args: vec![TypeShape::Named("String".to_string())],
    };
    ctx.add_pattern_match_binding(&pattern, &source_type, cce_types::Span::default());
    let binding = ctx.get_variable_type("items").unwrap();
    assert_eq!(binding.type_name, "List<String>");
    assert_eq!(
        binding.shape,
        Some(TypeShape::Generic {
            base: "List".to_string(),
            args: vec![TypeShape::Named("String".to_string())]
        })
    );
}

// ==================== Destructuring assignment tests ====================

#[test]
fn test_destructuring_binding_tuple_index_out_of_bounds() {
    let mut ctx = ScopedTypeContext::new(Language::Python);
    let source_type = TypeShape::Generic {
        base: "tuple".to_string(),
        args: vec![TypeShape::Named("int".to_string())],
    };
    ctx.add_destructuring_binding("a", &source_type, Some(5), cce_types::Span::default());
    let binding = ctx.get_variable_type("a").unwrap();
    assert_eq!(binding.type_name, "unknown");
}

#[test]
fn test_destructuring_binding_array_element() {
    let mut ctx = ScopedTypeContext::new(Language::Python);
    let source_type = TypeShape::Array(Box::new(TypeShape::Named("String".to_string())));
    ctx.add_destructuring_binding("elem", &source_type, None, cce_types::Span::default());
    assert_eq!(ctx.get_variable_type("elem").unwrap().type_name, "String");
    assert_eq!(
        ctx.get_variable_type("elem").unwrap().shape,
        Some(TypeShape::Named("String".to_string()))
    );
}

#[test]
fn test_destructuring_binding_non_destructured_type() {
    let mut ctx = ScopedTypeContext::new(Language::Python);
    let source_type = TypeShape::Named("String".to_string());
    ctx.add_destructuring_binding("x", &source_type, Some(0), cce_types::Span::default());
    let binding = ctx.get_variable_type("x").unwrap();
    assert_eq!(binding.type_name, "unknown");
    assert_eq!(
        binding.origin,
        Some(InferenceOrigin::DestructuringAssignment)
    );
}

#[test]
fn test_destructuring_binding_generic_tuple() {
    let mut ctx = ScopedTypeContext::new(Language::Python);
    let source_type = TypeShape::Generic {
        base: "Tuple".to_string(),
        args: vec![
            TypeShape::Named("i32".to_string()),
            TypeShape::Named("bool".to_string()),
            TypeShape::Named("String".to_string()),
        ],
    };
    ctx.add_destructuring_binding("x", &source_type, Some(2), cce_types::Span::default());
    assert_eq!(ctx.get_variable_type("x").unwrap().type_name, "String");
}

// ==================== ScopedTypeContext resolve_type with nested generics ====================

#[test]
fn test_resolve_type_nested_generic() {
    let mut ctx = ScopedTypeContext::new(Language::Rust);
    ctx.bind_type_param("T".to_string(), "String".to_string());
    ctx.bind_type_param("K".to_string(), "i32".to_string());
    assert_eq!(ctx.resolve_type("HashMap<K, T>"), "HashMap<i32, String>");
}

#[test]
fn test_resolve_type_deeply_nested() {
    let mut ctx = ScopedTypeContext::new(Language::Rust);
    ctx.bind_type_param("T".to_string(), "String".to_string());
    assert_eq!(ctx.resolve_type("Vec<Option<T>>"), "Vec<Option<String>>");
}

#[test]
fn test_resolve_type_no_bindings() {
    let ctx = ScopedTypeContext::new(Language::Rust);
    assert_eq!(ctx.resolve_type("Vec<T>"), "Vec<T>");
}

// ==================== Narrow union with multiple operations ====================

#[test]
fn test_narrow_union_sequential() {
    let mut ctx = ScopedTypeContext::new(Language::Python);
    let shape = TypeShape::Union(vec![
        TypeShape::Named("String".to_string()),
        TypeShape::Named("int".to_string()),
        TypeShape::Named("None".to_string()),
    ]);
    ctx.add_variable_type(
        "x".to_string(),
        TypeBinding {
            type_name: type_shape_to_string(&shape),
            shape: Some(shape),
            ..Default::default()
        },
    );
    ctx.narrow_union("x", &TypeShape::Named("None".to_string()));
    let binding = ctx.get_variable_type("x").unwrap();
    assert_eq!(binding.type_name, "String | int");
}

#[test]
fn test_narrow_union_single_member_left() {
    let mut ctx = ScopedTypeContext::new(Language::Python);
    let shape = TypeShape::Union(vec![TypeShape::Named("String".to_string())]);
    ctx.add_variable_type(
        "x".to_string(),
        TypeBinding {
            type_name: type_shape_to_string(&shape),
            shape: Some(shape),
            ..Default::default()
        },
    );
    ctx.narrow_union("x", &TypeShape::Named("String".to_string()));
    let binding = ctx.get_variable_type("x").unwrap();
    assert_eq!(binding.type_name, "String");
}

#[test]
fn test_destructuring_binding_maps_generic_position() {
    let mut ctx = ScopedTypeContext::new(Language::Python);
    let source = TypeShape::Generic {
        base: "Pair".to_string(),
        args: vec![
            TypeShape::Named("String".to_string()),
            TypeShape::Named("int".to_string()),
        ],
    };
    ctx.add_destructuring_binding("second", &source, Some(1), cce_types::Span::default());
    let binding = ctx
        .get_variable_type("second")
        .expect("destructured binding exists");
    assert_eq!(binding.type_name, "int");
}

#[test]
fn test_destructuring_binding_out_of_range_stays_unknown() {
    let mut ctx = ScopedTypeContext::new(Language::Python);
    let source = TypeShape::Generic {
        base: "Pair".to_string(),
        args: vec![TypeShape::Named("String".to_string())],
    };
    ctx.add_destructuring_binding("second", &source, Some(1), cce_types::Span::default());
    let binding = ctx
        .get_variable_type("second")
        .expect("destructured binding exists");
    assert_eq!(binding.type_name, "unknown");
}

fn branch_binding(type_name: &str) -> TypeBinding {
    TypeBinding {
        type_name: type_name.to_string(),
        origin: Some(InferenceOrigin::ControlFlowNarrowing),
        ..Default::default()
    }
}

#[test]
fn test_resolve_branch_aware_prefers_else_binding() {
    let mut ctx = ScopedTypeContext::new(Language::Java);
    ctx.add_narrowed_type("x".to_string(), branch_binding("String"));
    ctx.add_narrowed_type_in_branch(
        "x".to_string(),
        branch_binding("Integer"),
        BranchPolarity::Else,
    );
    assert_eq!(
        ctx.resolve_branch_aware("x", BranchPolarity::Else)
            .expect("else binding")
            .type_name,
        "Integer"
    );
    assert_eq!(
        ctx.resolve_branch_aware("x", BranchPolarity::Then)
            .expect("then binding")
            .type_name,
        "String"
    );
}

#[test]
fn test_resolve_branch_aware_falls_back_to_default() {
    let mut ctx = ScopedTypeContext::new(Language::Java);
    ctx.add_narrowed_type("x".to_string(), branch_binding("String"));
    assert_eq!(
        ctx.resolve_branch_aware("x", BranchPolarity::Else)
            .expect("fallback binding")
            .type_name,
        "String"
    );
    assert!(
        ctx.resolve_branch_aware("missing", BranchPolarity::Else)
            .is_none()
    );
}

fn nested_shape() -> TypeShape {
    TypeShape::Generic {
        base: "Tuple".to_string(),
        args: vec![
            TypeShape::Named("String".to_string()),
            TypeShape::Generic {
                base: "Tuple".to_string(),
                args: vec![
                    TypeShape::Named("int".to_string()),
                    TypeShape::Named("bool".to_string()),
                ],
            },
        ],
    }
}

#[test]
fn test_nested_destructuring_binds_each_level() {
    let mut ctx = ScopedTypeContext::new(Language::Python);
    let parts = vec![
        NestedPatternPart::Name("a".to_string()),
        NestedPatternPart::Group(vec![
            NestedPatternPart::Name("b".to_string()),
            NestedPatternPart::Name("c".to_string()),
        ]),
    ];
    ctx.add_nested_destructuring_binding(&parts, &nested_shape(), cce_types::Span::default());
    assert_eq!(ctx.get_variable_type("a").unwrap().type_name, "String");
    assert_eq!(ctx.get_variable_type("b").unwrap().type_name, "int");
    assert_eq!(ctx.get_variable_type("c").unwrap().type_name, "bool");
}

#[test]
fn test_nested_destructuring_skips_unmappable_group() {
    let mut ctx = ScopedTypeContext::new(Language::Python);
    let parts = vec![
        NestedPatternPart::Name("a".to_string()),
        NestedPatternPart::Group(vec![NestedPatternPart::Name("b".to_string())]),
    ];
    let source = TypeShape::Generic {
        base: "Tuple".to_string(),
        args: vec![TypeShape::Named("String".to_string())],
    };
    ctx.add_nested_destructuring_binding(&parts, &source, cce_types::Span::default());
    assert_eq!(ctx.get_variable_type("a").unwrap().type_name, "String");
    assert!(ctx.get_variable_type("b").is_none());
}

#[test]
fn test_nested_destructuring_wildcard_binds_nothing() {
    let mut ctx = ScopedTypeContext::new(Language::Python);
    let parts = vec![
        NestedPatternPart::Wildcard,
        NestedPatternPart::Name("b".to_string()),
    ];
    ctx.add_nested_destructuring_binding(&parts, &nested_shape(), cce_types::Span::default());
    assert_eq!(
        ctx.get_variable_type("b").unwrap().type_name,
        "Tuple<int, bool>"
    );
}

fn overload_binding(type_name: &str) -> TypeBinding {
    TypeBinding {
        type_name: type_name.to_string(),
        type_entity_id: None,
        span: cce_types::Span::default(),
        origin: None,
        shape: Some(TypeShape::Named(type_name.to_string())),
    }
}

/// Same-name return entries accumulate per overload; resolution picks
/// by arity and argument shapes while the legacy lookup keeps
/// last-wins semantics.
#[test]
fn test_return_by_name_keeps_overloads_and_resolves() {
    let mut ctx = ScopedTypeContext::new(Language::Kotlin);
    ctx.add_return_type(EntityId(2), overload_binding("Int"));
    ctx.add_parameter_types(
        EntityId(2),
        vec![overload_binding("Int"), overload_binding("Int")],
    );
    ctx.set_param_arity(
        EntityId(2),
        crate::type_inference::overload::ParamArity::exact(2),
    );
    ctx.add_return_type_by_name("combine".to_string(), EntityId(2), overload_binding("Int"));
    ctx.add_return_type(EntityId(4), overload_binding("String"));
    ctx.add_parameter_types(
        EntityId(4),
        vec![overload_binding("String"), overload_binding("String")],
    );
    ctx.set_param_arity(
        EntityId(4),
        crate::type_inference::overload::ParamArity::exact(2),
    );
    ctx.add_return_type_by_name(
        "combine".to_string(),
        EntityId(4),
        overload_binding("String"),
    );

    // Legacy lookup: most recently recorded wins.
    assert_eq!(
        ctx.get_return_type_by_name("combine").unwrap().type_name,
        "String"
    );
    // Resolution by shapes.
    let int_args = vec![
        Some(TypeShape::Named("Int".to_string())),
        Some(TypeShape::Named("Int".to_string())),
    ];
    assert_eq!(
        ctx.resolve_return_by_name("combine", &int_args, Language::Kotlin)
            .unwrap()
            .type_name,
        "Int"
    );
    let string_args = vec![
        Some(TypeShape::Named("String".to_string())),
        Some(TypeShape::Named("String".to_string())),
    ];
    assert_eq!(
        ctx.resolve_return_by_name("combine", &string_args, Language::Kotlin)
            .unwrap()
            .type_name,
        "String"
    );
    // Unknown shapes keep the legacy binding.
    assert_eq!(
        ctx.resolve_return_by_name("combine", &[None, None], Language::Kotlin)
            .unwrap()
            .type_name,
        "String"
    );
    // Arity mismatches with known args yield the overload union
    // instead of one arbitrary overload.
    let ambiguous = ctx
        .resolve_return_by_name(
            "combine",
            &[Some(TypeShape::Named("Int".to_string()))],
            Language::Kotlin,
        )
        .unwrap();
    assert_eq!(ambiguous.type_name, "Int | String");
    assert_eq!(ambiguous.origin, Some(InferenceOrigin::OverloadResolution));
}
