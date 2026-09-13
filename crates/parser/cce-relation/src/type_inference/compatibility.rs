//! Type compatibility checking for overload resolution.
//!
//! Provides language-specific coercion rules and structural assignability
//! checks used by [`super::OverloadSet`] to rank candidate functions.

use std::collections::HashMap;

use cce_types::language::Language;

use super::types::TypeShape;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompatibilityLevel {
    Exact,
    Coerce,
    Incompatible,
}

#[derive(Debug, Clone)]
pub struct TypeCompatibility {
    compatibility: HashMap<(String, String), CompatibilityLevel>,
}

impl TypeCompatibility {
    pub fn new_for_language(lang: Language) -> Self {
        let mut compatibility = HashMap::new();
        match lang {
            Language::Rust => {
                for (a, b) in [
                    ("i32", "i64"),
                    ("i32", "f64"),
                    ("i32", "f32"),
                    ("i64", "f64"),
                    ("u32", "u64"),
                    ("u32", "i64"),
                    ("f32", "f64"),
                    ("&str", "String"),
                    ("String", "&str"),
                    ("i32", "isize"),
                    ("u32", "usize"),
                ] {
                    compatibility
                        .insert((a.to_string(), b.to_string()), CompatibilityLevel::Coerce);
                    compatibility
                        .insert((b.to_string(), a.to_string()), CompatibilityLevel::Coerce);
                }
                compatibility.insert(
                    ("i32".to_string(), "isize".to_string()),
                    CompatibilityLevel::Coerce,
                );
            }
            Language::Python => {
                for (a, b) in [
                    ("int", "float"),
                    ("str", "Any"),
                    ("int", "Any"),
                    ("float", "Any"),
                    ("bool", "int"),
                    ("list", "Any"),
                    ("dict", "Any"),
                ] {
                    compatibility
                        .insert((a.to_string(), b.to_string()), CompatibilityLevel::Coerce);
                }
                compatibility.insert(
                    ("float".to_string(), "int".to_string()),
                    CompatibilityLevel::Coerce,
                );
            }
            Language::Java => {
                for (a, b) in [
                    ("int", "long"),
                    ("int", "double"),
                    ("int", "float"),
                    ("long", "double"),
                    ("float", "double"),
                    ("String", "Object"),
                    ("Integer", "int"),
                    ("Double", "double"),
                ] {
                    compatibility
                        .insert((a.to_string(), b.to_string()), CompatibilityLevel::Coerce);
                }
            }
            Language::TypeScript | Language::JavaScript | Language::Tsx | Language::Jsx => {
                for (a, b) in [("number", "int"), ("number", "float"), ("string", "String")] {
                    compatibility
                        .insert((a.to_string(), b.to_string()), CompatibilityLevel::Coerce);
                    compatibility
                        .insert((b.to_string(), a.to_string()), CompatibilityLevel::Coerce);
                }
            }
            Language::Go => {
                for (a, b) in [
                    ("int", "int64"),
                    ("int", "float64"),
                    ("int32", "int64"),
                    ("float32", "float64"),
                ] {
                    compatibility
                        .insert((a.to_string(), b.to_string()), CompatibilityLevel::Coerce);
                }
            }
            _ => {
                // generic numeric coercions
                for (a, b) in [("int", "float"), ("int", "double"), ("float", "double")] {
                    compatibility
                        .insert((a.to_string(), b.to_string()), CompatibilityLevel::Coerce);
                }
            }
        }
        Self { compatibility }
    }

    pub fn is_compatible(&self, expected: &str, actual: &str) -> CompatibilityLevel {
        if expected == actual {
            return CompatibilityLevel::Exact;
        }
        // case-insensitive exact
        if expected.eq_ignore_ascii_case(actual) {
            return CompatibilityLevel::Exact;
        }
        if let Some(level) = self
            .compatibility
            .get(&(expected.to_string(), actual.to_string()))
        {
            return level.clone();
        }
        if let Some(level) = self
            .compatibility
            .get(&(actual.to_string(), expected.to_string()))
        {
            if *level == CompatibilityLevel::Coerce {
                return CompatibilityLevel::Coerce;
            }
        }
        // numeric family heuristic
        let numeric = [
            "int", "i32", "i64", "u32", "u64", "float", "f32", "f64", "double", "long", "number",
        ];
        let exp_is_num = numeric.iter().any(|n| n.eq_ignore_ascii_case(expected));
        let act_is_num = numeric.iter().any(|n| n.eq_ignore_ascii_case(actual));
        if exp_is_num && act_is_num {
            return CompatibilityLevel::Coerce;
        }
        CompatibilityLevel::Incompatible
    }
}

impl Default for TypeCompatibility {
    fn default() -> Self {
        Self::new_for_language(Language::Unknown)
    }
}

