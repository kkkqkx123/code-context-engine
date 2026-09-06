//! Cross-file type propagation.
//!
//! Caches function return types with `High`/`Medium` confidence so that
//! callers in other files can infer variable types from `x = foo()` patterns
//! where `foo` is defined in a different file.
//!
//! The propagator is stored inside [`crate::symbol_table::ProjectSymbolTable`]
//! and is populated by [`crate::index::builder::symbol_table::SymbolTableBuilder`]
//! after per-file type inference. The resolver queries it when the
//! [`crate::symbol_table::TypeMemberIndex`] cannot determine an owner type.

use std::collections::HashMap;

use cce_types::entity::{Entity, EntityId};
use cce_types::normalize_project_path;
use dashmap::DashMap;

pub use super::generics::{
    shape_contains_param, split_call_args, split_call_target, substitute_call_return_type,
};
use super::types::{
    InferenceOrigin, ScopedTypeContext, TypeBinding, TypeShape, VariableTypeBinding,
    binding_supersedes, bindings_supersede, parse_type_shape, type_shape_to_string,
};

/// Cross-file return-type propagator.
///
/// Holds a global cache of function return types (`High`/`Medium` confidence)
/// and a per-file index for incremental invalidation. A name-based secondary
/// index is maintained for EntityId translation across remapped ID spaces
/// (parsed-file local IDs vs global index IDs).
#[derive(Debug, Default, Clone)]
pub struct CrossFilePropagator {
    /// Global cache: function EntityId (as seen in `ScopedTypeContext`) -> return type.
    return_type_cache: DashMap<EntityId, TypeBinding>,
    /// Per-file index: normalized file_path -> Vec<(EntityId, TypeBinding)>.
    file_return_index: DashMap<String, Vec<(EntityId, TypeBinding)>>,
    /// Name index: simple function name -> return type (fallback for remapped IDs).
    name_index: DashMap<String, TypeBinding>,
    /// Reverse name mapping: EntityId -> simple name (for removal).
    id_to_name: DashMap<EntityId, String>,
    /// Global cache: function EntityId -> parameter type bindings.
    param_type_cache: DashMap<EntityId, Vec<TypeBinding>>,
    /// Per-file index for parameter types.
    file_param_index: DashMap<String, Vec<(EntityId, Vec<TypeBinding>)>>,
    /// Name index for parameter types: function name -> param bindings.
    param_name_index: DashMap<String, Vec<TypeBinding>>,
    /// Reverse param name mapping.
    param_id_to_name: DashMap<EntityId, String>,
    /// Global cache: field/property EntityId -> type binding.
    field_type_cache: DashMap<EntityId, TypeBinding>,
    /// Per-file index for field types.
    file_field_index: DashMap<String, Vec<(EntityId, TypeBinding)>>,
    /// Name index for field types: field name -> type binding.
    field_name_index: DashMap<String, TypeBinding>,
    /// Reverse field name mapping.
    field_id_to_name: DashMap<EntityId, String>,
    /// Variable type bindings cache: variable name -> VariableTypeBinding
    variable_type_cache: DashMap<String, VariableTypeBinding>,
    /// Per-file index for variable types
    file_variable_index: DashMap<String, Vec<(String, VariableTypeBinding)>>,
    /// Name index for variable types
    variable_name_index: DashMap<String, VariableTypeBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeConfidence {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone)]
pub struct UseSiteTypeBinding {
    pub receiver_name: String,
    pub method_name: String,
    pub inferred_type: Option<String>,
    pub confidence: TypeConfidence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssignmentSource {
    FunctionCall(String),
    MemberAccess(String, String),
    Literal(String),
    Conditional {
        condition: String,
        true_branch: Box<AssignmentSource>,
        false_branch: Box<AssignmentSource>,
    },
    Destructuring {
        fields: Vec<String>,
        source: Box<AssignmentSource>,
    },
}

impl CrossFilePropagator {
    /// Create a new empty propagator.
    pub fn new() -> Self {
        Self::default()
    }

    pub fn trace_assignment_chain(
        &self,
        var_name: &str,
        file_path: &str,
        max_depth: usize,
        var_assignments: &HashMap<String, AssignmentSource>,
    ) -> Option<TypeBinding> {
        use std::collections::HashSet;
        let mut visited: HashSet<(String, String)> = HashSet::new();
        let mut current_name = var_name.to_string();
        let current_file = file_path.to_string();
        let mut depth = max_depth;

        while depth > 0 {
            let key = (current_file.clone(), current_name.clone());
            if !visited.insert(key) {
                return None;
            }
            if let Some(source) = var_assignments.get(&current_name) {
                match source {
                    AssignmentSource::FunctionCall(func_name) => {
                        if let Some(binding) = self.get_return_type_by_name(func_name) {
                            return Some(binding);
                        }
                        if let Some(binding) = self.get_cross_file_return_type_fallback(func_name) {
                            return Some(binding);
                        }
                        current_name = func_name.clone();
                        depth -= 1;
                        continue;
                    }
                    AssignmentSource::MemberAccess(obj, member) => {
                        if let Some(obj_type) = self.trace_assignment_chain(
                            obj,
                            &current_file.clone(),
                            depth - 1,
                            var_assignments,
                        ) {
                            if let Some(member_type) =
                                self.lookup_member_type(&obj_type.type_name, member)
                            {
                                return Some(member_type);
                            }
                        }
                        if let Some(field_binding) = self.get_field_type_by_name(member) {
                            return Some(field_binding);
                        }
                        return None;
                    }
                    AssignmentSource::Literal(lit_type) => {
                        return Some(TypeBinding {
                            type_name: lit_type.clone(),
                            type_entity_id: None,
                            span: cce_types::Span::default(),
                            origin: Some(InferenceOrigin::LiteralType),
                            shape: Self::parse_literal_shape_static(lit_type),
                        });
                    }
                    AssignmentSource::Conditional {
                        condition: _,
                        true_branch,
                        false_branch,
                    } => {
                        // Try true branch first (more likely path)
                        if let Some(binding) = self.trace_assignment_chain_source(
                            true_branch,
                            &current_file,
                            depth - 1,
                            var_assignments,
                        ) {
                            return Some(binding);
                        }
                        // Fall back to false branch
                        if let Some(binding) = self.trace_assignment_chain_source(
                            false_branch,
                            &current_file,
                            depth - 1,
                            var_assignments,
                        ) {
                            return Some(binding);
                        }
                        return None;
                    }
                    AssignmentSource::Destructuring { source, .. } => {
                        // For destructuring, trace the source to get the container type
                        // Then we can potentially infer field types from the container
                        if let Some(binding) = self.trace_assignment_chain_source(
                            source,
                            &current_file,
                            depth - 1,
                            var_assignments,
                        ) {
                            // If we know the container type, return it
                            // A full implementation would look up each field in the type's structure
                            // and return individual field types, but for now we return the source type
                            return Some(binding);
                        }
                        return None;
                    }
                }
            }
            break;
        }
        None
    }

    /// Trace an assignment source to resolve its type.
    ///
    /// This is a helper method used for recursive tracing of assignment sources
    /// (e.g., for conditional branches or destructuring sources).
    fn trace_assignment_chain_source(
        &self,
        source: &AssignmentSource,
        file_path: &str,
        depth: usize,
        var_assignments: &HashMap<String, AssignmentSource>,
    ) -> Option<TypeBinding> {
        match source {
            AssignmentSource::FunctionCall(func_name) => {
                if let Some(binding) = self.get_return_type_by_name(func_name) {
                    return Some(binding);
                }
                if let Some(binding) = self.get_cross_file_return_type_fallback(func_name) {
                    return Some(binding);
                }
                // Try to trace through the function name as a variable
                self.trace_assignment_chain(func_name, file_path, depth, var_assignments)
            }
            AssignmentSource::MemberAccess(obj, member) => {
                if let Some(obj_type) =
                    self.trace_assignment_chain(obj, file_path, depth, var_assignments)
                {
                    if let Some(member_type) = self.lookup_member_type(&obj_type.type_name, member)
                    {
                        return Some(member_type);
                    }
                }
                if let Some(field_binding) = self.get_field_type_by_name(member) {
                    return Some(field_binding);
                }
                None
            }
            AssignmentSource::Literal(lit_type) => Some(TypeBinding {
                type_name: lit_type.clone(),
                type_entity_id: None,
                span: cce_types::Span::default(),
                origin: Some(InferenceOrigin::LiteralType),
                shape: Self::parse_literal_shape_static(lit_type),
            }),
            AssignmentSource::Conditional {
                true_branch,
                false_branch,
                ..
            } => {
                // Try true branch first
                if let Some(binding) = self.trace_assignment_chain_source(
                    true_branch,
                    file_path,
                    depth,
                    var_assignments,
                ) {
                    return Some(binding);
                }
                // Fall back to false branch
                self.trace_assignment_chain_source(false_branch, file_path, depth, var_assignments)
            }
            AssignmentSource::Destructuring { source, .. } => {
                self.trace_assignment_chain_source(source, file_path, depth, var_assignments)
            }
        }
    }

    fn get_cross_file_return_type_fallback(&self, name: &str) -> Option<TypeBinding> {
        self.get_return_type_by_name(name)
    }

