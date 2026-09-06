//! Scoped type context for per-file inference.

use std::collections::HashMap;

use cce_types::entity::EntityId;
use cce_types::language::Language;

use super::binding::{NestedPatternPart, Pattern, TypeBinding};
use super::narrowing::BranchPolarity;
use super::origin::{InferenceOrigin, binding_supersedes, origin_priority};
use super::shape::{TypeShape, type_shape_to_string};
use crate::type_inference::generics;

/// A single scope frame containing variable bindings.
#[derive(Debug, Clone, Default)]
pub struct ScopeFrame {
    /// Variable bindings in this scope
    pub bindings: HashMap<String, TypeBinding>,
    /// Narrowed type bindings (for control flow analysis)
    pub narrowed: HashMap<String, Vec<TypeBinding>>,
    /// Else-branch narrowed bindings, shadowing `narrowed` only when the
    /// consumer explicitly queries the else side.
    pub narrowed_else: HashMap<String, Vec<TypeBinding>>,
}

/// Scoped type context for per-file type inference.
///
/// Built during symbol table construction and queried by the relation resolver
/// when disambiguating method calls on dynamically-typed receivers.
///
/// Supports nested scopes with proper shadowing semantics:
/// - Variables are looked up from innermost to outermost scope
/// - New bindings in inner scopes shadow outer bindings
/// - Popping a scope restores visibility of outer bindings
#[derive(Debug, Clone)]
pub struct ScopedTypeContext {
    /// Scope frames stack. `frames[0]` is the outermost scope.
    frames: Vec<ScopeFrame>,
    /// Return types (not affected by scope nesting)
    return_types: HashMap<EntityId, TypeBinding>,
    /// Name-based index for return types: function name -> return type binding.
    /// Populated alongside `return_types` to enable `call_target` resolution
    /// for variables assigned via `x = f()`.
    return_types_by_name: HashMap<String, TypeBinding>,
    /// Parameter types (not affected by scope nesting)
    parameter_types: HashMap<EntityId, Vec<TypeBinding>>,
    /// Language this context was built for
    language: Language,
    /// Type parameter bindings (e.g., "T" -> "String") per scope
    type_param_bindings: Vec<HashMap<String, String>>,
    /// Type parameter ownership: outer_type -> { param -> concrete }.
    /// Tracks which generic class a type parameter belongs to, preventing
    /// cross-contamination between e.g. Vec<T> and HashMap<T,V>.
    generic_param_ownership: HashMap<String, HashMap<String, String>>,
}

impl Default for ScopedTypeContext {
    fn default() -> Self {
        Self {
            frames: vec![ScopeFrame::default()],
            return_types: HashMap::new(),
            return_types_by_name: HashMap::new(),
            parameter_types: HashMap::new(),
            language: Language::Unknown,
            type_param_bindings: vec![HashMap::new()],
            generic_param_ownership: HashMap::new(),
        }
    }
}

impl ScopedTypeContext {
    /// Create a new empty type inference context for a language.
    pub fn new(language: Language) -> Self {
        Self {
            language,
            ..Default::default()
        }
    }

    /// Push a new scope frame onto the stack.
    pub fn push_scope(&mut self) {
        self.frames.push(ScopeFrame::default());
        self.type_param_bindings.push(HashMap::new());
    }

    /// Pop the innermost scope frame from the stack.
    ///
    /// # Panics
    ///
    /// Panics if there is only one scope frame (the root scope cannot be popped).
    pub fn pop_scope(&mut self) {
        assert!(self.frames.len() > 1, "Cannot pop the root scope");
        self.frames.pop();
        self.type_param_bindings.pop();
    }

    /// Get the current scope depth (number of frames).
    pub fn scope_depth(&self) -> usize {
        self.frames.len()
    }

    /// Bind a type parameter in the current scope.
    pub fn bind_type_param(&mut self, param: String, concrete: String) {
        if let Some(frame) = self.type_param_bindings.last_mut() {
            frame.insert(param, concrete);
        }
    }

    /// Look up a type parameter binding (searches scope stack).
    pub fn get_type_param(&self, param: &str) -> Option<&str> {
        for frame in self.type_param_bindings.iter().rev() {
            if let Some(v) = frame.get(param) {
                return Some(v.as_str());
            }
        }
        None
    }

    /// Bind a type parameter with ownership tracking.
    ///
    /// Records that `param` belongs to `outer_type` and maps to `concrete`.
    /// This prevents cross-contamination between different generic classes.
    pub fn bind_type_param_owned(&mut self, outer_type: &str, param: String, concrete: String) {
        self.generic_param_ownership
            .entry(outer_type.to_string())
            .or_default()
            .insert(param.clone(), concrete.clone());
        // Also bind in the scope stack for backward compatibility
        self.bind_type_param(param, concrete);
    }

    /// Look up a type parameter binding scoped to a specific outer type.
    ///
    /// Returns the concrete type if `param` is bound within `outer_type`.
    /// Falls back to the global `get_type_param` if no ownership record exists.
    pub fn get_type_param_for_owner(&self, outer_type: &str, param: &str) -> Option<&str> {
        if let Some(owners) = self.generic_param_ownership.get(outer_type) {
            if let Some(v) = owners.get(param) {
                return Some(v.as_str());
            }
        }
        self.get_type_param(param)
    }

    /// Resolve a potentially-generic type name by substituting bound parameters.
    ///
    /// Handles nested generics recursively, e.g. `Vec<HashMap<T, V>>` with
    /// `T=String, V=i32` → `Vec<HashMap<String, i32>>`.
    pub fn resolve_type(&self, type_name: &str) -> String {
        // Direct type parameter substitution
        if let Some(concrete) = self.get_type_param(type_name) {
            return concrete.to_string();
        }
        // Try parsing as a generic type for recursive resolution
        if let Some(gt) = generics::parse_generic_type(type_name) {
            let resolved = self.resolve_generic_type(&gt);
            return generics::format_generic_type(&resolved);
        }
        type_name.to_string()
    }

