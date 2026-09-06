//! TypeScript/JavaScript-specific type inference.

use cce_types::ControlFlowFactKind;
use cce_types::ControlFlowStore;
use cce_types::Span;
use cce_types::entity::{Entity, EntityKind};

use super::control_flow::shared::{
    extract_balanced_parens, is_valid_ident, parse_string_literal, split_comparison,
    split_top_level_conjuncts, strip_outer_parens,
};
use super::extractors::{extract_field_type, extract_function_types, extract_variable_type};
use super::traits::LanguageTypeInferer;
use super::types::{
    ScopedTypeContext, TypeBinding, TypeShape, add_polarity_aware_narrowings, declared_shape,
    narrow_discriminated_union, narrow_truthiness, parse_type_shape, subtract_union_members,
    type_shape_to_string,
};
use crate::symbol_table::TypeMemberIndex;
use cce_types::language::Language;

/// TypeScript/JavaScript type inference implementation.
pub struct TypeScriptTypeInferer;

impl LanguageTypeInferer for TypeScriptTypeInferer {
    fn infer_declarations(&self, entities: &[Entity], ctx: &mut ScopedTypeContext) {
        for entity in entities {
            match entity.kind {
                EntityKind::Function | EntityKind::Method => {
                    extract_function_types(entity, ctx);
                }
                EntityKind::Variable => {
                    extract_variable_type(entity, ctx);
                }
                EntityKind::Field | EntityKind::Property => {
                    extract_field_type(entity, ctx);
                }
                _ => {}
            }
        }

        // TypeScript-specific: extract constructor call types from metadata
        extract_constructor_call_types(entities, ctx);
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
                        let mut narrowed: Vec<(String, TypeBinding)> = narrow_typescript_if(
                            &fact.text,
                            ctx,
                            inference_ctx.type_index(),
                            &entity.parameters,
                        )
                        .into_iter()
                        .map(|result| (result.variable_name, result.narrowed_type))
                        .collect();
                        // Narrowing results carry no source position; anchor
                        // them to the enclosing entity so spans render.
                        for (_, binding) in narrowed.iter_mut() {
                            if !binding.span.is_available() {
                                binding.span = entity.span;
                            }
                        }
                        add_polarity_aware_narrowings(
                            ctx,
                            &entity.parameters,
                            Language::TypeScript,
                            fact,
                            &narrowed,
                        );
                    }
                    ControlFlowFactKind::Match => {
                        for result in narrow_typescript_switch(
                            &fact.text,
                            ctx,
                            inference_ctx.type_index(),
                            &entity.parameters,
                        ) {
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

/// Narrow types from a TypeScript `if` condition.
///
/// Patterns:
/// - `typeof x === "string"` → x: string
/// - `typeof x !== "string"` → x: declared-union-minus-string
/// - `x instanceof Class` → x: Class
/// - `x === null` → x: null
/// - `x !== null` / `x != null` → x: declared-union-minus-null
/// - `x.kind === "circle"` → x: Circle (discriminated union)
/// - `if (x)` → truthiness
/// - `if (!x)` → negated truthiness
/// - `"prop" in x` → x: HasKey<prop>
/// - `x == null` → x: null | undefined
/// - `x === undefined` → x: undefined
fn narrow_typescript_if(
    text: &str,
    ctx: &ScopedTypeContext,
    type_index: Option<&TypeMemberIndex>,
    params: &[(String, Option<String>)],
) -> Vec<NarrowingResult> {
    // Compound `A && B` conditions hold conjunct-wise in the then-branch.
    // Narrow each part separately so the right side never leaks into a
    // pseudo-type like `number" && typeof b === "number"`.
    if let Some(cond) = strip_typescript_condition_prefix(text) {
        let parts = split_top_level_conjuncts(cond);
        if parts.len() > 1 {
            let mut out = Vec::new();
            for part in parts {
                out.extend(narrow_single_typescript_condition(
                    part, ctx, type_index, params,
                ));
            }
            return out;
        }
    }
    narrow_single_typescript_condition(text, ctx, type_index, params)
}

fn narrow_single_typescript_condition(
    text: &str,
    ctx: &ScopedTypeContext,
    type_index: Option<&TypeMemberIndex>,
    params: &[(String, Option<String>)],
) -> Vec<NarrowingResult> {
    let text = text.trim();
    let mut results = vec![];

    if let Some(result) = narrow_typescript_typeof(text, ctx, params) {
        results.push(result);
        return results;
    }
    if let Some(result) = narrow_typescript_instanceof(text) {
        results.push(result);
        return results;
    }
    if let Some(result) = narrow_typescript_strict_equal_null(text, ctx, params) {
        results.push(result);
        return results;
    }

    // Additional patterns (allow multiple)
    for r in narrow_typescript_discriminated_union(text, ctx, type_index, params) {
        results.push(r);
    }
    for r in narrow_typescript_in_operator(text) {
        results.push(r);
    }
    for r in narrow_typescript_equality_loose(text, ctx, params) {
        results.push(r);
    }
    for r in narrow_typescript_truthiness(text, ctx, params) {
        results.push(r);
    }

    results
}

/// Declared shape for a variable: parameter annotation first, then a known
/// binding. Shared complement/truthiness plumbing for negated narrowing.
fn declared_shape_here(
    ctx: &ScopedTypeContext,
    params: &[(String, Option<String>)],
    var_name: &str,
) -> Option<TypeShape> {
    declared_shape(ctx, params, Language::TypeScript, var_name)
}

/// Build a narrowing result with a concrete shape.
fn narrowing_result(var_name: String, narrowed: TypeShape) -> NarrowingResult {
    NarrowingResult {
        variable_name: var_name,
        narrowed_type: TypeBinding {
            type_name: type_shape_to_string(&narrowed),
            type_entity_id: None,
            span: Span::default(),
            origin: Some(super::types::InferenceOrigin::ControlFlowNarrowing),
            shape: Some(narrowed),
        },
    }
}

fn narrow_typescript_discriminated_union(
    text: &str,
    ctx: &ScopedTypeContext,
    type_index: Option<&TypeMemberIndex>,
    params: &[(String, Option<String>)],
) -> Vec<NarrowingResult> {
    let Some((var_name, field_name, value)) = parse_typescript_strict_equality_pattern(text) else {
        return vec![];
    };
    let mut results = Vec::new();
    let shape_opt = ctx
        .get_variable_type(&var_name)
        .and_then(|existing| {
            existing
                .shape
                .clone()
                .or_else(|| parse_type_shape(&existing.type_name, Language::TypeScript))
        })
        .or_else(|| declared_shape_here(ctx, params, &var_name));
    if let Some(shape) = shape_opt {
        if let Some(narrowed) = narrow_discriminated_union(&shape, &field_name, &value, type_index)
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
    results
}

/// Narrow types from a TypeScript `switch` statement.
///
/// A `typeof` scrutinee binds each string-literal arm to its literal type,
/// mirroring the `typeof` equality narrowing. A plain field scrutinee
/// narrows through the discriminated-union index, mirroring the
/// strict-equality narrowing. Literal, numeric and default labels stay
/// conservative, as do scrutinees that are neither `typeof` applications
/// nor plain field selections.
fn narrow_typescript_switch(
    text: &str,
    ctx: &ScopedTypeContext,
    type_index: Option<&TypeMemberIndex>,
    params: &[(String, Option<String>)],
) -> Vec<NarrowingResult> {
    let Some(scrutinee) = extract_typescript_switch_scrutinee(text) else {
        return vec![];
    };
    let mut results = vec![];
    let mut search_start = 0;
    while let Some(case_pos) = text[search_start..].find("case ") {
        let abs_case = search_start + case_pos;
        // Case labels start a statement: the previous non-whitespace char
        // must open the switch body or terminate the prior arm, keeping
        // string literals such as `log("case x:")` from binding.
        let prev = text[..abs_case].chars().rev().find(|c| !c.is_whitespace());
        if !matches!(prev, None | Some('{') | Some('}') | Some(';')) {
            search_start = abs_case + 5;
            continue;
        }
        let after_case = &text[abs_case + 5..];
        let label_end = after_case.find(':').unwrap_or(after_case.len());
        let label = after_case[..label_end].trim();
        if let Some(literal) = parse_string_literal(label) {
            match &scrutinee {
                TypeScriptSwitchScrutinee::TypeOf(var_name) => {
                    results.push(NarrowingResult {
                        variable_name: var_name.clone(),
                        narrowed_type: TypeBinding {
                            type_name: literal,
                            type_entity_id: None,
                            span: Span::default(),
                            origin: Some(super::types::InferenceOrigin::ControlFlowNarrowing),
                            shape: None,
                        },
                    });
                }
                TypeScriptSwitchScrutinee::Field {
                    var_name,
                    field_name,
                } => {
                    let shape_opt = ctx
                        .get_variable_type(var_name)
                        .and_then(|existing| {
                            existing.shape.clone().or_else(|| {
                                parse_type_shape(&existing.type_name, Language::TypeScript)
                            })
                        })
                        .or_else(|| declared_shape_here(ctx, params, var_name));
                    if let Some(shape) = shape_opt {
                        if let Some(narrowed) =
                            narrow_discriminated_union(&shape, field_name, &literal, type_index)
                        {
                            results.push(narrowing_result(var_name.clone(), narrowed));
                        }
                    }
                }
            }
        }
        search_start = abs_case + 5;
    }
    results
}

/// Switch scrutinee forms eligible for narrowing.
enum TypeScriptSwitchScrutinee {
    /// `switch (typeof x)` narrows `x` per case literal.
    TypeOf(String),
    /// `switch (x.field)` narrows `x` through the discriminated union.
    Field {
        var_name: String,
        field_name: String,
    },
}

/// Extract the scrutinee of a `switch` statement.
fn extract_typescript_switch_scrutinee(text: &str) -> Option<TypeScriptSwitchScrutinee> {
    let switch_pos = text.find("switch")?;
    let after = text[switch_pos + "switch".len()..].trim_start();
    let scrutinee = extract_balanced_parens(after)?.trim().to_string();
    if let Some(rest) = scrutinee.strip_prefix("typeof") {
        let var_name = rest.trim().to_string();
        if is_valid_ident(&var_name) {
            return Some(TypeScriptSwitchScrutinee::TypeOf(var_name));
        }
        return None;
    }
    if scrutinee.contains("instanceof") || scrutinee.contains("typeof") {
        return None;
    }
    let (var_name, field_name) = scrutinee.split_once('.')?;
    let (var_name, field_name) = (var_name.trim(), field_name.trim());
    if !is_valid_ident(var_name) || !is_valid_ident(field_name) || field_name.contains('.') {
        return None;
    }
    Some(TypeScriptSwitchScrutinee::Field {
        var_name: var_name.to_string(),
        field_name: field_name.to_string(),
    })
}

fn parse_typescript_strict_equality_pattern(text: &str) -> Option<(String, String, String)> {
    let raw = strip_typescript_condition_prefix(text)?;
    let mut cleaned = raw.trim().to_string();
    cleaned = strip_outer_parens(&cleaned).trim().to_string();
    // try === then ==
    for op in &["===", "=="] {
        if let Some(pos) = cleaned.find(op) {
            let left = cleaned[..pos].trim();
            let right = cleaned[pos + op.len()..].trim();
            if left.contains('.') {
                if let Some(dot_pos) = left.rfind('.') {
                    let var_name = left[..dot_pos].trim().to_string();
                    let field_name = left[dot_pos + 1..].trim().to_string();
                    if !is_valid_ident(&var_name) || !is_valid_ident(&field_name) {
                        continue;
                    }
                    if let Some(value) = parse_string_literal(right) {
                        return Some((var_name, field_name, value));
                    }
                }
            }
        }
    }
    None
}

fn narrow_typescript_truthiness(
    text: &str,
    ctx: &ScopedTypeContext,
    params: &[(String, Option<String>)],
) -> Vec<NarrowingResult> {
    let mut results = Vec::new();
    if let Some(var_name) = parse_typescript_truthiness_pattern(text) {
        let shape_opt = ctx
            .get_variable_type(&var_name)
            .and_then(|existing| {
                existing
                    .shape
                    .clone()
                    .or_else(|| parse_type_shape(&existing.type_name, Language::TypeScript))
            })
            .or_else(|| declared_shape_here(ctx, params, &var_name));
        if let Some(shape) = shape_opt {
            if let Some(narrowed) = narrow_truthiness(&shape, true, Language::TypeScript) {
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
        results.push(NarrowingResult {
            variable_name: var_name,
            narrowed_type: TypeBinding {
                type_name: "truthy".to_string(),
                type_entity_id: None,
                span: Span::default(),
                origin: Some(super::types::InferenceOrigin::ControlFlowNarrowing),
                shape: None,
            },
        });
    } else if let Some(var_name) = parse_typescript_negated_truthiness_pattern(text) {
        let shape_opt = ctx
            .get_variable_type(&var_name)
            .and_then(|existing| {
                existing
                    .shape
                    .clone()
                    .or_else(|| parse_type_shape(&existing.type_name, Language::TypeScript))
            })
            .or_else(|| declared_shape_here(ctx, params, &var_name));
        if let Some(shape) = shape_opt {
            if let Some(narrowed) = narrow_truthiness(&shape, false, Language::TypeScript) {
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
        results.push(NarrowingResult {
            variable_name: var_name,
            narrowed_type: TypeBinding {
                type_name: "falsy".to_string(),
                type_entity_id: None,
                span: Span::default(),
                origin: Some(super::types::InferenceOrigin::ControlFlowNarrowing),
                shape: None,
            },
        });
    }
    results
}

fn parse_typescript_truthiness_pattern(text: &str) -> Option<String> {
    let raw = strip_typescript_condition_prefix(text)?;
    let mut cleaned = raw.trim().to_string();
    cleaned = strip_outer_parens(cleaned.trim()).trim().to_string();
    if is_valid_ident(&cleaned) {
        return Some(cleaned);
    }
    None
}

fn parse_typescript_negated_truthiness_pattern(text: &str) -> Option<String> {
    let raw = strip_typescript_condition_prefix(text)?;
    let mut cleaned = raw.trim().to_string();
    cleaned = strip_outer_parens(cleaned.trim()).trim().to_string();
    let rest = cleaned.strip_prefix('!')?.trim();
    if is_valid_ident(rest) {
        return Some(rest.to_string());
    }
    None
}

fn narrow_typescript_in_operator(text: &str) -> Vec<NarrowingResult> {
    let Some((key, var_name)) = parse_typescript_in_pattern(text) else {
        return vec![];
    };
    vec![NarrowingResult {
        variable_name: var_name,
        narrowed_type: TypeBinding {
            type_name: format!("HasKey<{}>", key),
            type_entity_id: None,
            span: Span::default(),
            origin: None,
            shape: None,
        },
    }]
}

fn parse_typescript_in_pattern(text: &str) -> Option<(String, String)> {
    let raw = strip_typescript_condition_prefix(text)?;
    let mut cleaned = raw.trim().to_string();
    cleaned = strip_outer_parens(cleaned.trim()).trim().to_string();
    let pos = cleaned.find(" in ")?;
    let left = cleaned[..pos].trim();
    let right = cleaned[pos + 4..].trim();
    let key = parse_string_literal(left)?;
    if !is_valid_ident(right) {
        return None;
    }
    Some((key, right.to_string()))
}

fn narrow_typescript_equality_loose(
    text: &str,
    ctx: &ScopedTypeContext,
    params: &[(String, Option<String>)],
) -> Vec<NarrowingResult> {
    let mut results = Vec::new();
    if let Some((var_name, value)) = parse_typescript_loose_equality_null_pattern(text) {
        results.push(NarrowingResult {
            variable_name: var_name,
            narrowed_type: TypeBinding {
                type_name: value.clone(),
                type_entity_id: None,
                span: Span::default(),
                origin: Some(super::types::InferenceOrigin::ControlFlowNarrowing),
                shape: parse_type_shape(&value, Language::TypeScript),
            },
        });
    }
    // `x != null`: bind the declared union minus null.
    if let Some((var_name, _)) = parse_typescript_loose_inequality_null_pattern(text) {
        if let Some(declared) = declared_shape_here(ctx, params, &var_name) {
            if let Some(narrowed) = subtract_union_members(&declared, &["null".to_string()]) {
                results.push(narrowing_result(var_name, narrowed));
            }
        }
    }
    if let Some((var_name, value)) = parse_typescript_strict_equality_undefined_pattern(text) {
        results.push(NarrowingResult {
            variable_name: var_name,
            narrowed_type: TypeBinding {
                type_name: value.clone(),
                type_entity_id: None,
                span: Span::default(),
                origin: Some(super::types::InferenceOrigin::ControlFlowNarrowing),
                shape: Some(TypeShape::Named(value)),
            },
        });
    }
    results
}

fn parse_typescript_loose_equality_null_pattern(text: &str) -> Option<(String, String)> {
    let raw = strip_typescript_condition_prefix(text)?;
    let mut cleaned = raw.trim().to_string();
    cleaned = strip_outer_parens(cleaned.trim()).trim().to_string();
    // Avoid instanceof/typeof cases
    if cleaned.contains("instanceof") || cleaned.contains("typeof") {
        return None;
    }
    // Need `==` but not `===` and not field access
    // Check for `==` occurrence
    // Prefer `==` handling for null; discriminate union already handled dot case
    // So skip if left contains '.'
    // Find `==` but not `===`
    let mut search_start = 0;
    while let Some(pos) = cleaned[search_start..].find("==") {
        let abs_pos = search_start + pos;
        // Check if it's `===` (followed by `=`)
        let is_strict = cleaned[abs_pos..].starts_with("===");
        if is_strict {
            search_start = abs_pos + 3;
            continue;
        }
        let left = cleaned[..abs_pos].trim();
        let right = cleaned[abs_pos + 2..].trim();
        if left.contains('.') {
            search_start = abs_pos + 2;
            continue;
        }
        if is_valid_ident(left) && right == "null" {
            return Some((left.to_string(), "null | undefined".to_string()));
        }
        search_start = abs_pos + 2;
    }
    None
}

/// Parse `x != null` (loose or strict): returns the variable name.
/// The caller subtracts `null` from the declared union.
fn parse_typescript_loose_inequality_null_pattern(text: &str) -> Option<(String, String)> {
    let raw = strip_typescript_condition_prefix(text)?;
    let mut cleaned = raw.trim().to_string();
    cleaned = strip_outer_parens(cleaned.trim()).trim().to_string();
    if cleaned.contains("instanceof") || cleaned.contains("typeof") {
        return None;
    }
    for op in &["!==", "!="] {
        if let Some(pos) = cleaned.find(op) {
            // `!==` contains `!=`; skipping the strict form here would
            // mis-split, so require the exact operator at this position.
            if *op == "!=" && cleaned[pos..].starts_with("!==") {
                continue;
            }
            let left = cleaned[..pos].trim();
            let right = cleaned[pos + op.len()..].trim();
            if left.contains('.') {
                // Safe-call receivers narrow their base; other dotted
                // expressions stay conservative.
                if right == "null" {
                    if let Some(receiver) = typescript_null_asserted_receiver(left) {
                        return Some((receiver.to_string(), "null".to_string()));
                    }
                }
                continue;
            }
            if is_valid_ident(left) && right == "null" {
                return Some((left.to_string(), "null".to_string()));
            }
            if right == "null" {
                if let Some(receiver) = typescript_null_asserted_receiver(left) {
                    return Some((receiver.to_string(), "null".to_string()));
                }
            }
        }
    }
    None
}

fn parse_typescript_strict_equality_undefined_pattern(text: &str) -> Option<(String, String)> {
    let raw = strip_typescript_condition_prefix(text)?;
    let mut cleaned = raw.trim().to_string();
    cleaned = strip_outer_parens(cleaned.trim()).trim().to_string();
    if let Some(pos) = cleaned.find("===") {
        let left = cleaned[..pos].trim();
        let right = cleaned[pos + 3..].trim();
        if left.contains('.') {
            return None;
        }
        if is_valid_ident(left) && right == "undefined" {
            return Some((left.to_string(), "undefined".to_string()));
        }
    }
    None
}

/// Parse `typeof x === "string"` or `"string" === typeof x`, plus the
/// negated `typeof x !== "..."` which binds the declared union minus the
/// excluded primitive.
fn narrow_typescript_typeof(
    text: &str,
    ctx: &ScopedTypeContext,
    params: &[(String, Option<String>)],
) -> Option<NarrowingResult> {
    let text = strip_typescript_condition_prefix(text)?;

    if let Some(result) = parse_typeof_pattern(text) {
        return Some(result);
    }

    if let Some(result) = parse_typeof_pattern_reversed(text) {
        return Some(result);
    }

    // Negated form: `typeof x !== "string"` / `"string" !== typeof x`.
    let (var_name, excluded) = parse_typeof_negated_pattern(text)?;
    let declared = declared_shape_here(ctx, params, &var_name)?;
    let narrowed = subtract_union_members(&declared, &[excluded])?;
    Some(narrowing_result(var_name, narrowed))
}

/// Parse `typeof x !== "T"` or `"T" !== typeof x` into `(variable, T)`.
fn parse_typeof_negated_pattern(text: &str) -> Option<(String, String)> {
    let text = text.trim();
    let (left, op, right) = split_comparison(text)?;
    if op != "!==" && op != "!=" {
        return None;
    }
    // `typeof x` on either side, string literal on the other.
    if let Some(var) = left
        .trim()
        .strip_prefix("typeof")
        .map(str::trim)
        .filter(|v| is_valid_ident(v))
    {
        if let Some(lit) = parse_string_literal(right.trim()) {
            return Some((var.to_string(), lit));
        }
    }
    if let Some(var) = right
        .trim()
        .strip_prefix("typeof")
        .map(str::trim)
        .filter(|v| is_valid_ident(v))
    {
        if let Some(lit) = parse_string_literal(left.trim()) {
            return Some((var.to_string(), lit));
        }
    }
    None
}

/// Parse `typeof var === "type"`.
fn parse_typeof_pattern(text: &str) -> Option<NarrowingResult> {
    let text = text.trim();

    let rest = text.strip_prefix("typeof")?.trim();

    let (var_name, op, type_literal) = split_comparison(rest)?;
    if op != "===" && op != "==" {
        return None;
    }

    let type_name = parse_string_literal(&type_literal)?;
    let var_name = var_name.trim().to_string();

    if !is_valid_ident(&var_name) {
        return None;
    }

    Some(NarrowingResult {
        variable_name: var_name,
        narrowed_type: TypeBinding {
            type_name,
            type_entity_id: None,
            span: Span::default(),
            origin: Some(super::types::InferenceOrigin::ControlFlowNarrowing),
            shape: None,
        },
    })
}

/// Parse `"type" === typeof var`.
fn parse_typeof_pattern_reversed(text: &str) -> Option<NarrowingResult> {
    let text = text.trim();

    let (left, op, right) = split_comparison(text)?;
    if op != "===" && op != "==" {
        return None;
    }

    let type_literal = parse_string_literal(&left)?;
    let right = right.trim();
    let var_name = right.strip_prefix("typeof")?.trim().to_string();

    if !is_valid_ident(&var_name) {
        return None;
    }

    Some(NarrowingResult {
        variable_name: var_name,
        narrowed_type: TypeBinding {
            type_name: type_literal,
            type_entity_id: None,
            span: Span::default(),
            origin: Some(super::types::InferenceOrigin::ControlFlowNarrowing),
            shape: None,
        },
    })
}

/// Parse `x instanceof Class`.
fn narrow_typescript_instanceof(text: &str) -> Option<NarrowingResult> {
    let text = strip_typescript_condition_prefix(text)?;
    let text = text.trim();

    let parts: Vec<&str> = text.splitn(2, "instanceof").collect();
    if parts.len() != 2 {
        return None;
    }

    let var_name = parts[0].trim().to_string();
    let type_name = parts[1].trim().to_string();

    if var_name.is_empty() || !is_valid_ident(&var_name) || type_name.is_empty() {
        return None;
    }

    Some(NarrowingResult {
        variable_name: var_name,
        narrowed_type: TypeBinding {
            type_name,
            type_entity_id: None,
            span: Span::default(),
            origin: Some(super::types::InferenceOrigin::ControlFlowNarrowing),
            shape: None,
        },
    })
}

/// Receiver of a safe-call chain or non-null assertion (`x?.foo`, `x!`).
///
/// Returns the base identifier when the text carries an assertion marker,
/// or `None` for plain identifiers and non-identifier expressions, so
/// existing plain-identifier behavior stays untouched.
fn typescript_null_asserted_receiver(text: &str) -> Option<&str> {
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

/// Parse `x === null` and the negated `x !== null` (declared union
/// minus null).
fn narrow_typescript_strict_equal_null(
    text: &str,
    ctx: &ScopedTypeContext,
    params: &[(String, Option<String>)],
) -> Option<NarrowingResult> {
    let text = strip_typescript_condition_prefix(text)?;
    let text = text.trim();

    // Negated form first: `!==` contains `==` but not `===`.
    if let Some(pos) = text.find("!==") {
        let var_name = text[..pos].trim();
        let rhs = text[pos + 3..].trim();
        if rhs == "null" {
            if is_valid_ident(var_name) {
                let declared = declared_shape_here(ctx, params, var_name)?;
                let narrowed = subtract_union_members(&declared, &["null".to_string()])?;
                return Some(narrowing_result(var_name.to_string(), narrowed));
            }
            // Safe-call receivers and non-null assertions narrow their base.
            if let Some(receiver) = typescript_null_asserted_receiver(var_name) {
                let declared = declared_shape_here(ctx, params, receiver)?;
                let narrowed = subtract_union_members(&declared, &["null".to_string()])?;
                return Some(narrowing_result(receiver.to_string(), narrowed));
            }
        }
        return None;
    }

    let parts: Vec<&str> = text.splitn(2, "===").collect();
    if parts.len() != 2 {
        return None;
    }

    let var_name = parts[0].trim().to_string();
    let rhs = parts[1].trim();

    if rhs == "null" && !var_name.is_empty() && is_valid_ident(&var_name) {
        Some(NarrowingResult {
            variable_name: var_name,
            narrowed_type: TypeBinding {
                type_name: "null".to_string(),
                type_entity_id: None,
                span: Span::default(),
                origin: Some(super::types::InferenceOrigin::ControlFlowNarrowing),
                shape: None,
            },
        })
    } else {
        None
    }
}

/// Strip TypeScript condition prefixes.
fn strip_typescript_condition_prefix(text: &str) -> Option<&str> {
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

/// Extract constructor call types from entity metadata.
fn extract_constructor_call_types(entities: &[Entity], ctx: &mut ScopedTypeContext) {
    for entity in entities {
        if entity.kind != EntityKind::Variable {
            continue;
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::type_inference::InferenceOrigin;
    use cce_types::entity::EntityId;
    use cce_types::language::Language;

    fn dummy_ctx() -> ScopedTypeContext {
        ScopedTypeContext::new(Language::TypeScript)
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
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "x");
    }

    #[test]
    fn test_typescript_negated_truthiness() {
        let ctx = dummy_ctx();
        let results = narrow_typescript_if("if (!x)", &ctx, None, &[]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "x");
    }

    #[test]
    fn test_typescript_in_operator() {
        let ctx = dummy_ctx();
        let results = narrow_typescript_if("if (\"prop\" in x)", &ctx, None, &[]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "x");
        assert_eq!(results[0].narrowed_type.type_name, "HasKey<prop>");
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
}