    pub fn lookup_member_type(&self, type_name: &str, member: &str) -> Option<TypeBinding> {
        let key = format!("{}::{}", type_name, member);
        if let Some(binding) = self.get_field_type_by_name(&key) {
            return Some(binding);
        }
        if let Some(binding) = self.get_field_type_by_name(member) {
            return Some(binding);
        }
        None
    }

    fn parse_literal_shape_static(lit_type: &str) -> Option<TypeShape> {
        match lit_type {
            "int" | "i32" | "i64" | "u32" | "u64" => Some(TypeShape::Named("int".to_string())),
            "float" | "f32" | "f64" | "double" => Some(TypeShape::Named("float".to_string())),
            "str" | "String" | "&str" => Some(TypeShape::Named("str".to_string())),
            "bool" | "boolean" => Some(TypeShape::Named("bool".to_string())),
            "list" | "array" | "Vec" => Some(TypeShape::Named("list".to_string())),
            _ => Some(TypeShape::Named(lit_type.to_string())),
        }
    }

    /// Insert return types from a single file's type context.
    ///
    /// Only `High` and `Medium` confidence bindings are cached. The file's
    /// previous entries are removed first so the call is idempotent.
    pub fn insert_file(&self, file_path: &str, ctx: &ScopedTypeContext, entities: &[Entity]) {
        let normalized = normalize_project_path(file_path);
        self.remove_file(&normalized);

        let name_map: HashMap<EntityId, String> =
            entities.iter().map(|e| (e.id, e.name.clone())).collect();

        let mut file_entries: Vec<(EntityId, TypeBinding)> = Vec::new();

        for (entity_id, binding) in ctx.return_types_iter() {
            // Only propagate if the function entity exists in this file.
            let func_name = match name_map.get(entity_id) {
                Some(n) => n.clone(),
                None => continue,
            };

            let binding_clone = (*binding).clone();
            self.return_type_cache
                .insert(*entity_id, binding_clone.clone());
            self.name_index
                .entry(func_name.clone())
                .and_modify(|existing| {
                    if binding_supersedes(binding.origin, existing.origin) {
                        *existing = binding_clone.clone();
                    }
                })
                .or_insert_with(|| binding_clone.clone());

            self.id_to_name.insert(*entity_id, func_name);
            file_entries.push((*entity_id, binding_clone));
        }

        if !file_entries.is_empty() {
            self.file_return_index
                .insert(normalized.clone(), file_entries);
        }

        // Parameter types propagation
        let mut file_param_entries: Vec<(EntityId, Vec<TypeBinding>)> = Vec::new();
        for (entity_id, bindings) in ctx.parameter_types_iter() {
            let func_name = match name_map.get(entity_id) {
                Some(n) => n.clone(),
                None => continue,
            };
            let bindings_clone = (*bindings).clone();
            self.param_type_cache
                .insert(*entity_id, bindings_clone.clone());
            self.param_name_index
                .entry(func_name.clone())
                .and_modify(|existing| {
                    if bindings_supersede(&bindings_clone, existing) {
                        *existing = bindings_clone.clone();
                    }
                })
                .or_insert_with(|| bindings_clone.clone());
            self.param_id_to_name.insert(*entity_id, func_name);
            file_param_entries.push((*entity_id, bindings_clone));
        }
        if !file_param_entries.is_empty() {
            self.file_param_index
                .insert(normalized.clone(), file_param_entries);
        }

        // Field/property types propagation
        let mut file_field_entries: Vec<(EntityId, TypeBinding)> = Vec::new();
        for entity in entities {
            if !matches!(
                entity.kind,
                cce_types::entity::EntityKind::Field | cce_types::entity::EntityKind::Property
            ) {
                continue;
            }
            if let Some(binding) = ctx.get_variable_type(&entity.name) {
                let binding_clone = binding.clone();
                self.field_type_cache
                    .insert(entity.id, binding_clone.clone());
                self.field_name_index
                    .entry(entity.name.clone())
                    .and_modify(|existing| {
                        if binding_supersedes(binding.origin, existing.origin) {
                            *existing = binding_clone.clone();
                        }
                    })
                    .or_insert_with(|| binding_clone.clone());
                self.field_id_to_name.insert(entity.id, entity.name.clone());
                file_field_entries.push((entity.id, binding_clone));
            }
        }
        if !file_field_entries.is_empty() {
            self.file_field_index.insert(normalized, file_field_entries);
        }
    }

    /// Remove all cached entries contributed by a file.
    pub fn remove_file(&self, file_path: &str) {
        let normalized = normalize_project_path(file_path);
        let mut needs_rebuild_return = false;
        if let Some((_, entries)) = self.file_return_index.remove(&normalized) {
            for (entity_id, _) in entries {
                self.return_type_cache.remove(&entity_id);
                self.id_to_name.remove(&entity_id);
            }
            needs_rebuild_return = true;
        }
        if needs_rebuild_return {
            self.name_index.clear();
            for file_entries in self.file_return_index.iter() {
                for (id, binding) in file_entries.value() {
                    if let Some(name) = self.id_to_name.get(id).map(|n| n.clone()) {
                        self.name_index
                            .entry(name)
                            .and_modify(|existing: &mut TypeBinding| {
                                if binding_supersedes(binding.origin, existing.origin) {
                                    *existing = binding.clone();
                                }
                            })
                            .or_insert_with(|| binding.clone());
                    }
                }
            }
        }

        let mut needs_rebuild_param = false;
        if let Some((_, entries)) = self.file_param_index.remove(&normalized) {
            for (entity_id, _) in entries {
                self.param_type_cache.remove(&entity_id);
                self.param_id_to_name.remove(&entity_id);
            }
            needs_rebuild_param = true;
        }
        if needs_rebuild_param {
            self.param_name_index.clear();
            for file_entries in self.file_param_index.iter() {
                for (id, bindings) in file_entries.value() {
                    if let Some(name) = self.param_id_to_name.get(id).map(|n| n.clone()) {
                        self.param_name_index
                            .entry(name)
                            .and_modify(|existing: &mut Vec<TypeBinding>| {
                                if bindings_supersede(bindings, existing) {
                                    *existing = bindings.clone();
                                }
                            })
                            .or_insert_with(|| bindings.clone());
                    }
                }
            }
        }

        let mut needs_rebuild_field = false;
        if let Some((_, entries)) = self.file_field_index.remove(&normalized) {
            for (entity_id, _) in entries {
                self.field_type_cache.remove(&entity_id);
                self.field_id_to_name.remove(&entity_id);
            }
            needs_rebuild_field = true;
        }
        if needs_rebuild_field {
            self.field_name_index.clear();
            for file_entries in self.file_field_index.iter() {
                for (id, binding) in file_entries.value() {
                    if let Some(name) = self.field_id_to_name.get(id).map(|n| n.clone()) {
                        self.field_name_index
                            .entry(name)
                            .and_modify(|existing: &mut TypeBinding| {
                                if binding_supersedes(binding.origin, existing.origin) {
                                    *existing = binding.clone();
                                }
                            })
                            .or_insert_with(|| binding.clone());
                    }
                }
            }
        }

        let mut needs_rebuild_variable = false;
        if let Some((_, entries)) = self.file_variable_index.remove(&normalized) {
            for (var_name, _) in entries {
                self.variable_type_cache.remove(&var_name);
                self.variable_name_index.remove(&var_name);
            }
            needs_rebuild_variable = true;
        }
        if needs_rebuild_variable {
            self.variable_name_index.clear();
            for file_entries in self.file_variable_index.iter() {
                for (var_name, binding) in file_entries.value() {
                    self.variable_name_index
                        .entry(var_name.clone())
                        .and_modify(|existing: &mut VariableTypeBinding| {
                            if binding_supersedes(binding.primary.origin, existing.primary.origin) {
                                *existing = binding.clone();
                            }
                        })
                        .or_insert_with(|| binding.clone());
                    self.variable_type_cache
                        .insert(var_name.clone(), binding.clone());
                }
            }
        }
    }

    /// Get return type by EntityId (local or global).
    ///
    /// Falls back to name-based lookup when the ID is not found (handles
    /// remapped global IDs).
    pub fn get_return_type(&self, entity_id: EntityId) -> Option<TypeBinding> {
        if let Some(entry) = self.return_type_cache.get(&entity_id) {
            return Some(entry.clone());
        }
        None
    }

    /// Get return type by simple function name.
    pub fn get_return_type_by_name(&self, name: &str) -> Option<TypeBinding> {
        self.name_index.get(name).map(|v| v.clone())
    }

    /// Get all return types contributed by a file.
    pub fn get_file_returns(&self, file_path: &str) -> Option<Vec<(EntityId, TypeBinding)>> {
        let normalized = normalize_project_path(file_path);
        self.file_return_index.get(&normalized).map(|v| v.clone())
    }

    /// Get parameter types by EntityId.
    pub fn get_parameter_types(&self, entity_id: EntityId) -> Option<Vec<TypeBinding>> {
        self.param_type_cache.get(&entity_id).map(|v| v.clone())
    }

    /// Get parameter types by function name.
    pub fn get_parameter_types_by_name(&self, name: &str) -> Option<Vec<TypeBinding>> {
        self.param_name_index.get(name).map(|v| v.clone())
    }

