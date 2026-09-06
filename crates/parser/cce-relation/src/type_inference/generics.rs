//! Generic type parsing and parameter binding framework.
//!
//! Parses generic type strings like `List<T>` or `HashMap<String, Integer>`
//! into structured `GenericType` values and provides utilities for
//! substituting type parameters.

use std::collections::HashMap;

use cce_types::language::Language;

use super::types::{TypeShape, instantiate_type_shape, parse_type_shape};

/// Parsed generic type with base name and type arguments.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GenericType {
    pub base: String,
    pub args: Vec<GenericTypeArg>,
}

/// A single generic type argument.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GenericTypeArg {
    /// Concrete type argument (e.g., "String")
    Concrete(String),
    /// Type parameter reference (e.g., "T", "K")
    Param(String),
    /// Nested generic (e.g., "List<T>")
    Nested(GenericType),
    /// Wildcard (e.g., "?", "? extends Number")
    Wildcard { bound: Option<String> },
}

/// Parse "HashMap<String, Integer>" into GenericType.
pub fn parse_generic_type(type_name: &str) -> Option<GenericType> {
    let trimmed = type_name.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Wildcard solo?
    if trimmed == "?" || trimmed.starts_with("? ") {
        let bound = trimmed.strip_prefix('?').and_then(|rest| {
            let rest = rest.trim();
            if rest.is_empty() {
                None
            } else if let Some(stripped) = rest.strip_prefix("extends") {
                Some(stripped.trim().to_string())
            } else if let Some(stripped) = rest.strip_prefix("super") {
                Some(stripped.trim().to_string())
            } else {
                Some(rest.to_string())
            }
        });
        return Some(GenericType {
            base: "?".to_string(),
            args: vec![GenericTypeArg::Wildcard { bound }],
        });
    }
    // No generic args -> simple type not generic
    let start = trimmed.find('<')?;
    let end = trimmed.rfind('>')?;
    if start >= end {
        return None;
    }
    let base = trimmed[..start].trim().to_string();
    if base.is_empty() {
        return None;
    }
    let inner = trimmed[start + 1..end].trim();
    if inner.is_empty() {
        return Some(GenericType { base, args: vec![] });
    }
    let args = split_generic_args(inner)
        .into_iter()
        .filter_map(|arg| parse_generic_arg(&arg))
        .collect::<Vec<_>>();
    if args.is_empty() {
        return None;
    }
    Some(GenericType { base, args })
}

