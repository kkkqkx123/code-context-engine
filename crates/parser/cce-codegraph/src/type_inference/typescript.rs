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
    ScopedTypeContext, TypeBinding, TypeShape, add_polarity_aware_narrowings,
    covers_declared_members, declared_shape, narrow_discriminated_union, narrow_truthiness,
    parse_type_shape, subtract_union_members, type_shape_to_string,
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

        normalize_literal_types(entities, ctx);
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
                            entity.span,
                        );
                    }
                    ControlFlowFactKind::Match => {
                        for result in narrow_typescript_switch(
                            &fact.text,
                            ctx,
                            inference_ctx.type_index(),
                            &entity.parameters,
                        ) {
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

/// Map parser-generic literal names to TypeScript declaration spellings.
///
/// The shared literal extractor reports lowercase `array` for array
/// literals; TypeScript spells the type `Array`. The remaining literal
/// vocabulary (`string`, `number`, `boolean`, `null`, `object`) already
/// matches TypeScript spelling, so only `array` needs remapping. Without
/// this the same type appears in both forms across origins.
fn normalize_typescript_literal_type(raw: &str) -> Option<&'static str> {
    match raw.trim() {
        "array" => Some("Array"),
        _ => None,
    }
}

/// Rebind literal-origin variables to declaration spellings.
///
/// Only touches unannotated variables whose current binding came from a
/// literal; annotated variables keep the user's text untouched so a
/// precise annotation is never downgraded to the bare literal name.
fn normalize_literal_types(entities: &[Entity], ctx: &mut ScopedTypeContext) {
    for entity in entities {
        if !matches!(
            entity.kind,
            EntityKind::Variable | EntityKind::Field | EntityKind::Property
        ) {
            continue;
        }
        // Composite destructuring names are owned by the destructuring
        // pass; rebinding the joined name would resurrect the leak that
        // the shared extractor skips.
        if entity.name.contains(',') {
            continue;
        }
        if entity
            .metadata
            .get("type_annotation")
            .is_some_and(|ann| !ann.trim().is_empty())
        {
            continue;
        }
        let Some(lit) = entity.metadata.get("literal_type") else {
            continue;
        };
        let Some(normalized) = normalize_typescript_literal_type(lit) else {
            continue;
        };
        let Some(current) = ctx.get_variable_type(&entity.name).cloned() else {
            continue;
        };
        if current.origin != Some(super::types::InferenceOrigin::LiteralType) {
            continue;
        }
        let binding = TypeBinding {
            type_name: normalized.to_string(),
            type_entity_id: None,
            span: entity.span,
            origin: Some(super::types::InferenceOrigin::LiteralType),
            shape: parse_type_shape(normalized, Language::TypeScript),
        };
        ctx.add_variable_type(entity.name.clone(), binding);
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
    if let Some(shape) = shape_opt
        && let Some(narrowed) = narrow_discriminated_union(&shape, &field_name, &value, type_index)
    {
        // Identity narrowings prove nothing; drop them instead of
        // rendering a row equal to the declared type.
        if covers_declared_members(&shape, &narrowed)
            || type_shape_to_string(&shape) == type_shape_to_string(&narrowed)
        {
            return results;
        }
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
                            type_name: literal.clone(),
                            type_entity_id: None,
                            span: Span::default(),
                            origin: Some(super::types::InferenceOrigin::ControlFlowNarrowing),
                            shape: parse_type_shape(&literal, Language::TypeScript),
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
                    if let Some(shape) = shape_opt
                        && let Some(narrowed) =
                            narrow_discriminated_union(&shape, field_name, &literal, type_index)
                    {
                        if covers_declared_members(&shape, &narrowed)
                            || type_shape_to_string(&shape) == type_shape_to_string(&narrowed)
                        {
                            continue;
                        }
                        results.push(narrowing_result(var_name.clone(), narrowed));
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
            if left.contains('.')
                && let Some(dot_pos) = left.rfind('.')
            {
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
    None
}

fn narrow_typescript_truthiness(
    text: &str,
    ctx: &ScopedTypeContext,
    params: &[(String, Option<String>)],
) -> Vec<NarrowingResult> {
    let mut results = Vec::new();
    if let Some(var_name) = parse_typescript_truthiness_pattern(text) {
        // Parameter annotations win over variable bindings (which may
        // leak across sibling scopes); declared_shape encodes that order.
        let shape_opt = declared_shape_here(ctx, params, &var_name);
        if let Some(shape) = shape_opt
            && let Some(narrowed) = narrow_truthiness(&shape, true, Language::TypeScript)
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
        // Unknown type: stay conservative and emit nothing rather than a
        // placeholder pseudo-type.
    } else if let Some(var_name) = parse_typescript_negated_truthiness_pattern(text) {
        let shape_opt = declared_shape_here(ctx, params, &var_name);
        if let Some(shape) = shape_opt
            && let Some(narrowed) = narrow_truthiness(&shape, false, Language::TypeScript)
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
        // Unknown type: stay conservative and emit nothing rather than a
        // placeholder pseudo-type.
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
    let type_name = format!("HasKey<{}>", key);
    vec![NarrowingResult {
        variable_name: var_name,
        narrowed_type: TypeBinding {
            type_name: type_name.clone(),
            type_entity_id: None,
            span: Span::default(),
            origin: Some(super::types::InferenceOrigin::ControlFlowNarrowing),
            shape: parse_type_shape(&type_name, Language::TypeScript),
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
    if let Some((var_name, _)) = parse_typescript_loose_inequality_null_pattern(text)
        && let Some(declared) = declared_shape_here(ctx, params, &var_name)
        && let Some(narrowed) = subtract_union_members(&declared, &["null".to_string()])
    {
        results.push(narrowing_result(var_name, narrowed));
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
                if right == "null"
                    && let Some(receiver) = typescript_null_asserted_receiver(left)
                {
                    return Some((receiver.to_string(), "null".to_string()));
                }
                continue;
            }
            if is_valid_ident(left) && right == "null" {
                return Some((left.to_string(), "null".to_string()));
            }
            if right == "null"
                && let Some(receiver) = typescript_null_asserted_receiver(left)
            {
                return Some((receiver.to_string(), "null".to_string()));
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
        && let Some(lit) = parse_string_literal(right.trim())
    {
        return Some((var.to_string(), lit));
    }
    if let Some(var) = right
        .trim()
        .strip_prefix("typeof")
        .map(str::trim)
        .filter(|v| is_valid_ident(v))
        && let Some(lit) = parse_string_literal(left.trim())
    {
        return Some((var.to_string(), lit));
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
            type_name: type_name.clone(),
            type_entity_id: None,
            span: Span::default(),
            origin: Some(super::types::InferenceOrigin::ControlFlowNarrowing),
            shape: parse_type_shape(&type_name, Language::TypeScript),
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
            type_name: type_literal.clone(),
            type_entity_id: None,
            span: Span::default(),
            origin: Some(super::types::InferenceOrigin::ControlFlowNarrowing),
            shape: parse_type_shape(&type_literal, Language::TypeScript),
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
            type_name: type_name.clone(),
            type_entity_id: None,
            span: Span::default(),
            origin: Some(super::types::InferenceOrigin::ControlFlowNarrowing),
            shape: parse_type_shape(&type_name, Language::TypeScript),
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

#[cfg(test)]
mod tests;
