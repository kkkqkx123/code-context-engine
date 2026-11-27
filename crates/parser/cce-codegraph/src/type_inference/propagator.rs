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

use std::collections::{BTreeSet, HashMap};

use cce_types::entity::{Entity, EntityId};
use cce_types::language::Language;
use cce_types::normalize_project_path;
use dashmap::DashMap;

use super::types::{
    ScopedTypeContext, TypeBinding, TypeShape, binding_supersedes, bindings_supersede,
    parse_type_shape,
};

/// Distinct call-site shapes that share one overload-resolution answer.
///
/// The cache is keyed by the resolved shapes, so the context-dependent
/// argument inference runs once while repeated calls with the same shapes
/// reuse the ranked result.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct OverloadResolutionKey {
    name: String,
    language: Language,
    arg_shapes: Vec<Option<TypeShape>>,
}

const OVERLOAD_CACHE_LIMIT: usize = 16_384;

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
    /// Overload members grouped by name and declared parameter shape.
    /// Call-site resolution only visits buckets whose arity accepts the
    /// call, so a popular name does not force a scan of every same-name
    /// function in the project.
    name_to_ids: DashMap<(String, super::overload::ParamArity), BTreeSet<EntityId>>,
    /// Distinct declared arities per name, kept small so a lookup can
    /// select compatible buckets without walking candidate ids.
    name_arities: DashMap<String, Vec<super::overload::ParamArity>>,
    /// Bounded results of overload ranking for repeated call-site shapes.
    overload_cache: DashMap<OverloadResolutionKey, Option<TypeBinding>>,
    /// Global cache: function EntityId -> parameter type bindings.
    param_type_cache: DashMap<EntityId, Vec<TypeBinding>>,
    /// Per-file index for parameter types.
    file_param_index: DashMap<String, Vec<(EntityId, Vec<TypeBinding>)>>,
    /// Name index for parameter types: function name -> param bindings.
    param_name_index: DashMap<String, Vec<TypeBinding>>,
    /// Reverse param name mapping.
    param_id_to_name: DashMap<EntityId, String>,
    /// Global cache: function EntityId -> extraction-time arity record.
    /// Copied alongside `param_type_cache` so overload resolution judges
    /// call arity from declared parameters, not just typed bindings.
    param_arity_cache: DashMap<EntityId, super::overload::ParamArity>,
    /// Global cache: field/property EntityId -> type binding.
    field_type_cache: DashMap<EntityId, TypeBinding>,
    /// Per-file index for field types.
    file_field_index: DashMap<String, Vec<(EntityId, TypeBinding)>>,
    /// Name index for field types: field name -> type binding.
    field_name_index: DashMap<String, TypeBinding>,
    /// Reverse field name mapping.
    field_id_to_name: DashMap<EntityId, String>,
}