    /// Recursively resolve a parsed generic type using scope bindings.
    fn resolve_generic_type(&self, gt: &generics::GenericType) -> generics::GenericType {
        let resolved_args = gt
            .args
            .iter()
            .map(|arg| match arg {
                generics::GenericTypeArg::Param(p) => {
                    if let Some(concrete) = self.get_type_param(p) {
                        // The resolved concrete might itself be generic, resolve recursively
                        if let Some(nested_gt) = generics::parse_generic_type(concrete) {
                            generics::GenericTypeArg::Nested(self.resolve_generic_type(&nested_gt))
                        } else {
                            generics::GenericTypeArg::Concrete(concrete.to_string())
                        }
                    } else {
                        generics::GenericTypeArg::Param(p.clone())
                    }
                }
                generics::GenericTypeArg::Nested(nested) => {
                    generics::GenericTypeArg::Nested(self.resolve_generic_type(nested))
                }
                other => other.clone(),
            })
            .collect();
        generics::GenericType {
            base: gt.base.clone(),
            args: resolved_args,
        }
    }

    /// Look up variable type, but only within the current scope (not parent scopes).
    pub fn get_variable_type_current_scope(&self, name: &str) -> Option<&TypeBinding> {
        self.frames
            .last()
            .and_then(|frame| frame.bindings.get(name))
    }

    /// Get the number of bindings in the current scope (for diagnostics).
    pub fn current_scope_binding_count(&self) -> usize {
        self.frames.last().map(|f| f.bindings.len()).unwrap_or(0)
    }

    /// Narrow a union type by excluding a variant.
    pub fn narrow_union(&mut self, name: &str, exclude: &TypeShape) {
        // Find the binding for `name` and if its shape is Union, remove the excluded variant
        for frame in self.frames.iter_mut().rev() {
            if let Some(binding) = frame.bindings.get_mut(name) {
                if let Some(TypeShape::Union(members)) = binding.shape.clone() {
                    let filtered: Vec<TypeShape> =
                        members.into_iter().filter(|m| m != exclude).collect();
                    let new_shape = match filtered.len() {
                        0 => None,
                        1 => Some(filtered.into_iter().next().expect("one element")),
                        _ => Some(TypeShape::Union(filtered)),
                    };
                    if let Some(ns) = new_shape.clone() {
                        binding.type_name = type_shape_to_string(&ns);
                        binding.shape = Some(ns);
                    } else {
                        // No members left; keep original
                    }
                    return;
                }
                // Fallback: string-based narrowing for simple cases
                if let TypeShape::Named(exclude_name) = exclude {
                    let type_name_clone = binding.type_name.clone();
                    let parts: Vec<&str> = type_name_clone.split(" | ").collect();
                    if parts.len() > 1 {
                        let filtered: Vec<String> = parts
                            .into_iter()
                            .filter(|p| p.trim() != exclude_name)
                            .map(|s| s.to_string())
                            .collect();
                        if filtered.len() == 1 {
                            binding.type_name = filtered[0].clone();
                            binding.shape = Some(TypeShape::Named(filtered[0].clone()));
                        } else if filtered.len() > 1 {
                            let joined = filtered.join(" | ");
                            binding.type_name = joined;
                            // Re-parse shape if possible
                            binding.shape = Some(TypeShape::Union(
                                filtered.into_iter().map(TypeShape::Named).collect(),
                            ));
                        }
                    }
                }
                return;
            }
        }
    }

    /// Record a return type binding for a function entity.
    pub fn add_return_type(&mut self, entity_id: EntityId, binding: TypeBinding) {
        self.return_types.insert(entity_id, binding);
    }

    /// Record a return type binding indexed by function name.
    ///
    /// Used alongside `add_return_type` to enable `call_target` resolution
    /// for variables assigned via `x = f()` where `f` is in the same file.
    pub fn add_return_type_by_name(&mut self, name: String, binding: TypeBinding) {
        self.return_types_by_name.insert(name, binding);
    }

    /// Look up a function's return type by name.
    ///
    /// Returns the highest-priority binding for the given function name.
    /// Used for local `call_target` resolution in variable type inference.
    pub fn get_return_type_by_name(&self, name: &str) -> Option<&TypeBinding> {
        self.return_types_by_name.get(name)
    }

    /// Record parameter type bindings for a function entity.
    pub fn add_parameter_types(&mut self, entity_id: EntityId, bindings: Vec<TypeBinding>) {
        self.parameter_types.insert(entity_id, bindings);
    }

    /// Record a variable type binding in the current (innermost) scope.
    pub fn add_variable_type(&mut self, name: String, binding: TypeBinding) {
        self.frames
            .last_mut()
            .expect("Scope stack is never empty")
            .bindings
            .insert(name, binding);
    }

    /// Record a narrowed type binding for a variable in the current scope.
    ///
    /// Narrowed bindings are produced by control-flow analysis (e.g.,
    /// `isinstance(x, str)` narrows `x` to `str` inside the true branch).
    /// Multiple narrowings for the same variable accumulate in a list.
    pub fn add_narrowed_type(&mut self, name: String, binding: TypeBinding) {
        self.add_narrowed_type_in_branch(name, binding, BranchPolarity::Then);
    }

    /// Record a narrowed binding, falling back to the enclosing entity span
    /// when the narrowing carries no position of its own.
    ///
    /// Switch/match arms and catch clauses synthesize bindings without
    /// source positions; without a fallback the export renders `n/a`.
    pub fn add_narrowed_type_anchored(
        &mut self,
        name: String,
        mut binding: TypeBinding,
        fallback: cce_types::Span,
    ) {
        if !binding.span.is_available() {
            binding.span = fallback;
        }
        self.add_narrowed_type(name, binding);
    }