    /// Get all parameter types contributed by a file.
    pub fn get_file_params(&self, file_path: &str) -> Option<Vec<(EntityId, Vec<TypeBinding>)>> {
        let normalized = normalize_project_path(file_path);
        self.file_param_index.get(&normalized).map(|v| v.clone())
    }

    /// Get field type by EntityId.
    pub fn get_field_type(&self, entity_id: EntityId) -> Option<TypeBinding> {
        self.field_type_cache.get(&entity_id).map(|v| v.clone())
    }

    /// Get field type by field name.
    pub fn get_field_type_by_name(&self, name: &str) -> Option<TypeBinding> {
        self.field_name_index.get(name).map(|v| v.clone())
    }

    /// Get all field types contributed by a file.
    pub fn get_file_fields(&self, file_path: &str) -> Option<Vec<(EntityId, TypeBinding)>> {
        let normalized = normalize_project_path(file_path);
        self.file_field_index.get(&normalized).map(|v| v.clone())
    }

    /// Insert variable type binding for a file
    pub fn insert_variable_type(
        &self,
        file_path: &str,
        var_name: &str,
        binding: VariableTypeBinding,
    ) {
        let normalized = normalize_project_path(file_path);
        self.variable_type_cache
            .insert(var_name.to_string(), binding.clone());
        self.file_variable_index
            .entry(normalized.clone())
            .or_default()
            .push((var_name.to_string(), binding.clone()));
        self.variable_name_index
            .entry(var_name.to_string())
            .and_modify(|existing| {
                if binding_supersedes(binding.primary.origin, existing.primary.origin) {
                    *existing = binding.clone();
                }
            })
            .or_insert(binding);
    }

    /// Get variable type binding by name
    pub fn get_variable_type(&self, var_name: &str) -> Option<VariableTypeBinding> {
        self.variable_type_cache.get(var_name).map(|r| r.clone())
    }

    /// Get variable type binding with alternatives by name
    pub fn get_variable_type_with_alternatives(&self, var_name: &str) -> Option<Vec<TypeBinding>> {
        self.variable_type_cache
            .get(var_name)
            .map(|r| r.all_types().into_iter().cloned().collect())
    }

    /// Propagate conditional assignment types
    /// Handles: `x = a if cond else b` → merge types of a and b
    pub fn propagate_conditional_assignment(
        &self,
        file_path: &str,
        var_name: &str,
        true_branch_type: Option<TypeBinding>,
        false_branch_type: Option<TypeBinding>,
    ) {
        let primary = true_branch_type.unwrap_or_else(|| TypeBinding {
            type_name: "unknown".to_string(),
            type_entity_id: None,
            span: cce_types::Span::default(),
            origin: None,
            shape: None,
        });
        let mut binding = VariableTypeBinding::new(primary);
        if let Some(false_type) = false_branch_type {
            binding.add_alternative(false_type);
        }
        self.insert_variable_type(file_path, var_name, binding);
    }

    /// Remove variable types for a file
    pub fn remove_variable_types_for_file(&self, file_path: &str) {
        let normalized = normalize_project_path(file_path);
        if let Some((_, var_names)) = self.file_variable_index.remove(&normalized) {
            for (var_name, _) in var_names {
                self.variable_type_cache.remove(&var_name);
                self.variable_name_index.remove(&var_name);
            }
        }
        // Rebuild variable name index from remaining files if needed
        if self.variable_name_index.is_empty() && !self.file_variable_index.is_empty() {
            for file_entries in self.file_variable_index.iter() {
                for (var_name, binding) in file_entries.value() {
                    self.variable_name_index
                        .entry(var_name.clone())
                        .and_modify(|existing: &mut VariableTypeBinding| {
                            if binding_supersedes(binding.primary.origin, existing.primary.origin) {
                                *existing = binding.clone();
                            }
                        })
                        .or_insert_with(|| binding.clone());
                }
            }
        }
    }

    /// Clear all caches.
    pub fn clear(&self) {
        self.return_type_cache.clear();
        self.file_return_index.clear();
        self.name_index.clear();
        self.id_to_name.clear();
        self.param_type_cache.clear();
        self.file_param_index.clear();
        self.param_name_index.clear();
        self.param_id_to_name.clear();
        self.field_type_cache.clear();
        self.file_field_index.clear();
        self.field_name_index.clear();
        self.field_id_to_name.clear();
        self.variable_type_cache.clear();
        self.file_variable_index.clear();
        self.variable_name_index.clear();
    }

    /// Number of cached return types.
    pub fn len(&self) -> usize {
        self.return_type_cache.len()
    }

    /// Total number of cached entries (return + param + field + variable).
    pub fn total_len(&self) -> usize {
        self.return_type_cache.len()
            + self.param_type_cache.len()
            + self.field_type_cache.len()
            + self.variable_type_cache.len()
    }

    /// Check if empty (all caches empty).
    pub fn is_empty(&self) -> bool {
        self.return_type_cache.is_empty()
            && self.param_type_cache.is_empty()
            && self.field_type_cache.is_empty()
            && self.variable_type_cache.is_empty()
    }

    /// Rebuild from all per-file contexts.
    pub fn rebuild_from_contexts(&self, file_contexts: Vec<(&str, &ScopedTypeContext, &[Entity])>) {
        self.clear();
        for (path, ctx, entities) in file_contexts {
            self.insert_file(path, ctx, entities);
        }
    }
}

/// A step in a call chain like `foo().bar()` or `module.func`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallStep {
    pub receiver: Option<String>,
    pub method_name: String,
    pub args: Vec<String>,
}

