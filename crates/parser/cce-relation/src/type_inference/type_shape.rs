//! Structured type representation utilities.
//!
//! Provides parsing and helpers for compound types including union, intersection,
//! array and generic forms. This module re-exports the core types from `types`
//! for convenience and implements additional helpers required by the
//! type inference pipeline.

use cce_types::language::Language;

pub use super::types::{
    TypeShape, parse_type_shape, shape_is_subtype, shape_members, type_shape_to_string,
};

// Additional helper required by spec 3.3.3: `parse_type_shape` already provided in `types.rs`.
// This file ensures the canonical import path `crate::type_inference::type_shape`
// exists for downstream modules.

/// Get all possible member names from a TypeShape (alias).
pub fn collect_shape_members(shape: &TypeShape) -> Vec<String> {
    shape_members(shape)
}

/// Check subtyping alias.
pub fn is_subtype(sub: &TypeShape, super_: &TypeShape) -> bool {
    shape_is_subtype(sub, super_)
}

/// Narrow a union shape by excluding a variant (pure function, does not mutate context).
pub fn narrow_union_shape(union_shape: &TypeShape, exclude: &TypeShape) -> Option<TypeShape> {
    match union_shape {
        TypeShape::Union(members) => {
            let filtered: Vec<TypeShape> =
                members.iter().filter(|m| *m != exclude).cloned().collect();
            match filtered.len() {
                0 => None,
                1 => filtered.into_iter().next(),
                _ => Some(TypeShape::Union(filtered)),
            }
        }
        _ => None,
    }
}

/// Parse helper used by control-flow narrowers to build union shapes from type strings.
pub fn parse_union_type(type_name: &str, language: Language) -> Option<TypeShape> {
    parse_type_shape(type_name, language)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::language::Language;

    #[test]
    fn test_parse_union() {
        let shape = parse_type_shape("string | number", Language::TypeScript).unwrap();
        assert!(matches!(shape, TypeShape::Union(_)));
        if let TypeShape::Union(members) = shape {
            assert_eq!(members.len(), 2);
        }
    }

    #[test]
    fn test_parse_generic() {
        let shape = parse_type_shape("Map<string, number>", Language::TypeScript).unwrap();
        assert!(matches!(shape, TypeShape::Generic { .. }));
    }

    #[test]
    fn test_parse_array() {
        let shape = parse_type_shape("string[]", Language::TypeScript).unwrap();
        assert!(matches!(shape, TypeShape::Array(_)));
    }

    #[test]
    fn test_parse_reference() {
        let shape = parse_type_shape("&str", Language::Rust).unwrap();
        assert!(matches!(shape, TypeShape::Reference { .. }));
    }

    #[test]
    fn test_shape_members_union() {
        let shape = parse_type_shape("A | B", Language::TypeScript).unwrap();
        let members = shape_members(&shape);
        assert_eq!(members.len(), 2);
        assert!(members.contains(&"A".to_string()));
        assert!(members.contains(&"B".to_string()));
    }

    #[test]
    fn test_narrow_union() {
        let shape = parse_type_shape("string | number", Language::TypeScript).unwrap();
        let exclude = TypeShape::Named("number".to_string());
        let narrowed = narrow_union_shape(&shape, &exclude).unwrap();
        assert_eq!(narrowed, TypeShape::Named("string".to_string()));
    }

    #[test]
    fn test_shape_is_subtype() {
        let sub = TypeShape::Named("string".to_string());
        let sup = TypeShape::Union(vec![
            TypeShape::Named("string".to_string()),
            TypeShape::Named("number".to_string()),
        ]);
        assert!(shape_is_subtype(&sub, &sup));
        assert!(!shape_is_subtype(&sup, &sub));
    }
}
