use super::*;
use crate::type_inference::InferenceOrigin;
use cce_types::entity::EntityId;
use cce_types::language::Language;

fn dummy_ctx() -> ScopedTypeContext {
    ScopedTypeContext::new(Language::TypeScript)
}

#[test]
fn test_typescript_array_literal_normalizes_to_declaration_spelling() {
    let mut ctx = dummy_ctx();
    ctx.add_variable_type(
        "items".to_string(),
        TypeBinding {
            type_name: "array".to_string(),
            type_entity_id: None,
            span: Span::default(),
            origin: Some(InferenceOrigin::LiteralType),
            shape: parse_type_shape("array", Language::TypeScript),
        },
    );
    let entity = Entity {
        id: EntityId(1),
        name: "items".to_string(),
        kind: EntityKind::Variable,
        return_type: None,
        parameters: Vec::new(),
        metadata: [("literal_type".to_string(), "array".to_string())]
            .into_iter()
            .collect(),
        span: Span::default(),
        ..Default::default()
    };
    normalize_literal_types(std::slice::from_ref(&entity), &mut ctx);
    assert_eq!(ctx.get_variable_type("items").unwrap().type_name, "Array");
}

#[test]
fn test_typescript_normalize_keeps_explicit_annotation() {
    let mut ctx = dummy_ctx();
    ctx.add_variable_type(
        "items".to_string(),
        TypeBinding {
            type_name: "Array<[string, number]>".to_string(),
            type_entity_id: None,
            span: Span::default(),
            origin: Some(InferenceOrigin::TypeAnnotation),
            shape: parse_type_shape("Array<[string, number]>", Language::TypeScript),
        },
    );
    let entity = Entity {
        id: EntityId(1),
        name: "items".to_string(),
        kind: EntityKind::Variable,
        return_type: None,
        parameters: Vec::new(),
        metadata: [
            ("literal_type".to_string(), "array".to_string()),
            (
                "type_annotation".to_string(),
                "Array<[string, number]>".to_string(),
            ),
        ]
        .into_iter()
        .collect(),
        span: Span::default(),
        ..Default::default()
    };
    normalize_literal_types(std::slice::from_ref(&entity), &mut ctx);
    assert_eq!(
        ctx.get_variable_type("items").unwrap().type_name,
        "Array<[string, number]>"
    );
}

#[test]
fn test_typescript_switch_typeof_binds_each_literal() {
    let ctx = dummy_ctx();
    let results = narrow_typescript_switch(
        "switch (typeof x) { case \"string\": use(x); case \"number\": use(x); default: use(x); }",
        &ctx,
        None,
        &[],
    );
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].variable_name, "x");
    assert_eq!(results[0].narrowed_type.type_name, "string");
    assert_eq!(results[1].narrowed_type.type_name, "number");
}

#[test]
fn test_typescript_switch_typeof_skips_non_literal_labels() {
    let ctx = dummy_ctx();
    let results = narrow_typescript_switch(
        "switch (typeof x) { case 1: use(x); case null: use(x); default: use(x); }",
        &ctx,
        None,
        &[],
    );
    assert!(results.is_empty());
}

#[test]
fn test_typescript_switch_field_without_index_stays_empty() {
    let ctx = dummy_ctx();
    let results = narrow_typescript_switch(
        "switch (x.kind) { case \"circle\": use(x); default: use(x); }",
        &ctx,
        None,
        &[],
    );
    assert!(results.is_empty());
}

#[test]
fn test_typescript_switch_non_narrowable_scrutinee_stays_empty() {
    let ctx = dummy_ctx();
    let results = narrow_typescript_switch(
        "switch (x + 1) { case \"a\": use(x); default: use(x); }",
        &ctx,
        None,
        &[],
    );
    assert!(results.is_empty());
}

#[test]
fn test_typescript_switch_string_literal_does_not_bind() {
    let ctx = dummy_ctx();
    let results = narrow_typescript_switch(
        "switch (typeof x) { log(\"case y:\"); default: use(x); }",
        &ctx,
        None,
        &[],
    );
    assert!(results.is_empty());
}

