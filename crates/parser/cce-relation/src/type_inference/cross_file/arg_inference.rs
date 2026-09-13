use cce_types::{LiteralKind, classify_numeric_literal, literal_type_name};

use super::super::call_utils::split_call_args;
use super::super::types::{ScopedTypeContext, TypeShape, parse_type_shape};

/// Infer the type shape of a call-site argument expression.
///
/// Literals map to the target language's own vocabulary (`int` for C++,
/// `number` only for JavaScript/TypeScript); identifiers resolve against
/// already-known variable bindings in the caller's context; constructor
/// expressions (`new Foo()`, `Foo()`) resolve to their base type. Anything
/// else yields `None` so the caller keeps its fallback.
pub fn infer_arg_shape(
    ctx: &ScopedTypeContext,
    language: cce_types::language::Language,
    arg: &str,
) -> Option<TypeShape> {
    let trimmed = arg.trim();
    if trimmed.is_empty() {
        return None;
    }
    // String literal (single-quoted single chars resolve to the char type
    // in languages that have one).
    if (trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2)
        || (trimmed.starts_with('\'') && trimmed.ends_with('\'') && trimmed.len() > 3)
        || trimmed.starts_with("r\"")
        || trimmed.starts_with("r#")
        || trimmed.starts_with('`')
    {
        return Some(TypeShape::Named(
            literal_type_name(&language, LiteralKind::String).to_string(),
        ));
    }
    if trimmed.starts_with('\'') && trimmed.ends_with('\'') && trimmed.len() == 3 {
        return Some(TypeShape::Named(
            literal_type_name(&language, LiteralKind::Char).to_string(),
        ));
    }
    // Numeric literal (int, float and suffixed forms like `10u32`).
    if let Some(kind) = classify_numeric_literal(trimmed) {
        return Some(TypeShape::Named(
            literal_type_name(&language, kind).to_string(),
        ));
    }
    // Boolean / null literals.
    if trimmed == "true" || trimmed == "false" {
        return Some(TypeShape::Named(
            literal_type_name(&language, LiteralKind::Boolean).to_string(),
        ));
    }
    if trimmed == "None" || trimmed == "null" || trimmed == "nil" {
        return Some(TypeShape::Named(
            literal_type_name(&language, LiteralKind::Null).to_string(),
        ));
    }
    // Array literal: element type comes from the first element so
    // `first([1, 2])` against `first<T>(arr: T[])` binds `T = int`.
    if trimmed.starts_with('[') && trimmed.ends_with(']') && trimmed.len() >= 2 {
        let inner = trimmed[1..trimmed.len() - 1].trim();
        if inner.is_empty() {
            return Some(TypeShape::Array(Box::new(TypeShape::Named(
                "unknown".to_string(),
            ))));
        }
        let first = split_call_args(inner).into_iter().next();
        let element_shape = first.and_then(|element| infer_arg_shape(ctx, language, &element))?;
        return Some(TypeShape::Array(Box::new(element_shape)));
    }
    // Object literal.
    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        return Some(TypeShape::Named(
            literal_type_name(&language, LiteralKind::Object).to_string(),
        ));
    }
    // Composite literal (`Person{...}`, `[]int{...}`): the head before
    // `{` names the constructed type or, for `[]T`, the element type.
    if let Some(brace_pos) = trimmed.find('{')
        && let Some(end) = trimmed.rfind('}')
        && brace_pos < end
    {
        let head = trimmed[..brace_pos].trim();
        let looks_like_type = !head.is_empty()
            && !head.contains(char::is_whitespace)
            && !head.contains('(')
            && !head.contains(')')
            && !head.contains(',')
            && !head.contains('"')
            && !head.contains('\'');
        if looks_like_type {
            if let Some(element) = head.strip_prefix("[]")
                && !element.contains('[')
                && !element.contains(']')
            {
                let element_shape = parse_type_shape(element, language)
                    .unwrap_or(TypeShape::Named(element.to_string()));
                return Some(TypeShape::Array(Box::new(element_shape)));
            }
            if !head.contains('[') && !head.contains(']') {
                let base = super::super::call_utils::simple_callee_name(head)
                    .split('<')
                    .next()
                    .unwrap_or(head)
                    .trim();
                if !base.is_empty() {
                    return Some(TypeShape::Named(base.to_string()));
                }
            }
        }
    }
    // Constructor expression: `new Foo(...)` or `Foo(...)`.
    let constructor_base = trimmed
        .strip_prefix("new ")
        .unwrap_or(trimmed)
        .split('(')
        .next()
        .unwrap_or(trimmed)
        .trim();
    if constructor_base != trimmed
        && !constructor_base.is_empty()
        && constructor_base
            .chars()
            .next()
            .is_some_and(|c| c.is_uppercase())
    {
        let base = super::super::call_utils::simple_callee_name(constructor_base);
        let base = base.split('<').next().unwrap_or(base).trim();
        if !base.is_empty() {
            return Some(TypeShape::Named(base.to_string()));
        }
    }
    // Identifier: resolve against the caller's known bindings.
    if let Some(binding) = ctx.get_variable_type(trimmed) {
        if let Some(shape) = binding.shape.clone() {
            return Some(shape);
        }
        if !binding.type_name.is_empty() {
            return parse_type_shape(&binding.type_name, language);
        }
    }
    None
}
