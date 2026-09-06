//! Dart-specific type inference.
//!
//! Handles Dart-specific patterns:
//! - `var`/`final`/`const` declarations with inferred types
//! - `Type name = expr` explicit type declarations
//! - Constructor calls via `ClassName()`
//! - Null narrowing (`x != null` → x: non-null)
//! - `is` type checks with smart casting
//! - Discriminated union narrowing (`x.field == "value"` → x: narrowed union)

use cce_types::ControlFlowFactKind;
use cce_types::ControlFlowStore;
use cce_types::Span;
use cce_types::entity::{Entity, EntityKind};
use cce_types::language::Language;

use super::control_flow::shared::{extract_balanced_parens, is_valid_ident, strip_outer_parens};
use super::extractors::{extract_field_type, extract_function_types, extract_variable_type};
use super::traits::LanguageTypeInferer;
use super::types::{
    ScopedTypeContext, TypeBinding, add_polarity_aware_narrowings, declared_shape,
    narrow_discriminated_union, parse_type_shape, subtract_union_members, type_shape_to_string,
};

/// Dart type inference implementation.
pub struct DartTypeInferer;

impl LanguageTypeInferer for DartTypeInferer {
    fn infer_declarations(&self, entities: &[Entity], ctx: &mut ScopedTypeContext) {
        for entity in entities {
            match entity.kind {
                EntityKind::Function | EntityKind::Method => {
                    extract_function_types(entity, ctx);
                }
                EntityKind::Variable => {
                    extract_variable_type(entity, ctx);

                    // Dart-specific: `var_type` for var/final/const inferred types
                    if let Some(var_type) = entity.metadata.get("var_type") {
                        let binding = TypeBinding {
                            type_name: var_type.clone(),
                            type_entity_id: None,
                            span: entity.span,
                            origin: Some(super::types::InferenceOrigin::TypeAnnotation),
                            shape: None,
                        };
                        ctx.add_variable_type(entity.name.clone(), binding);
                    }

                    if let Some(constructor_type) = entity.metadata.get("constructor_type") {
                        let binding = TypeBinding {
                            type_name: constructor_type.clone(),
                            type_entity_id: None,
                            span: entity.span,
                            origin: Some(super::types::InferenceOrigin::ConstructorCall),
                            shape: None,
                        };
                        ctx.add_variable_type(entity.name.clone(), binding);
                    }

                    // Dart-specific: explicit type declaration
                    if let Some(explicit_type) = entity.metadata.get("explicit_type") {
                        let binding = TypeBinding {
                            type_name: explicit_type.clone(),
                            type_entity_id: None,
                            span: entity.span,
                            origin: Some(super::types::InferenceOrigin::TypeAnnotation),
                            shape: None,
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

    fn infer_control_flow(
        &self,
        entities: &[Entity],
        control_flow: &ControlFlowStore,
        ctx: &mut ScopedTypeContext,
        inference_ctx: &super::traits::InferenceContext<'_>,
    ) {
        for entity in entities {
            let Some(entity_cf) = control_flow.get(entity.id) else {
                continue;
            };
            for fact in &entity_cf.facts {
                match fact.kind {
                    ControlFlowFactKind::If => {
                        let narrowed: Vec<(String, TypeBinding)> = narrow_dart_if(
                            &fact.text,
                            ctx,
                            inference_ctx.type_index(),
                            &entity.parameters,
                        )
                        .into_iter()
                        .map(|result| (result.variable_name, result.narrowed_type))
                        .collect();
                        add_polarity_aware_narrowings(
                            ctx,
                            &entity.parameters,
                            Language::Dart,
                            fact,
                            &narrowed,
                        );
                    }
                    ControlFlowFactKind::Match => {
                        for result in narrow_dart_switch(&fact.text) {
                            ctx.add_narrowed_type(result.variable_name, result.narrowed_type);
                        }
                    }
                    _ => continue,
                }
            }
        }
    }
}

/// Result of a single narrowing operation.
#[derive(Debug, Clone)]
struct NarrowingResult {
    variable_name: String,
    narrowed_type: TypeBinding,
}

/// Narrow types from a Dart `if` condition.
///
/// Patterns:
/// - `if (x is Type)` → x: Type
/// - `if (x is! Type)` → x: declared-minus-Type (union only)
/// - `if (x != null)` → x: declared (non-null)
/// - `if (x == null)` → x: null
/// - `if (x.field == "value")` → x: narrowed union (discriminated union)
fn narrow_dart_if(
    text: &str,
    ctx: &ScopedTypeContext,
    type_index: Option<&crate::symbol_table::TypeMemberIndex>,
    params: &[(String, Option<String>)],
) -> Vec<NarrowingResult> {
    let text = text.trim();
    let mut results = vec![];

    if let Some(result) = narrow_dart_is_check(text, ctx, params) {
        results.push(result);
        return results;
    }

    if let Some(result) = narrow_dart_null_check(text, ctx, params) {
        results.push(result);
    }

    for result in narrow_dart_discriminated_union(text, ctx, type_index) {
        results.push(result);
    }

    results
}

/// Parse `if (x is Type)` or route `if (x is! Type)` to complement narrowing.
fn narrow_dart_is_check(
    text: &str,
    ctx: &ScopedTypeContext,
    params: &[(String, Option<String>)],
) -> Option<NarrowingResult> {
    let text = strip_dart_condition_prefix(text)?;
    let text = text.trim();

    if text.contains("is!") {
        return narrow_dart_negated_is(text, ctx, params);
    }

    let parts: Vec<&str> = text.splitn(2, " is ").collect();
    if parts.len() != 2 {
        return None;
    }

    let var_name = parts[0].trim();
    let type_name = parts[1].trim();

    if var_name.is_empty() || !is_valid_ident(var_name) || type_name.is_empty() {
        return None;
    }

    Some(NarrowingResult {
        variable_name: var_name.to_string(),
        narrowed_type: TypeBinding {
            type_name: type_name.to_string(),
            type_entity_id: None,
            span: Span::default(),
            origin: Some(super::types::InferenceOrigin::ControlFlowNarrowing),
            shape: None,
        },
    })
}

/// Parse `x is! Type` → x: declared-minus-Type.
///
/// Only fires when the declared shape is a union that the exclusion can
/// actually shrink; a bare null exclusion (`is! Null`) on a non-union
/// declared type keeps the declared type, while other non-union
/// complements stay conservative.
fn narrow_dart_negated_is(
    text: &str,
    ctx: &ScopedTypeContext,
    params: &[(String, Option<String>)],
) -> Option<NarrowingResult> {
    let parts: Vec<&str> = text.splitn(2, "is!").collect();
    if parts.len() != 2 {
        return None;
    }
    let var_name = parts[0].trim();
    let excluded = parts[1].split_whitespace().next()?;
    if var_name.is_empty() || !is_valid_ident(var_name) || excluded.is_empty() {
        return None;
    }
    let declared = declared_shape(ctx, params, Language::Dart, var_name)?;
    let narrowed =
        subtract_union_members(&declared, &[excluded.to_string()]).unwrap_or(declared.clone());
    if excluded != "Null"
        && excluded != "null"
        && type_shape_to_string(&narrowed) == type_shape_to_string(&declared)
    {
        return None;
    }
    Some(NarrowingResult {
        variable_name: var_name.to_string(),
        narrowed_type: TypeBinding {
            type_name: type_shape_to_string(&narrowed),
            type_entity_id: None,
            span: Span::default(),
            origin: Some(super::types::InferenceOrigin::ControlFlowNarrowing),
            shape: Some(narrowed),
        },
    })
}

/// Parse Dart null checks: `x != null` → x: declared, `x == null` → x: null.
/// Receiver of a safe-call chain or null assertion (`x?.foo`, `x!`).
///
/// Returns the base identifier when the text carries an assertion marker,
/// or `None` for plain identifiers and non-identifier expressions, so
/// existing plain-identifier behavior stays untouched.
fn dart_null_asserted_receiver(text: &str) -> Option<&str> {
    let trimmed = text.trim();
    if !trimmed.contains("?.") && !trimmed.ends_with('!') {
        return None;
    }
    let base = trimmed
        .split("?.")
        .next()
        .unwrap_or("")
        .trim()
        .trim_end_matches('!')
        .trim();
    if base.is_empty() || !is_valid_ident(base) {
        return None;
    }
    Some(base)
}

fn narrow_dart_null_check(
    text: &str,
    ctx: &ScopedTypeContext,
    params: &[(String, Option<String>)],
) -> Option<NarrowingResult> {
    let text = strip_dart_condition_prefix(text)?;
    let text = text.trim();
    for (op, negated) in [("!=", true), ("==", false)] {
        let parts: Vec<&str> = text.splitn(2, op).collect();
        if parts.len() != 2 {
            continue;
        }
        let raw_name = parts[0].trim();
        let rhs = parts[1].trim();
        if rhs != "null" || raw_name.is_empty() {
            continue;
        }
        if !negated {
            // Equality binds null only for plain identifiers: an asserted
            // form never proves the receiver itself is null.
            if !is_valid_ident(raw_name) {
                continue;
            }
            return Some(NarrowingResult {
                variable_name: raw_name.to_string(),
                narrowed_type: TypeBinding {
                    type_name: "null".to_string(),
                    type_entity_id: None,
                    span: Span::default(),
                    origin: Some(super::types::InferenceOrigin::ControlFlowNarrowing),
                    shape: None,
                },
            });
        }
        // Inequality narrows the declared union; safe-call receivers and
        // null assertions narrow their base identifier.
        let var_name = if is_valid_ident(raw_name) {
            raw_name
        } else if let Some(receiver) = dart_null_asserted_receiver(raw_name) {
            receiver
        } else {
            continue;
        };
        let declared = declared_shape(ctx, params, Language::Dart, var_name)?;
        let narrowed =
            subtract_union_members(&declared, &["null".to_string()]).unwrap_or(declared.clone());
        return Some(NarrowingResult {
            variable_name: var_name.to_string(),
            narrowed_type: TypeBinding {
                type_name: type_shape_to_string(&narrowed),
                type_entity_id: None,
                span: Span::default(),
                origin: Some(super::types::InferenceOrigin::ControlFlowNarrowing),
                shape: Some(narrowed),
            },
        });
    }
    None
}

/// Narrow types from Dart switch case arms.
///
/// `case String s:` / `case int n when ...:` binds the arm designation.
/// Literal, null and multi-token (guard-heavy) labels stay conservative.
fn narrow_dart_switch(text: &str) -> Vec<NarrowingResult> {
    let mut results = vec![];
    let mut search_start = 0;
    while let Some(case_pos) = text[search_start..].find("case ") {
        let abs_case = search_start + case_pos;
        // Case labels start a statement: the previous non-whitespace char
        // must open the switch body or terminate the prior arm. This keeps
        // string literals such as `log("case int i:")` from binding.
        let prev = text[..abs_case].chars().rev().find(|c| !c.is_whitespace());
        if !matches!(prev, None | Some('{') | Some('}') | Some(';')) {
            search_start = abs_case + 5;
            continue;
        }
        let after_case = &text[abs_case + 5..];
        let label_end = after_case.find(':').unwrap_or(after_case.len());
        let mut label = after_case[..label_end].trim();
        if let Some(when_pos) = label.find(" when ") {
            label = label[..when_pos].trim();
        }
        let tokens: Vec<&str> = label.split_whitespace().collect();
        if tokens.len() == 2 && is_valid_ident(tokens[0]) && is_valid_ident(tokens[1]) {
            results.push(NarrowingResult {
                variable_name: tokens[1].to_string(),
                narrowed_type: TypeBinding {
                    type_name: tokens[0].to_string(),
                    type_entity_id: None,
                    span: Span::default(),
                    origin: Some(super::types::InferenceOrigin::ControlFlowNarrowing),
                    shape: None,
                },
            });
        }
        search_start = abs_case + 5;
    }
    results
}

/// Strip Dart condition prefixes.
fn strip_dart_condition_prefix(text: &str) -> Option<&str> {
    let text = text.trim();
    for prefix in &["if", "while", "else if", "return", "assert"] {
        if let Some(rest) = text.strip_prefix(prefix) {
            let rest = rest.trim();
            // Real facts carry the branch body after the condition, so
            // prefer balanced-paren extraction over naive outer stripping.
            return Some(extract_balanced_parens(rest).unwrap_or_else(|| strip_outer_parens(rest)));
        }
    }
    None
}

/// Dart discriminated union narrowing: `x.field == "value"` → x: narrowed union.
///
/// Dart 3 sealed classes use field-based discrimination patterns similar to
/// TypeScript and Python. This function handles equality checks like:
/// - `if (x.kind == "circle")` → x: Circle
/// - `if (shape.type == "rectangle")` → x: Rectangle
fn narrow_dart_discriminated_union(
    text: &str,
    ctx: &ScopedTypeContext,
    type_index: Option<&crate::symbol_table::TypeMemberIndex>,
) -> Vec<NarrowingResult> {
    let Some((var_name, field_name, value)) = parse_dart_equality_pattern(text) else {
        return vec![];
    };
    let mut results = Vec::new();
    if let Some(existing) = ctx.get_variable_type(&var_name) {
        if let Some(shape) = existing
            .shape
            .clone()
            .or_else(|| parse_type_shape(&existing.type_name, Language::Dart))
        {
            if let Some(narrowed) =
                narrow_discriminated_union(&shape, &field_name, &value, type_index)
            {
                let type_name = type_shape_to_string(&narrowed);
                results.push(NarrowingResult {
                    variable_name: var_name.clone(),
                    narrowed_type: TypeBinding {
                        type_name: type_name.clone(),
                        type_entity_id: None,
                        span: Span::default(),
                        origin: Some(super::types::InferenceOrigin::ControlFlowNarrowing),
                        shape: Some(narrowed),
                    },
                });
                return results;
            }
        }
    }
    results
}

/// Parse Dart equality pattern like `x.field == "value"` or `x.field == 'value'`.
fn parse_dart_equality_pattern(text: &str) -> Option<(String, String, String)> {
    let raw = strip_dart_condition_prefix(text)?;
    let cleaned = raw.trim().to_string();
    // Handle `==` operator
    let pos = cleaned.find("==")?;
    let left = cleaned[..pos].trim();
    let right = cleaned[pos + 2..].trim();
    if left.is_empty() || right.is_empty() {
        return None;
    }
    // Ensure we have dot notation for field access
    let dot_pos = left.rfind('.')?;
    let var_name = left[..dot_pos].trim().to_string();
    let field_name = left[dot_pos + 1..].trim().to_string();
    if var_name.is_empty() || field_name.is_empty() {
        return None;
    }
    if !is_valid_ident(&var_name) || !is_valid_ident(&field_name) {
        return None;
    }
    // Parse string literal (single or double quotes)
    let value = parse_string_literal(right)?;
    Some((var_name, field_name, value))
}

/// Parse a string literal, handling both single and double quotes.
fn parse_string_literal(text: &str) -> Option<String> {
    let text = text.trim();
    if text.starts_with('"') && text.ends_with('"') && text.len() >= 2 {
        return Some(text[1..text.len() - 1].to_string());
    }
    if text.starts_with('\'') && text.ends_with('\'') && text.len() >= 2 {
        return Some(text[1..text.len() - 1].to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::entity::EntityId;
    use cce_types::language::Language;

    fn dummy_span() -> Span {
        Span::default()
    }

    #[test]
    fn test_dart_method_signature() {
        let mut ctx = ScopedTypeContext::new(Language::Dart);
        let entities = vec![
            Entity::new(
                EntityId(1),
                EntityKind::Method,
                "getValue".to_string(),
                dummy_span(),
            )
            .with_return_type(Some("String".to_string())),
        ];

        DartTypeInferer.infer_declarations(&entities, &mut ctx);
        let rt = ctx.get_return_type(EntityId(1)).unwrap();
        assert_eq!(rt.type_name, "String");
    }

    #[test]
    fn test_dart_var_declaration() {
        let mut ctx = ScopedTypeContext::new(Language::Dart);
        let entities = vec![
            Entity::new(
                EntityId(2),
                EntityKind::Variable,
                "name".to_string(),
                dummy_span(),
            )
            .with_metadata("var_type", "String"),
        ];

        DartTypeInferer.infer_declarations(&entities, &mut ctx);
        let vt = ctx.get_variable_type("name").unwrap();
        assert_eq!(vt.type_name, "String");
        assert!(vt.origin.is_some());
    }

    #[test]
    fn test_dart_constructor_call() {
        let mut ctx = ScopedTypeContext::new(Language::Dart);
        let entities = vec![
            Entity::new(
                EntityId(3),
                EntityKind::Variable,
                "user".to_string(),
                dummy_span(),
            )
            .with_metadata("constructor_type", "User"),
        ];

        DartTypeInferer.infer_declarations(&entities, &mut ctx);
        let vt = ctx.get_variable_type("user").unwrap();
        assert_eq!(vt.type_name, "User");
        assert!(vt.origin.is_some());
    }

    #[test]
    fn test_dart_explicit_type() {
        let mut ctx = ScopedTypeContext::new(Language::Dart);
        let entities = vec![
            Entity::new(
                EntityId(4),
                EntityKind::Variable,
                "count".to_string(),
                dummy_span(),
            )
            .with_metadata("explicit_type", "int"),
        ];

        DartTypeInferer.infer_declarations(&entities, &mut ctx);
        let vt = ctx.get_variable_type("count").unwrap();
        assert_eq!(vt.type_name, "int");
        assert!(vt.origin.is_some());
    }

    #[test]
    fn test_dart_is_check() {
        let results = narrow_dart_if(
            "if (x is String)",
            &ScopedTypeContext::new(Language::Dart),
            None,
            &[],
        );
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "x");
        assert_eq!(results[0].narrowed_type.type_name, "String");
        assert!(results[0].narrowed_type.origin.is_some());
    }

    #[test]
    fn test_dart_is_check_with_body() {
        // Fact text carries the branch body; balanced-paren extraction must
        // still isolate the condition.
        let results = narrow_dart_if(
            "if (x is String) { return x; }",
            &ScopedTypeContext::new(Language::Dart),
            None,
            &[],
        );
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "x");
        assert_eq!(results[0].narrowed_type.type_name, "String");
    }

    #[test]
    fn test_dart_is_not_without_declared_skipped() {
        let results = narrow_dart_if(
            "if (x is! String)",
            &ScopedTypeContext::new(Language::Dart),
            None,
            &[],
        );
        assert!(results.is_empty());
    }

    #[test]
    fn test_dart_is_not_complement() {
        let ctx = ScopedTypeContext::new(Language::Dart);
        let params = [("x".to_string(), Some("String | int".to_string()))];
        let results = narrow_dart_if("if (x is! String)", &ctx, None, &params);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "x");
        assert_eq!(results[0].narrowed_type.type_name, "int");
    }

    #[test]
    fn test_dart_is_not_with_body_complement() {
        let ctx = ScopedTypeContext::new(Language::Dart);
        let params = [("x".to_string(), Some("String | int".to_string()))];
        let results = narrow_dart_if("if (x is! String) { return 'no'; }", &ctx, None, &params);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "x");
        assert_eq!(results[0].narrowed_type.type_name, "int");
    }

    #[test]
    fn test_dart_is_not_plain_type_skipped() {
        let ctx = ScopedTypeContext::new(Language::Dart);
        let params = [("obj".to_string(), Some("Object".to_string()))];
        let results = narrow_dart_if("if (obj is! String)", &ctx, None, &params);
        assert!(results.is_empty());
    }

    #[test]
    fn test_dart_is_not_null_narrows_declared() {
        let ctx = ScopedTypeContext::new(Language::Dart);
        let params = [("value".to_string(), Some("String".to_string()))];
        let results = narrow_dart_if("if (value is! Null)", &ctx, None, &params);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "value");
        assert_eq!(results[0].narrowed_type.type_name, "String");
    }

    #[test]
    fn test_dart_not_null_narrows_declared() {
        let ctx = ScopedTypeContext::new(Language::Dart);
        let params = [("value".to_string(), Some("String".to_string()))];
        let results = narrow_dart_if("if (value != null)", &ctx, None, &params);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "value");
        assert_eq!(results[0].narrowed_type.type_name, "String");
    }

    #[test]
    fn test_dart_not_null_without_declared_skipped() {
        let results = narrow_dart_if(
            "if (x != null)",
            &ScopedTypeContext::new(Language::Dart),
            None,
            &[],
        );
        assert!(results.is_empty());
    }

    #[test]
    fn test_dart_safe_call_narrows_receiver() {
        let ctx = ScopedTypeContext::new(Language::Dart);
        let params = [("user".to_string(), Some("String?".to_string()))];
        let results = narrow_dart_if("if (user?.length != null)", &ctx, None, &params);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "user");
        assert_eq!(results[0].narrowed_type.type_name, "String");
    }

    #[test]
    fn test_dart_assertion_narrows_receiver() {
        let ctx = ScopedTypeContext::new(Language::Dart);
        let params = [("user".to_string(), Some("String?".to_string()))];
        let results = narrow_dart_if("if (user! != null)", &ctx, None, &params);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "user");
        assert_eq!(results[0].narrowed_type.type_name, "String");
    }