#[test]
fn test_typescript_typeof_string() {
    let ctx = dummy_ctx();
    let results = narrow_typescript_if("if (typeof x === \"string\")", &ctx, None, &[]);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].variable_name, "x");
    assert_eq!(results[0].narrowed_type.type_name, "string");
}

#[test]
fn test_typescript_typeof_number() {
    let ctx = dummy_ctx();
    let results = narrow_typescript_if("if (typeof x === \"number\")", &ctx, None, &[]);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].narrowed_type.type_name, "number");
}

#[test]
fn test_typescript_typeof_reversed() {
    let ctx = dummy_ctx();
    let results = narrow_typescript_if("if (\"string\" === typeof x)", &ctx, None, &[]);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].variable_name, "x");
    assert_eq!(results[0].narrowed_type.type_name, "string");
}

#[test]
fn test_typescript_instanceof() {
    let ctx = dummy_ctx();
    let results = narrow_typescript_if("if (x instanceof MyClass)", &ctx, None, &[]);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].variable_name, "x");
    assert_eq!(results[0].narrowed_type.type_name, "MyClass");
}

#[test]
fn test_typescript_safe_call_strict_not_equal_narrows_receiver() {
    let ctx = dummy_ctx();
    let params = [("x".to_string(), Some("string | null".to_string()))];
    let results = narrow_typescript_if("if (x?.length !== null)", &ctx, None, &params);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].variable_name, "x");
    assert_eq!(results[0].narrowed_type.type_name, "string");
}

#[test]
fn test_typescript_assertion_strict_not_equal_narrows_receiver() {
    let ctx = dummy_ctx();
    let params = [("x".to_string(), Some("string | null".to_string()))];
    let results = narrow_typescript_if("if (x! !== null)", &ctx, None, &params);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].variable_name, "x");
    assert_eq!(results[0].narrowed_type.type_name, "string");
}

#[test]
fn test_typescript_safe_call_strict_equal_null_stays_empty() {
    let ctx = dummy_ctx();
    let params = [("x".to_string(), Some("string | null".to_string()))];
    let results = narrow_typescript_if("if (x?.length === null)", &ctx, None, &params);
    assert!(results.is_empty());
}

#[test]
fn test_typescript_safe_call_loose_not_equal_narrows_receiver() {
    let ctx = dummy_ctx();
    let params = [("x".to_string(), Some("string | null".to_string()))];
    let results = narrow_typescript_if("if (x?.length != null)", &ctx, None, &params);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].variable_name, "x");
    assert_eq!(results[0].narrowed_type.type_name, "string");
}

#[test]
fn test_typescript_strict_equal_null() {
    let ctx = dummy_ctx();
    let results = narrow_typescript_if("if (x === null)", &ctx, None, &[]);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].variable_name, "x");
    assert_eq!(results[0].narrowed_type.type_name, "null");
}

#[test]
fn test_typescript_discriminated_union() {
    // Deterministic: dummy_ctx has no TypeMemberIndex, heuristic removed -> empty
    let ctx = dummy_ctx();
    let results = narrow_typescript_if("if (x.kind === \"circle\")", &ctx, None, &[]);
    assert!(results.is_empty());
}

#[test]
fn test_typescript_discriminated_union_with_union() {
    // Deterministic: heuristic removed; without TypeMemberIndex returns empty
    let mut ctx = ScopedTypeContext::new(Language::TypeScript);
    ctx.add_variable_type(
        "x".to_string(),
        TypeBinding {
            type_name: "Circle | Square".to_string(),
            type_entity_id: None,
            span: Span::default(),
            origin: None,
            shape: parse_type_shape("Circle | Square", Language::TypeScript),
        },
    );
    let results = narrow_typescript_if("if (x.kind === \"circle\")", &ctx, None, &[]);
    assert!(results.is_empty());
}

#[test]
fn test_typescript_truthiness() {
    let ctx = dummy_ctx();
    let results = narrow_typescript_if("if (x)", &ctx, None, &[]);
    // Unknown type: no placeholder pseudo-type is emitted.
    assert!(results.is_empty());
}

#[test]
fn test_typescript_negated_truthiness() {
    let ctx = dummy_ctx();
    let results = narrow_typescript_if("if (!x)", &ctx, None, &[]);
    // Unknown type: no placeholder pseudo-type is emitted.
    assert!(results.is_empty());
}

