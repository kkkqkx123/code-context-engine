//! Java-specific type inference.
//!
//! Reference: `docs/research/eclipse-jdt-type-inference.md`

use cce_types::ControlFlowFactKind;
use cce_types::ControlFlowStore;
use cce_types::Language;
use cce_types::Span;
use cce_types::entity::{Entity, EntityKind};

use super::control_flow::shared::{extract_balanced_parens, is_valid_ident, strip_outer_parens};
use super::extractors::{extract_field_type, extract_function_types, extract_variable_type};
use super::traits::LanguageTypeInferer;
use super::types::{
    ScopedTypeContext, TypeBinding, add_polarity_aware_narrowings, declared_shape,
    parse_type_shape, subtract_union_members, type_shape_to_string,
};

/// Java type inference implementation.
///
/// Handles Java-specific patterns:
/// - Method signatures with generic parameters
/// - Field type declarations
/// - `new Constructor<T>()` patterns
/// - `var x = expr` local variable type inference
pub struct JavaTypeInferer;

impl LanguageTypeInferer for JavaTypeInferer {
    fn infer_declarations(&self, entities: &[Entity], ctx: &mut ScopedTypeContext) {
        for entity in entities {
            match entity.kind {
                EntityKind::Function | EntityKind::Method => {
                    extract_function_types(entity, ctx);

                    // Java-specific: store generic type parameter info as metadata
                    // on the parameter types rather than overwriting the return type.
                    // The full generic signature (e.g. "<T extends Number>") is kept
                    // in metadata for downstream consumers that need it.
                    if let Some(_generic_params) = entity.metadata.get("generic_type_params") {
                        // Generic type parameters are informational metadata; the actual
                        // return type and parameter types are already extracted by
                        // extract_function_types. Full generic inference would require
                        // type parameter binding logic which is out of scope.
                    }
                }
                EntityKind::Variable => {
                    extract_variable_type(entity, ctx);

                    if let Some(var_type) = entity.metadata.get("var_type") {
                        let binding = TypeBinding {
                            type_name: var_type.clone(),
                            type_entity_id: None,
                            span: entity.span,
                            origin: Some(super::types::InferenceOrigin::TypeAnnotation),
                            shape: parse_type_shape(var_type, Language::Java),
                        };
                        ctx.add_variable_type(entity.name.clone(), binding);
                    }
                    // Constructor calls (`new Constructor<T>()`) are handled by
                    // the shared `extract_variable_type`, which binds
                    // `constructor_type` with a resolved shape only when no
                    // concrete annotation is present. No duplicate handling
                    // here so explicit annotations keep priority.
                }
                EntityKind::Field | EntityKind::Property => {
                    extract_field_type(entity, ctx);

                    if let Some(field_types) = entity.metadata.get("field_types") {
                        let binding = TypeBinding {
                            type_name: field_types.clone(),
                            type_entity_id: None,
                            span: entity.span,
                            origin: None,
                            shape: parse_type_shape(field_types, Language::Java),
                        };
                        ctx.add_variable_type(entity.name.clone(), binding);
                    }
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
        _inference_ctx: &super::traits::InferenceContext<'_>,
    ) {
        for entity in entities {
            let Some(entity_cf) = control_flow.get(entity.id) else {
                continue;
            };
            for fact in &entity_cf.facts {
                match fact.kind {
                    ControlFlowFactKind::If | ControlFlowFactKind::Loop => {
                        let mut narrowed: Vec<(String, TypeBinding)> =
                            narrow_java_if(&fact.text, ctx, &entity.parameters)
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
                            Language::Java,
                            fact,
                            &narrowed,
                        );
                    }
                    ControlFlowFactKind::Match => {
                        for result in narrow_java_switch(&fact.text) {
                            ctx.add_narrowed_type_anchored(
                                result.variable_name,
                                result.narrowed_type,
                                entity.span,
                            );
                        }
                    }
                    ControlFlowFactKind::Try => {
                        for result in narrow_java_catch(&fact.text) {
                            let mut variable_binding = result.narrowed_type.clone();
                            if !variable_binding.span.is_available() {
                                variable_binding.span = entity.span;
                            }
                            ctx.add_variable_type(result.variable_name.clone(), variable_binding);
                            // Also add as narrowed for scope sensitivity
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

/// Narrow types from a Java `if` condition.
///
/// Patterns:
/// - `if (x instanceof Type)` → x: Type
/// - `if (x instanceof Type name)` → name: Type (pattern variable)
/// - `if (!(x instanceof Type))` → x: declared-minus-Type (union only)
/// - `if (x != null)` → x: declared (non-null)
/// - `if (x == null)` → x: null
fn narrow_java_if(
    text: &str,
    ctx: &ScopedTypeContext,
    params: &[(String, Option<String>)],
) -> Vec<NarrowingResult> {
    // Split `A && B` so compound guards narrow per-conjunct instead of
    // smearing the right side into a pseudo-type.
    if let Some(cond) = strip_java_condition_prefix(text) {
        let parts = super::control_flow::shared::split_top_level_conjuncts(cond);
        if parts.len() > 1 {
            let mut out = Vec::new();
            for part in parts {
                out.extend(narrow_single_java_condition(part, ctx, params));
            }
            return out;
        }
    }
    narrow_single_java_condition(text, ctx, params)
}

fn narrow_single_java_condition(
    text: &str,
    ctx: &ScopedTypeContext,
    params: &[(String, Option<String>)],
) -> Vec<NarrowingResult> {
    let text = text.trim();
    let mut results = vec![];

    if let Some(result) = narrow_java_instanceof(text, ctx, params) {
        results.push(result);
    }

    if let Some(result) = narrow_java_null_check(text, ctx, params) {
        results.push(result);
    }

    results
}

/// Parse `if (x instanceof Type)` (with optional pattern variable).
fn narrow_java_instanceof(
    text: &str,
    ctx: &ScopedTypeContext,
    params: &[(String, Option<String>)],
) -> Option<NarrowingResult> {
    let text = strip_java_condition_prefix(text)?;
    let text = text.trim();

    // Negated instanceof checks narrow the complement instead.
    if let Some(inner) = text.strip_prefix('!').map(|rest| {
        let rest = rest.trim();
        extract_balanced_parens(rest).unwrap_or_else(|| strip_outer_parens(rest))
    }) {
        return narrow_java_negated_instanceof(inner, ctx, params);
    }

    let parts: Vec<&str> = text.splitn(2, " instanceof ").collect();
    if parts.len() != 2 {
        return None;
    }

    let var_name = parts[0].trim();
    let rhs = parts[1].trim();

    if var_name.is_empty() || !is_valid_ident(var_name) || rhs.is_empty() {
        return None;
    }

    // `x instanceof Type name` binds the pattern variable, not `x`.
    let mut rhs_parts = rhs.split_whitespace();
    let type_name = rhs_parts.next()?;
    let (bind_name, bind_type) = match rhs_parts.next() {
        Some(pattern_var) if is_valid_ident(pattern_var) && rhs_parts.next().is_none() => {
            (pattern_var.to_string(), type_name.to_string())
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

/// Parse `!(x instanceof Type)` → x: declared-minus-Type.
///
/// Only fires when the declared shape is a union (or Optional-like) that
/// the exclusion can actually shrink; otherwise stays conservative.
fn narrow_java_negated_instanceof(
    inner: &str,
    ctx: &ScopedTypeContext,
    params: &[(String, Option<String>)],
) -> Option<NarrowingResult> {
    let parts: Vec<&str> = inner.splitn(2, " instanceof ").collect();
    if parts.len() != 2 {
        return None;
    }
    let var_name = parts[0].trim();
    let excluded = parts[1].split_whitespace().next()?;
    if var_name.is_empty() || !is_valid_ident(var_name) || excluded.is_empty() {
        return None;
    }
    let declared = declared_shape(ctx, params, Language::Java, var_name)?;
    let narrowed = subtract_union_members(&declared, &[excluded.to_string()])?;
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

/// Parse Java null checks: `x != null` → x: declared, `x == null` → x: null.
fn narrow_java_null_check(
    text: &str,
    ctx: &ScopedTypeContext,
    params: &[(String, Option<String>)],
) -> Option<NarrowingResult> {
    let text = strip_java_condition_prefix(text)?;
    let text = text.trim();
    for (op, negated) in [("!=", true), ("==", false)] {
        let parts: Vec<&str> = text.splitn(2, op).collect();
        if parts.len() != 2 {
            continue;
        }
        let var_name = parts[0].trim();
        let rhs = parts[1].trim();
        if rhs != "null" || var_name.is_empty() || !is_valid_ident(var_name) {
            continue;
        }
        if !negated {
            return Some(NarrowingResult {
                variable_name: var_name.to_string(),
                narrowed_type: TypeBinding {
                    type_name: "null".to_string(),
                    type_entity_id: None,
                    span: Span::default(),
                    origin: Some(super::types::InferenceOrigin::ControlFlowNarrowing),
                    shape: None,
                },
            });
        }
        let declared = declared_shape(ctx, params, Language::Java, var_name)?;
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

/// Narrow types from Java switch pattern arms.
///
/// `case String s ->` / `case String s:` binds the arm designation.
/// Literal, null and multi-token (record/guard) labels stay conservative.
fn narrow_java_switch(text: &str) -> Vec<NarrowingResult> {
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
        let label_end = after_case
            .find("->")
            .or_else(|| after_case.find(':'))
            .unwrap_or(after_case.len());
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

/// Strip Java condition prefixes.
fn strip_java_condition_prefix(text: &str) -> Option<&str> {
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

/// Narrow types from a Java `try-catch` block.
///
/// Patterns:
/// - `try { ... } catch (IOException e) { ... }` → e: IOException
/// - `try { ... } catch (IOException | SQLException e) { ... }` → e: IOException | SQLException
/// - `try { ... } catch (final IOException e) { ... }` → e: IOException
fn narrow_java_catch(text: &str) -> Vec<NarrowingResult> {
    let mut results = vec![];
    let mut search = text;
    while let Some(catch_pos) = search.find("catch") {
        let after_catch = &search[catch_pos + 5..];
        let after_catch = after_catch.trim_start();
        if !after_catch.starts_with('(') {
            search = &after_catch[1.min(after_catch.len())..];
            continue;
        }
        // Find matching ')'
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
        if let Some(result) = parse_java_catch_param(param) {
            results.push(result);
        }
        search = &after_catch[end_pos + 1..];
    }
    results
}

/// Parse a Java catch parameter `IOException e` or `final IOException | SQLException e`.
fn parse_java_catch_param(param: &str) -> Option<NarrowingResult> {
    let param = param.trim();
    if param.is_empty() {
        return None;
    }
    // Strip optional modifiers like `final`
    let param = param.strip_prefix("final").unwrap_or(param).trim();
    // Split into type and variable: last token is variable name, rest is type
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
    use cce_types::Span;
    use cce_types::entity::EntityId;
    use cce_types::language::Language;

    fn dummy_span() -> Span {
        Span::default()
    }

    #[test]
    fn test_java_method_signature_extraction() {
        let mut ctx = ScopedTypeContext::new(Language::Java);
        let entities = vec![
            Entity::new(
                EntityId(1),
                EntityKind::Method,
                "getValue".to_string(),
                dummy_span(),
            )
            .with_return_type(Some("String".to_string())),
        ];

        JavaTypeInferer.infer_declarations(&entities, &mut ctx);

        let return_type = ctx.get_return_type(EntityId(1)).unwrap();
        assert_eq!(return_type.type_name, "String");
    }

    #[test]
    fn test_java_var_declaration() {
        let mut ctx = ScopedTypeContext::new(Language::Java);
        let entities = vec![
            Entity::new(
                EntityId(2),
                EntityKind::Variable,
                "list".to_string(),
                dummy_span(),
            )
            .with_metadata("var_type", "ArrayList<String>"),
        ];

        JavaTypeInferer.infer_declarations(&entities, &mut ctx);

        let var_type = ctx.get_variable_type("list").unwrap();
        assert_eq!(var_type.type_name, "ArrayList<String>");
        assert!(var_type.origin.is_some());
    }

    #[test]
    fn test_java_constructor_call() {
        let mut ctx = ScopedTypeContext::new(Language::Java);
        let entities = vec![
            Entity::new(
                EntityId(3),
                EntityKind::Variable,
                "map".to_string(),
                dummy_span(),
            )
            .with_metadata("constructor_type", "HashMap<String, Integer>"),
        ];

        JavaTypeInferer.infer_declarations(&entities, &mut ctx);

        let var_type = ctx.get_variable_type("map").unwrap();
        assert_eq!(var_type.type_name, "HashMap<String, Integer>");
        assert!(var_type.origin.is_some());
    }

    #[test]
    fn test_java_field_type_extraction() {
        let mut ctx = ScopedTypeContext::new(Language::Java);
        let entities = vec![
            Entity::new(
                EntityId(4),
                EntityKind::Field,
                "userName".to_string(),
                dummy_span(),
            )
            .with_metadata("type_annotation", "String"),
        ];

        JavaTypeInferer.infer_declarations(&entities, &mut ctx);

        let field_type = ctx.get_variable_type("userName").unwrap();
        assert_eq!(field_type.type_name, "String");
        assert!(field_type.origin.is_some());
    }

    #[test]
    fn test_java_generic_type_params() {
        let mut ctx = ScopedTypeContext::new(Language::Java);
        let entities = vec![
            Entity::new(
                EntityId(5),
                EntityKind::Method,
                "transform".to_string(),
                dummy_span(),
            )
            .with_return_type(Some("T".to_string()))
            .with_metadata("generic_type_params", "<T extends Number>"),
        ];

        JavaTypeInferer.infer_declarations(&entities, &mut ctx);

        let return_type = ctx.get_return_type(EntityId(5)).unwrap();
        assert_eq!(return_type.type_name, "T");
    }

    #[test]
    fn test_java_instanceof() {
        let ctx = ScopedTypeContext::new(Language::Java);
        let results = narrow_java_if("if (x instanceof String)", &ctx, &[]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "x");
        assert_eq!(results[0].narrowed_type.type_name, "String");
        assert!(results[0].narrowed_type.origin.is_some());
    }

    #[test]
    fn test_java_instanceof_pattern_variable() {
        let ctx = ScopedTypeContext::new(Language::Java);
        let results = narrow_java_if("if (obj instanceof String s)", &ctx, &[]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "s");
        assert_eq!(results[0].narrowed_type.type_name, "String");
    }

    #[test]
    fn test_java_negated_instanceof_complement() {
        let ctx = ScopedTypeContext::new(Language::Java);
        let params = [("x".to_string(), Some("String | Integer".to_string()))];
        let results = narrow_java_if("if (!(x instanceof String))", &ctx, &params);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "x");
        assert_eq!(results[0].narrowed_type.type_name, "Integer");
    }

    #[test]
    fn test_java_negated_instanceof_plain_type_skipped() {
        let ctx = ScopedTypeContext::new(Language::Java);
        let params = [("obj".to_string(), Some("Object".to_string()))];
        let results = narrow_java_if("if (!(obj instanceof String))", &ctx, &params);
        assert!(results.is_empty());
    }

    #[test]
    fn test_java_not_null_narrows_declared() {
        let ctx = ScopedTypeContext::new(Language::Java);
        let params = [("value".to_string(), Some("String".to_string()))];
        let results = narrow_java_if("if (value != null)", &ctx, &params);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "value");
        assert_eq!(results[0].narrowed_type.type_name, "String");
    }

    #[test]
    fn test_java_equal_null_binds_null() {
        let ctx = ScopedTypeContext::new(Language::Java);
        let results = narrow_java_if("if (value == null)", &ctx, &[]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "value");
        assert_eq!(results[0].narrowed_type.type_name, "null");
    }

    #[test]
    fn test_java_switch_pattern_arm() {
        let results = narrow_java_switch(
            "switch (obj) { case String s -> s; case Integer n -> n; default -> x; }",
        );
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].variable_name, "s");
        assert_eq!(results[0].narrowed_type.type_name, "String");
        assert_eq!(results[1].variable_name, "n");
        assert_eq!(results[1].narrowed_type.type_name, "Integer");
    }

    #[test]
    fn test_java_switch_literal_and_string_skipped() {
        let results = narrow_java_switch(
            "switch (x) { case 1 -> a; case \"s\" -> b; case null -> c; default -> d; }",
        );
        assert!(results.is_empty());
    }

    #[test]
    fn test_java_catch_single() {
        let results = narrow_java_catch("try {} catch (IOException e) {}");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "e");
        assert_eq!(results[0].narrowed_type.type_name, "IOException");
    }

    #[test]
    fn test_java_catch_multi() {
        let results = narrow_java_catch("try {} catch (IOException | SQLException e) {}");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "e");
        assert_eq!(
            results[0].narrowed_type.type_name,
            "IOException | SQLException"
        );
    }

    #[test]
    fn test_java_catch_final() {
        let results = narrow_java_catch("try {} catch (final IOException e) {}");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "e");
        assert_eq!(results[0].narrowed_type.type_name, "IOException");
    }

    #[test]
    fn test_java_catch_multiple_clauses() {
        let results =
            narrow_java_catch("try {} catch (IOException e) {} catch (SQLException ex) {}");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].variable_name, "e");
        assert_eq!(results[1].variable_name, "ex");
    }

    // ==================== Additional control flow tests ====================

    #[test]
    fn test_java_catch_multi_union_types() {
        let results =
            narrow_java_catch("try {} catch (IOException | SQLException | TimeoutException e) {}");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "e");
        assert_eq!(
            results[0].narrowed_type.type_name,
            "IOException | SQLException | TimeoutException"
        );
    }

    #[test]
    fn test_java_catch_final_with_multi_union() {
        let results = narrow_java_catch("try {} catch (final IOException | SQLException e) {}");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "e");
        assert_eq!(
            results[0].narrowed_type.type_name,
            "IOException | SQLException"
        );
    }

    #[test]
    fn test_java_catch_three_clauses() {
        let results = narrow_java_catch(
            "try {} catch (IOException e) {} catch (SQLException ex) {} catch (Exception err) {}",
        );
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].variable_name, "e");
        assert_eq!(results[1].variable_name, "ex");
        assert_eq!(results[2].variable_name, "err");
    }

    #[test]
    fn test_java_instanceof_in_while() {
        let ctx = ScopedTypeContext::new(Language::Java);
        let results = narrow_java_if("while (x instanceof String)", &ctx, &[]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "x");
        assert_eq!(results[0].narrowed_type.type_name, "String");
    }

    #[test]
    fn test_java_instanceof_in_else_if() {
        let ctx = ScopedTypeContext::new(Language::Java);
        let results = narrow_java_if("else if (x instanceof Integer)", &ctx, &[]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "x");
        assert_eq!(results[0].narrowed_type.type_name, "Integer");
    }

    #[test]
    fn test_java_instanceof_in_return() {
        let ctx = ScopedTypeContext::new(Language::Java);
        let results = narrow_java_if("return x instanceof List", &ctx, &[]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "x");
        assert_eq!(results[0].narrowed_type.type_name, "List");
    }

    #[test]
    fn test_java_catch_empty_block() {
        let results = narrow_java_catch("try {} catch (Exception e) { }");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "e");
        assert_eq!(results[0].narrowed_type.type_name, "Exception");
    }

    #[test]
    fn test_java_catch_no_catch() {
        let results = narrow_java_catch("try {} finally {}");
        assert!(results.is_empty());
    }

    #[test]
    fn test_java_catch_empty_param() {
        let results = narrow_java_catch("try {} catch () {}");
        assert!(results.is_empty());
    }

    #[test]
    fn test_java_generic_method_multiple_params() {
        let mut ctx = ScopedTypeContext::new(Language::Java);
        let entities = vec![
            Entity::new(
                EntityId(10),
                EntityKind::Method,
                "combine".to_string(),
                dummy_span(),
            )
            .with_return_type(Some("Pair<T, U>".to_string()))
            .with_metadata("generic_type_params", "<T, U>"),
        ];

        JavaTypeInferer.infer_declarations(&entities, &mut ctx);

        let return_type = ctx.get_return_type(EntityId(10)).unwrap();
        assert_eq!(return_type.type_name, "Pair<T, U>");
    }

    #[test]
    fn test_java_constructor_nested_generic() {
        let mut ctx = ScopedTypeContext::new(Language::Java);
        let entities = vec![
            Entity::new(
                EntityId(11),
                EntityKind::Variable,
                "matrix".to_string(),
                dummy_span(),
            )
            .with_metadata("constructor_type", "ArrayList<ArrayList<Integer>>"),
        ];

        JavaTypeInferer.infer_declarations(&entities, &mut ctx);

        let var_type = ctx.get_variable_type("matrix").unwrap();
        assert_eq!(var_type.type_name, "ArrayList<ArrayList<Integer>>");
    }

    #[test]
    fn test_java_multiple_var_declarations() {
        let mut ctx = ScopedTypeContext::new(Language::Java);
        let entities = vec![
            Entity::new(
                EntityId(20),
                EntityKind::Variable,
                "name".to_string(),
                dummy_span(),
            )
            .with_metadata("var_type", "String"),
            Entity::new(
                EntityId(21),
                EntityKind::Variable,
                "count".to_string(),
                dummy_span(),
            )
            .with_metadata("var_type", "int"),
            Entity::new(
                EntityId(22),
                EntityKind::Variable,
                "items".to_string(),
                dummy_span(),
            )
            .with_metadata("constructor_type", "HashMap<String, List<String>>"),
        ];

        JavaTypeInferer.infer_declarations(&entities, &mut ctx);

        assert_eq!(ctx.get_variable_type("name").unwrap().type_name, "String");
        assert_eq!(ctx.get_variable_type("count").unwrap().type_name, "int");
        assert_eq!(
            ctx.get_variable_type("items").unwrap().type_name,
            "HashMap<String, List<String>>"
        );
    }

    #[test]
    fn test_java_field_generic_type() {
        let mut ctx = ScopedTypeContext::new(Language::Java);
        let entities = vec![
            Entity::new(
                EntityId(30),
                EntityKind::Field,
                "cache".to_string(),
                dummy_span(),
            )
            .with_metadata(
                "field_types",
                "ConcurrentHashMap<String, AtomicReference<T>>",
            ),
        ];

        JavaTypeInferer.infer_declarations(&entities, &mut ctx);

        let field_type = ctx.get_variable_type("cache").unwrap();
        assert_eq!(
            field_type.type_name,
            "ConcurrentHashMap<String, AtomicReference<T>>"
        );
    }
}