impl CrossFilePropagator {
    /// Create a new empty propagator.
    pub fn new() -> Self {
        Self::default()
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

    /// Insert return types from a single file's type context.
    ///
    /// Only `High` and `Medium` confidence bindings are cached. The file's
    /// previous entries are removed first so the call is idempotent.
    pub fn insert_file(&self, file_path: &str, ctx: &ScopedTypeContext, entities: &[Entity]) {
        let normalized = normalize_project_path(file_path);
        self.remove_file(&normalized);
        self.overload_cache.clear();

        let name_map: HashMap<EntityId, String> =
            entities.iter().map(|e| (e.id, e.name.clone())).collect();

        // Arity is part of the overload bucket key, so it must be available
        // while return types register their candidate ids.
        for (entity_id, arity) in ctx.param_arity_iter() {
            self.param_arity_cache.insert(*entity_id, *arity);
        }

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

            self.id_to_name.insert(*entity_id, func_name.clone());
            let arity = self
                .param_arity_cache
                .get(entity_id)
                .map(|entry| *entry)
                .unwrap_or_default();
            {
                let mut arities = self.name_arities.entry(func_name.clone()).or_default();
                if !arities.contains(&arity) {
                    arities.push(arity);
                }
            }
            self.name_to_ids
                .entry((func_name.clone(), arity))
                .or_default()
                .insert(*entity_id);
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
        self.overload_cache.clear();
        let mut needs_rebuild_return = false;
        if let Some((_, entries)) = self.file_return_index.remove(&normalized) {
            for (entity_id, _) in entries {
                self.return_type_cache.remove(&entity_id);
                if let Some((_, name)) = self.id_to_name.remove(&entity_id)
                    && let Some(arity) = self.param_arity_cache.get(&entity_id).map(|a| *a)
                {
                    let bucket = (name.clone(), arity);
                    if let Some(mut ids) = self.name_to_ids.get_mut(&bucket)
                        && ids.remove(&entity_id)
                        && ids.is_empty()
                    {
                        drop(ids);
                        self.name_to_ids.remove(&bucket);
                        if let Some(mut arities) = self.name_arities.get_mut(&name) {
                            arities.retain(|candidate| *candidate != arity);
                        }
                    }
                }
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
                self.param_arity_cache.remove(&entity_id);
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

    /// Resolve a cross-file call target with overload awareness.
    ///
    /// When several same-name functions are cached (overloads, possibly
    /// across files) and at least one call-site argument shape is known,
    /// rank the candidates by arity and structural assignability and
    /// return the winner's return binding. Returns `None` for a single
    /// candidate, unknown argument shapes, or no feasible match so
    /// callers keep their legacy lookup paths unchanged.
    pub fn resolve_overload_by_name<F>(
        &self,
        name: &str,
        arg_exprs: &[String],
        language: cce_types::language::Language,
        resolve_arg: &mut F,
    ) -> Option<TypeBinding>
    where
        F: FnMut(&str) -> Option<TypeShape>,
    {
        use super::overload::{OverloadCandidate, OverloadSet, compute_specificity};
        let arities = self.name_arities.get(name)?.clone();
        let actual_shapes: Vec<Option<TypeShape>> =
            arg_exprs.iter().map(|arg| resolve_arg(arg)).collect();
        if !actual_shapes.iter().any(Option::is_some) {
            return None;
        }
        let cache_key = OverloadResolutionKey {
            name: name.to_string(),
            language,
            arg_shapes: actual_shapes.clone(),
        };
        if let Some(cached) = self.overload_cache.get(&cache_key) {
            return cached.clone();
        }
        let mut candidates = Vec::new();
        for arity in arities {
            if !arity.accepts(arg_exprs.len()) {
                continue;
            }
            let Some(ids) = self.name_to_ids.get(&(name.to_string(), arity)) else {
                continue;
            };
            for entity_id in ids.iter() {
                let Some(return_binding) = self.return_type_cache.get(entity_id) else {
                    continue;
                };
                let parameter_types: Vec<TypeShape> = self
                    .param_type_cache
                    .get(entity_id)
                    .map(|params| {
                        params
                            .iter()
                            .map(|param| {
                                param.shape.clone().unwrap_or_else(|| {
                                    parse_type_shape(&param.type_name, language)
                                        .unwrap_or(TypeShape::Named(param.type_name.clone()))
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let return_type = return_binding.shape.clone().unwrap_or_else(|| {
                    parse_type_shape(&return_binding.type_name, language)
                        .unwrap_or(TypeShape::Named(return_binding.type_name.clone()))
                });
                let specificity = parameter_types.iter().map(compute_specificity).sum();
                candidates.push(OverloadCandidate {
                    entity_id: *entity_id,
                    parameter_types,
                    return_type,
                    specificity,
                    arity,
                });
            }
        }
        if candidates.len() < 2 {
            return None;
        }
        // Rank once per call-site shape instead of sorting the candidate
        // vector after every insertion.
        candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.specificity));
        let set = OverloadSet {
            name: name.to_string(),
            owner_type: String::new(),
            candidates,
        };
        let arg_refs: Vec<Option<&TypeShape>> =
            actual_shapes.iter().map(|shape| shape.as_ref()).collect();
        let result = set.resolve(&arg_refs).and_then(|winner| {
            self.return_type_cache
                .get(&winner.entity_id)
                .map(|binding| binding.clone())
        });
        if self.overload_cache.len() >= OVERLOAD_CACHE_LIMIT {
            self.overload_cache.clear();
        }
        self.overload_cache.insert(cache_key, result.clone());
        result
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

    /// Drop per-file invalidation indexes after a bulk build. The global
    /// return/parameter/field caches and name indexes used by resolution stay
    /// intact; only incremental invalidation state is removed.
    pub fn drop_invalidation_indexes(&self) {
        self.file_return_index.clear();
        self.id_to_name.clear();
        self.file_param_index.clear();
        self.param_id_to_name.clear();
        self.file_field_index.clear();
        self.field_id_to_name.clear();
    }

    /// Clear all caches.
    pub fn clear(&self) {
        self.return_type_cache.clear();
        self.file_return_index.clear();
        self.name_index.clear();
        self.id_to_name.clear();
        self.name_to_ids.clear();
        self.name_arities.clear();
        self.overload_cache.clear();
        self.param_type_cache.clear();
        self.file_param_index.clear();
        self.param_name_index.clear();
        self.param_id_to_name.clear();
        self.param_arity_cache.clear();
        self.field_type_cache.clear();
        self.file_field_index.clear();
        self.field_name_index.clear();
        self.field_id_to_name.clear();
    }

    /// Number of cached return types.
    pub fn len(&self) -> usize {
        self.return_type_cache.len()
    }

    /// Total number of cached entries (return + param + field).
    pub fn total_len(&self) -> usize {
        self.return_type_cache.len() + self.param_type_cache.len() + self.field_type_cache.len()
    }

    /// Check if empty (all caches empty).
    pub fn is_empty(&self) -> bool {
        self.return_type_cache.is_empty()
            && self.param_type_cache.is_empty()
            && self.param_arity_cache.is_empty()
            && self.field_type_cache.is_empty()
    }

    /// Rebuild from all per-file contexts.
    pub fn rebuild_from_contexts(&self, file_contexts: Vec<(&str, &ScopedTypeContext, &[Entity])>) {
        self.clear();
        for (path, ctx, entities) in file_contexts {
            self.insert_file(path, ctx, entities);
        }
    }
}

#[cfg(test)]
mod tests {
    use cce_types::Span;
    use cce_types::entity::{Entity, EntityId, EntityKind};

    use super::*;
    use crate::type_inference::overload::ParamArity;

    fn named_binding(name: &str) -> TypeBinding {
        TypeBinding {
            type_name: name.to_string(),
            type_entity_id: None,
            span: Span::default(),
            origin: None,
            shape: Some(TypeShape::Named(name.to_string())),
        }
    }

    #[test]
    fn overload_resolution_visits_only_compatible_arity_bucket() {
        let propagator = CrossFilePropagator::new();
        let mut ctx = ScopedTypeContext::new(Language::Rust);
        ctx.add_return_type(EntityId(1), named_binding("i32"));
        ctx.add_parameter_types(EntityId(1), vec![named_binding("i32")]);
        ctx.set_param_arity(EntityId(1), ParamArity::exact(1));
        ctx.add_return_type(EntityId(2), named_binding("String"));
        ctx.add_parameter_types(EntityId(2), vec![named_binding("String")]);
        ctx.set_param_arity(EntityId(2), ParamArity::exact(1));
        ctx.add_return_type(EntityId(3), named_binding("TwoArgs"));
        ctx.add_parameter_types(
            EntityId(3),
            vec![named_binding("i32"), named_binding("i32")],
        );
        ctx.set_param_arity(EntityId(3), ParamArity::exact(2));
        let entities = [1, 2, 3]
            .into_iter()
            .map(|id| {
                Entity::new(
                    EntityId(id),
                    EntityKind::Function,
                    "pick".into(),
                    Span::default(),
                )
            })
            .collect::<Vec<_>>();
        propagator.insert_file("a.rs", &ctx, &entities);

        assert_eq!(
            propagator
                .name_to_ids
                .get(&("pick".into(), ParamArity::exact(1)))
                .map(|ids| ids.len()),
            Some(2)
        );
        assert_eq!(
            propagator
                .name_to_ids
                .get(&("pick".into(), ParamArity::exact(2)))
                .map(|ids| ids.len()),
            Some(1)
        );
        let resolved = propagator
            .resolve_overload_by_name("pick", &["1".into()], Language::Rust, &mut |_| {
                Some(TypeShape::Named("i32".into()))
            })
            .expect("one-argument call must resolve");
        assert_eq!(resolved.type_name, "i32");
    }

    #[test]
    fn overload_resolution_cache_is_invalidated_by_file_removal() {
        let propagator = CrossFilePropagator::new();
        let mut ctx = ScopedTypeContext::new(Language::Rust);
        ctx.add_return_type(EntityId(1), named_binding("i32"));
        ctx.add_return_type(EntityId(2), named_binding("String"));
        ctx.add_parameter_types(EntityId(1), vec![named_binding("i32")]);
        ctx.set_param_arity(EntityId(1), ParamArity::exact(1));
        ctx.add_parameter_types(EntityId(2), vec![named_binding("String")]);
        ctx.set_param_arity(EntityId(2), ParamArity::exact(1));
        let entities = vec![
            Entity::new(
                EntityId(1),
                EntityKind::Function,
                "pick".into(),
                Span::default(),
            ),
            Entity::new(
                EntityId(2),
                EntityKind::Function,
                "pick".into(),
                Span::default(),
            ),
        ];
        propagator.insert_file("a.rs", &ctx, &entities);
        let resolve = || {
            propagator.resolve_overload_by_name("pick", &["1".into()], Language::Rust, &mut |_| {
                Some(TypeShape::Named("i32".into()))
            })
        };
        assert!(resolve().is_some());

        propagator.remove_file("a.rs");
        assert!(resolve().is_none());
    }
}