#[test]
fn test_typescript_in_operator() {
    let ctx = dummy_ctx();
    let results = narrow_typescript_if("if (\"prop\" in x)", &ctx, None, &[]);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].variable_name, "x");
    assert_eq!(results[0].narrowed_type.type_name, "HasKey<prop>");
    assert_eq!(
        results[0].narrowed_type.origin,
        Some(InferenceOrigin::ControlFlowNarrowing)
    );
    assert!(results[0].narrowed_type.shape.is_some());
}

#[test]
fn test_typescript_loose_equality_null() {
    let ctx = dummy_ctx();
    let results = narrow_typescript_if("if (x == null)", &ctx, None, &[]);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].variable_name, "x");
    assert_eq!(results[0].narrowed_type.type_name, "null | undefined");
}

#[test]
fn test_typescript_strict_equality_undefined() {
    let ctx = dummy_ctx();
    let results = narrow_typescript_if("if (x === undefined)", &ctx, None, &[]);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].variable_name, "x");
    assert_eq!(results[0].narrowed_type.type_name, "undefined");
}

// ==================== Additional control flow tests ====================

#[test]
fn test_typescript_typeof_in_while() {
    let ctx = dummy_ctx();
    let results = narrow_typescript_if("while (typeof x === \"string\")", &ctx, None, &[]);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].variable_name, "x");
    assert_eq!(results[0].narrowed_type.type_name, "string");
}

#[test]
fn test_typescript_typeof_in_else_if() {
    let ctx = dummy_ctx();
    let results = narrow_typescript_if("else if (typeof x === \"boolean\")", &ctx, None, &[]);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].variable_name, "x");
    assert_eq!(results[0].narrowed_type.type_name, "boolean");
}

#[test]
fn test_typescript_typeof_in_return() {
    let ctx = dummy_ctx();
    let results = narrow_typescript_if("return typeof x === \"number\"", &ctx, None, &[]);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].variable_name, "x");
    assert_eq!(results[0].narrowed_type.type_name, "number");
}

#[test]
fn test_typescript_instanceof_in_while() {
    let ctx = dummy_ctx();
    let results = narrow_typescript_if("while (x instanceof Array)", &ctx, None, &[]);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].variable_name, "x");
    assert_eq!(results[0].narrowed_type.type_name, "Array");
}

#[test]
fn test_typescript_strict_equal_null_in_else_if() {
    let ctx = dummy_ctx();
    let results = narrow_typescript_if("else if (x === null)", &ctx, None, &[]);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].variable_name, "x");
    assert_eq!(results[0].narrowed_type.type_name, "null");
}

#[test]
fn test_typescript_discriminated_union_multi_variant() {
    // Deterministic: heuristic removed, without TypeMemberIndex returns empty
    let mut ctx = ScopedTypeContext::new(Language::TypeScript);
    ctx.add_variable_type(
        "result".to_string(),
        TypeBinding {
            type_name: "Success | Error | Pending".to_string(),
            type_entity_id: None,
            span: Span::default(),
            origin: None,
            shape: parse_type_shape("Success | Error | Pending", Language::TypeScript),
        },
    );
    let results = narrow_typescript_if("if (result.kind === \"success\")", &ctx, None, &[]);
    assert!(results.is_empty());
}

#[test]
fn test_typescript_discriminated_union_no_match() {
    // Deterministic: heuristic fallback removed, unknown value yields empty
    let mut ctx = ScopedTypeContext::new(Language::TypeScript);
    ctx.add_variable_type(
        "result".to_string(),
        TypeBinding {
            type_name: "Success | Error".to_string(),
            type_entity_id: None,
            span: Span::default(),
            origin: None,
            shape: parse_type_shape("Success | Error", Language::TypeScript),
        },
    );
    let results = narrow_typescript_if("if (result.kind === \"unknown\")", &ctx, None, &[]);
    assert!(results.is_empty());
}

