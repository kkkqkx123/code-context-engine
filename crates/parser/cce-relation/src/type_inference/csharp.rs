//! C#-specific type inference.
//!
//! Handles C#-specific patterns:
//! - Method signatures with generic parameters
//! - `var x = new T()` local variable type inference
//! - `var x = expr` and `T x = new T()` patterns
//! - Lambda expressions and anonymous methods
//! - Class/struct/interface/enum field types
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

/// C# type inference implementation.
pub struct CSharpTypeInferer;

impl LanguageTypeInferer for CSharpTypeInferer {
    fn infer_declarations(&self, entities: &[Entity], ctx: &mut ScopedTypeContext) {
        for entity in entities {
            match entity.kind {
                EntityKind::Function | EntityKind::Method | EntityKind::Constructor => {
                    extract_function_types(entity, ctx);
                }
                EntityKind::Variable => {
                    extract_variable_type(entity, ctx);

                    if let Some(var_type) = entity.metadata.get("var_type") {
                        let binding = TypeBinding {
                            type_name: var_type.clone(),
                            type_entity_id: None,
                            span: entity.span,
                            origin: Some(super::types::InferenceOrigin::TypeAnnotation),
                            shape: parse_type_shape(var_type, Language::CSharp),
                        };
                        ctx.add_variable_type(entity.name.clone(), binding);
                    }

                    // Constructor calls (`var x = new T()`, `T x = new T()`)
                    // are handled by the shared `extract_variable_type`, which
                    // binds `constructor_type` with a resolved shape only when
                    // no concrete annotation is present. No duplicate handling
                    // here so explicit annotations keep priority.
                    if let Some(inferred) = entity.metadata.get("inferred_type") {
                        let binding = TypeBinding {
                            type_name: inferred.clone(),
                            type_entity_id: None,
                            span: entity.span,
                            origin: Some(super::types::InferenceOrigin::GenericInference),
                            shape: parse_type_shape(inferred, Language::CSharp),
                        };
                        ctx.add_variable_type(entity.name.clone(), binding);
                    }

                    if let Some(explicit) = entity.metadata.get("explicit_type") {
                        let binding = TypeBinding {
                            type_name: explicit.clone(),
                            type_entity_id: None,
                            span: entity.span,
                            origin: Some(super::types::InferenceOrigin::TypeAnnotation),
                            shape: parse_type_shape(explicit, Language::CSharp),
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
                    ControlFlowFactKind::If | ControlFlowFactKind::Loop => {
                        let mut narrowed: Vec<(String, TypeBinding)> = narrow_csharp_if(
                            &fact.text,
                            ctx,
                            inference_ctx.type_index(),
                            &entity.parameters,
                        )
                        .into_iter()
                        .map(|result| (result.variable_name, result.narrowed_type))
                        .collect();
                        for (_, binding) in narrowed.iter_mut() {
                            if !binding.span.is_available() {
                                binding.span = entity.span;
                            }
                        }
                        add_polarity_aware_narrowings(
                            ctx,
                            &entity.parameters,
                            Language::CSharp,
                            fact,
                            &narrowed,
                        );
                    }
                    ControlFlowFactKind::Match => {
                        for result in narrow_csharp_switch(&fact.text) {
                            ctx.add_narrowed_type_anchored(
                                result.variable_name,
                                result.narrowed_type,
                                entity.span,
                            );
                        }
                    }
                    ControlFlowFactKind::Try => {
                        for result in narrow_csharp_catch(&fact.text) {
                            let mut variable_binding = result.narrowed_type.clone();
                            if !variable_binding.span.is_available() {
                                variable_binding.span = entity.span;
                            }
                            ctx.add_variable_type(result.variable_name.clone(), variable_binding);
                            ctx.add_narrowed_type_anchored(
                                result.variable_name,
                                result.narrowed_type,
                                entity.span,
                            );
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

/// Narrow types from a C# `if` condition.
///
/// Patterns:
/// - `if (x is Type)` → x: Type (`x is Type name` binds the designation)
/// - `if (x is not Type)` → x: declared-minus-Type (union only)
/// - `if (x is not null)` → x: declared (non-null)
/// - `if (x.field == "value")` → x: narrowed union (discriminated union)
fn narrow_csharp_if(
    text: &str,
    ctx: &ScopedTypeContext,
    type_index: Option<&crate::symbol_table::TypeMemberIndex>,
    params: &[(String, Option<String>)],
) -> Vec<NarrowingResult> {
    let text = text.trim();
    let mut results = vec![];

    if let Some(result) = narrow_csharp_is_check(text, ctx, params) {
        results.push(result);
        return results;
    }

    for result in narrow_csharp_discriminated_union(text, ctx, type_index) {
        results.push(result);
    }

    results
}

/// Parse `if (x is Type)`, `if (x is not Type)` and `is [not] null` checks.
fn narrow_csharp_is_check(
    text: &str,
    ctx: &ScopedTypeContext,
    params: &[(String, Option<String>)],
) -> Option<NarrowingResult> {
    let text = strip_csharp_condition_prefix(text)?;
    let text = text.trim();

    // Negated `is` checks narrow the complement instead.
    if let Some(rest) = split_is_not(text) {
        return narrow_csharp_is_not(text, rest, ctx, params);
    }

    let parts: Vec<&str> = text.splitn(2, " is ").collect();
    if parts.len() != 2 {
        return None;
    }

    let var_name = parts[0].trim();
    let rhs = parts[1].trim();

    if var_name.is_empty() || !is_valid_ident(var_name) || rhs.is_empty() {
        return None;
    }

    // `x is Type name` binds the designation, not `x`.
    let mut rhs_parts = rhs.split_whitespace();
    let type_name = rhs_parts.next()?;
    let (bind_name, bind_type) = match rhs_parts.next() {
        Some(designation) if is_valid_ident(designation) && rhs_parts.next().is_none() => {
            (designation.to_string(), type_name.to_string())
        }
        _ => (var_name.to_string(), rhs.to_string()),
    };

    Some(NarrowingResult {
        variable_name: bind_name,
        narrowed_type: TypeBinding {
            type_name: bind_type,
            type_entity_id: None,
            span: Span::default(),
            origin: Some(super::types::InferenceOrigin::ControlFlowNarrowing),
            shape: None,
        },
    })
}

/// Split `x is not T` into (`x`, `T`).
fn split_is_not(text: &str) -> Option<(&str, &str)> {
    let parts: Vec<&str> = text.splitn(2, " is not ").collect();
    if parts.len() != 2 {
        return None;
    }
    Some((parts[0].trim(), parts[1].trim()))
}

/// Parse `x is not Type` → x: declared-minus-Type (union only), and
/// `x is not null` → x: declared (non-null).
fn narrow_csharp_is_not(
    _full: &str,
    rest: (&str, &str),
    ctx: &ScopedTypeContext,
    params: &[(String, Option<String>)],
) -> Option<NarrowingResult> {
    let (var_name, excluded) = rest;
    if var_name.is_empty() || !is_valid_ident(var_name) || excluded.is_empty() {
        return None;
    }
    // `x is not Type name` excludes the type, not the designation.
    let excluded = excluded.split_whitespace().next().unwrap_or(excluded);
    let declared = declared_shape(ctx, params, Language::CSharp, var_name)?;
    let narrowed =
        subtract_union_members(&declared, &[excluded.to_string()]).unwrap_or(declared.clone());
    // A bare `is not null` on a non-union declared type keeps the declared
    // type (null excluded); other non-union complements stay conservative.
    if excluded != "null" && type_shape_to_string(&narrowed) == type_shape_to_string(&declared) {
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

/// Strip C# condition prefixes.
fn strip_csharp_condition_prefix(text: &str) -> Option<&str> {
    let text = text.trim();
    for prefix in &["if", "while", "else if", "return", "assert"] {
        if let Some(rest) = text.strip_prefix(prefix) {
            let rest = rest.trim();
            // Fact text carries the branch body (`if (c) { ... }`), so take
            // the balanced paren group instead of requiring a clean suffix.
            return Some(extract_balanced_parens(rest).unwrap_or_else(|| strip_outer_parens(rest)));
        }
    }
    None
}

/// Narrow types from C# switch case arms.
///
/// `case string s:` / `case int i when ...:` binds the arm designation.
/// Literal, null and multi-token (recursive/guard-heavy) labels stay
/// conservative.
fn narrow_csharp_switch(text: &str) -> Vec<NarrowingResult> {
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

/// C# discriminated union narrowing: `x.field == "value"` → x: narrowed union.
///
/// C# 9+ record types and discriminated unions use field-based discrimination
/// patterns similar to TypeScript and Python. This function handles equality
/// checks like:
/// - `if (shape.Kind == "Circle")` → shape: Circle
/// - `if (result.Status == "Success")` → result: Success
fn narrow_csharp_discriminated_union(
    text: &str,
    ctx: &ScopedTypeContext,
    type_index: Option<&crate::symbol_table::TypeMemberIndex>,
) -> Vec<NarrowingResult> {
    let Some((var_name, field_name, value)) = parse_csharp_equality_pattern(text) else {
        return vec![];
    };
    let mut results = Vec::new();
    if let Some(existing) = ctx.get_variable_type(&var_name) {
        if let Some(shape) = existing
            .shape
            .clone()
            .or_else(|| parse_type_shape(&existing.type_name, Language::CSharp))
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

/// Parse C# equality pattern like `x.Field == "value"` or `x.Field == 'value'`.
fn parse_csharp_equality_pattern(text: &str) -> Option<(String, String, String)> {
    let raw = strip_csharp_condition_prefix(text)?;
    let cleaned = raw.trim().to_string();
    // Handle `==` operator
    let pos = cleaned.find("==")?;
    let left = cleaned[..pos].trim();
    let right = cleaned[pos + 2..].trim();
    if left.is_empty() || right.is_empty() {
        return None;
    }
    // Ensure we have dot notation for property access
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

/// Narrow types from a C# `try-catch` block.
///
/// Patterns:
/// - `try {} catch (InvalidOperationException ex) {}` → ex: InvalidOperationException
/// - `try {} catch (Exception ex) when (ex.Message != null) {}` → ex: Exception
fn narrow_csharp_catch(text: &str) -> Vec<NarrowingResult> {
    let mut results = vec![];
    let mut search = text;
    while let Some(catch_pos) = search.find("catch") {
        let after_catch = &search[catch_pos + 5..];
        let after_catch = after_catch.trim_start();
        if !after_catch.starts_with('(') {
            search = &after_catch[1.min(after_catch.len())..];
            continue;
        }
        let mut depth = 0;
        let mut end = None;
        for (i, ch) in after_catch.char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(end_pos) = end else {
            break;
        };
        let param = after_catch[1..end_pos].trim();
        if let Some(result) = parse_csharp_catch_param(param) {
            results.push(result);
        }
        search = &after_catch[end_pos + 1..];
    }
    results
}

/// Parse a C# catch parameter `InvalidOperationException ex` or `Exception ex when ...`.
fn parse_csharp_catch_param(param: &str) -> Option<NarrowingResult> {
    let param = param.trim();
    if param.is_empty() {
        return None;
    }
    // Strip exception filter `when (...)` if present
    let param = param.split(" when ").next().unwrap_or(param).trim();
    // Last token is variable name, preceding is type
    let last_space = param.rfind(char::is_whitespace)?;
    let type_part = param[..last_space].trim();
    let var_name = param[last_space + 1..].trim();
    if type_part.is_empty() || var_name.is_empty() || !is_valid_ident(var_name) {
        return None;
    }
    Some(NarrowingResult {
        variable_name: var_name.to_string(),
        narrowed_type: TypeBinding {
            type_name: type_part.to_string(),
            type_entity_id: None,
            span: Span::default(),
            origin: Some(super::types::InferenceOrigin::ControlFlowNarrowing),
            shape: None,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::type_inference::InferenceOrigin;
    use cce_types::Span;
    use cce_types::entity::EntityId;
    use cce_types::language::Language;

    fn dummy_span() -> Span {
        Span::default()
    }

    #[test]
    fn test_csharp_method_signature() {
        let mut ctx = ScopedTypeContext::new(Language::CSharp);
        let entities = vec![
            Entity::new(
                EntityId(1),
                EntityKind::Method,
                "GetValue".to_string(),
                dummy_span(),
            )
            .with_return_type(Some("string".to_string())),
        ];

        CSharpTypeInferer.infer_declarations(&entities, &mut ctx);
        let rt = ctx.get_return_type(EntityId(1)).unwrap();
        assert_eq!(rt.type_name, "string");
    }

    #[test]
    fn test_csharp_var_new() {
        let mut ctx = ScopedTypeContext::new(Language::CSharp);
        let entities = vec![
            Entity::new(
                EntityId(2),
                EntityKind::Variable,
                "list".to_string(),
                dummy_span(),
            )
            .with_metadata("constructor_type", "List<string>"),
        ];

        CSharpTypeInferer.infer_declarations(&entities, &mut ctx);
        let vt = ctx.get_variable_type("list").unwrap();
        assert_eq!(vt.type_name, "List<string>");
    }

    #[test]
    fn test_csharp_var_inferred() {
        let mut ctx = ScopedTypeContext::new(Language::CSharp);
        let entities = vec![
            Entity::new(
                EntityId(3),
                EntityKind::Variable,
                "x".to_string(),
                dummy_span(),
            )
            .with_metadata("var_type", "int"),
        ];

        CSharpTypeInferer.infer_declarations(&entities, &mut ctx);
        let vt = ctx.get_variable_type("x").unwrap();
        assert_eq!(vt.type_name, "int");
        assert!(vt.origin.is_some());
    }

    #[test]
    fn test_csharp_field_type() {
        let mut ctx = ScopedTypeContext::new(Language::CSharp);
        let entities = vec![
            Entity::new(
                EntityId(4),
                EntityKind::Field,
                "Count".to_string(),
                dummy_span(),
            )
            .with_metadata("type_annotation", "int"),
        ];

        CSharpTypeInferer.infer_declarations(&entities, &mut ctx);
        let ft = ctx.get_variable_type("Count").unwrap();
        assert_eq!(ft.type_name, "int");
        assert!(ft.origin.is_some());
    }

    #[test]
    fn test_csharp_property_type() {
        let mut ctx = ScopedTypeContext::new(Language::CSharp);
        let entities = vec![
            Entity::new(
                EntityId(5),
                EntityKind::Property,
                "Name".to_string(),
                dummy_span(),
            )
            .with_metadata("type_annotation", "string"),
        ];

        CSharpTypeInferer.infer_declarations(&entities, &mut ctx);
        let pt = ctx.get_variable_type("Name").unwrap();
        assert_eq!(pt.type_name, "string");
    }

    #[test]
    fn test_csharp_is_check() {
        let results = narrow_csharp_if(
            "if (x is string)",
            &ScopedTypeContext::new(Language::CSharp),
            None,
            &[],
        );
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "x");
        assert_eq!(results[0].narrowed_type.type_name, "string");
        assert!(results[0].narrowed_type.origin.is_some());
    }

    #[test]
    fn test_csharp_is_not_complement() {
        let ctx = ScopedTypeContext::new(Language::CSharp);
        let params = [("x".to_string(), Some("string | int".to_string()))];
        let results = narrow_csharp_if("if (x is not string)", &ctx, None, &params);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "x");
        assert_eq!(results[0].narrowed_type.type_name, "int");
    }

    #[test]
    fn test_csharp_is_not_plain_type_skipped() {
        let ctx = ScopedTypeContext::new(Language::CSharp);
        let params = [("obj".to_string(), Some("object".to_string()))];
        let results = narrow_csharp_if("if (obj is not string)", &ctx, None, &params);
        assert!(results.is_empty());
    }

    #[test]
    fn test_csharp_is_not_null_narrows_declared() {
        let ctx = ScopedTypeContext::new(Language::CSharp);
        let params = [("value".to_string(), Some("string".to_string()))];
        let results = narrow_csharp_if("if (value is not null)", &ctx, None, &params);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "value");
        assert_eq!(results[0].narrowed_type.type_name, "string");
    }

    #[test]
    fn test_csharp_is_designation_binds_name() {
        let results = narrow_csharp_if(
            "if (obj is string s)",
            &ScopedTypeContext::new(Language::CSharp),
            None,
            &[],
        );
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "s");
        assert_eq!(results[0].narrowed_type.type_name, "string");
    }

    #[test]
    fn test_csharp_switch_case_arm() {
        let results = narrow_csharp_switch(
            "switch (obj) { case string s: a(); break; case int n when n > 0: b(); break; default: c(); break; }",
        );
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].variable_name, "s");
        assert_eq!(results[0].narrowed_type.type_name, "string");
        assert_eq!(results[1].variable_name, "n");
        assert_eq!(results[1].narrowed_type.type_name, "int");
    }

    #[test]
    fn test_csharp_switch_literal_skipped() {
        let results = narrow_csharp_switch(
            "switch (x) { case 1: a(); break; case null: b(); break; default: c(); break; }",
        );
        assert!(results.is_empty());
    }

    #[test]
    fn test_csharp_catch_single() {
        let results = narrow_csharp_catch("try {} catch (InvalidOperationException ex) {}");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "ex");
        assert_eq!(
            results[0].narrowed_type.type_name,
            "InvalidOperationException"
        );
    }

    #[test]
    fn test_csharp_catch_when_filter() {
        let results =
            narrow_csharp_catch("try {} catch (Exception ex) when (ex.Message != null) {}");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "ex");
        assert_eq!(results[0].narrowed_type.type_name, "Exception");
    }

    #[test]
    fn test_csharp_catch_multiple() {
        let results =
            narrow_csharp_catch("try {} catch (IOException e) {} catch (ArgumentException ex) {}");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].variable_name, "e");
        assert_eq!(results[1].variable_name, "ex");
    }

    #[test]
    fn test_csharp_discriminated_union_fallback() {
        // Deterministic: without TypeMemberIndex no heuristic fallback, returns empty
        let results = narrow_csharp_if(
            "if (shape.Kind == \"Circle\")",
            &ScopedTypeContext::new(Language::CSharp),
            None,
            &[],
        );
        assert!(results.is_empty());
    }

    #[test]
    fn test_csharp_parse_equality_pattern() {
        let result = parse_csharp_equality_pattern("if (x.Type == \"value\")");
        assert!(result.is_some());
        let (var, field, value) = result.unwrap();
        assert_eq!(var, "x");
        assert_eq!(field, "Type");
        assert_eq!(value, "value");
    }

    #[test]
    fn test_csharp_parse_equality_pattern_single_quotes() {
        let result = parse_csharp_equality_pattern("if (shape.Kind == 'Circle')");
        assert!(result.is_some());
        let (var, field, value) = result.unwrap();
        assert_eq!(var, "shape");
        assert_eq!(field, "Kind");
        assert_eq!(value, "Circle");
    }

    // ==================== Additional control flow tests ====================

    #[test]
    fn test_csharp_catch_multiple_different_types() {
        let results = narrow_csharp_catch(
            "try {} catch (IOException e) {} catch (InvalidOperationException ex) {} catch (ArgumentException err) {}",
        );
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].variable_name, "e");
        assert_eq!(results[0].narrowed_type.type_name, "IOException");
        assert_eq!(results[1].variable_name, "ex");
        assert_eq!(
            results[1].narrowed_type.type_name,
            "InvalidOperationException"
        );
        assert_eq!(results[2].variable_name, "err");
        assert_eq!(results[2].narrowed_type.type_name, "ArgumentException");
    }

    #[test]
    fn test_csharp_catch_nested_generic_type() {
        let results = narrow_csharp_catch("try {} catch (KeyNotFoundException<string> ex) {}");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "ex");
        assert_eq!(
            results[0].narrowed_type.type_name,
            "KeyNotFoundException<string>"
        );
    }

    #[test]
    fn test_csharp_is_check_in_while() {
        let results = narrow_csharp_if(
            "while (x is string)",
            &ScopedTypeContext::new(Language::CSharp),
            None,
            &[],
        );
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "x");
        assert_eq!(results[0].narrowed_type.type_name, "string");
    }

    #[test]
    fn test_csharp_is_check_in_else_if() {
        let results = narrow_csharp_if(
            "else if (x is int)",
            &ScopedTypeContext::new(Language::CSharp),
            None,
            &[],
        );
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "x");
        assert_eq!(results[0].narrowed_type.type_name, "int");
    }

    #[test]
    fn test_csharp_is_check_in_return() {
        let results = narrow_csharp_if(
            "return x is List<string>",
            &ScopedTypeContext::new(Language::CSharp),
            None,
            &[],
        );
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "x");
        assert_eq!(results[0].narrowed_type.type_name, "List<string>");
    }

    #[test]
    fn test_csharp_discriminated_union_with_union_shape() {
        // Deterministic: heuristic removed; without TypeMemberIndex returns empty
        let mut ctx = ScopedTypeContext::new(Language::CSharp);
        ctx.add_variable_type(
            "shape".to_string(),
            TypeBinding {
                type_name: "Circle | Square | Triangle".to_string(),
                type_entity_id: None,
                span: Span::default(),
                origin: None,
                shape: parse_type_shape("Circle | Square | Triangle", Language::CSharp),
            },
        );
        let results = narrow_csharp_if("if (shape.Kind == \"Circle\")", &ctx, None, &[]);
        assert!(results.is_empty());
    }

    #[test]
    fn test_csharp_discriminated_union_no_match() {
        // Deterministic: no heuristic fallback, unknown value yields empty
        let mut ctx = ScopedTypeContext::new(Language::CSharp);
        ctx.add_variable_type(
            "shape".to_string(),
            TypeBinding {
                type_name: "Circle | Square".to_string(),
                type_entity_id: None,
                span: Span::default(),
                origin: None,
                shape: parse_type_shape("Circle | Square", Language::CSharp),
            },
        );
        let results = narrow_csharp_if("if (shape.Kind == \"Triangle\")", &ctx, None, &[]);
        assert!(results.is_empty());
    }

    #[test]
    fn test_csharp_var_inferred_generic() {
        let mut ctx = ScopedTypeContext::new(Language::CSharp);
        let entities = vec![
            Entity::new(
                EntityId(10),
                EntityKind::Variable,
                "lookup".to_string(),
                dummy_span(),
            )
            .with_metadata("inferred_type", "Dictionary<string, List<int>>"),
        ];

        CSharpTypeInferer.infer_declarations(&entities, &mut ctx);
        let vt = ctx.get_variable_type("lookup").unwrap();
        assert_eq!(vt.type_name, "Dictionary<string, List<int>>");
        assert_eq!(vt.origin, Some(InferenceOrigin::GenericInference));
    }

    #[test]
    fn test_csharp_explicit_type() {
        let mut ctx = ScopedTypeContext::new(Language::CSharp);
        let entities = vec![
            Entity::new(
                EntityId(11),
                EntityKind::Variable,
                "items".to_string(),
                dummy_span(),
            )
            .with_metadata("explicit_type", "IEnumerable<string>"),
        ];

        CSharpTypeInferer.infer_declarations(&entities, &mut ctx);
        let vt = ctx.get_variable_type("items").unwrap();
        assert_eq!(vt.type_name, "IEnumerable<string>");
        assert_eq!(vt.origin, Some(InferenceOrigin::TypeAnnotation));
    }

    #[test]
    fn test_csharp_var_new_nested_generic() {
        let mut ctx = ScopedTypeContext::new(Language::CSharp);
        let entities = vec![
            Entity::new(
                EntityId(12),
                EntityKind::Variable,
                "nested".to_string(),
                dummy_span(),
            )
            .with_metadata("constructor_type", "List<Dictionary<string, int>>"),
        ];

        CSharpTypeInferer.infer_declarations(&entities, &mut ctx);
        let vt = ctx.get_variable_type("nested").unwrap();
        assert_eq!(vt.type_name, "List<Dictionary<string, int>>");
    }

    #[test]
    fn test_csharp_parse_equality_pattern_with_spaces() {
        let result = parse_csharp_equality_pattern("if (  x.Type  ==  \"value\"  )");
        assert!(result.is_some());
        let (var, field, value) = result.unwrap();
        assert_eq!(var, "x");
        assert_eq!(field, "Type");
        assert_eq!(value, "value");
    }

    #[test]
    fn test_csharp_parse_string_literal_double_quotes() {
        assert_eq!(parse_string_literal("\"hello\""), Some("hello".to_string()));
        assert_eq!(parse_string_literal("'world'"), Some("world".to_string()));
        assert_eq!(parse_string_literal("plain"), None);
    }
}
