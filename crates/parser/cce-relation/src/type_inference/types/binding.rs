//! Variable and parameter type bindings.

use cce_types::entity::EntityId;

use super::origin::{InferenceOrigin, origin_priority};
use super::shape::TypeShape;

/// A single type binding: a variable or parameter name maps to a type.
#[derive(Debug, Clone, Default)]
pub struct TypeBinding {
    /// The type's qualified name (e.g., "path::to::Struct", "builtins.int")
    pub type_name: String,
    /// Entity ID of the type definition (if resolved within the project)
    pub type_entity_id: Option<EntityId>,
    /// Source span where this binding was inferred
    pub span: cce_types::Span,
    /// Origin tag for debugging and ranking; determines priority when merging.
    pub origin: Option<InferenceOrigin>,
    /// Parsed structured type (None for simple types)
    pub shape: Option<TypeShape>,
}

/// Confidence level for variable type bindings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TypeConfidence {
    High,
    Medium,
    Low,
}

/// Variable type binding with support for conditional assignments.
#[derive(Debug, Clone)]
pub struct VariableTypeBinding {
    /// Primary type binding
    pub primary: TypeBinding,
    /// Alternative types from conditional assignments
    pub alternatives: Vec<TypeBinding>,
    /// Confidence level of this binding
    pub confidence: TypeConfidence,
}

impl VariableTypeBinding {
    /// Create a new variable type binding
    pub fn new(primary: TypeBinding) -> Self {
        Self {
            primary,
            alternatives: Vec::new(),
            confidence: TypeConfidence::Medium,
        }
    }

    /// Add an alternative type binding
    pub fn add_alternative(&mut self, alternative: TypeBinding) {
        self.alternatives.push(alternative);
    }

    /// Get all possible types (primary + alternatives)
    pub fn all_types(&self) -> Vec<&TypeBinding> {
        let mut types = vec![&self.primary];
        types.extend(self.alternatives.iter());
        types
    }

    /// Merge two variable type bindings
    pub fn merge(self, other: VariableTypeBinding) -> Self {
        let mut result = self;
        result
            .alternatives
            .extend(other.all_types().into_iter().cloned());
        result
    }

    /// Get the most specific type (highest priority origin)
    pub fn most_specific(&self) -> &TypeBinding {
        self.all_types()
            .iter()
            .max_by_key(|b| origin_priority(b.origin))
            .copied()
            .unwrap_or(&self.primary)
    }
}

/// Pattern for pattern matching
#[derive(Debug, Clone)]
pub enum Pattern {
    /// Identifier pattern: `let x = ...`
    Identifier(String),
    /// Tuple pattern: `let (a, b) = ...`
    Tuple(Vec<String>),
    /// Struct pattern: `let Point { x, y } = ...`
    Struct(Vec<String>),
    /// Wildcard pattern: `let _ = ...`
    Wildcard,
}

/// One element of a possibly nested destructuring pattern.
///
/// Flat comma-joined entity names lose grouping, so the engine recovers
/// nesting from the statement source when a group is present. Parts that
/// cannot be classified keep the whole statement on the flat path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NestedPatternPart {
    /// Plain bound name: `a`
    Name(String),
    /// Parenthesized group: `(b, c)` maps into the positional element shape.
    Group(Vec<NestedPatternPart>),
    /// Placeholder: `_` binds nothing.
    Wildcard,
}

#[cfg(test)]
mod tests {

    use super::*;

    // ==================== VariableTypeBinding tests ====================

    #[test]
    fn test_variable_type_binding_new() {
        let primary = TypeBinding {
            type_name: "String".to_string(),
            ..Default::default()
        };
        let binding = VariableTypeBinding::new(primary.clone());
        assert_eq!(binding.primary.type_name, "String");
        assert!(binding.alternatives.is_empty());
        assert_eq!(binding.confidence, TypeConfidence::Medium);
    }

    #[test]
    fn test_variable_type_binding_add_alternative() {
        let primary = TypeBinding {
            type_name: "String".to_string(),
            ..Default::default()
        };
        let mut binding = VariableTypeBinding::new(primary);
        let alt = TypeBinding {
            type_name: "int".to_string(),
            ..Default::default()
        };
        binding.add_alternative(alt);
        assert_eq!(binding.alternatives.len(), 1);
        assert_eq!(binding.alternatives[0].type_name, "int");
    }

    #[test]
    fn test_variable_type_binding_all_types() {
        let primary = TypeBinding {
            type_name: "String".to_string(),
            ..Default::default()
        };
        let mut binding = VariableTypeBinding::new(primary);
        let alt = TypeBinding {
            type_name: "int".to_string(),
            ..Default::default()
        };
        binding.add_alternative(alt);
        let all = binding.all_types();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].type_name, "String");
        assert_eq!(all[1].type_name, "int");
    }

    #[test]
    fn test_variable_type_binding_merge() {
        let primary1 = TypeBinding {
            type_name: "String".to_string(),
            origin: Some(InferenceOrigin::TypeAnnotation),
            ..Default::default()
        };
        let binding1 = VariableTypeBinding::new(primary1);
        let primary2 = TypeBinding {
            type_name: "int".to_string(),
            origin: Some(InferenceOrigin::ConstructorCall),
            ..Default::default()
        };
        let binding2 = VariableTypeBinding::new(primary2);
        let merged = binding1.merge(binding2);
        assert_eq!(merged.primary.type_name, "String");
        assert_eq!(merged.alternatives.len(), 1);
        assert_eq!(merged.alternatives[0].type_name, "int");
    }

    #[test]
    fn test_variable_type_binding_most_specific() {
        let primary = TypeBinding {
            type_name: "String".to_string(),
            origin: Some(InferenceOrigin::LiteralType),
            ..Default::default()
        };
        let mut binding = VariableTypeBinding::new(primary);
        let alt = TypeBinding {
            type_name: "int".to_string(),
            origin: Some(InferenceOrigin::TypeAnnotation),
            ..Default::default()
        };
        binding.add_alternative(alt);
        let most = binding.most_specific();
        assert_eq!(most.type_name, "int");
        assert_eq!(most.origin, Some(InferenceOrigin::TypeAnnotation));
    }
}