    #[test]
    fn test_dart_safe_call_equal_null_stays_empty() {
        let ctx = ScopedTypeContext::new(Language::Dart);
        let params = [("user".to_string(), Some("String?".to_string()))];
        let results = narrow_dart_if("if (user?.length == null)", &ctx, None, &params);
        assert!(results.is_empty());
    }

    #[test]
    fn test_dart_equal_null_binds_null() {
        let ctx = ScopedTypeContext::new(Language::Dart);
        let results = narrow_dart_if("if (value == null)", &ctx, None, &[]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "value");
        assert_eq!(results[0].narrowed_type.type_name, "null");
    }

    #[test]
    fn test_dart_field_type() {
        let mut ctx = ScopedTypeContext::new(Language::Dart);
        let entities = vec![
            Entity::new(
                EntityId(5),
                EntityKind::Field,
                "name".to_string(),
                dummy_span(),
            )
            .with_metadata("type_annotation", "String"),
        ];

        DartTypeInferer.infer_declarations(&entities, &mut ctx);
        let ft = ctx.get_variable_type("name").unwrap();
        assert_eq!(ft.type_name, "String");
        assert!(ft.origin.is_some());
    }

    #[test]
    fn test_dart_discriminated_union_fallback() {
        // Deterministic: without TypeMemberIndex no heuristic fallback, returns empty
        let results = narrow_dart_if(
            "if (shape.kind == \"circle\")",
            &ScopedTypeContext::new(Language::Dart),
            None,
            &[],
        );
        assert!(results.is_empty());
    }

