//! Kotlin-specific type inference.
//!
//! Handles Kotlin-specific patterns:
//! - `val`/`var` declarations with optional type annotations
//! - `when` expression narrowing (smart casts)
//! - `is` type checks with smart casting
//! - Lambda expressions with typed parameters
//! - Constructor calls via `ClassName()`
//! - Discriminated union narrowing (`x.field == "value"` → x: narrowed union)

use cce_types::ControlFlowFactKind;
use cce_types::ControlFlowStore;
use cce_types::Span;
use cce_types::entity::{Entity, EntityKind};
use cce_types::language::Language;

use super::control_flow::shared::{
    extract_balanced_parens, is_valid_ident, split_top_level_conjuncts, strip_outer_parens,
};
use super::extractors::{extract_field_type, extract_function_types, extract_variable_type};
use super::traits::LanguageTypeInferer;
use super::types::{
    ScopedTypeContext, TypeBinding, add_polarity_aware_narrowings, declared_shape,
    narrow_discriminated_union, parse_type_shape, subtract_union_members, type_shape_to_string,
};

/// Kotlin type inference implementation.
pub struct KotlinTypeInferer;

impl LanguageTypeInferer for KotlinTypeInferer {
    fn infer_declarations(&self, entities: &[Entity], ctx: &mut ScopedTypeContext) {
        for entity in entities {
            match entity.kind {
                EntityKind::Function | EntityKind::Method => {
                    extract_function_types(entity, ctx);
                }
                EntityKind::Variable => {
                    extract_variable_type(entity, ctx);

                    // Kotlin-specific: `var_type` for val/var inferred types
                    if let Some(var_type) = entity.metadata.get("var_type") {
                        let binding = TypeBinding {
                            type_name: var_type.clone(),
                            type_entity_id: None,
                            span: entity.span,
                            origin: Some(super::types::InferenceOrigin::TypeAnnotation),
                            shape: parse_type_shape(var_type, Language::Kotlin),
                        };
                        ctx.add_variable_type(entity.name.clone(), binding);
                    }
                    // Constructor calls (`ClassName()`) are handled by the shared
                    // `extract_variable_type`, which binds `constructor_type` with
                    // a resolved shape only when no concrete annotation is present.
                    // No duplicate handling here so explicit annotations keep priority.
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
                        let mut narrowed: Vec<(String, TypeBinding)> = narrow_kotlin_if(
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
                            Language::Kotlin,
                            fact,
                            &narrowed,
                        );
                    }
                    ControlFlowFactKind::Match => {
                        for result in narrow_kotlin_when(&fact.text, ctx, &entity.parameters) {
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

/// Narrow types from a Kotlin `if` condition.
///
/// Patterns:
/// - `if (x is Type)` → x: Type
/// - `if (x !is Type)` → x: declared-minus-Type (union only)
/// - `if (x != null)` → x: declared (non-null)
/// - `if (x == null)` → x: null
/// - `x is Type && x.prop` → x: Type (via is check)
/// - `if (x.field == "value")` → x: narrowed union (discriminated union)
fn narrow_kotlin_if(
    text: &str,
    ctx: &ScopedTypeContext,
    type_index: Option<&crate::symbol_table::TypeMemberIndex>,
    params: &[(String, Option<String>)],
) -> Vec<NarrowingResult> {
    if let Some(cond) = strip_kotlin_condition_prefix(text) {
        let parts = split_top_level_conjuncts(cond);
        if parts.len() > 1 {
            let mut out = Vec::new();
            for part in parts {
                out.extend(narrow_single_kotlin_condition(
                    part, ctx, type_index, params,
                ));
            }
            return out;
        }
    }
    narrow_single_kotlin_condition(text, ctx, type_index, params)
}

fn narrow_single_kotlin_condition(
    text: &str,
    ctx: &ScopedTypeContext,
    type_index: Option<&crate::symbol_table::TypeMemberIndex>,
    params: &[(String, Option<String>)],
) -> Vec<NarrowingResult> {
    let text = text.trim();
    let mut results = vec![];

    if let Some(result) = narrow_kotlin_is_check(text, ctx, params) {
        results.push(result);
        return results;
    }

    if let Some(result) = narrow_kotlin_null_check(text, ctx, params) {
        results.push(result);
        return results;
    }

    for result in narrow_kotlin_discriminated_union(text, ctx, type_index) {
        results.push(result);
    }

    results
}

/// Parse `if (x is Type)` or `if (x !is Type)` (complement).
fn narrow_kotlin_is_check(
    text: &str,
    ctx: &ScopedTypeContext,
    params: &[(String, Option<String>)],
) -> Option<NarrowingResult> {
    let text = strip_kotlin_condition_prefix(text)?;
    let text = text.trim();

    if let Some(rest) = split_not_is(text) {
        return narrow_kotlin_not_is(rest, ctx, params);
    }
    // `!is` without spaces (e.g. `x!is T`) is not a valid check.
    if text.contains("!is") {
        return None;
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

/// Split `x !is T` into (`x`, `T`).
fn split_not_is(text: &str) -> Option<(&str, &str)> {
    let parts: Vec<&str> = text.splitn(2, " !is ").collect();
    if parts.len() != 2 {
        return None;
    }
    Some((parts[0].trim(), parts[1].trim()))
}

/// Parse `x !is Type` → x: declared-minus-Type (union only).
fn narrow_kotlin_not_is(
    rest: (&str, &str),
    ctx: &ScopedTypeContext,
    params: &[(String, Option<String>)],
) -> Option<NarrowingResult> {
    let (var_name, excluded) = rest;
    if var_name.is_empty() || !is_valid_ident(var_name) || excluded.is_empty() {
        return None;
    }
    let excluded = excluded.split_whitespace().next().unwrap_or(excluded);
    let declared = declared_shape(ctx, params, Language::Kotlin, var_name)?;
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

/// Parse Kotlin null checks: `x != null` → x: declared, `x == null` → x: null.
///
/// A forced assertion (`x!!`) acts on `x`, and a safe call (`x?.foo`)
/// conditions on its receiver `x`. Both forms only narrow on the non-null
/// arm; the null arm stays conservative since a null result does not prove
/// the receiver itself is null.
fn narrow_kotlin_null_check(
    text: &str,
    ctx: &ScopedTypeContext,
    params: &[(String, Option<String>)],
) -> Option<NarrowingResult> {
    let text = strip_kotlin_condition_prefix(text)?;
    let text = text.trim();
    // Parenthesized safety: conditions may carry grouping parens.
    let text = extract_balanced_parens(text).unwrap_or(text);
    for (op, negated) in [("!=", true), ("==", false)] {
        let parts: Vec<&str> = text.splitn(2, op).collect();
        if parts.len() != 2 {
            continue;
        }
        let lhs = parts[0].trim();
        let rhs = parts[1].trim();
        if rhs != "null" || lhs.is_empty() {
            continue;
        }
        let asserted = lhs.contains("!!") || lhs.contains("?.");
        if asserted && !negated {
            continue;
        }
        let receiver = lhs.split("?.").next().unwrap_or("").trim();
        let var_name = receiver.trim_end_matches('!').trim();
        if var_name.is_empty() || !is_valid_ident(var_name) {
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
        let declared = declared_shape(ctx, params, Language::Kotlin, var_name)?;
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

/// Strip Kotlin condition prefixes (if, while, else if, when).
///
/// Handles both `if (cond)` and `if(cond)`, extracting the condition text
/// even when trailing braces or other tokens follow the closing paren.
fn strip_kotlin_condition_prefix(text: &str) -> Option<&str> {
    let text = text.trim();
    for prefix in &["if", "while", "else if", "else if (", "return"] {
        if let Some(rest) = text.strip_prefix(prefix) {
            let rest = rest.trim();
            let rest = strip_outer_parens(rest);
            // If strip_outer_parens didn't strip (no matching outer parens),
            // try to extract content between the first '(' and its matching ')'
            if let Some(inner) = extract_inner_condition(rest) {
                return Some(inner);
            }
            return Some(rest);
        }
    }
    None
}

/// Extract condition text between matching parentheses, ignoring trailing content.
///
/// e.g. `x is Int) {` → `x is Int` (from input `(x is Int) {`)
fn extract_inner_condition(text: &str) -> Option<&str> {
    let text = text.trim();
    if !text.starts_with('(') {
        return None;
    }
    let mut depth = 0;
    for (i, ch) in text.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[1..i]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Kotlin discriminated union narrowing: `x.field == "value"` → x: narrowed union.
///
/// Kotlin sealed classes use field-based discrimination patterns similar to
/// TypeScript and Python. This function handles equality checks like:
/// - `if (shape.kind == "circle")` → shape: Circle
/// - `if (result.status == "success")` → result: Success
fn narrow_kotlin_discriminated_union(
    text: &str,
    ctx: &ScopedTypeContext,
    type_index: Option<&crate::symbol_table::TypeMemberIndex>,
) -> Vec<NarrowingResult> {
    let Some((var_name, field_name, value)) = parse_kotlin_equality_pattern(text) else {
        return vec![];
    };
    let mut results = Vec::new();
    if let Some(existing) = ctx.get_variable_type(&var_name) {
        if let Some(shape) = existing
            .shape
            .clone()
            .or_else(|| parse_type_shape(&existing.type_name, Language::Kotlin))
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

/// Parse Kotlin equality pattern like `x.field == "value"` or `x.field == 'value'`.
fn parse_kotlin_equality_pattern(text: &str) -> Option<(String, String, String)> {
    let raw = strip_kotlin_condition_prefix(text)?;
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

/// Narrow types from a Kotlin `when` expression.
///
/// Patterns:
/// - `when (x) { is String -> ... }` → x: String
/// - `when { x is String -> ... }` → x: String
/// - `when (x) { is Int, is String -> ... }` → x: Int | String (first only)
/// - `when (x) { x !is String -> ... }` → x: declared-minus-String (union only)
fn narrow_kotlin_when(
    text: &str,
    ctx: &ScopedTypeContext,
    params: &[(String, Option<String>)],
) -> Vec<NarrowingResult> {
    let text = text.trim();
    // Find subject variable if `when (subject)`
    let subject = extract_when_subject(text);
    if let Some(brace_start) = text.find('{') {
        let body = &text[brace_start + 1..];
        return narrow_kotlin_when_arms(body, subject.as_deref(), ctx, params);
    }
    vec![]
}

/// Extract the subject of `when (subject)`.
fn extract_when_subject(text: &str) -> Option<String> {
    let text = text.trim();
    let when_pos = text.find("when")?;
    let after_when = text[when_pos + 4..].trim();
    if !after_when.starts_with('(') {
        return None;
    }
    let inner = extract_inner_condition(after_when)?;
    let inner = inner.trim();
    if inner.is_empty() || !is_valid_ident(inner) {
        return None;
    }
    Some(inner.to_string())
}

/// Extract variable bindings from Kotlin when arms.
///
/// Scans the when body for `is` type checks, handling both `is Type` (with
/// subject) and `x is Type` forms. Each `is` occurrence is evaluated with its
/// surrounding arm context to avoid mixing result expressions from previous arms.
/// Negated `!is` arms narrow to the declared type minus the excluded member,
/// following the same complement convention as negated `if` checks.
fn narrow_kotlin_when_arms(
    arms_text: &str,
    subject: Option<&str>,
    ctx: &ScopedTypeContext,
    params: &[(String, Option<String>)],
) -> Vec<NarrowingResult> {
    let mut results = vec![];
    let mut search_start = 0;
    while let Some(is_pos) = arms_text[search_start..].find(" is ") {
        let abs_is = search_start + is_pos;
        // Check for negated `!is` (preceding char is '!')
        if abs_is > 0 && arms_text.as_bytes()[abs_is - 1] == b'!' {
            search_start = abs_is + 4;
            continue;
        }
        // Also handle `is Type` at arm start (e.g., `is String`)
        // Find condition start: after last `;`, `,`, `{`, `}`, `\n` before ` is `
        let cond_start = arms_text[..abs_is]
            .rfind(|c| [';', ',', '{', '}', '\n'].contains(&c))
            .map(|p| p + 1)
            .unwrap_or(0);
        let var_part = arms_text[cond_start..abs_is].trim();
        // Extract type after ` is `
        let after_is = arms_text[abs_is + 4..].trim_start();
        let type_end = after_is
            .find(|c: char| {
                c.is_whitespace() || c == ',' || c == '-' || c == '{' || c == '}' || c == ';'
            })
            .unwrap_or(after_is.len());
        let type_part = after_is[..type_end].trim();
        if type_part.is_empty() || type_part.contains('!') {
            search_start = abs_is + 4;
            continue;
        }
        let var_name = if var_part.is_empty() {
            if let Some(s) = subject {
                s.to_string()
            } else {
                search_start = abs_is + 4;
                continue;
            }
        } else if is_valid_ident(var_part) {
            var_part.to_string()
        } else {
            // var_part may be empty after trimming or be `is` artifact; fallback to subject
            if let Some(s) = subject {
                if var_part.is_empty() || var_part == "is" {
                    s.to_string()
                } else {
                    search_start = abs_is + 4;
                    continue;
                }
            } else {
                search_start = abs_is + 4;
                continue;
            }
        };
        if !is_valid_ident(&var_name) {
            search_start = abs_is + 4;
            continue;
        }
        results.push(NarrowingResult {
            variable_name: var_name,
            narrowed_type: TypeBinding {
                type_name: type_part.to_string(),
                type_entity_id: None,
                span: Span::default(),
                origin: Some(super::types::InferenceOrigin::ControlFlowNarrowing),
                shape: None,
            },
        });
        search_start = abs_is + 4;
    }
    // Negated arms (`x !is Type`, `!is Type` with a subject) narrow to the
    // declared type minus the excluded member. Without a union-shaped
    // declaration there is nothing to subtract, so the arm stays unbound.
    let mut neg_search = 0;
    while let Some(rel) = arms_text[neg_search..].find("!is ") {
        let abs_not = neg_search + rel;
        let cond_start = arms_text[..abs_not]
            .rfind(|c| [';', ',', '{', '}', '\n'].contains(&c))
            .map(|p| p + 1)
            .unwrap_or(0);
        let var_part = arms_text[cond_start..abs_not]
            .trim()
            .trim_end_matches('!')
            .trim();
        let after_not = arms_text[abs_not + 4..].trim_start();
        let type_end = after_not
            .find(|c: char| {
                c.is_whitespace() || c == ',' || c == '-' || c == '{' || c == '}' || c == ';'
            })
            .unwrap_or(after_not.len());
        let excluded = after_not[..type_end]
            .split_whitespace()
            .next()
            .unwrap_or("");
        neg_search = abs_not + 4;
        if excluded.is_empty() {
            continue;
        }
        let var_name = if var_part.is_empty() {
            match subject {
                Some(s) => s.to_string(),
                None => continue,
            }
        } else if is_valid_ident(var_part) {
            var_part.to_string()
        } else {
            continue;
        };
        if !is_valid_ident(&var_name) {
            continue;
        }
        let Some(declared) = declared_shape(ctx, params, Language::Kotlin, &var_name) else {
            continue;
        };
        let Some(narrowed) = subtract_union_members(&declared, &[excluded.to_string()]) else {
            continue;
        };
        results.push(NarrowingResult {
            variable_name: var_name,
            narrowed_type: TypeBinding {
                type_name: type_shape_to_string(&narrowed),
                type_entity_id: None,
                span: Span::default(),
                origin: Some(super::types::InferenceOrigin::ControlFlowNarrowing),
                shape: Some(narrowed),
            },
        });
    }
    // Also handle `is Type` at the very start of an arm without preceding space (e.g., `is String`)
    for arm in arms_text.split("->") {
        let arm = arm.trim();
        if let Some(stripped) = arm.strip_prefix("is ") {
            let type_name = stripped.split_whitespace().next().unwrap_or("").trim();
            if !type_name.is_empty() && !type_name.contains('!') {
                if let Some(s) = subject {
                    if is_valid_ident(s)
                        && !results
                            .iter()
                            .any(|r| r.narrowed_type.type_name == type_name && r.variable_name == s)
                    {
                        results.push(NarrowingResult {
                            variable_name: s.to_string(),
                            narrowed_type: TypeBinding {
                                type_name: type_name.to_string(),
                                type_entity_id: None,
                                span: Span::default(),
                                origin: Some(super::types::InferenceOrigin::ControlFlowNarrowing),
                                shape: None,
                            },
                        });
                    }
                }
            }
        }
    }
    results
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
    fn test_kotlin_method_signature() {
        let mut ctx = ScopedTypeContext::new(Language::Kotlin);
        let entities = vec![
            Entity::new(
                EntityId(1),
                EntityKind::Method,
                "getValue".to_string(),
                dummy_span(),
            )
            .with_return_type(Some("String".to_string())),
        ];

        KotlinTypeInferer.infer_declarations(&entities, &mut ctx);
        let rt = ctx.get_return_type(EntityId(1)).unwrap();
        assert_eq!(rt.type_name, "String");
    }

    #[test]
    fn test_kotlin_var_declaration() {
        let mut ctx = ScopedTypeContext::new(Language::Kotlin);
        let entities = vec![
            Entity::new(
                EntityId(2),
                EntityKind::Variable,
                "name".to_string(),
                dummy_span(),
            )
            .with_metadata("var_type", "String"),
        ];

        KotlinTypeInferer.infer_declarations(&entities, &mut ctx);
        let vt = ctx.get_variable_type("name").unwrap();
        assert_eq!(vt.type_name, "String");
        assert!(vt.origin.is_some());
    }

    #[test]
    fn test_kotlin_constructor_call() {
        let mut ctx = ScopedTypeContext::new(Language::Kotlin);
        let entities = vec![
            Entity::new(
                EntityId(3),
                EntityKind::Variable,
                "user".to_string(),
                dummy_span(),
            )
            .with_metadata("constructor_type", "User"),
        ];

        KotlinTypeInferer.infer_declarations(&entities, &mut ctx);
        let vt = ctx.get_variable_type("user").unwrap();
        assert_eq!(vt.type_name, "User");
        assert!(vt.origin.is_some());
    }

    #[test]
    fn test_kotlin_is_check() {
        let results = narrow_kotlin_if(
            "if (x is String)",
            &ScopedTypeContext::new(Language::Kotlin),
            None,
            &[],
        );
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "x");
        assert_eq!(results[0].narrowed_type.type_name, "String");
        assert!(results[0].narrowed_type.origin.is_some());
    }

    #[test]
    fn test_kotlin_not_is_complement() {
        let ctx = ScopedTypeContext::new(Language::Kotlin);
        let params = [("x".to_string(), Some("String | Int".to_string()))];
        let results = narrow_kotlin_if("if (x !is String)", &ctx, None, &params);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "x");
        assert_eq!(results[0].narrowed_type.type_name, "Int");
    }

    #[test]
    fn test_kotlin_not_is_plain_type_skipped() {
        let ctx = ScopedTypeContext::new(Language::Kotlin);
        let params = [("value".to_string(), Some("Any".to_string()))];
        let results = narrow_kotlin_if("if (value !is String)", &ctx, None, &params);
        assert!(results.is_empty());
    }

    #[test]
    fn test_kotlin_not_null_narrows_declared() {
        let ctx = ScopedTypeContext::new(Language::Kotlin);
        let params = [("value".to_string(), Some("String?".to_string()))];
        let results = narrow_kotlin_if("if (value != null)", &ctx, None, &params);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "value");
        assert_eq!(results[0].narrowed_type.type_name, "String");
    }

    #[test]
    fn test_kotlin_is_check_with_braces() {
        let results = narrow_kotlin_if(
            "if(x is Int) {",
            &ScopedTypeContext::new(Language::Kotlin),
            None,
            &[],
        );
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "x");
        assert_eq!(results[0].narrowed_type.type_name, "Int");
    }

    #[test]
    fn test_kotlin_when_subject_is() {
        let results = narrow_kotlin_when(
            "when (x) { is String -> print(x) }",
            &ScopedTypeContext::new(Language::Kotlin),
            &[],
        );
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "x");
        assert_eq!(results[0].narrowed_type.type_name, "String");
    }

    #[test]
    fn test_kotlin_when_explicit_is() {
        let results = narrow_kotlin_when(
            "when (x) { x is String -> print(x) }",
            &ScopedTypeContext::new(Language::Kotlin),
            &[],
        );
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "x");
        assert_eq!(results[0].narrowed_type.type_name, "String");
    }

    #[test]
    fn test_kotlin_when_multiple_arms() {
        let results = narrow_kotlin_when(
            "when (x) { is String -> 1; is Int -> 2 }",
            &ScopedTypeContext::new(Language::Kotlin),
            &[],
        );
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].variable_name, "x");
        assert_eq!(results[1].variable_name, "x");
    }

    #[test]
    fn test_kotlin_when_negated_skipped() {
        let results = narrow_kotlin_when(
            "when (x) { !is String -> print(x) }",
            &ScopedTypeContext::new(Language::Kotlin),
            &[],
        );
        assert!(results.is_empty());
    }

    #[test]
    fn test_kotlin_when_negated_arm_complement() {
        let ctx = ScopedTypeContext::new(Language::Kotlin);
        let params = [("x".to_string(), Some("String | Int".to_string()))];
        let results = narrow_kotlin_when("when (x) { x !is String -> print(x) }", &ctx, &params);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "x");
        assert_eq!(results[0].narrowed_type.type_name, "Int");
    }

    #[test]
    fn test_kotlin_when_no_subject_explicit() {
        let results = narrow_kotlin_when(
            "when { x is String -> print(x) }",
            &ScopedTypeContext::new(Language::Kotlin),
            &[],
        );
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "x");
        assert_eq!(results[0].narrowed_type.type_name, "String");
    }

    #[test]
    fn test_kotlin_forced_assertion_narrows_non_null() {
        let ctx = ScopedTypeContext::new(Language::Kotlin);
        let params = [("value".to_string(), Some("String | null".to_string()))];
        let results = narrow_kotlin_if("if (value!! != null)", &ctx, None, &params);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "value");
        assert_eq!(results[0].narrowed_type.type_name, "String");
    }

    #[test]
    fn test_kotlin_safe_call_narrows_receiver() {
        let ctx = ScopedTypeContext::new(Language::Kotlin);
        let params = [("user".to_string(), Some("User | null".to_string()))];
        let results = narrow_kotlin_if("if (user?.name != null)", &ctx, None, &params);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "user");
        assert_eq!(results[0].narrowed_type.type_name, "User");
    }

    #[test]
    fn test_kotlin_discriminated_union_fallback() {
        // Deterministic: without TypeMemberIndex no heuristic fallback, returns empty
        let results = narrow_kotlin_if(
            "if (shape.kind == \"circle\")",
            &ScopedTypeContext::new(Language::Kotlin),
            None,
            &[],
        );
        assert!(results.is_empty());
    }

    #[test]
    fn test_kotlin_parse_equality_pattern() {
        let result = parse_kotlin_equality_pattern("if (x.type == \"value\")");
        assert!(result.is_some());
        let (var, field, value) = result.unwrap();
        assert_eq!(var, "x");
        assert_eq!(field, "type");
        assert_eq!(value, "value");
    }

    #[test]
    fn test_kotlin_parse_equality_pattern_single_quotes() {
        let result = parse_kotlin_equality_pattern("if (shape.kind == 'circle')");
        assert!(result.is_some());
        let (var, field, value) = result.unwrap();
        assert_eq!(var, "shape");
        assert_eq!(field, "kind");
        assert_eq!(value, "circle");
    }
}