    /// Record a narrowed type binding attributed to one branch side.
    ///
    /// Then-branch bindings feed the default lookup; else-branch bindings
    /// are only visible through the polarity-aware accessor so the two
    /// sides never contaminate each other.
    pub fn add_narrowed_type_in_branch(
        &mut self,
        name: String,
        binding: TypeBinding,
        polarity: BranchPolarity,
    ) {
        let frame = self.frames.last_mut().expect("Scope stack is never empty");
        match polarity {
            BranchPolarity::Then => frame.narrowed.entry(name).or_default().push(binding),
            BranchPolarity::Else => frame.narrowed_else.entry(name).or_default().push(binding),
        }
    }

    /// Look up narrowed bindings for a variable on one branch side.
    ///
    /// Searches from the innermost scope outward and returns the
    /// highest-priority binding on the requested side, if any.
    pub fn get_narrowed_in_branch(
        &self,
        name: &str,
        polarity: BranchPolarity,
    ) -> Option<&TypeBinding> {
        for frame in self.frames.iter().rev() {
            let map = match polarity {
                BranchPolarity::Then => &frame.narrowed,
                BranchPolarity::Else => &frame.narrowed_else,
            };
            if let Some(list) = map.get(name) {
                if let Some(binding) = list.iter().max_by_key(|b| origin_priority(b.origin)) {
                    return Some(binding);
                }
            }
        }
        None
    }

    /// Look up the inferred type for a variable on one branch side.
    ///
    /// The requested side wins when it carries a binding; otherwise the
    /// lookup falls back to the default (then-biased) binding. Returns
    /// `None` only when the variable has no binding at all, so callers
    /// never need to second-guess a missing else side.
    pub fn resolve_branch_aware(
        &self,
        name: &str,
        polarity: BranchPolarity,
    ) -> Option<&TypeBinding> {
        match polarity {
            BranchPolarity::Else => self
                .get_narrowed_in_branch(name, BranchPolarity::Else)
                .or_else(|| self.get_variable_type(name)),
            BranchPolarity::Then => self.get_variable_type(name),
        }
    }

    /// Look up the inferred return type for a function entity.
    pub fn get_return_type(&self, entity_id: EntityId) -> Option<&TypeBinding> {
        self.return_types.get(&entity_id)
    }

    /// Look up the inferred parameter types for a function entity.
    pub fn get_parameter_types(&self, entity_id: EntityId) -> Option<&[TypeBinding]> {
        self.parameter_types.get(&entity_id).map(|v| v.as_slice())
    }

    /// Look up the inferred type for a variable name.
    ///
    /// Searches from the innermost scope outward. For each frame:
    /// 1. If narrowed bindings exist for the name, return the highest-priority
    ///    narrowed binding (by `origin_priority`).
    /// 2. Otherwise return the original binding.
    pub fn get_variable_type(&self, name: &str) -> Option<&TypeBinding> {
        for frame in self.frames.iter().rev() {
            let has_binding = frame.bindings.contains_key(name);
            let has_narrowed = frame.narrowed.contains_key(name);

            if has_narrowed {
                if let Some(narrowed_list) = frame.narrowed.get(name) {
                    if let Some(narrowed) = narrowed_list
                        .iter()
                        .max_by_key(|b| origin_priority(b.origin))
                    {
                        return Some(narrowed);
                    }
                }
            }

            if has_binding {
                return frame.bindings.get(name);
            }
        }
        None
    }

    /// Get the language this context was built for.
    pub fn language(&self) -> Language {
        self.language
    }

    /// Iterate over scope frames (outermost to innermost).
    pub fn frames_iter(&self) -> impl Iterator<Item = &ScopeFrame> {
        self.frames.iter()
    }

    /// Check if this context has any inferred types.
    pub fn is_empty(&self) -> bool {
        self.return_types.is_empty()
            && self.parameter_types.is_empty()
            && self.frames.iter().all(|f| {
                f.bindings.is_empty() && f.narrowed.is_empty() && f.narrowed_else.is_empty()
            })
    }

    /// Iterate over return type bindings.
    pub fn return_types_iter(&self) -> impl Iterator<Item = (&EntityId, &TypeBinding)> {
        self.return_types.iter()
    }

    /// Iterate over parameter type bindings.
    pub fn parameter_types_iter(&self) -> impl Iterator<Item = (&EntityId, &Vec<TypeBinding>)> {
        self.parameter_types.iter()
    }

    /// Iterate over variable type bindings in the outermost scope.
    pub fn variable_types_iter(&self) -> impl Iterator<Item = (&String, &TypeBinding)> {
        self.frames
            .first()
            .into_iter()
            .flat_map(|f| f.bindings.iter())
    }

    /// Get all return type bindings as a vector.
    pub fn all_return_types(&self) -> Vec<(EntityId, TypeBinding)> {
        self.return_types
            .iter()
            .map(|(k, v)| (*k, v.clone()))
            .collect()
    }