#[test]
fn test_typescript_truthiness_with_union() {
    let mut ctx = ScopedTypeContext::new(Language::TypeScript);
    ctx.add_variable_type(
        "value".to_string(),
        TypeBinding {
            type_name: "string | null | undefined".to_string(),
            type_entity_id: None,
            span: Span::default(),
            origin: None,
            shape: parse_type_shape("string | null | undefined", Language::TypeScript),
        },
    );
    let results = narrow_typescript_if("if (value)", &ctx, None, &[]);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].variable_name, "value");
    assert_eq!(results[0].narrowed_type.type_name, "string");
}

#[test]
fn test_typescript_negated_truthiness_with_union() {
    let mut ctx = ScopedTypeContext::new(Language::TypeScript);
    ctx.add_variable_type(
        "value".to_string(),
        TypeBinding {
            type_name: "string | null | undefined".to_string(),
            type_entity_id: None,
            span: Span::default(),
            origin: None,
            shape: parse_type_shape("string | null | undefined", Language::TypeScript),
        },
    );
    let results = narrow_typescript_if("if (!value)", &ctx, None, &[]);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].variable_name, "value");
}

#[test]
fn test_typescript_in_operator_nested_object() {
    let ctx = dummy_ctx();
    let results = narrow_typescript_if("if (\"key\" in obj.nested)", &ctx, None, &[]);
    assert!(results.is_empty());
}

#[test]
fn test_typescript_loose_equality_null_nested() {
    let ctx = dummy_ctx();
    let results = narrow_typescript_if("if (obj.value == null)", &ctx, None, &[]);
    assert!(results.is_empty());
}

#[test]
fn test_typescript_constructor_call_type() {
    let mut ctx = ScopedTypeContext::new(Language::TypeScript);
    let entities = vec![
        Entity::new(
            EntityId(10),
            EntityKind::Variable,
            "promise".to_string(),
            Span::default(),
        )
        .with_metadata("constructor_type", "Promise<string>"),
    ];

    TypeScriptTypeInferer.infer_declarations(&entities, &mut ctx);
    let vt = ctx.get_variable_type("promise").unwrap();
    assert_eq!(vt.type_name, "Promise<string>");
    assert_eq!(vt.origin, Some(InferenceOrigin::ConstructorCall));
}

#[test]
fn test_typescript_multiple_typeof_checks() {
    let ctx = dummy_ctx();
    let results = narrow_typescript_if("if (typeof x === \"string\")", &ctx, None, &[]);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].variable_name, "x");
    assert_eq!(results[0].narrowed_type.type_name, "string");
    assert_eq!(
        results[0].narrowed_type.origin,
        Some(InferenceOrigin::ControlFlowNarrowing)
    );
}

#[test]
fn test_typescript_equality_pattern_strict() {
    let result = parse_typescript_strict_equality_pattern("if (x.type === \"foo\")");
    assert!(result.is_some());
    let (var, field, value) = result.unwrap();
    assert_eq!(var, "x");
    assert_eq!(field, "type");
    assert_eq!(value, "foo");
}

#[test]
fn test_typescript_equality_pattern_loose() {
    let result = parse_typescript_strict_equality_pattern("if (x.kind == \"bar\")");
    assert!(result.is_some());
    let (var, field, value) = result.unwrap();
    assert_eq!(var, "x");
    assert_eq!(field, "kind");
    assert_eq!(value, "bar");
}

#[test]
fn test_typescript_equality_pattern_no_dot() {
    let result = parse_typescript_strict_equality_pattern("if (x === \"foo\")");
    assert!(result.is_none());
}

#[test]
fn test_typescript_loose_equality_null_pattern_valid() {
    let result = parse_typescript_loose_equality_null_pattern("if (x == null)");
    assert!(result.is_some());
    let (var, value) = result.unwrap();
    assert_eq!(var, "x");
    assert_eq!(value, "null | undefined");
}

#[test]
fn test_typescript_loose_equality_null_pattern_strict() {
    let result = parse_typescript_loose_equality_null_pattern("if (x === null)");
    assert!(result.is_none());
}

#[test]
fn test_typescript_loose_equality_null_pattern_field_access() {
    let result = parse_typescript_loose_equality_null_pattern("if (x.y == null)");
    assert!(result.is_none());
}

#[test]
fn test_typescript_strict_undefined_pattern_valid() {
    let result = parse_typescript_strict_equality_undefined_pattern("if (x === undefined)");
    assert!(result.is_some());
    let (var, value) = result.unwrap();
    assert_eq!(var, "x");
    assert_eq!(value, "undefined");
}