/// Check if `actual` is assignable to `expected` (with generic resolution).
pub fn is_assignable(
    actual: &TypeShape,
    expected: &TypeShape,
    type_params: &HashMap<String, String>,
) -> bool {
    let shape_bindings: HashMap<String, TypeShape> = type_params
        .iter()
        .map(|(param, bound)| (param.clone(), TypeShape::Named(bound.clone())))
        .collect();
    is_assignable_with_shapes(actual, expected, &shape_bindings)
}

/// Check if `actual` is assignable to `expected` with structured bindings.
///
/// This is the single structural implementation behind generic-aware
/// assignability: type parameters resolve against parsed shapes, unbound
/// parameters stay unassignable, and union handling recurses structurally.
pub fn is_assignable_with_shapes(
    actual: &TypeShape,
    expected: &TypeShape,
    type_params: &HashMap<String, TypeShape>,
) -> bool {
    if actual == expected {
        return true;
    }
    // Handle TypeShape::Param: check against shape bindings
    if let TypeShape::Param(param_name) = expected {
        if let Some(bound) = type_params.get(param_name) {
            return actual == bound;
        }
    }
    // Handle generic params in Named form (legacy): if expected is a type param name
    if let TypeShape::Named(exp_name) = expected {
        if let Some(bound) = type_params.get(exp_name) {
            return actual == bound;
        }
    }
    // Handle TypeShape::Param on actual side: a param matches if it's the same param
    if let (TypeShape::Param(a), TypeShape::Param(e)) = (actual, expected) {
        return a == e;
    }
    // Union handling
    match (actual, expected) {
        (TypeShape::Named(a), TypeShape::Union(members)) => members.iter().any(|m| {
            if let TypeShape::Named(n) = m {
                n == a
            } else {
                false
            }
        }),
        (TypeShape::Union(actual_members), TypeShape::Named(_)) => actual_members
            .iter()
            .any(|m| is_assignable_with_shapes(m, expected, type_params)),
        (TypeShape::Param(p), TypeShape::Union(members)) => {
            if let Some(bound) = type_params.get(p) {
                members.iter().any(|m| m == bound)
            } else {
                false
            }
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_assignable_exact() {
        let a = TypeShape::Named("i32".to_string());
        let b = TypeShape::Named("i32".to_string());
        assert!(is_assignable(&a, &b, &HashMap::new()));
    }

    #[test]
    fn test_is_assignable_union() {
        let actual = TypeShape::Named("i32".to_string());
        let expected = TypeShape::Union(vec![
            TypeShape::Named("i32".to_string()),
            TypeShape::Named("String".to_string()),
        ]);
        assert!(is_assignable(&actual, &expected, &HashMap::new()));
    }

    #[test]
    fn test_is_assignable_unbound_generic_param() {
        let actual = TypeShape::Named("i32".to_string());
        let expected = TypeShape::Param("T".to_string());
        assert!(!is_assignable(&actual, &expected, &HashMap::new()));
    }

    #[test]
    fn test_is_assignable_bound_generic_param() {
        let actual = TypeShape::Named("i32".to_string());
        let expected = TypeShape::Param("T".to_string());
        let mut type_params = HashMap::new();
        type_params.insert("T".to_string(), "i32".to_string());
        assert!(is_assignable(&actual, &expected, &type_params));
    }

    #[test]
    fn test_is_assignable_param_variant() {
        let actual = TypeShape::Named("i32".to_string());
        let expected = TypeShape::Param("T".to_string());
        let mut type_params = HashMap::new();
        type_params.insert("T".to_string(), TypeShape::Named("i32".to_string()));
        assert!(is_assignable_with_shapes(&actual, &expected, &type_params));
    }

    #[test]
    fn test_is_assignable_unbound_param_variant() {
        let actual = TypeShape::Named("i32".to_string());
        let expected = TypeShape::Param("T".to_string());
        assert!(!is_assignable_with_shapes(
            &actual,
            &expected,
            &HashMap::new()
        ));
    }

    #[test]
    fn test_is_assignable_param_to_param() {
        let a = TypeShape::Param("T".to_string());
        let b = TypeShape::Param("T".to_string());
        let c = TypeShape::Param("U".to_string());
        assert!(is_assignable(&a, &b, &HashMap::new()));
        assert!(!is_assignable(&a, &c, &HashMap::new()));
    }

    #[test]
    fn test_is_assignable_with_shapes_structural_bound() {
        let actual = TypeShape::Named("i32".to_string());
        let expected = TypeShape::Param("T".to_string());
        let mut bindings = HashMap::new();
        bindings.insert("T".to_string(), TypeShape::Named("i32".to_string()));
        assert!(is_assignable_with_shapes(&actual, &expected, &bindings));
        assert!(!is_assignable_with_shapes(
            &actual,
            &expected,
            &HashMap::new()
        ));
    }
}
