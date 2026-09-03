//! Generic type parsing and parameter binding framework.
//!
//! Parses generic type strings like `List<T>` or `HashMap<String, Integer>`
//! into structured `GenericType` values and provides utilities for
//! substituting type parameters.

use std::collections::HashMap;

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
}