#[test]
fn test_typescript_strict_undefined_pattern_field() {
    let result = parse_typescript_strict_equality_undefined_pattern("if (x.y === undefined)");
    assert!(result.is_none());
}

#[test]
fn test_typescript_truthiness_pattern_valid() {
    let result = parse_typescript_truthiness_pattern("if (myVar)");
    assert!(result.is_some());
    assert_eq!(result.unwrap(), "myVar");
}

#[test]
fn test_typescript_truthiness_pattern_not_ident() {
    let result = parse_typescript_truthiness_pattern("if (x.y)");
    assert!(result.is_none());
}

#[test]
fn test_typescript_negated_truthiness_pattern_valid() {
    let result = parse_typescript_negated_truthiness_pattern("if (!myVar)");
    assert!(result.is_some());
    assert_eq!(result.unwrap(), "myVar");
}

#[test]
fn test_typescript_negated_truthiness_pattern_not_ident() {
    let result = parse_typescript_negated_truthiness_pattern("if (!x.y)");
    assert!(result.is_none());
}

#[test]
fn test_typescript_not_equal_null_narrows() {
    // Negated null checks narrow to the remaining member.
    let ctx = dummy_ctx();
    let params = [("value".to_string(), Some("string | null".to_string()))];
    let results = narrow_typescript_if("if (value !== null)", &ctx, None, &params);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].variable_name, "value");
    assert_eq!(results[0].narrowed_type.type_name, "string");
}

#[test]
fn test_typescript_not_equal_null_without_declared_type_skipped() {
    let ctx = dummy_ctx();
    let results = narrow_typescript_if("if (value !== null)", &ctx, None, &[]);
    assert!(results.is_empty());
}

#[test]
fn test_typescript_typeof_not_equal_complement() {
    // Negated typeof checks narrow to the remaining member.
    let ctx = dummy_ctx();
    let params = [("value".to_string(), Some("string | number".to_string()))];
    let results = narrow_typescript_if("if (typeof value !== \"string\")", &ctx, None, &params);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].variable_name, "value");
    assert_eq!(results[0].narrowed_type.type_name, "number");
}

#[test]
fn test_typescript_negated_truthiness_declared() {
    // Negated truthiness narrows against the declared union.
    let ctx = dummy_ctx();
    let params = [("value".to_string(), Some("string | undefined".to_string()))];
    let results = narrow_typescript_if("if (!value)", &ctx, None, &params);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].variable_name, "value");
    assert_eq!(results[0].narrowed_type.type_name, "undefined");
}

#[test]
fn test_typescript_constructor_call_shape_resolved() {
    let mut ctx = ScopedTypeContext::new(Language::TypeScript);
    let entities = vec![
        Entity::new(
            EntityId(11),
            EntityKind::Variable,
            "state".to_string(),
            Span::default(),
        )
        .with_metadata("constructor_type", "PublicClass"),
    ];

    TypeScriptTypeInferer.infer_declarations(&entities, &mut ctx);
    let vt = ctx.get_variable_type("state").unwrap();
    assert_eq!(vt.type_name, "PublicClass");
    assert!(vt.shape.is_some());
}

#[test]
fn test_anchored_narrowing_falls_back_to_entity_span() {
    use cce_types::{Position, Span};

    use super::super::types::BranchPolarity;

    let mut ctx = ScopedTypeContext::new(Language::TypeScript);
    let fallback = Span {
        start_byte: 10,
        end_byte: 60,
        start_position: Position { row: 1, column: 0 },
        end_position: Position { row: 3, column: 1 },
    };
    ctx.add_narrowed_type_anchored(
        "x".to_string(),
        TypeBinding {
            type_name: "string".to_string(),
            type_entity_id: None,
            span: Span::default(),
            origin: Some(InferenceOrigin::ControlFlowNarrowing),
            shape: None,
        },
        fallback,
    );
    let narrowed = ctx
        .get_narrowed_in_branch("x", BranchPolarity::Then)
        .expect("narrowed binding stored");
    assert!(narrowed.span.is_available());
    assert_eq!(narrowed.span, fallback);
}
