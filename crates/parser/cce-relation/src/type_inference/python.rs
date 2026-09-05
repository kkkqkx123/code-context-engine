//! Python-specific type inference.

use cce_types::ControlFlowFactKind;
use cce_types::ControlFlowStore;
use cce_types::Span;
use cce_types::entity::{Entity, EntityKind};

use super::control_flow::shared::{
    extract_call_args, is_valid_ident, parse_string_literal, parse_type_arg, split_two_args,
    strip_outer_parens,
};
use super::extractors::{extract_field_type, extract_function_types, extract_variable_type};
use super::traits::LanguageTypeInferer;
use super::types::{
    ScopedTypeContext, TypeBinding, TypeShape, declared_shape, narrow_discriminated_union,
    narrow_truthiness, parse_type_shape, subtract_union_members, type_shape_to_string,
};
use crate::symbol_table::TypeMemberIndex;
use cce_types::language::Language;

/// Python type inference implementation.
pub struct PythonTypeInferer;

impl LanguageTypeInferer for PythonTypeInferer {
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

        // Python-specific: extract constructor call types from metadata
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
                    ControlFlowFactKind::If => {
                        for result in narrow_python_if(
                            &fact.text,
                            ctx,
                            inference_ctx.type_index(),
                            &entity.parameters,
                        ) {
                            ctx.add_narrowed_type(result.variable_name, result.narrowed_type);
                        }
                    }
                    ControlFlowFactKind::Match => {
                        for result in narrow_python_match(&fact.text) {
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

/// Narrow types from a Python `if` condition.
///
/// Patterns:
/// - `isinstance(x, Type)` → x: Type
/// - `isinstance(x, (Type1, Type2))` → x: Type1 | Type2
/// - `not isinstance(x, T)` → x: declared-union-minus-T
/// - `x is None` → x: None
/// - `x is not None` → x: declared-Optional-minus-None
/// - `x.kind == "circle"` → x: Circle (discriminated union)
/// - `if x:` → truthiness (exclude falsy)
/// - `if not x:` → negated truthiness
/// - `"prop" in x` → x: HasKey<prop>
/// - `x == None` → x: None
fn narrow_python_if(
    text: &str,
    ctx: &ScopedTypeContext,
    type_index: Option<&TypeMemberIndex>,
    params: &[(String, Option<String>)],
) -> Vec<NarrowingResult> {
    let text = text.trim();
    let mut results = vec![];

    if let Some(result) = narrow_python_isinstance(text, ctx, params) {
        results.push(result);
    }

    if let Some(result) = narrow_python_is_none(text, ctx, params) {
        results.push(result);
    }

    for result in narrow_python_discriminated_union(text, ctx, type_index) {
        results.push(result);
    }

    for result in narrow_python_truthiness(text, ctx, params) {
        results.push(result);
    }

    for result in narrow_python_in_operator(text) {
        results.push(result);
    }

    for result in narrow_python_equality(text) {
        results.push(result);
    }

    results
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

/// Parse `isinstance(var_name, Type)` or `isinstance(var_name, (T1, T2))`,
/// plus the negated complement `not isinstance(var_name, T)` which binds
/// the declared union minus the excluded type(s).
fn narrow_python_isinstance(
    text: &str,
    ctx: &ScopedTypeContext,
    params: &[(String, Option<String>)],
) -> Option<NarrowingResult> {
    let trimmed = text.trim();
    let trimmed = if trimmed.starts_with("isinstance(") || trimmed.starts_with("not isinstance(") {
        trimmed
    } else {
        strip_python_condition_prefix(trimmed)?
    };
    let trimmed = trimmed.trim();

    if let Some(rest) = trimmed
        .strip_prefix("not ")
        .map(str::trim)
        .filter(|r| r.starts_with("isinstance("))
    {
        let inner = extract_call_args(rest, "isinstance")?;
        let (var_name, type_arg) = split_two_args(inner)?;
        let var_name = var_name.trim();
        let excluded: Vec<String> = parse_type_arg(type_arg.trim())?
            .split(" | ")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if !is_valid_ident(var_name) || excluded.is_empty() {
            return None;
        }
        let declared = declared_shape(ctx, params, Language::Python, var_name)?;
        let narrowed = subtract_union_members(&declared, &excluded)?;
        return Some(narrowing_result(var_name.to_string(), narrowed));
    }

    if trimmed.starts_with("not ") {
        return None;
    }

    let inner = extract_call_args(trimmed, "isinstance")?;
    let (var_name, type_arg) = split_two_args(inner)?;
    let var_name = var_name.trim().to_string();
    let type_arg = type_arg.trim();

    let type_name = parse_type_arg(type_arg)?;

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

/// Parse `var_name is None` and the negated `var_name is not None`
/// (which binds the declared `Optional`/`Union` minus `None`).
fn narrow_python_is_none(
    text: &str,
    ctx: &ScopedTypeContext,
    params: &[(String, Option<String>)],
) -> Option<NarrowingResult> {
    let trimmed = text.trim();
    // Always strip the header prefix first: splitting the raw fact on
    // " is " would otherwise capture "if x" as the variable name.
    let trimmed = strip_python_condition_prefix(trimmed).unwrap_or(trimmed);
    let trimmed = trimmed.trim();

    let parts: Vec<&str> = trimmed.splitn(2, " is ").collect();
    if parts.len() != 2 {
        return None;
    }
    let var_name = parts[0].trim();
    let rest = parts[1].trim();

    if !is_valid_ident(var_name) || var_name.is_empty() {
        return None;
    }
    if rest == "None" {
        return Some(NarrowingResult {
            variable_name: var_name.to_string(),
            narrowed_type: TypeBinding {
                type_name: "None".to_string(),
                type_entity_id: None,
                span: Span::default(),
                origin: Some(super::types::InferenceOrigin::ControlFlowNarrowing),
                shape: None,
            },
        });
    }
    if rest == "not None" {
        let declared = declared_shape(ctx, params, Language::Python, var_name)?;
        let narrowed = subtract_union_members(&declared, &["None".to_string()])?;
        return Some(narrowing_result(var_name.to_string(), narrowed));
    }
    None
}

/// Python discriminated union narrowing: `x.field == "value"` → x: narrowed union.
fn narrow_python_discriminated_union(
    text: &str,
    ctx: &ScopedTypeContext,
    type_index: Option<&TypeMemberIndex>,
) -> Vec<NarrowingResult> {
    let Some((var_name, field_name, value)) = parse_python_equality_pattern(text) else {
        return vec![];
    };
    let mut results = Vec::new();
    if let Some(existing) = ctx.get_variable_type(&var_name) {
        if let Some(shape) = existing
            .shape
            .clone()
            .or_else(|| parse_type_shape(&existing.type_name, Language::Python))
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
            }
        }
    }
    results
}

fn parse_python_equality_pattern(text: &str) -> Option<(String, String, String)> {
    let raw = strip_python_condition_prefix(text)?;
    let mut cleaned = raw.trim().trim_end_matches(':').trim().to_string();
    // Handle possible trailing comments/colon already trimmed; also handle `==` and `!=`? only `==`
    cleaned = cleaned.trim().to_string();
    let pos = cleaned.find("==")?;
    let left = cleaned[..pos].trim();
    let right = cleaned[pos + 2..].trim();
    if left.is_empty() || right.is_empty() {
        return None;
    }
    // Avoid `is` case already handled elsewhere, but ensure we have dot.
    let dot_pos = left.rfind('.')?;
    let var_name = left[..dot_pos].trim().to_string();
    let field_name = left[dot_pos + 1..].trim().to_string();
    if !is_valid_ident(&var_name) || !is_valid_ident(&field_name) {
        return None;
    }
    // Right side may be string literal with trailing colon already removed, but may have extra `:`? Already trimmed.
    let right = right.trim().trim_end_matches(':').trim();
    let value = parse_string_literal(right)?;
    Some((var_name, field_name, value))
}

/// Python truthiness narrowing: `if x:` and `if not x:`, with the
/// declared (parameter-annotation-first) shape consulted before placeholders.
fn narrow_python_truthiness(
    text: &str,
    ctx: &ScopedTypeContext,
    params: &[(String, Option<String>)],
) -> Vec<NarrowingResult> {
    let mut results = Vec::new();
    if let Some(var_name) = parse_python_truthiness_pattern(text) {
        // Try shape-aware narrowing (known bindings, else declared type)
        let shape_opt = ctx
            .get_variable_type(&var_name)
            .and_then(|existing| {
                existing
                    .shape
                    .clone()
                    .or_else(|| parse_type_shape(&existing.type_name, Language::Python))
            })
            .or_else(|| declared_shape(ctx, params, Language::Python, &var_name));
        if let Some(shape) = shape_opt {
            if let Some(narrowed) = narrow_truthiness(&shape, true, Language::Python) {
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
        // Fallback placeholder
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
    } else if let Some(var_name) = parse_python_negated_truthiness_pattern(text) {
        let shape_opt = ctx
            .get_variable_type(&var_name)
            .and_then(|existing| {
                existing
                    .shape
                    .clone()
                    .or_else(|| parse_type_shape(&existing.type_name, Language::Python))
            })
            .or_else(|| declared_shape(ctx, params, Language::Python, &var_name));
        if let Some(shape) = shape_opt {
            if let Some(narrowed) = narrow_truthiness(&shape, false, Language::Python) {
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

fn parse_python_truthiness_pattern(text: &str) -> Option<String> {
    let raw = strip_python_condition_prefix(text)?;
    let cleaned = raw.trim().trim_end_matches(':').trim();
    if is_valid_ident(cleaned) {
        return Some(cleaned.to_string());
    }
    None
}

fn parse_python_negated_truthiness_pattern(text: &str) -> Option<String> {
    let raw = strip_python_condition_prefix(text)?;
    let cleaned = raw.trim().trim_end_matches(':').trim();
    let rest = cleaned.strip_prefix("not")?.trim();
    // must have space after not
    if rest.is_empty() || !is_valid_ident(rest) {
        return None;
    }
    Some(rest.to_string())
}

/// Python `in` operator: `"prop" in x` → x: HasKey<prop>
fn narrow_python_in_operator(text: &str) -> Vec<NarrowingResult> {
    let Some((key, var_name)) = parse_python_in_pattern(text) else {
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

fn parse_python_in_pattern(text: &str) -> Option<(String, String)> {
    let raw = strip_python_condition_prefix(text)?;
    let cleaned = raw.trim().trim_end_matches(':').trim();
    let pos = cleaned.find(" in ")?;
    let left = cleaned[..pos].trim();
    let right = cleaned[pos + 4..].trim();
    let key = parse_string_literal(left)?;
    if !is_valid_ident(right) {
        return None;
    }
    Some((key, right.to_string()))
}

/// Python equality `x == None` → x: None
fn narrow_python_equality(text: &str) -> Vec<NarrowingResult> {
    let Some((var_name, value)) = parse_python_equality_none_pattern(text) else {
        return vec![];
    };
    vec![NarrowingResult {
        variable_name: var_name,
        narrowed_type: TypeBinding {
            type_name: value.clone(),
            type_entity_id: None,
            span: Span::default(),
            origin: Some(super::types::InferenceOrigin::ControlFlowNarrowing),
            shape: Some(TypeShape::Named(value)),
        },
    }]
}

fn parse_python_equality_none_pattern(text: &str) -> Option<(String, String)> {
    let raw = strip_python_condition_prefix(text)?;
    let cleaned = raw.trim().trim_end_matches(':').trim();
    // skip discriminated union pattern which has dot
    let pos = cleaned.find("==")?;
    let left = cleaned[..pos].trim();
    let right = cleaned[pos + 2..].trim().trim_end_matches(':').trim();
    // Avoid field access: if left contains '.', it's discriminated union, skip
    if left.contains('.') {
        return None;
    }
    if !is_valid_ident(left) {
        return None;
    }
    if right == "None" {
        return Some((left.to_string(), "None".to_string()));
    }
    if right == "0" || right == "False" || right == "\"\"" || right == "''" {
        return Some((left.to_string(), right.to_string()));
    }
    None
}

/// Narrow types from a Python `match` expression.
///
/// Patterns (Python 3.10+ match-case):
/// - `case ClassName():` → _subject: ClassName
/// - `case ClassName(var):` → var: ClassName (inner binding)
fn narrow_python_match(text: &str) -> Vec<NarrowingResult> {
    let text = text.trim();
    let mut results = vec![];

    if let Some(result) = narrow_python_match_class_pattern(text) {
        results.push(result);
    }

    results
}

/// Parse a Python match case class pattern: `case ClassName():` or `case ClassName(var):`.
fn narrow_python_match_class_pattern(text: &str) -> Option<NarrowingResult> {
    let text = text.trim();
    let text = text.strip_prefix("case")?.trim();
    let text = text.trim_end_matches(':').trim();

    // Pattern: `ClassName(var)` or `ClassName()`
    let paren_start = text.find('(')?;
    let constructor = text[..paren_start].trim();

    if !is_valid_ident(constructor) || constructor == "_" {
        return None;
    }

    let content = &text[paren_start + 1..text.len() - 1];
    let content = content.trim();

    // If there's a binding variable, use it; otherwise skip (no variable to bind)
    if content.is_empty() || content == "_" {
        return None;
    }

    let var_name = content.trim().to_string();
    if !is_valid_ident(&var_name) {
        return None;
    }

    Some(NarrowingResult {
        variable_name: var_name,
        narrowed_type: TypeBinding {
            type_name: constructor.to_string(),
            type_entity_id: None,
            span: Span::default(),
            origin: Some(super::types::InferenceOrigin::ControlFlowNarrowing),
            shape: None,
        },
    })
}

/// Strip Python condition prefixes (if, elif, while, assert).
fn strip_python_condition_prefix(text: &str) -> Option<&str> {
    let text = text.trim();
    for prefix in &["if", "elif", "while", "assert"] {
        if let Some(rest) = text.strip_prefix(prefix) {
            let rest = rest.trim();
            // Real facts carry the branch body after the header colon;
            // truncate to the condition before stripping parens.
            return Some(strip_outer_parens(truncate_at_header_colon(rest)));
        }
    }
    None
}

/// Cut an `if/while` header at its terminating colon.
///
/// Control-flow facts keep the branch body in `fact.text`
/// (`if not x: return ...`); conditions parse from the header only.
/// Bracket- and string-aware so slices (`x[1:2]`), dict displays and
/// string literals containing colons survive.
fn truncate_at_header_colon(text: &str) -> &str {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut quote: Option<u8> = None;
    let mut triple = false;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if let Some(q) = quote {
            if b == b'\\' && !triple {
                i += 2;
                continue;
            }
            if b == q {
                if triple {
                    if bytes.get(i + 1) == Some(&q) && bytes.get(i + 2) == Some(&q) {
                        quote = None;
                        triple = false;
                        i += 3;
                        continue;
                    }
                } else {
                    quote = None;
                }
            }
            i += 1;
            continue;
        }
        match b {
            b'\'' | b'"' => {
                if bytes.get(i + 1) == Some(&b) && bytes.get(i + 2) == Some(&b) {
                    quote = Some(b);
                    triple = true;
                    i += 3;
                } else {
                    quote = Some(b);
                    i += 1;
                }
            }
            b'(' | b'[' | b'{' => {
                depth += 1;
                i += 1;
            }
            b')' | b']' | b'}' => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            b':' if depth == 0 => return text[..i].trim_end(),
            _ => i += 1,
        }
    }
    text
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
    use cce_types::language::Language;

    fn dummy_ctx() -> ScopedTypeContext {
        ScopedTypeContext::new(Language::Python)
    }

    #[test]
    fn test_python_isinstance_single_type() {
        let ctx = dummy_ctx();
        let result = narrow_python_isinstance("isinstance(x, MyClass)", &ctx, &[]);
        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r.variable_name, "x");
        assert_eq!(r.narrowed_type.type_name, "MyClass");
    }

    #[test]
    fn test_python_isinstance_tuple_type() {
        let ctx = dummy_ctx();
        let result = narrow_python_isinstance("isinstance(x, (int, float))", &ctx, &[]);
        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r.narrowed_type.type_name, "int | float");
    }

    #[test]
    fn test_python_isinstance_in_if() {
        let ctx = dummy_ctx();
        let results = narrow_python_if("if isinstance(x, str):", &ctx, None, &[]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "x");
        assert_eq!(results[0].narrowed_type.type_name, "str");
    }

    #[test]
    fn test_python_isinstance_not_complement() {
        // Negated type checks narrow to the remaining union member.
        let ctx = dummy_ctx();
        let params = [("x".to_string(), Some("Union[str, int]".to_string()))];
        let results = narrow_python_if("if not isinstance(x, str):", &ctx, None, &params);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "x");
        assert_eq!(results[0].narrowed_type.type_name, "int");
    }

    #[test]
    fn test_python_isinstance_not_without_declared_type_skipped() {
        // No declared union: no complement to compute, stay silent.
        let ctx = dummy_ctx();
        let results = narrow_python_if("if not isinstance(x, str):", &ctx, None, &[]);
        assert!(results.is_empty());
    }

    #[test]
    fn test_python_is_none() {
        let ctx = dummy_ctx();
        let result = narrow_python_is_none("x is None", &ctx, &[]);
        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r.variable_name, "x");
        assert_eq!(r.narrowed_type.type_name, "None");
    }

    #[test]
    fn test_python_is_not_none_narrows_optional() {
        // Negated none checks narrow to the remaining member.
        let ctx = dummy_ctx();
        let params = [("x".to_string(), Some("Optional[str]".to_string()))];
        let results = narrow_python_if("if x is not None:", &ctx, None, &params);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "x");
        assert_eq!(results[0].narrowed_type.type_name, "str");
    }

    #[test]
    fn test_python_is_not_none_without_declared_type_skipped() {
        let ctx = dummy_ctx();
        let results = narrow_python_if("if x is not None:", &ctx, None, &[]);
        assert!(results.is_empty());
    }

    #[test]
    fn test_python_header_colon_truncation() {
        // Facts carry the branch body; the condition parses from the header.
        assert_eq!(
            truncate_at_header_colon("not isinstance(x, str): return f\"{x}\""),
            "not isinstance(x, str)"
        );
        assert_eq!(
            truncate_at_header_colon("x[1:2] == y: return 1"),
            "x[1:2] == y"
        );
        assert_eq!(
            truncate_at_header_colon("d == {\"a\": 1}: return 1"),
            "d == {\"a\": 1}"
        );
        // No colon: unchanged.
        assert_eq!(truncate_at_header_colon("x is not None"), "x is not None");
    }

    #[test]
    fn test_python_match_class_pattern() {
        let results = narrow_python_match("case MyClass(val):");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "val");
        assert_eq!(results[0].narrowed_type.type_name, "MyClass");
        assert!(results[0].narrowed_type.origin.is_some());
    }

    #[test]
    fn test_python_match_wildcard_skipped() {
        let results = narrow_python_match("case _:");
        assert!(results.is_empty());
    }

    #[test]
    fn test_python_match_empty_parens_skipped() {
        let results = narrow_python_match("case MyClass():");
        assert!(results.is_empty());
    }

    #[test]
    fn test_python_discriminated_union() {
        // Deterministic: dummy_ctx has no TypeMemberIndex, heuristic removed -> empty
        let ctx = dummy_ctx();
        let results = narrow_python_if("if x.kind == \"circle\":", &ctx, None, &[]);
        assert!(results.is_empty());
    }

    #[test]
    fn test_python_discriminated_union_with_union_shape() {
        // Deterministic: heuristic string matching removed; without TypeMemberIndex returns empty
        let mut ctx = ScopedTypeContext::new(Language::Python);
        ctx.add_variable_type(
            "x".to_string(),
            TypeBinding {
                type_name: "Circle | Square".to_string(),
                type_entity_id: None,
                span: Span::default(),
                origin: None,
                shape: parse_type_shape("Circle | Square", Language::Python),
            },
        );
        let results = narrow_python_if("if x.kind == \"circle\":", &ctx, None, &[]);
        assert!(results.is_empty());
    }

    #[test]
    fn test_python_truthiness() {
        let ctx = dummy_ctx();
        let results = narrow_python_if("if x:", &ctx, None, &[]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "x");
    }

    #[test]
    fn test_python_negated_truthiness() {
        let ctx = dummy_ctx();
        let results = narrow_python_if("if not x:", &ctx, None, &[]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "x");
    }

    #[test]
    fn test_python_truthiness_with_union() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        ctx.add_variable_type(
            "x".to_string(),
            TypeBinding {
                type_name: "str | None".to_string(),
                type_entity_id: None,
                span: Span::default(),
                origin: None,
                shape: parse_type_shape("str | None", Language::Python),
            },
        );
        let results = narrow_python_if("if x:", &ctx, None, &[]);
        // Truthiness true should filter falsy None -> only str
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "x");
        // Should narrow to str (truthy branch excludes None)
        assert!(results[0].narrowed_type.type_name.contains("str"));
    }

    #[test]
    fn test_python_in_operator() {
        let ctx = dummy_ctx();
        let results = narrow_python_if("if \"prop\" in x:", &ctx, None, &[]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "x");
        assert_eq!(results[0].narrowed_type.type_name, "HasKey<prop>");
    }

    #[test]
    fn test_python_equality_none() {
        let ctx = dummy_ctx();
        let results = narrow_python_if("if x == None:", &ctx, None, &[]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].variable_name, "x");
        assert_eq!(results[0].narrowed_type.type_name, "None");
    }
}