    /// Merge another context into this one (for incremental updates).
    ///
    /// Only merges the top-level frame from `other` into the top-level frame
    /// of `self`. A binding is only overridden when the new binding
    /// supersedes the existing one, so equal-priority updates replace while
    /// lower-priority updates never cause inversions.
    ///
    /// `return_types` and `parameter_types` are merged directly (later values
    /// override earlier ones, as they are file-level and not scope-sensitive).
    pub fn merge_from(&mut self, other: &ScopedTypeContext) {
        if let Some(other_frame) = other.frames.last() {
            let self_frame = self.frames.last_mut().expect("Scope stack is never empty");
            for (name, binding) in &other_frame.bindings {
                self_frame
                    .bindings
                    .entry(name.clone())
                    .and_modify(|existing| {
                        if binding_supersedes(binding.origin, existing.origin) {
                            *existing = binding.clone();
                        }
                    })
                    .or_insert_with(|| binding.clone());
            }
            // Branch-attributed narrowings merge per side so polarity is
            // preserved across incremental updates; identical entries are
            // skipped to keep repeated merges bounded.
            for (name, bindings) in &other_frame.narrowed {
                let entry = self_frame.narrowed.entry(name.clone()).or_default();
                for binding in bindings {
                    if !entry.iter().any(|existing| {
                        existing.type_name == binding.type_name && existing.origin == binding.origin
                    }) {
                        entry.push(binding.clone());
                    }
                }
            }
            for (name, bindings) in &other_frame.narrowed_else {
                let entry = self_frame.narrowed_else.entry(name.clone()).or_default();
                for binding in bindings {
                    if !entry.iter().any(|existing| {
                        existing.type_name == binding.type_name && existing.origin == binding.origin
                    }) {
                        entry.push(binding.clone());
                    }
                }
            }
        }
        // return_types, return_types_by_name, and parameter_types are merged directly
        for (k, v) in &other.return_types {
            self.return_types.insert(*k, v.clone());
        }
        for (k, v) in &other.return_types_by_name {
            self.return_types_by_name
                .entry(k.clone())
                .and_modify(|existing| {
                    if binding_supersedes(v.origin, existing.origin) {
                        *existing = v.clone();
                    }
                })
                .or_insert_with(|| v.clone());
        }
        for (k, v) in &other.parameter_types {
            self.parameter_types.insert(*k, v.clone());
        }
    }

    /// Add pattern match binding
    /// Handles: `let (a, b) = tuple()` or `let {x, y} = point`
    pub fn add_pattern_match_binding(&mut self, pattern: &Pattern, source_type: &TypeShape) {
        match pattern {
            Pattern::Tuple(elements) => {
                if let TypeShape::Generic { args, .. } = source_type {
                    for (i, element) in elements.iter().enumerate() {
                        if let Some(arg_type) = args.get(i) {
                            self.add_variable_type(
                                element.clone(),
                                TypeBinding {
                                    type_name: type_shape_to_string(arg_type),
                                    type_entity_id: None,
                                    span: cce_types::Span::default(),
                                    origin: Some(InferenceOrigin::PatternMatching),
                                    shape: Some(arg_type.clone()),
                                },
                            );
                        }
                    }
                }
            }
            Pattern::Struct(fields) => {
                for field in fields {
                    self.add_variable_type(
                        field.clone(),
                        TypeBinding {
                            type_name: "unknown".to_string(),
                            type_entity_id: None,
                            span: cce_types::Span::default(),
                            origin: Some(InferenceOrigin::PatternMatching),
                            shape: None,
                        },
                    );
                }
            }
            Pattern::Identifier(name) => {
                self.add_variable_type(
                    name.clone(),
                    TypeBinding {
                        type_name: type_shape_to_string(source_type),
                        type_entity_id: None,
                        span: cce_types::Span::default(),
                        origin: Some(InferenceOrigin::PatternMatching),
                        shape: Some(source_type.clone()),
                    },
                );
            }
            Pattern::Wildcard => {}
        }
    }

    /// Add destructuring assignment binding
    /// Handles: `a, b = tuple()` or `a = list[0]`
    ///
    /// Positional mapping applies to any generic container with enough type
    /// arguments, not just tuple-named shapes. Positions without a matching
    /// argument bind to `unknown` so unrelated parts never inherit a guess.
    /// Reference wrappers (`&T`, `&mut T`) unwrap to their inner shape;
    /// named struct types without positional information still bind
    /// `unknown` since member resolution needs declaration context the
    /// context does not carry.
    pub fn add_destructuring_binding(
        &mut self,
        target: &str,
        source_type: &TypeShape,
        index: Option<usize>,
    ) {
        let source_type = match source_type {
            TypeShape::Reference { inner, .. } => inner.as_ref(),
            other => other,
        };
        let resolved_type = match source_type {
            TypeShape::Generic { args, .. } => index
                .and_then(|i| args.get(i).cloned())
                .unwrap_or_else(|| TypeShape::Named("unknown".to_string())),
            TypeShape::Array(element_type) => (**element_type).clone(),
            _ => TypeShape::Named("unknown".to_string()),
        };

        self.add_variable_type(
            target.to_string(),
            TypeBinding {
                type_name: type_shape_to_string(&resolved_type),
                type_entity_id: None,
                span: cce_types::Span::default(),
                origin: Some(InferenceOrigin::DestructuringAssignment),
                shape: Some(resolved_type),
            },
        );
    }

    /// Bind a possibly nested destructuring pattern by shape position.
    ///
    /// Plain names reuse the flat positional binding; groups recurse into
    /// the positional element shape; placeholders bind nothing. A group
    /// without a mappable element shape is skipped without affecting
    /// already-bound siblings, keeping partial failures conservative.
    pub fn add_nested_destructuring_binding(
        &mut self,
        parts: &[NestedPatternPart],
        source_type: &TypeShape,
    ) {
        for (index, part) in parts.iter().enumerate() {
            match part {
                NestedPatternPart::Name(name) => {
                    if let Some(element) = nested_positional_element(source_type, index) {
                        self.add_variable_type(
                            name.clone(),
                            TypeBinding {
                                type_name: type_shape_to_string(&element),
                                type_entity_id: None,
                                span: cce_types::Span::default(),
                                origin: Some(InferenceOrigin::PatternMatching),
                                shape: Some(element),
                            },
                        );
                    }
                    self.add_destructuring_binding(name, source_type, Some(index));
                }
                NestedPatternPart::Group(inner) => {
                    if let Some(element) = nested_positional_element(source_type, index) {
                        self.add_nested_destructuring_binding(inner, &element);
                    }
                }
                NestedPatternPart::Wildcard => {}
            }
        }
    }
}