/// Parse call targets like "module.func", "obj.method", "foo().bar()"
/// into a chain of calls.
pub fn parse_call_chain(call_target: &str) -> Vec<CallStep> {
    let mut steps: Vec<String> = Vec::new();
    let trimmed = call_target.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    // Split by '.' but respect parentheses: `foo().bar` -> ["foo()", "bar"]
    let mut current = String::new();
    let mut depth = 0;
    for ch in trimmed.chars() {
        match ch {
            '(' => {
                depth += 1;
                current.push(ch);
            }
            ')' => {
                depth -= 1;
                current.push(ch);
            }
            '.' if depth == 0 => {
                if !current.trim().is_empty() {
                    steps.push(current.trim().to_string());
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        steps.push(current.trim().to_string());
    }
    // Convert raw step strings into CallStep structs
    let mut result: Vec<CallStep> = Vec::new();
    for (idx, raw) in steps.iter().enumerate() {
        let (name_part, args) = if let Some(paren_start) = raw.find('(') {
            let name = raw[..paren_start].trim().to_string();
            let arg_str = raw[paren_start + 1..raw.rfind(')').unwrap_or(raw.len() - 1)].trim();
            let args = if arg_str.is_empty() {
                vec![]
            } else {
                split_call_args(arg_str)
            };
            (name, args)
        } else {
            (raw.clone(), vec![])
        };
        // Strip qualification like `module::func` -> `func`
        let simple = name_part
            .rsplit([':', '/', '.'])
            .next()
            .unwrap_or(&name_part)
            .to_string();
        result.push(CallStep {
            receiver: if idx == 0 {
                None
            } else {
                Some(steps[idx - 1].clone())
            },
            method_name: simple,
            args,
        });
    }
    result
}

/// Infer the type shape of a call-site argument expression.
///
/// Literals map to the same vocabulary the extractor records
/// (`number`/`string`/`boolean`/`null`/`array`/`object`); identifiers
/// resolve against already-known variable bindings in the caller's context;
/// constructor expressions (`new Foo()`, `Foo()`) resolve to their base
/// type. Anything else yields `None` so the caller keeps its fallback.
pub fn infer_arg_shape(
    ctx: &ScopedTypeContext,
    language: cce_types::language::Language,
    arg: &str,
) -> Option<TypeShape> {
    let trimmed = arg.trim();
    if trimmed.is_empty() {
        return None;
    }
    // String literal.
    if (trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2)
        || (trimmed.starts_with('\'') && trimmed.ends_with('\'') && trimmed.len() >= 2)
        || trimmed.starts_with("r\"")
        || trimmed.starts_with("r#")
        || trimmed.starts_with('`')
    {
        return Some(TypeShape::Named("string".to_string()));
    }
    // Numeric literal (int, float and suffixed forms like `10u32`).
    if trimmed.parse::<f64>().is_ok()
        || trimmed
            .trim_end_matches(|c: char| c.is_ascii_alphabetic() || c == '_')
            .parse::<f64>()
            .is_ok()
    {
        return Some(TypeShape::Named("number".to_string()));
    }
    // Boolean / null literals.
    if trimmed == "true" || trimmed == "false" {
        return Some(TypeShape::Named("boolean".to_string()));
    }
    if trimmed == "None" || trimmed == "null" || trimmed == "nil" {
        return Some(TypeShape::Named("null".to_string()));
    }
    // Array literal: element type comes from the first element so
    // `first([1, 2])` against `first<T>(arr: T[])` binds `T = number`.
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
        return Some(TypeShape::Named("object".to_string()));
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
                let base = head
                    .rsplit(['.', ':', '/'])
                    .next()
                    .unwrap_or(head)
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
        let base = constructor_base
            .rsplit(['.', ':', '/'])
            .next()
            .unwrap_or(constructor_base);
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

/// Refine a generic callee return type using call-site argument expressions.
///
/// Looks up the callee's return and formal parameter bindings in the
/// propagator, resolves each argument expression with `resolve_arg`, and
/// substitutes type parameters (`T` in `identity<T>(x: T): T` called as
/// `identity(42)` yields `number`). Returns `None` when there is nothing
/// to refine (no arguments, non-generic return, unknown formals) or when
/// the result is not fully concrete, so callers keep the unsubstituted
/// return type as their fallback.
pub fn refine_generic_call(
    propagator: &CrossFilePropagator,
    language: cce_types::language::Language,
    callee_name: &str,
    arg_exprs: &[String],
    resolve_arg: &mut dyn FnMut(&str) -> Option<TypeShape>,
) -> Option<(String, TypeShape)> {
    if arg_exprs.is_empty() {
        return None;
    }
    let return_binding = propagator.get_return_type_by_name(callee_name)?;
    let return_shape = return_binding
        .shape
        .clone()
        .or_else(|| parse_type_shape(&return_binding.type_name, language))?;
    if !shape_contains_param(&return_shape) {
        return None;
    }
    let param_bindings = propagator.get_parameter_types_by_name(callee_name)?;
    if param_bindings.is_empty() {
        return None;
    }
    let formal_shapes: Vec<TypeShape> = param_bindings
        .iter()
        .map(|binding| {
            binding
                .shape
                .clone()
                .or_else(|| parse_type_shape(&binding.type_name, language))
                .unwrap_or(TypeShape::Named("unknown".to_string()))
        })
        .collect();
    let actual_shapes: Vec<Option<TypeShape>> =
        arg_exprs.iter().map(|arg| resolve_arg(arg)).collect();
    let actual_refs: Vec<Option<&TypeShape>> =
        actual_shapes.iter().map(|opt| opt.as_ref()).collect();
    let substituted =
        substitute_call_return_type(&formal_shapes, &return_shape, &actual_refs, language)?;
    if shape_contains_param(&substituted) {
        return None;
    }
    Some((type_shape_to_string(&substituted), substituted))
}

/// Whether writing `candidate_shape` would downgrade a concrete binding.
///
/// A binding whose shape is fully resolved must never be overwritten by a
/// shape that still mentions a type parameter (e.g. same-file
/// `Pair<number, string>` must survive a later cross-file pass that only
/// knows the unsubstituted `Pair<A, B>`). Unknown shapes on either side
/// keep the existing priority logic.
pub fn candidate_downgrades_existing(
    existing: Option<&TypeBinding>,
    candidate_shape: Option<&TypeShape>,
) -> bool {
    match (
        existing.and_then(|binding| binding.shape.as_ref()),
        candidate_shape,
    ) {
        (Some(existing_shape), Some(candidate)) => {
            !shape_contains_param(existing_shape) && shape_contains_param(candidate)
        }
        _ => false,
    }
}

/// Resolve the propagated type for a single `x = f(...)` call target.
///
/// Tries call-site generic refinement first (`y = identity(42)` yields
/// `number`); falls back to the callee's unsubstituted return type.
/// Returns the type name, the (optional) defining entity id, and the
/// (optional) structured shape, or `None` when the callee is unknown.
/// Chain targets (`a.b().c()`) are not handled here; use
/// [`parse_call_chain`] for those.
pub fn resolve_single_call_binding(
    propagator: &CrossFilePropagator,
    language: cce_types::language::Language,
    target: &str,
    resolve_arg: &mut dyn FnMut(&str) -> Option<TypeShape>,
) -> Option<(String, Option<EntityId>, Option<TypeShape>)> {
    let (name, args) = split_call_target(target);
    let simple = name.rsplit(['.', ':', '/']).next().unwrap_or(&name).trim();
    if simple.is_empty() {
        return None;
    }
    if !args.is_empty()
        && let Some((refined_name, refined_shape)) =
            refine_generic_call(propagator, language, simple, &args, resolve_arg)
    {
        return Some((refined_name, None, Some(refined_shape)));
    }
    let return_binding = propagator.get_return_type_by_name(simple)?;
    Some((
        return_binding.type_name.clone(),
        return_binding.type_entity_id,
        return_binding.shape.clone(),
    ))
}

const MAX_ITERATIONS: usize = 10;
const MAX_CHAIN_DEPTH: usize = 5;

/// Propagate cross-file return types into variable bindings.
///
/// For each variable entity that lacks a high-confidence type but is assigned
/// via a function call (metadata `call_target` or `constructor_type`), look up
/// the callee's return type in the propagator and add a variable binding with
/// `Medium` confidence. This enables `x = foo()` where `foo` returns `MyType`
/// to infer `x: MyType` even when `foo` is defined in another file.
///
/// Supports iterative propagation and chain calls like `x = foo().bar()`.
pub fn propagate_variable_types(
    files: &[&cce_types::ParsedFile],
    propagator: &CrossFilePropagator,
    contexts: &DashMap<String, ScopedTypeContext>,
) {
    let mut changed = true;
    let mut iterations = 0;
    while changed && iterations < MAX_ITERATIONS {
        changed = false;
        iterations += 1;
        for file in files {
            let normalized = normalize_project_path(&file.path);
            let Some(mut ctx_ref) = contexts.get_mut(&normalized) else {
                continue;
            };
            let ctx = ctx_ref.value_mut();

            for entity in &file.entities {
                if entity.kind != cce_types::entity::EntityKind::Variable {
                    continue;
                }
                if let Some(existing) = ctx.get_variable_type(&entity.name) {
                    if super::types::origin_is_authoritative(existing.origin) {
                        continue;
                    }
                }

                // Try to find a call target that could provide a return type.
                let call_target = entity
                    .metadata
                    .get("call_target")
                    .or_else(|| entity.metadata.get("constructor_type"))
                    .cloned();

                if let Some(target) = call_target {
                    // Handle chain calls: try iterative resolution
                    let chain = parse_call_chain(&target);
                    // Stored targets may carry an argument list (`foo(a)`);
                    // name lookups always use the stripped callee name.
                    let stripped_name = || split_call_target(&target).0;
                    let simple_target = if chain.len() > MAX_CHAIN_DEPTH {
                        continue;
                    } else if chain.len() > 1 {
                        let mut visited = std::collections::HashSet::new();
                        let mut current_binding: Option<TypeBinding> = None;
                        let mut cycle_detected = false;
                        for (idx, step) in chain.iter().enumerate() {
                            if !visited.insert(step.method_name.clone()) {
                                cycle_detected = true;
                                break;
                            }
                            if visited.len() > MAX_CHAIN_DEPTH {
                                cycle_detected = true;
                                break;
                            }
                            if idx == 0 {
                                if let Some(var_binding) = ctx.get_variable_type(&step.method_name)
                                {
                                    current_binding = Some(var_binding.clone());
                                } else if let Some(binding) =
                                    propagator.get_return_type_by_name(&step.method_name)
                                {
                                    current_binding = Some(binding.clone());
                                } else if let Some(binding) =
                                    propagator.get_field_type_by_name(&step.method_name)
                                {
                                    current_binding = Some(binding.clone());
                                } else {
                                    break;
                                }
                            } else {
                                let cur = current_binding.as_ref().map(|b| b.type_name.clone());
                                if let Some(cur_type) = cur {
                                    if let Some(member_binding) =
                                        propagator.lookup_member_type(&cur_type, &step.method_name)
                                    {
                                        current_binding = Some(member_binding);
                                    } else if let Some(binding) =
                                        propagator.get_field_type_by_name(&step.method_name)
                                    {
                                        current_binding = Some(binding.clone());
                                    } else if let Some(binding) =
                                        propagator.get_return_type_by_name(&step.method_name)
                                    {
                                        current_binding = Some(binding.clone());
                                    } else {
                                        break;
                                    }
                                } else {
                                    break;
                                }
                            }
                        }
                        if cycle_detected {
                            continue;
                        }
                        if let Some(binding) = current_binding {
                            let propagated = TypeBinding {
                                type_name: binding.type_name.clone(),
                                type_entity_id: binding.type_entity_id,
                                span: entity.span,
                                origin: Some(InferenceOrigin::CrossFilePropagation),
                                shape: binding.shape.clone(),
                            };
                            let should_insert =
                                ctx.get_variable_type(&entity.name).is_none_or(|existing| {
                                    !candidate_downgrades_existing(
                                        Some(existing),
                                        propagated.shape.as_ref(),
                                    ) && binding_supersedes(propagated.origin, existing.origin)
                                });
                            if should_insert {
                                ctx.add_variable_type(entity.name.clone(), propagated);
                                changed = true;
                            }
                            continue;
                        }
                        // Fallback to simple target if chain resolution failed
                        stripped_name()
                            .rsplit(['.', ':', '/'])
                            .next()
                            .unwrap_or(&target)
                            .trim()
                            .to_string()
                    } else {
                        // Strip qualification: `module.func` -> `func`
                        stripped_name()
                            .rsplit(['.', ':', '/'])
                            .next()
                            .unwrap_or(&target)
                            .trim()
                            .to_string()
                    };
                    if simple_target.is_empty() {
                        continue;
                    }
                    // Generic refinement first, unsubstituted return second.
                    let language = file.language;
                    let resolved =
                        resolve_single_call_binding(propagator, language, &target, &mut |arg| {
                            infer_arg_shape(ctx, language, arg)
                        });
                    if let Some((type_name, type_entity_id, shape)) = resolved {
                        let propagated = TypeBinding {
                            type_name,
                            type_entity_id,
                            span: entity.span,
                            origin: Some(InferenceOrigin::CrossFilePropagation),
                            shape,
                        };
                        let should_insert =
                            ctx.get_variable_type(&entity.name).is_none_or(|existing| {
                                !candidate_downgrades_existing(
                                    Some(existing),
                                    propagated.shape.as_ref(),
                                ) && binding_supersedes(propagated.origin, existing.origin)
                            });
                        if should_insert {
                            ctx.add_variable_type(entity.name.clone(), propagated);
                            changed = true;
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::Span;
    use cce_types::entity::{Entity, EntityKind};
    use cce_types::language::Language;

    fn dummy_span() -> Span {
        Span::default()
    }

    #[test]
    fn test_propagator_insert_and_lookup() {
        let propagator = CrossFilePropagator::new();
        let mut ctx = ScopedTypeContext::new(Language::Python);
        let binding = TypeBinding {
            type_name: "MyClass".to_string(),
            type_entity_id: None,
            span: dummy_span(),
            origin: None,
            shape: None,
        };
        ctx.add_return_type(EntityId(1), binding.clone());

        let entities = vec![
            Entity::new(
                EntityId(1),
                EntityKind::Function,
                "create_user".to_string(),
                dummy_span(),
            )
            .with_return_type(Some("MyClass".to_string())),
        ];

        propagator.insert_file("a.py", &ctx, &entities);
        assert_eq!(propagator.len(), 1);
        assert!(propagator.get_return_type(EntityId(1)).is_some());
        assert!(propagator.get_return_type_by_name("create_user").is_some());
        assert_eq!(
            propagator
                .get_return_type_by_name("create_user")
                .unwrap()
                .type_name,
            "MyClass"
        );
    }

    #[test]
    fn test_propagator_remove_file() {
        let propagator = CrossFilePropagator::new();
        let mut ctx = ScopedTypeContext::new(Language::Rust);
        ctx.add_return_type(
            EntityId(1),
            TypeBinding {
                type_name: "String".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );
        let entities = vec![Entity::new(
            EntityId(1),
            EntityKind::Function,
            "get_name".to_string(),
            dummy_span(),
        )];
        propagator.insert_file("b.rs", &ctx, &entities);
        assert_eq!(propagator.len(), 1);
        propagator.remove_file("b.rs");
        assert_eq!(propagator.len(), 0);
        assert!(propagator.get_return_type_by_name("get_name").is_none());
    }

    #[test]
    fn test_propagator_medium_confidence_cached() {
        let propagator = CrossFilePropagator::new();
        let mut ctx = ScopedTypeContext::new(Language::Python);
        ctx.add_return_type(
            EntityId(1),
            TypeBinding {
                type_name: "MyType".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );
        let entities = vec![Entity::new(
            EntityId(1),
            EntityKind::Function,
            "foo".to_string(),
            dummy_span(),
        )];
        propagator.insert_file("c.py", &ctx, &entities);
        assert_eq!(propagator.len(), 1);
    }

    #[test]
    fn test_variable_propagation() {
        let propagator = CrossFilePropagator::new();
        let mut ctx_a = ScopedTypeContext::new(Language::Python);
        ctx_a.add_return_type(
            EntityId(1),
            TypeBinding {
                type_name: "User".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );
        let entities_a = vec![Entity::new(
            EntityId(1),
            EntityKind::Function,
            "create_user".to_string(),
            dummy_span(),
        )];
        propagator.insert_file("a.py", &ctx_a, &entities_a);

        let mut file_b = cce_types::ParsedFile::new(Language::Python, "b.py".to_string(), "");
        let var = Entity::new(
            EntityId(2),
            EntityKind::Variable,
            "u".to_string(),
            dummy_span(),
        )
        .with_metadata("call_target", "create_user");
        file_b.add_entity(var);

        let contexts: DashMap<String, ScopedTypeContext> = DashMap::new();
        let ctx_b = ScopedTypeContext::new(Language::Python);
        contexts.insert("b.py".to_string(), ctx_b);

        propagate_variable_types(&[&file_b], &propagator, &contexts);

        let ctx = contexts.get("b.py").unwrap();
        let binding = ctx.get_variable_type("u").unwrap();
        assert_eq!(binding.type_name, "User");
        assert!(binding.origin.is_some());
    }

    fn generic_return_binding(type_name: &str, language: Language) -> TypeBinding {
        TypeBinding {
            type_name: type_name.to_string(),
            type_entity_id: None,
            span: dummy_span(),
            origin: None,
            shape: parse_type_shape(type_name, language),
        }
    }

    fn generic_param_binding(type_name: &str, language: Language) -> TypeBinding {
        TypeBinding {
            type_name: type_name.to_string(),
            type_entity_id: None,
            span: dummy_span(),
            origin: None,
            shape: parse_type_shape(type_name, language),
        }
    }

    #[test]
    fn test_generic_call_refinement() {
        let propagator = CrossFilePropagator::new();
        let mut ctx_a = ScopedTypeContext::new(Language::TypeScript);
        ctx_a.add_return_type(
            EntityId(1),
            generic_return_binding("T", Language::TypeScript),
        );
        ctx_a.add_parameter_types(
            EntityId(1),
            vec![generic_param_binding("T", Language::TypeScript)],
        );
        let entities_a = vec![Entity::new(
            EntityId(1),
            EntityKind::Function,
            "identity".to_string(),
            dummy_span(),
        )];
        propagator.insert_file("a.ts", &ctx_a, &entities_a);

        let mut file_b = cce_types::ParsedFile::new(Language::TypeScript, "b.ts".to_string(), "");
        let var = Entity::new(
            EntityId(2),
            EntityKind::Variable,
            "y".to_string(),
            dummy_span(),
        )
        .with_metadata("call_target", "identity(42)");
        file_b.add_entity(var);

        let contexts: DashMap<String, ScopedTypeContext> = DashMap::new();
        contexts.insert(
            "b.ts".to_string(),
            ScopedTypeContext::new(Language::TypeScript),
        );

        propagate_variable_types(&[&file_b], &propagator, &contexts);

        let ctx = contexts.get("b.ts").unwrap();
        let binding = ctx.get_variable_type("y").unwrap();
        assert_eq!(binding.type_name, "number");
    }

    #[test]
    fn test_generic_refinement_falls_back_without_args() {
        // Bare `identity` (no argument list) keeps the unsubstituted return.
        let propagator = CrossFilePropagator::new();
        let mut ctx_a = ScopedTypeContext::new(Language::TypeScript);
        ctx_a.add_return_type(
            EntityId(1),
            generic_return_binding("T", Language::TypeScript),
        );
        ctx_a.add_parameter_types(
            EntityId(1),
            vec![generic_param_binding("T", Language::TypeScript)],
        );
        let entities_a = vec![Entity::new(
            EntityId(1),
            EntityKind::Function,
            "identity".to_string(),
            dummy_span(),
        )];
        propagator.insert_file("a.ts", &ctx_a, &entities_a);

        let mut file_b = cce_types::ParsedFile::new(Language::TypeScript, "b.ts".to_string(), "");
        let var = Entity::new(
            EntityId(2),
            EntityKind::Variable,
            "y".to_string(),
            dummy_span(),
        )
        .with_metadata("call_target", "identity");
        file_b.add_entity(var);

        let contexts: DashMap<String, ScopedTypeContext> = DashMap::new();
        contexts.insert(
            "b.ts".to_string(),
            ScopedTypeContext::new(Language::TypeScript),
        );

        propagate_variable_types(&[&file_b], &propagator, &contexts);

        let ctx = contexts.get("b.ts").unwrap();
        let binding = ctx.get_variable_type("y").unwrap();
        assert_eq!(binding.type_name, "T");
    }

    #[test]
    fn test_generic_coarse_result_never_downgrades_concrete() {
        // `y` already holds a fully substituted type from an earlier pass;
        // an unresolvable call must not overwrite it with bare parameters.
        let propagator = CrossFilePropagator::new();
        let mut ctx_a = ScopedTypeContext::new(Language::TypeScript);
        ctx_a.add_return_type(
            EntityId(1),
            generic_return_binding("Pair<A, B>", Language::TypeScript),
        );
        ctx_a.add_parameter_types(
            EntityId(1),
            vec![
                generic_param_binding("A", Language::TypeScript),
                generic_param_binding("B", Language::TypeScript),
            ],
        );
        let entities_a = vec![Entity::new(
            EntityId(1),
            EntityKind::Function,
            "makePair".to_string(),
            dummy_span(),
        )];
        propagator.insert_file("a.ts", &ctx_a, &entities_a);

        let mut file_b = cce_types::ParsedFile::new(Language::TypeScript, "b.ts".to_string(), "");
        let var = Entity::new(
            EntityId(2),
            EntityKind::Variable,
            "p".to_string(),
            dummy_span(),
        )
        .with_metadata("call_target", "makePair(42, z)");
        file_b.add_entity(var);

        let contexts: DashMap<String, ScopedTypeContext> = DashMap::new();
        let mut ctx_b = ScopedTypeContext::new(Language::TypeScript);
        ctx_b.add_variable_type(
            "p".to_string(),
            TypeBinding {
                type_name: "Pair<number, string>".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: Some(InferenceOrigin::CrossFilePropagation),
                shape: parse_type_shape("Pair<number, string>", Language::TypeScript),
            },
        );
        contexts.insert("b.ts".to_string(), ctx_b);

        propagate_variable_types(&[&file_b], &propagator, &contexts);

        let ctx = contexts.get("b.ts").unwrap();
        assert_eq!(
            ctx.get_variable_type("p").unwrap().type_name,
            "Pair<number, string>"
        );
    }

    #[test]
    fn test_propagation_preserves_authoritative_binding() {
        let propagator = CrossFilePropagator::new();
        let mut ctx_a = ScopedTypeContext::new(Language::Python);
        ctx_a.add_return_type(
            EntityId(1),
            TypeBinding {
                type_name: "User".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );
        let entities_a = vec![Entity::new(
            EntityId(1),
            EntityKind::Function,
            "create_user".to_string(),
            dummy_span(),
        )];
        propagator.insert_file("a.py", &ctx_a, &entities_a);

        let mut file_b = cce_types::ParsedFile::new(Language::Python, "b.py".to_string(), "");
        let var = Entity::new(
            EntityId(2),
            EntityKind::Variable,
            "u".to_string(),
            dummy_span(),
        )
        .with_metadata("call_target", "create_user");
        file_b.add_entity(var);

        let contexts: DashMap<String, ScopedTypeContext> = DashMap::new();
        let mut ctx_b = ScopedTypeContext::new(Language::Python);
        ctx_b.add_variable_type(
            "u".to_string(),
            TypeBinding {
                type_name: "Admin".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: Some(InferenceOrigin::TypeAnnotation),
                shape: None,
            },
        );
        contexts.insert("b.py".to_string(), ctx_b);

        propagate_variable_types(&[&file_b], &propagator, &contexts);

        let ctx = contexts.get("b.py").unwrap();
        assert_eq!(ctx.get_variable_type("u").unwrap().type_name, "Admin");
    }

    #[test]
    fn test_param_propagation_insert_and_lookup() {
        let propagator = CrossFilePropagator::new();
        let mut ctx = ScopedTypeContext::new(Language::Java);
        ctx.add_parameter_types(
            EntityId(10),
            vec![TypeBinding {
                type_name: "String".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            }],
        );
        let entities = vec![Entity::new(
            EntityId(10),
            EntityKind::Method,
            "doSomething".to_string(),
            dummy_span(),
        )];
        propagator.insert_file("A.java", &ctx, &entities);
        assert!(propagator.get_parameter_types(EntityId(10)).is_some());
        assert!(
            propagator
                .get_parameter_types_by_name("doSomething")
                .is_some()
        );
        assert_eq!(
            propagator
                .get_parameter_types_by_name("doSomething")
                .unwrap()[0]
                .type_name,
            "String"
        );
    }

    #[test]
    fn test_field_propagation_insert_and_lookup() {
        let propagator = CrossFilePropagator::new();
        let mut ctx = ScopedTypeContext::new(Language::Java);
        ctx.add_variable_type(
            "myField".to_string(),
            TypeBinding {
                type_name: "int".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );
        let entities = vec![Entity::new(
            EntityId(20),
            EntityKind::Field,
            "myField".to_string(),
            dummy_span(),
        )];
        propagator.insert_file("B.java", &ctx, &entities);
        assert!(propagator.get_field_type(EntityId(20)).is_some());
        assert!(propagator.get_field_type_by_name("myField").is_some());
        assert_eq!(
            propagator
                .get_field_type_by_name("myField")
                .unwrap()
                .type_name,
            "int"
        );
    }

    #[test]
    fn test_propagator_remove_file_clears_params_and_fields() {
        let propagator = CrossFilePropagator::new();
        let mut ctx = ScopedTypeContext::new(Language::Java);
        ctx.add_return_type(
            EntityId(1),
            TypeBinding {
                type_name: "String".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );
        ctx.add_parameter_types(
            EntityId(1),
            vec![TypeBinding {
                type_name: "int".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            }],
        );
        ctx.add_variable_type(
            "myField".to_string(),
            TypeBinding {
                type_name: "bool".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );
        let entities = vec![
            Entity::new(
                EntityId(1),
                EntityKind::Function,
                "foo".to_string(),
                dummy_span(),
            ),
            Entity::new(
                EntityId(2),
                EntityKind::Field,
                "myField".to_string(),
                dummy_span(),
            ),
        ];
        propagator.insert_file("C.java", &ctx, &entities);
        assert!(!propagator.is_empty());
        propagator.remove_file("C.java");
        assert!(propagator.is_empty());
        assert!(propagator.get_return_type_by_name("foo").is_none());
        assert!(propagator.get_parameter_types_by_name("foo").is_none());
        assert!(propagator.get_field_type_by_name("myField").is_none());
    }

    // ==================== parse_call_chain tests ====================

    #[test]
    fn test_parse_call_chain_empty() {
        let result = parse_call_chain("");
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_call_chain_single_name() {
        let result = parse_call_chain("foo");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].method_name, "foo");
        assert!(result[0].receiver.is_none());
    }

    #[test]
    fn test_parse_call_chain_simple_call() {
        let result = parse_call_chain("foo()");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].method_name, "foo");
        assert!(result[0].args.is_empty());
    }

    #[test]
    fn test_parse_call_chain_call_with_args() {
        let result = parse_call_chain("foo(x, y)");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].method_name, "foo");
        assert_eq!(result[0].args, vec!["x", "y"]);
    }

    #[test]
    fn test_parse_call_chain_chain_calls() {
        let result = parse_call_chain("foo().bar()");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].method_name, "foo");
        assert!(result[0].receiver.is_none());
        assert_eq!(result[1].method_name, "bar");
        assert_eq!(result[1].receiver, Some("foo()".to_string()));
    }

    #[test]
    fn test_parse_call_chain_module_qualified() {
        let result = parse_call_chain("module.func");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].method_name, "module");
        assert_eq!(result[1].method_name, "func");
    }

    #[test]
    fn test_parse_call_chain_nested_parens() {
        let result = parse_call_chain("foo(a)");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].args, vec!["a"]);
    }

    #[test]
    fn test_parse_call_chain_strip_qualification() {
        let result = parse_call_chain("module::func");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].method_name, "func");
    }

    // ==================== lookup_member_type tests ====================

    #[test]
    fn test_lookup_member_type() {
        let propagator = CrossFilePropagator::new();
        let mut ctx = ScopedTypeContext::new(Language::Java);
        ctx.add_variable_type(
            "MyType::name".to_string(),
            TypeBinding {
                type_name: "String".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );
        let entities = vec![Entity::new(
            EntityId(20),
            EntityKind::Field,
            "MyType::name".to_string(),
            dummy_span(),
        )];
        propagator.insert_file("A.java", &ctx, &entities);
        let result = propagator.lookup_member_type("MyType", "name");
        assert!(result.is_some());
        assert_eq!(result.unwrap().type_name, "String");
    }

    #[test]
    fn test_lookup_member_type_fallback() {
        let propagator = CrossFilePropagator::new();
        let mut ctx = ScopedTypeContext::new(Language::Java);
        ctx.add_variable_type(
            "name".to_string(),
            TypeBinding {
                type_name: "String".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );
        let entities = vec![Entity::new(
            EntityId(20),
            EntityKind::Field,
            "name".to_string(),
            dummy_span(),
        )];
        propagator.insert_file("A.java", &ctx, &entities);
        let result = propagator.lookup_member_type("MyType", "name");
        assert!(result.is_some());
        assert_eq!(result.unwrap().type_name, "String");
    }

    #[test]
    fn test_lookup_member_type_not_found() {
        let propagator = CrossFilePropagator::new();
        let result = propagator.lookup_member_type("MyType", "name");
        assert!(result.is_none());
    }

    // ==================== trace_assignment_chain tests ====================

    #[test]
    fn test_trace_assignment_chain_function_call() {
        let propagator = CrossFilePropagator::new();
        let mut ctx = ScopedTypeContext::new(Language::Java);
        ctx.add_return_type(
            EntityId(1),
            TypeBinding {
                type_name: "String".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );
        let entities = vec![Entity::new(
            EntityId(1),
            EntityKind::Function,
            "myFunc".to_string(),
            dummy_span(),
        )];
        propagator.insert_file("file.java", &ctx, &entities);
        let mut assignments = HashMap::new();
        assignments.insert(
            "x".to_string(),
            AssignmentSource::FunctionCall("myFunc".to_string()),
        );
        let result = propagator.trace_assignment_chain("x", "file.java", 10, &assignments);
        assert!(result.is_some());
        assert_eq!(result.unwrap().type_name, "String");
    }

    #[test]
    fn test_trace_assignment_chain_literal() {
        let propagator = CrossFilePropagator::new();
        let mut assignments = HashMap::new();
        assignments.insert(
            "x".to_string(),
            AssignmentSource::Literal("int".to_string()),
        );
        let result = propagator.trace_assignment_chain("x", "file.rs", 10, &assignments);
        assert!(result.is_some());
        assert_eq!(result.unwrap().type_name, "int");
    }

    #[test]
    fn test_trace_assignment_chain_no_assignment() {
        let propagator = CrossFilePropagator::new();
        let assignments = HashMap::new();
        let result = propagator.trace_assignment_chain("x", "file.rs", 10, &assignments);
        assert!(result.is_none());
    }

    #[test]
    fn test_trace_assignment_chain_cycle_detection() {
        let propagator = CrossFilePropagator::new();
        let mut assignments = HashMap::new();
        assignments.insert(
            "x".to_string(),
            AssignmentSource::FunctionCall("y".to_string()),
        );
        assignments.insert(
            "y".to_string(),
            AssignmentSource::FunctionCall("x".to_string()),
        );
        let result = propagator.trace_assignment_chain("x", "file.rs", 10, &assignments);
        assert!(result.is_none());
    }

    // ==================== propagate_conditional_assignment tests ====================

    #[test]
    fn test_propagate_conditional_assignment() {
        let propagator = CrossFilePropagator::new();
        let true_type = TypeBinding {
            type_name: "String".to_string(),
            type_entity_id: None,
            span: dummy_span(),
            origin: Some(InferenceOrigin::TypeAnnotation),
            shape: None,
        };
        let false_type = TypeBinding {
            type_name: "None".to_string(),
            type_entity_id: None,
            span: dummy_span(),
            origin: Some(InferenceOrigin::TypeAnnotation),
            shape: None,
        };
        propagator.propagate_conditional_assignment(
            "file.py",
            "x",
            Some(true_type),
            Some(false_type),
        );
        let vars = propagator.get_variable_type_with_alternatives("x");
        assert!(vars.is_some());
        let vars = vars.unwrap();
        assert_eq!(vars.len(), 2);
        assert_eq!(vars[0].type_name, "String");
        assert_eq!(vars[1].type_name, "None");
    }

    // ==================== insert_variable_type tests ====================

    #[test]
    fn test_insert_and_get_variable_type() {
        let propagator = CrossFilePropagator::new();
        let binding = VariableTypeBinding::new(TypeBinding {
            type_name: "int".to_string(),
            type_entity_id: None,
            span: dummy_span(),
            origin: Some(InferenceOrigin::TypeAnnotation),
            shape: None,
        });
        propagator.insert_variable_type("file.rs", "x", binding);
        let result = propagator.get_variable_type_with_alternatives("x");
        assert!(result.is_some());
        assert_eq!(result.unwrap()[0].type_name, "int");
    }

    // ==================== is_empty / len / total_len tests ====================

    #[test]
    fn test_propagator_is_empty() {
        let propagator = CrossFilePropagator::new();
        assert!(propagator.is_empty());
        assert_eq!(propagator.len(), 0);
        assert_eq!(propagator.total_len(), 0);
    }

    #[test]
    fn test_propagator_len_and_total_len() {
        let propagator = CrossFilePropagator::new();
        let mut ctx = ScopedTypeContext::new(Language::Java);
        ctx.add_return_type(
            EntityId(1),
            TypeBinding {
                type_name: "String".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );
        ctx.add_variable_type(
            "bar".to_string(),
            TypeBinding {
                type_name: "int".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );
        let entities = vec![
            Entity::new(
                EntityId(1),
                EntityKind::Function,
                "foo".to_string(),
                dummy_span(),
            ),
            Entity::new(
                EntityId(2),
                EntityKind::Field,
                "bar".to_string(),
                dummy_span(),
            ),
        ];
        propagator.insert_file("A.java", &ctx, &entities);
        assert_eq!(propagator.len(), 1);
        assert!(propagator.total_len() >= 1);
    }

    // ==================== clear tests ====================

    #[test]
    fn test_propagator_clear() {
        let propagator = CrossFilePropagator::new();
        let mut ctx = ScopedTypeContext::new(Language::Java);
        ctx.add_return_type(
            EntityId(1),
            TypeBinding {
                type_name: "String".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );
        let entities = vec![Entity::new(
            EntityId(1),
            EntityKind::Function,
            "foo".to_string(),
            dummy_span(),
        )];
        propagator.insert_file("A.java", &ctx, &entities);
        assert!(!propagator.is_empty());
        propagator.clear();
        assert!(propagator.is_empty());
    }

    // ==================== parse_literal_shape_static tests ====================

    #[test]
    fn test_parse_literal_shape_static() {
        assert_eq!(
            CrossFilePropagator::parse_literal_shape_static("int"),
            Some(TypeShape::Named("int".to_string()))
        );
        assert_eq!(
            CrossFilePropagator::parse_literal_shape_static("String"),
            Some(TypeShape::Named("str".to_string()))
        );
        assert_eq!(
            CrossFilePropagator::parse_literal_shape_static("bool"),
            Some(TypeShape::Named("bool".to_string()))
        );
        assert_eq!(
            CrossFilePropagator::parse_literal_shape_static("float"),
            Some(TypeShape::Named("float".to_string()))
        );
        assert_eq!(
            CrossFilePropagator::parse_literal_shape_static("CustomType"),
            Some(TypeShape::Named("CustomType".to_string()))
        );
    }

    // ==================== trace_assignment_chain Conditional tests ====================

    #[test]
    fn test_trace_assignment_chain_conditional_true_branch() {
        let propagator = CrossFilePropagator::new();
        let mut ctx = ScopedTypeContext::new(Language::Python);
        ctx.add_return_type(
            EntityId(1),
            TypeBinding {
                type_name: "String".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );
        let entities = vec![Entity::new(
            EntityId(1),
            EntityKind::Function,
            "get_value".to_string(),
            dummy_span(),
        )];
        propagator.insert_file("file.py", &ctx, &entities);

        let mut assignments = HashMap::new();
        assignments.insert(
            "x".to_string(),
            AssignmentSource::Conditional {
                condition: "flag".to_string(),
                true_branch: Box::new(AssignmentSource::FunctionCall("get_value".to_string())),
                false_branch: Box::new(AssignmentSource::Literal("int".to_string())),
            },
        );

        let result = propagator.trace_assignment_chain("x", "file.py", 10, &assignments);
        assert!(result.is_some());
        assert_eq!(result.unwrap().type_name, "String");
    }

    #[test]
    fn test_trace_assignment_chain_conditional_fallback_to_false_branch() {
        let propagator = CrossFilePropagator::new();
        let mut assignments = HashMap::new();
        assignments.insert(
            "x".to_string(),
            AssignmentSource::Conditional {
                condition: "flag".to_string(),
                true_branch: Box::new(AssignmentSource::FunctionCall(
                    "nonexistent_func".to_string(),
                )),
                false_branch: Box::new(AssignmentSource::Literal("float".to_string())),
            },
        );

        let result = propagator.trace_assignment_chain("x", "file.rs", 10, &assignments);
        assert!(result.is_some());
        assert_eq!(result.unwrap().type_name, "float");
    }

    #[test]
    fn test_trace_assignment_chain_conditional_both_fail() {
        let propagator = CrossFilePropagator::new();
        let mut assignments = HashMap::new();
        assignments.insert(
            "x".to_string(),
            AssignmentSource::Conditional {
                condition: "flag".to_string(),
                true_branch: Box::new(AssignmentSource::FunctionCall(
                    "nonexistent_func1".to_string(),
                )),
                false_branch: Box::new(AssignmentSource::FunctionCall(
                    "nonexistent_func2".to_string(),
                )),
            },
        );

        let result = propagator.trace_assignment_chain("x", "file.rs", 10, &assignments);
        assert!(result.is_none());
    }

    #[test]
    fn test_trace_assignment_chain_nested_conditional() {
        let propagator = CrossFilePropagator::new();
        let mut ctx = ScopedTypeContext::new(Language::Python);
        ctx.add_return_type(
            EntityId(1),
            TypeBinding {
                type_name: "bool".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );
        let entities = vec![Entity::new(
            EntityId(1),
            EntityKind::Function,
            "check".to_string(),
            dummy_span(),
        )];
        propagator.insert_file("file.py", &ctx, &entities);

        let mut assignments = HashMap::new();
        assignments.insert(
            "x".to_string(),
            AssignmentSource::Conditional {
                condition: "a".to_string(),
                true_branch: Box::new(AssignmentSource::Conditional {
                    condition: "b".to_string(),
                    true_branch: Box::new(AssignmentSource::FunctionCall("check".to_string())),
                    false_branch: Box::new(AssignmentSource::Literal("int".to_string())),
                }),
                false_branch: Box::new(AssignmentSource::Literal("String".to_string())),
            },
        );

        let result = propagator.trace_assignment_chain("x", "file.py", 10, &assignments);
        assert!(result.is_some());
        assert_eq!(result.unwrap().type_name, "bool");
    }

    // ==================== trace_assignment_chain Destructuring tests ====================

    #[test]
    fn test_trace_assignment_chain_destructuring_with_function_call() {
        let propagator = CrossFilePropagator::new();
        let mut ctx = ScopedTypeContext::new(Language::Python);
        ctx.add_return_type(
            EntityId(1),
            TypeBinding {
                type_name: "tuple".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: Some(TypeShape::Generic {
                    base: "tuple".to_string(),
                    args: vec![
                        TypeShape::Named("String".to_string()),
                        TypeShape::Named("int".to_string()),
                    ],
                }),
            },
        );
        let entities = vec![Entity::new(
            EntityId(1),
            EntityKind::Function,
            "get_pair".to_string(),
            dummy_span(),
        )];
        propagator.insert_file("file.py", &ctx, &entities);

        let mut assignments = HashMap::new();
        assignments.insert(
            "x".to_string(),
            AssignmentSource::Destructuring {
                fields: vec!["a".to_string(), "b".to_string()],
                source: Box::new(AssignmentSource::FunctionCall("get_pair".to_string())),
            },
        );

        let result = propagator.trace_assignment_chain("x", "file.py", 10, &assignments);
        assert!(result.is_some());
        assert_eq!(result.unwrap().type_name, "tuple");
    }

    #[test]
    fn test_trace_assignment_chain_destructuring_with_literal() {
        let propagator = CrossFilePropagator::new();
        let mut assignments = HashMap::new();
        assignments.insert(
            "x".to_string(),
            AssignmentSource::Destructuring {
                fields: vec!["key".to_string(), "value".to_string()],
                source: Box::new(AssignmentSource::Literal("dict".to_string())),
            },
        );

        let result = propagator.trace_assignment_chain("x", "file.py", 10, &assignments);
        assert!(result.is_some());
        assert_eq!(result.unwrap().type_name, "dict");
    }

    #[test]
    fn test_trace_assignment_chain_destructuring_empty_fields() {
        let propagator = CrossFilePropagator::new();
        let mut ctx = ScopedTypeContext::new(Language::Python);
        ctx.add_return_type(
            EntityId(1),
            TypeBinding {
                type_name: "list".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );
        let entities = vec![Entity::new(
            EntityId(1),
            EntityKind::Function,
            "get_items".to_string(),
            dummy_span(),
        )];
        propagator.insert_file("file.py", &ctx, &entities);

        let mut assignments = HashMap::new();
        assignments.insert(
            "x".to_string(),
            AssignmentSource::Destructuring {
                fields: vec![],
                source: Box::new(AssignmentSource::FunctionCall("get_items".to_string())),
            },
        );

        let result = propagator.trace_assignment_chain("x", "file.py", 10, &assignments);
        assert!(result.is_some());
        assert_eq!(result.unwrap().type_name, "list");
    }

    // ==================== trace_assignment_chain_source tests ====================

    #[test]
    fn test_trace_assignment_chain_source_function_call() {
        let propagator = CrossFilePropagator::new();
        let mut ctx = ScopedTypeContext::new(Language::Java);
        ctx.add_return_type(
            EntityId(1),
            TypeBinding {
                type_name: "MyClass".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );
        let entities = vec![Entity::new(
            EntityId(1),
            EntityKind::Function,
            "create".to_string(),
            dummy_span(),
        )];
        propagator.insert_file("file.java", &ctx, &entities);

        let assignments = HashMap::new();
        let source = AssignmentSource::FunctionCall("create".to_string());
        let result =
            propagator.trace_assignment_chain_source(&source, "file.java", 10, &assignments);
        assert!(result.is_some());
        assert_eq!(result.unwrap().type_name, "MyClass");
    }

    #[test]
    fn test_trace_assignment_chain_source_member_access() {
        let propagator = CrossFilePropagator::new();
        let mut ctx = ScopedTypeContext::new(Language::Java);
        ctx.add_variable_type(
            "obj".to_string(),
            TypeBinding {
                type_name: "MyClass".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );
        ctx.add_variable_type(
            "MyClass::field".to_string(),
            TypeBinding {
                type_name: "String".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );
        let entities = vec![Entity::new(
            EntityId(1),
            EntityKind::Field,
            "MyClass::field".to_string(),
            dummy_span(),
        )];
        propagator.insert_file("file.java", &ctx, &entities);

        let mut assignments = HashMap::new();
        assignments.insert(
            "obj".to_string(),
            AssignmentSource::FunctionCall("create_obj".to_string()),
        );

        let source =
            AssignmentSource::MemberAccess("obj".to_string(), "MyClass::field".to_string());
        let result =
            propagator.trace_assignment_chain_source(&source, "file.java", 10, &assignments);
        assert!(result.is_some());
        assert_eq!(result.unwrap().type_name, "String");
    }

    #[test]
    fn test_trace_assignment_chain_source_literal() {
        let propagator = CrossFilePropagator::new();
        let assignments = HashMap::new();
        let source = AssignmentSource::Literal("i32".to_string());
        let result = propagator.trace_assignment_chain_source(&source, "file.rs", 10, &assignments);
        assert!(result.is_some());
        assert_eq!(result.unwrap().type_name, "i32");
    }

    #[test]
    fn test_trace_assignment_chain_source_conditional() {
        let propagator = CrossFilePropagator::new();
        let mut ctx = ScopedTypeContext::new(Language::Python);
        ctx.add_return_type(
            EntityId(1),
            TypeBinding {
                type_name: "String".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );
        let entities = vec![Entity::new(
            EntityId(1),
            EntityKind::Function,
            "get_str".to_string(),
            dummy_span(),
        )];
        propagator.insert_file("file.py", &ctx, &entities);

        let assignments = HashMap::new();
        let source = AssignmentSource::Conditional {
            condition: "cond".to_string(),
            true_branch: Box::new(AssignmentSource::FunctionCall("get_str".to_string())),
            false_branch: Box::new(AssignmentSource::Literal("int".to_string())),
        };
        let result = propagator.trace_assignment_chain_source(&source, "file.py", 10, &assignments);
        assert!(result.is_some());
        assert_eq!(result.unwrap().type_name, "String");
    }

    #[test]
    fn test_trace_assignment_chain_source_destructuring() {
        let propagator = CrossFilePropagator::new();
        let mut ctx = ScopedTypeContext::new(Language::Python);
        ctx.add_return_type(
            EntityId(1),
            TypeBinding {
                type_name: "pair".to_string(),
                type_entity_id: None,
                span: dummy_span(),
                origin: None,
                shape: None,
            },
        );
        let entities = vec![Entity::new(
            EntityId(1),
            EntityKind::Function,
            "make_pair".to_string(),
            dummy_span(),
        )];
        propagator.insert_file("file.py", &ctx, &entities);

        let assignments = HashMap::new();
        let source = AssignmentSource::Destructuring {
            fields: vec!["a".to_string(), "b".to_string()],
            source: Box::new(AssignmentSource::FunctionCall("make_pair".to_string())),
        };
        let result = propagator.trace_assignment_chain_source(&source, "file.py", 10, &assignments);
        assert!(result.is_some());
        assert_eq!(result.unwrap().type_name, "pair");
    }

    // ==================== AssignmentSource equality tests ====================

    #[test]
    fn test_assignment_source_equality() {
        assert_eq!(
            AssignmentSource::FunctionCall("foo".to_string()),
            AssignmentSource::FunctionCall("foo".to_string())
        );
        assert_ne!(
            AssignmentSource::FunctionCall("foo".to_string()),
            AssignmentSource::FunctionCall("bar".to_string())
        );
        assert_eq!(
            AssignmentSource::Literal("int".to_string()),
            AssignmentSource::Literal("int".to_string())
        );
        assert_eq!(
            AssignmentSource::MemberAccess("obj".to_string(), "field".to_string()),
            AssignmentSource::MemberAccess("obj".to_string(), "field".to_string())
        );
    }

    #[test]
    fn test_assignment_source_conditional_equality() {
        let source1 = AssignmentSource::Conditional {
            condition: "x".to_string(),
            true_branch: Box::new(AssignmentSource::Literal("int".to_string())),
            false_branch: Box::new(AssignmentSource::Literal("String".to_string())),
        };
        let source2 = AssignmentSource::Conditional {
            condition: "x".to_string(),
            true_branch: Box::new(AssignmentSource::Literal("int".to_string())),
            false_branch: Box::new(AssignmentSource::Literal("String".to_string())),
        };
        assert_eq!(source1, source2);

        let source3 = AssignmentSource::Conditional {
            condition: "y".to_string(),
            true_branch: Box::new(AssignmentSource::Literal("int".to_string())),
            false_branch: Box::new(AssignmentSource::Literal("String".to_string())),
        };
        assert_ne!(source1, source3);
    }

    #[test]
    fn test_assignment_source_destructuring_equality() {
        let source1 = AssignmentSource::Destructuring {
            fields: vec!["a".to_string(), "b".to_string()],
            source: Box::new(AssignmentSource::Literal("tuple".to_string())),
        };
        let source2 = AssignmentSource::Destructuring {
            fields: vec!["a".to_string(), "b".to_string()],
            source: Box::new(AssignmentSource::Literal("tuple".to_string())),
        };
        assert_eq!(source1, source2);

        let source3 = AssignmentSource::Destructuring {
            fields: vec!["x".to_string()],
            source: Box::new(AssignmentSource::Literal("tuple".to_string())),
        };
        assert_ne!(source1, source3);
    }
}