fn split_generic_args(inner: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut depth = 0usize;
    let mut current = String::new();
    for ch in inner.chars() {
        match ch {
            '<' => {
                depth += 1;
                current.push(ch);
            }
            '>' => {
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            ',' if depth == 0 => {
                args.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        args.push(current.trim().to_string());
    }
    args
}

fn parse_generic_arg(arg: &str) -> Option<GenericTypeArg> {
    let trimmed = arg.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed == "?" || trimmed.starts_with("? ") || trimmed.starts_with("?extends") {
        // Wildcard with optional bound
        let bound = if trimmed == "?" {
            None
        } else {
            let rest = trimmed[1..].trim();
            if let Some(stripped) = rest.strip_prefix("extends") {
                Some(stripped.trim().to_string())
            } else if let Some(stripped) = rest.strip_prefix("super") {
                Some(stripped.trim().to_string())
            } else {
                Some(rest.to_string())
            }
        };
        return Some(GenericTypeArg::Wildcard { bound });
    }
    // Single uppercase letter is likely a type param
    if trimmed.len() == 1
        && trimmed
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_uppercase())
    {
        return Some(GenericTypeArg::Param(trimmed.to_string()));
    }
    // Common type param names
    if ["T", "K", "V", "E", "U", "R", "A", "B", "C"].contains(&trimmed) {
        return Some(GenericTypeArg::Param(trimmed.to_string()));
    }
    // Nested generic?
    if trimmed.contains('<') {
        if let Some(nested) = parse_generic_type(trimmed) {
            return Some(GenericTypeArg::Nested(nested));
        }
    }
    Some(GenericTypeArg::Concrete(trimmed.to_string()))
}

/// Format GenericType back to string.
pub fn format_generic_type(gt: &GenericType) -> String {
    if gt.base == "?" {
        if let Some(GenericTypeArg::Wildcard { bound: Some(b) }) = gt.args.first() {
            return format!("? extends {}", b);
        }
        return "?".to_string();
    }
    if gt.args.is_empty() {
        return gt.base.clone();
    }
    let args_str = gt
        .args
        .iter()
        .map(|arg| match arg {
            GenericTypeArg::Concrete(s) => s.clone(),
            GenericTypeArg::Param(p) => p.clone(),
            GenericTypeArg::Nested(n) => format_generic_type(n),
            GenericTypeArg::Wildcard { bound: Some(b) } => format!("? extends {}", b),
            GenericTypeArg::Wildcard { bound: None } => "?".to_string(),
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("{}<{}>", gt.base, args_str)
}

/// Resolve a generic type by substituting type parameters.
///
/// E.g., resolve "List<T>" with {"T" -> "String"} = "List<String>"
pub fn resolve_generic(gt: &GenericType, bindings: &HashMap<String, String>) -> GenericType {
    let resolved_args = gt
        .args
        .iter()
        .map(|arg| match arg {
            GenericTypeArg::Param(p) => {
                if let Some(concrete) = bindings.get(p) {
                    GenericTypeArg::Concrete(concrete.clone())
                } else {
                    GenericTypeArg::Param(p.clone())
                }
            }
            GenericTypeArg::Nested(n) => {
                let resolved = resolve_generic(n, bindings);
                GenericTypeArg::Nested(resolved)
            }
            other => other.clone(),
        })
        .collect();
    GenericType {
        base: gt.base.clone(),
        args: resolved_args,
    }
}

/// Extract element type from a collection type using generic framework.
pub fn extract_element_type(type_name: &str) -> Option<String> {
    let gt = parse_generic_type(type_name)?;
    gt.args.first().map(|arg| match arg {
        GenericTypeArg::Concrete(s) => s.clone(),
        GenericTypeArg::Param(p) => p.clone(),
        GenericTypeArg::Nested(n) => n.base.clone(),
        GenericTypeArg::Wildcard { bound: Some(b) } => b.clone(),
        GenericTypeArg::Wildcard { bound: None } => "?".to_string(),
    })
}

/// Infer generic return type from function entity and argument types.
pub fn infer_generic_return_type(
    func_name: &str,
    return_type: &Option<String>,
    bindings: &HashMap<String, String>,
) -> Option<String> {
    let ret = return_type.as_ref()?;
    // If return type is a type param directly, substitute
    if let Some(concrete) = bindings.get(ret) {
        return Some(concrete.clone());
    }
    // If return type is generic, resolve
    if let Some(gt) = parse_generic_type(ret) {
        let resolved = resolve_generic(&gt, bindings);
        return Some(format_generic_type(&resolved));
    }
    // For simple cases, return as is if not param
    let _ = func_name;
    None
}

/// Whether a type shape still references an unbound type parameter.
///
/// Used to decide if generic substitution can make progress (the return
/// type mentions `T`) and whether a substitution result is fully concrete.
pub fn shape_contains_param(shape: &TypeShape) -> bool {
    match shape {
        TypeShape::Param(_) => true,
        TypeShape::Generic { args, .. } => args.iter().any(shape_contains_param),
        TypeShape::Array(inner) => shape_contains_param(inner),
        TypeShape::Union(members) | TypeShape::Intersection(members) => {
            members.iter().any(shape_contains_param)
        }
        TypeShape::Reference { inner, .. } => shape_contains_param(inner),
        TypeShape::Named(_) | TypeShape::Wildcard { .. } => false,
    }
}

/// Split a comma-separated argument list while respecting nesting.
///
/// Tracks `()`, `[]`, `{}` and `<>` depth plus string quotes so call-site
/// argument expressions such as `f(a, g(1, 2), [x, y])` split into exactly
/// three items. Unbalanced input yields a single item (the whole string).
pub fn split_call_args(args_text: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut depth_paren = 0usize;
    let mut depth_bracket = 0usize;
    let mut depth_brace = 0usize;
    let mut depth_angle = 0usize;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut current = String::new();
    for ch in args_text.chars() {
        if let Some(q) = quote {
            current.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == q {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' => {
                quote = Some(ch);
                current.push(ch);
            }
            '(' => {
                depth_paren += 1;
                current.push(ch);
            }
            ')' => {
                depth_paren = depth_paren.saturating_sub(1);
                current.push(ch);
            }
            '[' => {
                depth_bracket += 1;
                current.push(ch);
            }
            ']' => {
                depth_bracket = depth_bracket.saturating_sub(1);
                current.push(ch);
            }
            '{' => {
                depth_brace += 1;
                current.push(ch);
            }
            '}' => {
                depth_brace = depth_brace.saturating_sub(1);
                current.push(ch);
            }
            '<' => {
                depth_angle += 1;
                current.push(ch);
            }
            '>' => {
                depth_angle = depth_angle.saturating_sub(1);
                current.push(ch);
            }
            ',' if depth_paren == 0
                && depth_bracket == 0
                && depth_brace == 0
                && depth_angle == 0 =>
            {
                args.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() || !args.is_empty() {
        args.push(current.trim().to_string());
    }
    args
}

/// Split a stored call target into its callee name and argument expressions.
///
/// Stored targets look like `foo`, `module.func(a, b)` or `obj.m(x)`.
/// Returns the full callee path (qualification is stripped by callers) and
/// the raw argument texts (possibly empty). Malformed input yields the whole
/// string as the name with no arguments.
pub fn split_call_target(target: &str) -> (String, Vec<String>) {
    let trimmed = target.trim();
    let Some(paren_pos) = trimmed.find('(') else {
        return (trimmed.to_string(), Vec::new());
    };
    let name = trimmed[..paren_pos].trim().to_string();
    let rest = &trimmed[paren_pos + 1..];
    let Some(close_pos) = rest.rfind(')') else {
        return (trimmed.to_string(), Vec::new());
    };
    let args = split_call_args(rest[..close_pos].trim());
    (name, args)
}

/// Unify one formal parameter shape against a call-site actual shape.
///
/// Records `Param(name) -> actual` bindings, recursing through generic
/// arguments, arrays, references and unions positionally. Bracket-tuple
/// spellings (`[K, V]` vs `[string, number]`) unify element-wise so tuple
///-style parameters still bind. The first binding for a parameter wins so
/// repeated parameters (`f<T>(a: T, b: T)`) stay deterministic. Unknown
/// actuals and unparseable fragments are skipped without failing the rest.
fn unify_shape(
    formal: &TypeShape,
    actual: &TypeShape,
    bindings: &mut HashMap<String, TypeShape>,
    language: Language,
) {
    match (formal, actual) {
        (TypeShape::Param(name), actual) => {
            if matches!(actual, TypeShape::Param(other) if other == name) {
                return;
            }
            bindings
                .entry(name.clone())
                .or_insert_with(|| actual.clone());
        }
        (
            TypeShape::Generic { base, args },
            TypeShape::Generic {
                base: actual_base,
                args: actual_args,
            },
        ) if base == actual_base => {
            for (formal_arg, actual_arg) in args.iter().zip(actual_args.iter()) {
                unify_shape(formal_arg, actual_arg, bindings, language);
            }
        }
        (TypeShape::Array(formal_inner), TypeShape::Array(actual_inner)) => {
            unify_shape(formal_inner, actual_inner, bindings, language);
        }
        (
            TypeShape::Reference { inner, .. },
            TypeShape::Reference {
                inner: actual_inner,
                ..
            },
        ) => {
            unify_shape(inner, actual_inner, bindings, language);
        }
        (TypeShape::Union(formal_members), TypeShape::Union(actual_members))
        | (TypeShape::Intersection(formal_members), TypeShape::Intersection(actual_members)) => {
            for (formal_member, actual_member) in formal_members.iter().zip(actual_members.iter()) {
                unify_shape(formal_member, actual_member, bindings, language);
            }
        }
        (TypeShape::Named(formal_name), TypeShape::Named(actual_name)) => {
            unify_bracket_tuple(formal_name, actual_name, bindings, language);
        }
        _ => {}
    }
}

/// Element-wise unification for bracket-tuple spellings.
///
/// `parse_type_shape` keeps tuple fragments such as `[K, V]` as opaque
/// `Named` text; when both sides are bracketed lists of equal length, parse
/// each element and unify pairwise so `K`/`V` still bind against
/// `[string, number]`. Anything else is left alone.
fn unify_bracket_tuple(
    formal_name: &str,
    actual_name: &str,
    bindings: &mut HashMap<String, TypeShape>,
    language: Language,
) {
    let formal_inner = bracket_tuple_inner(formal_name);
    let actual_inner = bracket_tuple_inner(actual_name);
    let (Some(formal_inner), Some(actual_inner)) = (formal_inner, actual_inner) else {
        return;
    };
    let formal_parts = split_call_args(&formal_inner);
    let actual_parts = split_call_args(&actual_inner);
    if formal_parts.len() != actual_parts.len() || formal_parts.is_empty() {
        return;
    }
    for (formal_part, actual_part) in formal_parts.iter().zip(actual_parts.iter()) {
        let (Some(formal_shape), Some(actual_shape)) = (
            parse_type_shape(formal_part, language),
            parse_type_shape(actual_part, language),
        ) else {
            continue;
        };
        unify_shape(&formal_shape, &actual_shape, bindings, language);
    }
}

/// Extract the inner text of a bracket-tuple spelling like `[K, V]`.
fn bracket_tuple_inner(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if !(trimmed.starts_with('[') && trimmed.ends_with(']')) {
        return None;
    }
    Some(trimmed[1..trimmed.len() - 1].trim().to_string())
}

/// Bind formal parameter shapes to call-site actual shapes.
///
/// Pairs formals with actuals by position; extras on either side are
/// ignored and unknown (`None`) actuals are skipped, so partially known
/// call sites still yield usable bindings for substitution.
pub fn bind_call_site_generics(
    formal_params: &[TypeShape],
    actual_args: &[Option<&TypeShape>],
    language: Language,
) -> HashMap<String, TypeShape> {
    let mut bindings = HashMap::new();
    for (formal, actual) in formal_params.iter().zip(actual_args.iter()) {
        if let Some(actual_shape) = actual {
            unify_shape(formal, actual_shape, &mut bindings, language);
        }
    }
    bindings
}

/// Substitute a generic return shape using call-site argument shapes.
///
/// Parses nothing itself: callers supply already-parsed formal parameter
/// shapes, the return shape, and per-argument shapes (`None` for unknown).
/// Returns `None` when the return type mentions no type parameter or when
/// no binding applies, so callers keep their existing fallback. The result
/// may still contain parameters when only some bind; use
/// [`shape_contains_param`] to require fully concrete results.
pub fn substitute_call_return_type(
    formal_params: &[TypeShape],
    return_shape: &TypeShape,
    actual_args: &[Option<&TypeShape>],
    language: Language,
) -> Option<TypeShape> {
    if !shape_contains_param(return_shape) {
        return None;
    }
    let bindings = bind_call_site_generics(formal_params, actual_args, language);
    if bindings.is_empty() {
        return None;
    }
    Some(instantiate_type_shape(return_shape, &bindings))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_generic() {
        let gt = parse_generic_type("List<String>").unwrap();
        assert_eq!(gt.base, "List");
        assert_eq!(gt.args.len(), 1);
        assert_eq!(gt.args[0], GenericTypeArg::Concrete("String".to_string()));
    }

    #[test]
    fn test_parse_multi_param() {
        let gt = parse_generic_type("HashMap<String, Integer>").unwrap();
        assert_eq!(gt.base, "HashMap");
        assert_eq!(gt.args.len(), 2);
        assert_eq!(gt.args[0], GenericTypeArg::Concrete("String".to_string()));
        assert_eq!(gt.args[1], GenericTypeArg::Concrete("Integer".to_string()));
    }

    #[test]
    fn test_parse_nested_generic() {
        let gt = parse_generic_type("Map<String, List<Integer>>").unwrap();
        assert_eq!(gt.base, "Map");
        assert_eq!(gt.args.len(), 2);
        assert!(matches!(gt.args[1], GenericTypeArg::Nested(_)));
    }

    #[test]
    fn test_parse_type_param() {
        let gt = parse_generic_type("List<T>").unwrap();
        assert_eq!(gt.base, "List");
        assert_eq!(gt.args[0], GenericTypeArg::Param("T".to_string()));
    }

    #[test]
    fn test_parse_wildcard() {
        let gt = parse_generic_type("List<?>").unwrap();
        assert_eq!(gt.base, "List");
        assert_eq!(gt.args[0], GenericTypeArg::Wildcard { bound: None });
    }

    #[test]
    fn test_format_generic() {
        let gt = GenericType {
            base: "HashMap".to_string(),
            args: vec![
                GenericTypeArg::Concrete("String".to_string()),
                GenericTypeArg::Concrete("Integer".to_string()),
            ],
        };
        assert_eq!(format_generic_type(&gt), "HashMap<String, Integer>");
    }

    #[test]
    fn test_resolve_generic() {
        let gt = parse_generic_type("List<T>").unwrap();
        let mut bindings = HashMap::new();
        bindings.insert("T".to_string(), "String".to_string());
        let resolved = resolve_generic(&gt, &bindings);
        assert_eq!(format_generic_type(&resolved), "List<String>");
    }

    #[test]
    fn test_extract_element_type() {
        assert_eq!(
            extract_element_type("Array<string>"),
            Some("string".to_string())
        );
        assert_eq!(extract_element_type("Vec<i32>"), Some("i32".to_string()));
        assert_eq!(extract_element_type("List<User>"), Some("User".to_string()));
        assert_eq!(extract_element_type("number"), None);
    }

    #[test]
    fn test_parse_non_generic_returns_none() {
        assert_eq!(parse_generic_type("string"), None);
        assert_eq!(parse_generic_type(""), None);
    }

    // ==================== Complex nested generics ====================

    #[test]
    fn test_parse_deeply_nested_generic() {
        let gt = parse_generic_type("Map<String, List<Option<Integer>>>").unwrap();
        assert_eq!(gt.base, "Map");
        assert_eq!(gt.args.len(), 2);
        assert_eq!(gt.args[0], GenericTypeArg::Concrete("String".to_string()));
        match &gt.args[1] {
            GenericTypeArg::Nested(inner) => {
                assert_eq!(inner.base, "List");
                assert_eq!(inner.args.len(), 1);
                match &inner.args[0] {
                    GenericTypeArg::Nested(option_inner) => {
                        assert_eq!(option_inner.base, "Option");
                        assert_eq!(
                            option_inner.args[0],
                            GenericTypeArg::Concrete("Integer".to_string())
                        );
                    }
                    _ => panic!("Expected Nested for Option<T>"),
                }
            }
            _ => panic!("Expected Nested for List<Option<Integer>>"),
        }
    }

    #[test]
    fn test_parse_triple_nested_generic() {
        let gt = parse_generic_type("Vec<HashMap<String, Vec<i32>>>").unwrap();
        assert_eq!(gt.base, "Vec");
        assert_eq!(gt.args.len(), 1);
        match &gt.args[0] {
            GenericTypeArg::Nested(inner) => {
                assert_eq!(inner.base, "HashMap");
                assert_eq!(inner.args.len(), 2);
                assert_eq!(
                    inner.args[0],
                    GenericTypeArg::Concrete("String".to_string())
                );
                match &inner.args[1] {
                    GenericTypeArg::Nested(vec_inner) => {
                        assert_eq!(vec_inner.base, "Vec");
                        assert_eq!(
                            vec_inner.args[0],
                            GenericTypeArg::Concrete("i32".to_string())
                        );
                    }
                    _ => panic!("Expected Nested for Vec<i32>"),
                }
            }
            _ => panic!("Expected Nested for HashMap<String, Vec<i32>>"),
        }
    }

    // ==================== Wildcard with bounds ====================

    #[test]
    fn test_parse_wildcard_extends() {
        let gt = parse_generic_type("List<? extends Number>").unwrap();
        assert_eq!(gt.base, "List");
        assert_eq!(
            gt.args[0],
            GenericTypeArg::Wildcard {
                bound: Some("Number".to_string())
            }
        );
    }

    #[test]
    fn test_parse_wildcard_super() {
        let gt = parse_generic_type("List<? super String>").unwrap();
        assert_eq!(gt.base, "List");
        assert_eq!(
            gt.args[0],
            GenericTypeArg::Wildcard {
                bound: Some("String".to_string())
            }
        );
    }

    #[test]
    fn test_parse_solo_wildcard_extends() {
        let gt = parse_generic_type("? extends Comparable").unwrap();
        assert_eq!(gt.base, "?");
        assert_eq!(
            gt.args[0],
            GenericTypeArg::Wildcard {
                bound: Some("Comparable".to_string())
            }
        );
    }

    #[test]
    fn test_parse_solo_wildcard_super() {
        let gt = parse_generic_type("? super Number").unwrap();
        assert_eq!(gt.base, "?");
        assert_eq!(
            gt.args[0],
            GenericTypeArg::Wildcard {
                bound: Some("Number".to_string())
            }
        );
    }

    #[test]
    fn test_format_wildcard_extends() {
        let gt = GenericType {
            base: "List".to_string(),
            args: vec![GenericTypeArg::Wildcard {
                bound: Some("Number".to_string()),
            }],
        };
        assert_eq!(format_generic_type(&gt), "List<? extends Number>");
    }

    #[test]
    fn test_format_wildcard_none() {
        let gt = GenericType {
            base: "List".to_string(),
            args: vec![GenericTypeArg::Wildcard { bound: None }],
        };
        assert_eq!(format_generic_type(&gt), "List<?>");
    }

    // ==================== Resolve with multiple params ====================

    #[test]
    fn test_resolve_multi_param_generic() {
        let gt = parse_generic_type("HashMap<K, V>").unwrap();
        let mut bindings = HashMap::new();
        bindings.insert("K".to_string(), "String".to_string());
        bindings.insert("V".to_string(), "i32".to_string());
        let resolved = resolve_generic(&gt, &bindings);
        assert_eq!(format_generic_type(&resolved), "HashMap<String, i32>");
    }

    #[test]
    fn test_resolve_nested_generic() {
        let gt = parse_generic_type("List<Option<T>>").unwrap();
        let mut bindings = HashMap::new();
        bindings.insert("T".to_string(), "String".to_string());
        let resolved = resolve_generic(&gt, &bindings);
        assert_eq!(format_generic_type(&resolved), "List<Option<String>>");
    }

    #[test]
    fn test_resolve_partial_bindings() {
        let gt = parse_generic_type("Pair<T, U>").unwrap();
        let mut bindings = HashMap::new();
        bindings.insert("T".to_string(), "String".to_string());
        let resolved = resolve_generic(&gt, &bindings);
        assert_eq!(format_generic_type(&resolved), "Pair<String, U>");
    }

    // ==================== Extract element type edge cases ====================

    #[test]
    fn test_extract_element_type_nested() {
        assert_eq!(
            extract_element_type("Vec<Option<String>>"),
            Some("Option".to_string())
        );
    }

    #[test]
    fn test_extract_element_type_multi_arg() {
        assert_eq!(
            extract_element_type("HashMap<String, Integer>"),
            Some("String".to_string())
        );
    }

    #[test]
    fn test_extract_element_type_wildcard() {
        assert_eq!(extract_element_type("List<?>"), Some("?".to_string()));
    }

    #[test]
    fn test_extract_element_type_wildcard_bound() {
        assert_eq!(
            extract_element_type("List<? extends Number>"),
            Some("Number".to_string())
        );
    }

    // ==================== infer_generic_return_type ====================

    #[test]
    fn test_infer_generic_return_type_direct_param() {
        let mut bindings = HashMap::new();
        bindings.insert("T".to_string(), "String".to_string());
        let result = infer_generic_return_type("identity", &Some("T".to_string()), &bindings);
        assert_eq!(result, Some("String".to_string()));
    }

    #[test]
    fn test_infer_generic_return_type_generic_container() {
        let mut bindings = HashMap::new();
        bindings.insert("T".to_string(), "i32".to_string());
        let result = infer_generic_return_type("wrap", &Some("Vec<T>".to_string()), &bindings);
        assert_eq!(result, Some("Vec<i32>".to_string()));
    }

    #[test]
    fn test_infer_generic_return_type_no_return() {
        let bindings = HashMap::new();
        let result = infer_generic_return_type("void_func", &None, &bindings);
        assert!(result.is_none());
    }

    #[test]
    fn test_infer_generic_return_type_non_generic_return() {
        let bindings = HashMap::new();
        let result =
            infer_generic_return_type("get_string", &Some("String".to_string()), &bindings);
        assert!(result.is_none());
    }

    #[test]
    fn test_infer_generic_return_type_unbound_param() {
        let bindings = HashMap::new();
        let result = infer_generic_return_type("generic", &Some("T".to_string()), &bindings);
        assert!(result.is_none());
    }

    // ==================== Format roundtrip ====================

    #[test]
    fn test_format_roundtrip_complex() {
        let gt = parse_generic_type("HashMap<String, List<Option<i32>>>").unwrap();
        let formatted = format_generic_type(&gt);
        let reparsed = parse_generic_type(&formatted).unwrap();
        assert_eq!(gt, reparsed);
    }

    #[test]
    fn test_format_empty_args() {
        let gt = GenericType {
            base: "Vec".to_string(),
            args: vec![],
        };
        assert_eq!(format_generic_type(&gt), "Vec");
    }

    #[test]
    fn test_parse_empty_angle_brackets() {
        let gt = parse_generic_type("Vec<>");
        assert!(gt.is_some());
        let gt = gt.unwrap();
        assert_eq!(gt.base, "Vec");
        assert!(gt.args.is_empty());
    }

    #[test]
    fn test_parse_whitespace_handling() {
        let gt = parse_generic_type("  List<  String  >  ").unwrap();
        assert_eq!(gt.base, "List");
        assert_eq!(gt.args[0], GenericTypeArg::Concrete("String".to_string()));
    }

    // ==================== split_call_args ====================

    #[test]
    fn test_split_call_args_simple() {
        assert_eq!(split_call_args(""), Vec::<String>::new());
        assert_eq!(split_call_args("42"), vec!["42".to_string()]);
        assert_eq!(
            split_call_args("42, \"answer\", x"),
            vec!["42".to_string(), "\"answer\"".to_string(), "x".to_string()]
        );
    }

    #[test]
    fn test_split_call_args_nested() {
        assert_eq!(
            split_call_args("a, g(1, 2), [x, y]"),
            vec!["a".to_string(), "g(1, 2)".to_string(), "[x, y]".to_string()]
        );
        assert_eq!(
            split_call_args("f(\"a,b\"), {k: 1}"),
            vec!["f(\"a,b\")".to_string(), "{k: 1}".to_string()]
        );
    }

    #[test]
    fn test_split_call_target() {
        let (name, args) = split_call_target("makePair");
        assert_eq!(name, "makePair");
        assert!(args.is_empty());
        let (name, args) = split_call_target("makePair(42, \"answer\")");
        assert_eq!(name, "makePair");
        assert_eq!(args, vec!["42".to_string(), "\"answer\"".to_string()]);
        let (name, args) = split_call_target("obj.method(x)");
        assert_eq!(name, "obj.method");
        assert_eq!(args, vec!["x".to_string()]);
    }

    // ==================== bind_call_site_generics ====================

    fn shape_of(text: &str) -> TypeShape {
        parse_type_shape(text, Language::TypeScript).expect("test shape should parse")
    }

    #[test]
    fn test_bind_direct_param() {
        let formals = vec![shape_of("T")];
        let actual = shape_of("number");
        let bindings = bind_call_site_generics(&formals, &[Some(&actual)], Language::TypeScript);
        assert_eq!(bindings.get("T"), Some(&actual));
    }

    #[test]
    fn test_bind_nested_generic() {
        let formals = vec![shape_of("Array<T>")];
        let actual = shape_of("Array<string>");
        let bindings = bind_call_site_generics(&formals, &[Some(&actual)], Language::TypeScript);
        assert_eq!(
            bindings.get("T"),
            Some(&TypeShape::Named("string".to_string()))
        );
    }

    #[test]
    fn test_bind_bracket_tuple() {
        let formals = vec![shape_of("Array<[K, V]>")];
        let actual = shape_of("Array<[string, number]>");
        let bindings = bind_call_site_generics(&formals, &[Some(&actual)], Language::TypeScript);
        assert_eq!(
            bindings.get("K"),
            Some(&TypeShape::Named("string".to_string()))
        );
        assert_eq!(
            bindings.get("V"),
            Some(&TypeShape::Named("number".to_string()))
        );
    }

    #[test]
    fn test_bind_skips_unknown_actual() {
        let formals = vec![shape_of("T")];
        let bindings = bind_call_site_generics(&formals, &[None], Language::TypeScript);
        assert!(bindings.is_empty());
    }

    #[test]
    fn test_substitute_identity() {
        let formals = vec![shape_of("T")];
        let ret = shape_of("T");
        let actual = shape_of("number");
        let result =
            substitute_call_return_type(&formals, &ret, &[Some(&actual)], Language::TypeScript);
        assert_eq!(result, Some(TypeShape::Named("number".to_string())));
    }

    #[test]
    fn test_substitute_container() {
        let formals = vec![shape_of("T")];
        let ret = shape_of("Array<T>");
        let actual = shape_of("number");
        let result =
            substitute_call_return_type(&formals, &ret, &[Some(&actual)], Language::TypeScript);
        assert_eq!(result, Some(shape_of("Array<number>")));
    }

    #[test]
    fn test_substitute_pair_swap() {
        let formals = vec![shape_of("Pair<A, B>")];
        let ret = shape_of("Pair<B, A>");
        let actual = shape_of("Pair<number, string>");
        let result =
            substitute_call_return_type(&formals, &ret, &[Some(&actual)], Language::TypeScript);
        assert_eq!(result, Some(shape_of("Pair<string, number>")));
    }

    #[test]
    fn test_substitute_non_generic_return_is_none() {
        let formals = vec![shape_of("T")];
        let ret = shape_of("string");
        let actual = shape_of("number");
        assert_eq!(
            substitute_call_return_type(&formals, &ret, &[Some(&actual)], Language::TypeScript),
            None
        );
    }

    #[test]
    fn test_substitute_unbound_is_none() {
        let formals = vec![shape_of("T")];
        let ret = shape_of("T");
        assert_eq!(
            substitute_call_return_type(&formals, &ret, &[None], Language::TypeScript),
            None
        );
    }

    #[test]
    fn test_shape_contains_param() {
        assert!(shape_contains_param(&shape_of("T")));
        assert!(shape_contains_param(&shape_of("Array<T>")));
        assert!(!shape_contains_param(&shape_of("Array<string>")));
        assert!(!shape_contains_param(&shape_of("string")));
    }
}