/// Positional element of a container shape for destructuring.
///
/// Generic arguments map by index; arrays yield their element type for any
/// position, mirroring the flat binding; anything else has no positional
/// element so the caller stays conservative.
fn nested_positional_element(source_type: &TypeShape, index: usize) -> Option<TypeShape> {
    match source_type {
        TypeShape::Generic { args, .. } => args.get(index).cloned(),
        TypeShape::Array(element) => Some((**element).clone()),
        TypeShape::Reference { inner, .. } => nested_positional_element(inner, index),
        _ => None,
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    use cce_types::language::Language;

    // ==================== ScopedTypeContext tests ====================

    #[test]
    fn test_scoped_type_context_new() {
        let ctx = ScopedTypeContext::new(Language::Python);
        assert_eq!(ctx.language(), Language::Python);
        assert!(ctx.is_empty());
        assert_eq!(ctx.scope_depth(), 1);
    }

    #[test]
    fn test_scoped_type_context_default() {
        let ctx = ScopedTypeContext::default();
        assert_eq!(ctx.language(), Language::Unknown);
        assert!(ctx.is_empty());
    }

    #[test]
    fn test_scoped_type_context_push_pop_scope() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        assert_eq!(ctx.scope_depth(), 1);
        ctx.push_scope();
        assert_eq!(ctx.scope_depth(), 2);
        ctx.pop_scope();
        assert_eq!(ctx.scope_depth(), 1);
    }

    #[test]
    #[should_panic(expected = "Cannot pop the root scope")]
    fn test_scoped_type_context_pop_root_scope_panics() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        ctx.pop_scope();
    }

    #[test]
    fn test_scoped_type_context_shadowing() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        ctx.add_variable_type(
            "x".to_string(),
            TypeBinding {
                type_name: "int".to_string(),
                ..Default::default()
            },
        );
        assert_eq!(ctx.get_variable_type("x").unwrap().type_name, "int");
        ctx.push_scope();
        ctx.add_variable_type(
            "x".to_string(),
            TypeBinding {
                type_name: "String".to_string(),
                ..Default::default()
            },
        );
        assert_eq!(ctx.get_variable_type("x").unwrap().type_name, "String");
        ctx.pop_scope();
        assert_eq!(ctx.get_variable_type("x").unwrap().type_name, "int");
    }

    #[test]
    fn test_scoped_type_context_get_variable_type_current_scope() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        ctx.add_variable_type(
            "x".to_string(),
            TypeBinding {
                type_name: "int".to_string(),
                ..Default::default()
            },
        );
        assert!(ctx.get_variable_type_current_scope("x").is_some());
        ctx.push_scope();
        ctx.add_variable_type(
            "x".to_string(),
            TypeBinding {
                type_name: "String".to_string(),
                ..Default::default()
            },
        );
        assert_eq!(
            ctx.get_variable_type_current_scope("x").unwrap().type_name,
            "String"
        );
        ctx.pop_scope();
        assert_eq!(
            ctx.get_variable_type_current_scope("x").unwrap().type_name,
            "int"
        );
    }

    #[test]
    fn test_scoped_type_context_return_type() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        let entity_id = cce_types::EntityId(1);
        ctx.add_return_type(
            entity_id,
            TypeBinding {
                type_name: "String".to_string(),
                ..Default::default()
            },
        );
        assert_eq!(ctx.get_return_type(entity_id).unwrap().type_name, "String");
        assert!(ctx.get_return_type(cce_types::EntityId(999)).is_none());
    }

    #[test]
    fn test_scoped_type_context_parameter_types() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        let entity_id = cce_types::EntityId(1);
        ctx.add_parameter_types(
            entity_id,
            vec![
                TypeBinding {
                    type_name: "int".to_string(),
                    ..Default::default()
                },
                TypeBinding {
                    type_name: "String".to_string(),
                    ..Default::default()
                },
            ],
        );
        let params = ctx.get_parameter_types(entity_id).unwrap();
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].type_name, "int");
        assert_eq!(params[1].type_name, "String");
        assert!(ctx.get_parameter_types(cce_types::EntityId(999)).is_none());
    }

    #[test]
    fn test_scoped_type_context_bind_type_param() {
        let mut ctx = ScopedTypeContext::new(Language::Rust);
        ctx.bind_type_param("T".to_string(), "String".to_string());
        assert_eq!(ctx.get_type_param("T"), Some("String"));
        assert_eq!(ctx.get_type_param("U"), None);
    }

    #[test]
    fn test_scoped_type_context_type_param_scope() {
        let mut ctx = ScopedTypeContext::new(Language::Rust);
        ctx.bind_type_param("T".to_string(), "int".to_string());
        ctx.push_scope();
        ctx.bind_type_param("T".to_string(), "String".to_string());
        assert_eq!(ctx.get_type_param("T"), Some("String"));
        ctx.pop_scope();
        assert_eq!(ctx.get_type_param("T"), Some("int"));
    }

    #[test]
    fn test_scoped_type_context_bind_type_param_owned() {
        let mut ctx = ScopedTypeContext::new(Language::Rust);
        ctx.bind_type_param_owned("Vec", "T".to_string(), "String".to_string());
        assert_eq!(ctx.get_type_param_for_owner("Vec", "T"), Some("String"));
        // Falls back to global get_type_param since no ownership record for HashMap
        assert_eq!(ctx.get_type_param_for_owner("HashMap", "T"), Some("String"));
    }

    #[test]
    fn test_scoped_type_context_resolve_type_simple() {
        let mut ctx = ScopedTypeContext::new(Language::Rust);
        ctx.bind_type_param("T".to_string(), "String".to_string());
        assert_eq!(ctx.resolve_type("T"), "String");
        assert_eq!(ctx.resolve_type("int"), "int");
    }

    #[test]
    fn test_scoped_type_context_resolve_type_generic() {
        let mut ctx = ScopedTypeContext::new(Language::Rust);
        ctx.bind_type_param("T".to_string(), "String".to_string());
        assert_eq!(ctx.resolve_type("Vec<T>"), "Vec<String>");
    }

    #[test]
    fn test_scoped_type_context_narrow_union() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        let shape = TypeShape::Union(vec![
            TypeShape::Named("String".to_string()),
            TypeShape::Named("None".to_string()),
        ]);
        ctx.add_variable_type(
            "x".to_string(),
            TypeBinding {
                type_name: type_shape_to_string(&shape),
                shape: Some(shape),
                ..Default::default()
            },
        );
        ctx.narrow_union("x", &TypeShape::Named("None".to_string()));
        let binding = ctx.get_variable_type("x").unwrap();
        assert_eq!(binding.type_name, "String");
    }

    #[test]
    fn test_scoped_type_context_merge_from() {
        let mut ctx1 = ScopedTypeContext::new(Language::Python);
        ctx1.add_variable_type(
            "x".to_string(),
            TypeBinding {
                type_name: "int".to_string(),
                origin: Some(InferenceOrigin::LiteralType),
                ..Default::default()
            },
        );
        let mut ctx2 = ScopedTypeContext::new(Language::Python);
        ctx2.add_variable_type(
            "x".to_string(),
            TypeBinding {
                type_name: "String".to_string(),
                origin: Some(InferenceOrigin::TypeAnnotation),
                ..Default::default()
            },
        );
        ctx1.merge_from(&ctx2);
        assert_eq!(ctx1.get_variable_type("x").unwrap().type_name, "String");
    }

    #[test]
    fn test_scoped_type_context_merge_from_lower_priority() {
        let mut ctx1 = ScopedTypeContext::new(Language::Python);
        ctx1.add_variable_type(
            "x".to_string(),
            TypeBinding {
                type_name: "int".to_string(),
                origin: Some(InferenceOrigin::TypeAnnotation),
                ..Default::default()
            },
        );
        let mut ctx2 = ScopedTypeContext::new(Language::Python);
        ctx2.add_variable_type(
            "x".to_string(),
            TypeBinding {
                type_name: "String".to_string(),
                origin: Some(InferenceOrigin::LiteralType),
                ..Default::default()
            },
        );
        ctx1.merge_from(&ctx2);
        assert_eq!(ctx1.get_variable_type("x").unwrap().type_name, "int");
    }

    #[test]
    fn test_scoped_type_context_add_pattern_match_binding_identifier() {
        let mut ctx = ScopedTypeContext::new(Language::Rust);
        let pattern = Pattern::Identifier("x".to_string());
        let source_type = TypeShape::Named("String".to_string());
        ctx.add_pattern_match_binding(&pattern, &source_type);
        assert_eq!(ctx.get_variable_type("x").unwrap().type_name, "String");
    }

    #[test]
    fn test_scoped_type_context_add_pattern_match_binding_tuple() {
        let mut ctx = ScopedTypeContext::new(Language::Rust);
        let pattern = Pattern::Tuple(vec!["a".to_string(), "b".to_string()]);
        let source_type = TypeShape::Generic {
            base: "Tuple".to_string(),
            args: vec![
                TypeShape::Named("int".to_string()),
                TypeShape::Named("String".to_string()),
            ],
        };
        ctx.add_pattern_match_binding(&pattern, &source_type);
        assert_eq!(ctx.get_variable_type("a").unwrap().type_name, "int");
        assert_eq!(ctx.get_variable_type("b").unwrap().type_name, "String");
    }

    #[test]
    fn test_scoped_type_context_add_pattern_match_binding_struct() {
        let mut ctx = ScopedTypeContext::new(Language::Rust);
        let pattern = Pattern::Struct(vec!["x".to_string(), "y".to_string()]);
        let source_type = TypeShape::Named("Point".to_string());
        ctx.add_pattern_match_binding(&pattern, &source_type);
        assert_eq!(ctx.get_variable_type("x").unwrap().type_name, "unknown");
        assert_eq!(ctx.get_variable_type("y").unwrap().type_name, "unknown");
    }

    #[test]
    fn test_scoped_type_context_add_pattern_match_binding_wildcard() {
        let mut ctx = ScopedTypeContext::new(Language::Rust);
        let pattern = Pattern::Wildcard;
        let source_type = TypeShape::Named("Point".to_string());
        ctx.add_pattern_match_binding(&pattern, &source_type);
    }

    #[test]
    fn test_scoped_type_context_add_destructuring_binding_tuple() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        let source_type = TypeShape::Generic {
            base: "tuple".to_string(),
            args: vec![
                TypeShape::Named("int".to_string()),
                TypeShape::Named("String".to_string()),
            ],
        };
        ctx.add_destructuring_binding("a", &source_type, Some(0));
        assert_eq!(ctx.get_variable_type("a").unwrap().type_name, "int");
        ctx.add_destructuring_binding("b", &source_type, Some(1));
        assert_eq!(ctx.get_variable_type("b").unwrap().type_name, "String");
    }

    #[test]
    fn test_scoped_type_context_add_destructuring_binding_array() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        let source_type = TypeShape::Array(Box::new(TypeShape::Named("int".to_string())));
        ctx.add_destructuring_binding("elem", &source_type, None);
        assert_eq!(ctx.get_variable_type("elem").unwrap().type_name, "int");
    }

    #[test]
    fn test_scoped_type_context_is_empty() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        assert!(ctx.is_empty());
        ctx.add_variable_type(
            "x".to_string(),
            TypeBinding {
                type_name: "int".to_string(),
                ..Default::default()
            },
        );
        assert!(!ctx.is_empty());
    }

    // ==================== Nested destructuring pattern tests ====================

    #[test]
    fn test_pattern_match_nested_tuple() {
        let mut ctx = ScopedTypeContext::new(Language::Rust);
        let inner_tuple = Pattern::Tuple(vec!["x".to_string(), "y".to_string()]);
        let source_type = TypeShape::Generic {
            base: "Tuple".to_string(),
            args: vec![
                TypeShape::Named("i32".to_string()),
                TypeShape::Named("String".to_string()),
            ],
        };
        ctx.add_pattern_match_binding(&inner_tuple, &source_type);
        assert_eq!(ctx.get_variable_type("x").unwrap().type_name, "i32");
        assert_eq!(ctx.get_variable_type("y").unwrap().type_name, "String");
    }

    #[test]
    fn test_pattern_match_struct_multiple_fields() {
        let mut ctx = ScopedTypeContext::new(Language::Rust);
        let pattern = Pattern::Struct(vec![
            "name".to_string(),
            "age".to_string(),
            "email".to_string(),
        ]);
        let source_type = TypeShape::Named("User".to_string());
        ctx.add_pattern_match_binding(&pattern, &source_type);
        assert_eq!(ctx.get_variable_type("name").unwrap().type_name, "unknown");
        assert_eq!(ctx.get_variable_type("age").unwrap().type_name, "unknown");
        assert_eq!(ctx.get_variable_type("email").unwrap().type_name, "unknown");
    }

    #[test]
    fn test_pattern_match_tuple_three_elements() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        let pattern = Pattern::Tuple(vec!["a".to_string(), "b".to_string(), "c".to_string()]);
        let source_type = TypeShape::Generic {
            base: "Tuple".to_string(),
            args: vec![
                TypeShape::Named("int".to_string()),
                TypeShape::Named("String".to_string()),
                TypeShape::Named("bool".to_string()),
            ],
        };
        ctx.add_pattern_match_binding(&pattern, &source_type);
        assert_eq!(ctx.get_variable_type("a").unwrap().type_name, "int");
        assert_eq!(ctx.get_variable_type("b").unwrap().type_name, "String");
        assert_eq!(ctx.get_variable_type("c").unwrap().type_name, "bool");
    }

    #[test]
    fn test_pattern_match_tuple_fewer_args_than_source() {
        let mut ctx = ScopedTypeContext::new(Language::Rust);
        let pattern = Pattern::Tuple(vec!["a".to_string()]);
        let source_type = TypeShape::Generic {
            base: "Tuple".to_string(),
            args: vec![
                TypeShape::Named("i32".to_string()),
                TypeShape::Named("String".to_string()),
            ],
        };
        ctx.add_pattern_match_binding(&pattern, &source_type);
        assert_eq!(ctx.get_variable_type("a").unwrap().type_name, "i32");
        assert!(ctx.get_variable_type("b").is_none());
    }

    #[test]
    fn test_pattern_match_identifier_with_generic_source() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        let pattern = Pattern::Identifier("items".to_string());
        let source_type = TypeShape::Generic {
            base: "List".to_string(),
            args: vec![TypeShape::Named("String".to_string())],
        };
        ctx.add_pattern_match_binding(&pattern, &source_type);
        let binding = ctx.get_variable_type("items").unwrap();
        assert_eq!(binding.type_name, "List<String>");
        assert_eq!(
            binding.shape,
            Some(TypeShape::Generic {
                base: "List".to_string(),
                args: vec![TypeShape::Named("String".to_string())]
            })
        );
    }

    // ==================== Destructuring assignment tests ====================

    #[test]
    fn test_destructuring_binding_tuple_index_out_of_bounds() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        let source_type = TypeShape::Generic {
            base: "tuple".to_string(),
            args: vec![TypeShape::Named("int".to_string())],
        };
        ctx.add_destructuring_binding("a", &source_type, Some(5));
        let binding = ctx.get_variable_type("a").unwrap();
        assert_eq!(binding.type_name, "unknown");
    }

    #[test]
    fn test_destructuring_binding_array_element() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        let source_type = TypeShape::Array(Box::new(TypeShape::Named("String".to_string())));
        ctx.add_destructuring_binding("elem", &source_type, None);
        assert_eq!(ctx.get_variable_type("elem").unwrap().type_name, "String");
        assert_eq!(
            ctx.get_variable_type("elem").unwrap().shape,
            Some(TypeShape::Named("String".to_string()))
        );
    }

    #[test]
    fn test_destructuring_binding_non_destructured_type() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        let source_type = TypeShape::Named("String".to_string());
        ctx.add_destructuring_binding("x", &source_type, Some(0));
        let binding = ctx.get_variable_type("x").unwrap();
        assert_eq!(binding.type_name, "unknown");
        assert_eq!(
            binding.origin,
            Some(InferenceOrigin::DestructuringAssignment)
        );
    }

    #[test]
    fn test_destructuring_binding_generic_tuple() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        let source_type = TypeShape::Generic {
            base: "Tuple".to_string(),
            args: vec![
                TypeShape::Named("i32".to_string()),
                TypeShape::Named("bool".to_string()),
                TypeShape::Named("String".to_string()),
            ],
        };
        ctx.add_destructuring_binding("x", &source_type, Some(2));
        assert_eq!(ctx.get_variable_type("x").unwrap().type_name, "String");
    }

    // ==================== ScopedTypeContext resolve_type with nested generics ====================

    #[test]
    fn test_resolve_type_nested_generic() {
        let mut ctx = ScopedTypeContext::new(Language::Rust);
        ctx.bind_type_param("T".to_string(), "String".to_string());
        ctx.bind_type_param("K".to_string(), "i32".to_string());
        assert_eq!(ctx.resolve_type("HashMap<K, T>"), "HashMap<i32, String>");
    }

    #[test]
    fn test_resolve_type_deeply_nested() {
        let mut ctx = ScopedTypeContext::new(Language::Rust);
        ctx.bind_type_param("T".to_string(), "String".to_string());
        assert_eq!(ctx.resolve_type("Vec<Option<T>>"), "Vec<Option<String>>");
    }

    #[test]
    fn test_resolve_type_no_bindings() {
        let ctx = ScopedTypeContext::new(Language::Rust);
        assert_eq!(ctx.resolve_type("Vec<T>"), "Vec<T>");
    }

    // ==================== Narrow union with multiple operations ====================

    #[test]
    fn test_narrow_union_sequential() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        let shape = TypeShape::Union(vec![
            TypeShape::Named("String".to_string()),
            TypeShape::Named("int".to_string()),
            TypeShape::Named("None".to_string()),
        ]);
        ctx.add_variable_type(
            "x".to_string(),
            TypeBinding {
                type_name: type_shape_to_string(&shape),
                shape: Some(shape),
                ..Default::default()
            },
        );
        ctx.narrow_union("x", &TypeShape::Named("None".to_string()));
        let binding = ctx.get_variable_type("x").unwrap();
        assert_eq!(binding.type_name, "String | int");
    }

    #[test]
    fn test_narrow_union_single_member_left() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        let shape = TypeShape::Union(vec![TypeShape::Named("String".to_string())]);
        ctx.add_variable_type(
            "x".to_string(),
            TypeBinding {
                type_name: type_shape_to_string(&shape),
                shape: Some(shape),
                ..Default::default()
            },
        );
        ctx.narrow_union("x", &TypeShape::Named("String".to_string()));
        let binding = ctx.get_variable_type("x").unwrap();
        assert_eq!(binding.type_name, "String");
    }

    #[test]
    fn test_destructuring_binding_maps_generic_position() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        let source = TypeShape::Generic {
            base: "Pair".to_string(),
            args: vec![
                TypeShape::Named("String".to_string()),
                TypeShape::Named("int".to_string()),
            ],
        };
        ctx.add_destructuring_binding("second", &source, Some(1));
        let binding = ctx
            .get_variable_type("second")
            .expect("destructured binding exists");
        assert_eq!(binding.type_name, "int");
    }

    #[test]
    fn test_destructuring_binding_out_of_range_stays_unknown() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        let source = TypeShape::Generic {
            base: "Pair".to_string(),
            args: vec![TypeShape::Named("String".to_string())],
        };
        ctx.add_destructuring_binding("second", &source, Some(1));
        let binding = ctx
            .get_variable_type("second")
            .expect("destructured binding exists");
        assert_eq!(binding.type_name, "unknown");
    }

    fn branch_binding(type_name: &str) -> TypeBinding {
        TypeBinding {
            type_name: type_name.to_string(),
            origin: Some(InferenceOrigin::ControlFlowNarrowing),
            ..Default::default()
        }
    }

    #[test]
    fn test_resolve_branch_aware_prefers_else_binding() {
        let mut ctx = ScopedTypeContext::new(Language::Java);
        ctx.add_narrowed_type("x".to_string(), branch_binding("String"));
        ctx.add_narrowed_type_in_branch(
            "x".to_string(),
            branch_binding("Integer"),
            BranchPolarity::Else,
        );
        assert_eq!(
            ctx.resolve_branch_aware("x", BranchPolarity::Else)
                .expect("else binding")
                .type_name,
            "Integer"
        );
        assert_eq!(
            ctx.resolve_branch_aware("x", BranchPolarity::Then)
                .expect("then binding")
                .type_name,
            "String"
        );
    }

    #[test]
    fn test_resolve_branch_aware_falls_back_to_default() {
        let mut ctx = ScopedTypeContext::new(Language::Java);
        ctx.add_narrowed_type("x".to_string(), branch_binding("String"));
        assert_eq!(
            ctx.resolve_branch_aware("x", BranchPolarity::Else)
                .expect("fallback binding")
                .type_name,
            "String"
        );
        assert!(
            ctx.resolve_branch_aware("missing", BranchPolarity::Else)
                .is_none()
        );
    }

    fn nested_shape() -> TypeShape {
        TypeShape::Generic {
            base: "Tuple".to_string(),
            args: vec![
                TypeShape::Named("String".to_string()),
                TypeShape::Generic {
                    base: "Tuple".to_string(),
                    args: vec![
                        TypeShape::Named("int".to_string()),
                        TypeShape::Named("bool".to_string()),
                    ],
                },
            ],
        }
    }

    #[test]
    fn test_nested_destructuring_binds_each_level() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        let parts = vec![
            NestedPatternPart::Name("a".to_string()),
            NestedPatternPart::Group(vec![
                NestedPatternPart::Name("b".to_string()),
                NestedPatternPart::Name("c".to_string()),
            ]),
        ];
        ctx.add_nested_destructuring_binding(&parts, &nested_shape());
        assert_eq!(ctx.get_variable_type("a").unwrap().type_name, "String");
        assert_eq!(ctx.get_variable_type("b").unwrap().type_name, "int");
        assert_eq!(ctx.get_variable_type("c").unwrap().type_name, "bool");
    }

    #[test]
    fn test_nested_destructuring_skips_unmappable_group() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        let parts = vec![
            NestedPatternPart::Name("a".to_string()),
            NestedPatternPart::Group(vec![NestedPatternPart::Name("b".to_string())]),
        ];
        let source = TypeShape::Generic {
            base: "Tuple".to_string(),
            args: vec![TypeShape::Named("String".to_string())],
        };
        ctx.add_nested_destructuring_binding(&parts, &source);
        assert_eq!(ctx.get_variable_type("a").unwrap().type_name, "String");
        assert!(ctx.get_variable_type("b").is_none());
    }

    #[test]
    fn test_nested_destructuring_wildcard_binds_nothing() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        let parts = vec![
            NestedPatternPart::Wildcard,
            NestedPatternPart::Name("b".to_string()),
        ];
        ctx.add_nested_destructuring_binding(&parts, &nested_shape());
        assert_eq!(
            ctx.get_variable_type("b").unwrap().type_name,
            "Tuple<int, bool>"
        );
    }
}