    #[test]
    fn test_dart_parse_equality_pattern() {
        let result = parse_dart_equality_pattern("if (x.type == \"value\")");
        assert!(result.is_some());
        let (var, field, value) = result.unwrap();
        assert_eq!(var, "x");
        assert_eq!(field, "type");
        assert_eq!(value, "value");
    }

    #[test]
    fn test_dart_parse_equality_pattern_single_quotes() {
        let result = parse_dart_equality_pattern("if (shape.kind == 'circle')");
        assert!(result.is_some());
        let (var, field, value) = result.unwrap();
        assert_eq!(var, "shape");
        assert_eq!(field, "kind");
        assert_eq!(value, "circle");
    }

    #[test]
    fn test_dart_switch_pattern_arm() {
        let results = narrow_dart_switch(
            "switch (obj) { case String s: print(s); case int n: print(n); default: print(x); }",
        );
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].variable_name, "s");
        assert_eq!(results[0].narrowed_type.type_name, "String");
        assert_eq!(results[1].variable_name, "n");
        assert_eq!(results[1].narrowed_type.type_name, "int");
    }

    #[test]
    fn test_dart_switch_guard_arm() {
        let results = narrow_dart_switch(
            "switch (obj) { case int n when n > 0: print(n); default: print(x); }",
        );
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "n");
        assert_eq!(results[0].narrowed_type.type_name, "int");
    }

    #[test]
    fn test_dart_switch_literal_and_string_skipped() {
        let results = narrow_dart_switch(
            "switch (x) { case 1: a(); case \"s\": b(); case null: c(); default: d(); }",
        );
        assert!(results.is_empty());
    }
}
