//! Relation resolver for converting raw relations to resolved relations
//!
//! Provides functionality to resolve raw relations by looking up symbols in
//! a global symbol table and classifying external calls (standard library,
//! external packages, or unknown).

use super::core::{RelationEdgeIdentity, RelationIndex, relation_identity};
use super::dependency_index::DependencyIndex;
use crate::config_parser::UntypedDependency;
use crate::index::EntityIndexOps;
use crate::symbol::SymbolRef;
use crate::symbol_table::ProjectSymbolTable;
use crate::symbol_table::ResolutionContext;
use crate::symbol_table::project::OverloadContext;
use crate::type_inference::types::{BranchPolarity, TypeShape, parse_type_shape};
use cce_metrics::domain::pipeline::RelationMetrics;
use cce_types::entity::meta_keys::CFG_PREDICATE;
use cce_types::entity::{Entity, EntityId};
use cce_types::relation::{CallContext, ExternalCallType};
use cce_types::{
    ControlFlowFactKind, ParsedFile, RawRelationData, ResolvedRelation, language::Language,
};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

mod post_processor;
pub use post_processor::{RelationFilterConfig, RelationPostProcessor, global_post_processor};

/// Relation resolver
///
/// Converts raw relations to resolved relations by looking up symbols in a
/// global symbol table and classifying external calls.
pub struct RelationResolver {
    /// Whether to filter out standard library calls
    ///
    /// filtering applies only to relations whose callee did NOT resolve
    /// to an internal entity; internally-resolved callees are kept regardless
    /// of their name.
    filter_stdlib_calls: bool,
    /// Number of standard library calls filtered out
    filtered_count: AtomicUsize,
    /// External packages for import classification (language -> package names)
    external_packages: Option<HashMap<Language, HashSet<String>>>,
    /// Full dependency information for enhanced classification (language -> dependencies)
    external_dependencies: Option<HashMap<Language, Vec<UntypedDependency>>>,
    /// Dependency index for efficient lookup
    dependency_index: Option<DependencyIndex>,
    /// Relation metrics for stdlib preservation/filter accounting
    metrics: Option<Arc<RelationMetrics>>,
    /// Total resolution calls (one per raw relation processed)
    resolve_calls: AtomicUsize,
    /// Total symbol-table resolution lookups across all resolution attempts
    resolve_lookups: AtomicUsize,
    /// Optional custom post-processor for relation filtering.
    ///
    /// When `None`, the global default processor is used. Injecting a custom
    /// processor allows deterministic, configuration-driven filtering without
    /// scattered heuristics.
    post_processor: Option<RelationPostProcessor>,
}

impl RelationResolver {
    /// Create a new relation resolver with default settings
    ///
    /// By default, standard library calls are filtered out.
    pub fn new() -> Self {
        Self {
            filter_stdlib_calls: true,
            filtered_count: AtomicUsize::new(0),
            external_packages: None,
            external_dependencies: None,
            dependency_index: None,
            metrics: None,
            resolve_calls: AtomicUsize::new(0),
            resolve_lookups: AtomicUsize::new(0),
            post_processor: None,
        }
    }

    /// Inject a custom relation post-processor for filtering.
    pub fn with_post_processor(&mut self, processor: RelationPostProcessor) -> &mut Self {
        self.post_processor = Some(processor);
        self
    }

    /// Inject a custom filter config, constructing a post-processor from it.
    pub fn with_filter_config(&mut self, config: RelationFilterConfig) -> &mut Self {
        self.post_processor = Some(RelationPostProcessor::new(config));
        self
    }

    fn effective_post_processor(&self) -> &RelationPostProcessor {
        self.post_processor
            .as_ref()
            .unwrap_or_else(|| global_post_processor())
    }

    /// Total resolution calls processed
    pub fn resolve_calls(&self) -> usize {
        self.resolve_calls
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Total symbol-table resolution lookups performed
    pub fn resolve_lookups(&self) -> usize {
        self.resolve_lookups
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Reset per-build resolution counters
    pub fn reset_resolve_counters(&self) {
        self.resolve_calls
            .store(0, std::sync::atomic::Ordering::Relaxed);
        self.resolve_lookups
            .store(0, std::sync::atomic::Ordering::Relaxed);
    }

    /// Attach relation metrics for stdlib preservation/filter accounting.
    pub fn with_metrics(&mut self, metrics: Option<Arc<RelationMetrics>>) -> &mut Self {
        self.metrics = metrics;
        self
    }

    /// Set whether to filter out standard library calls
    pub fn with_filter(&mut self, filter: bool) -> &mut Self {
        self.filter_stdlib_calls = filter;
        self
    }

    /// Set external packages for import classification
    pub fn with_external_packages(
        &mut self,
        packages: HashMap<Language, HashSet<String>>,
    ) -> &mut Self {
        self.external_packages = Some(packages);
        self
    }

    /// Set full dependency information for enhanced classification
    pub fn with_external_dependencies(
        &mut self,
        dependencies: HashMap<Language, Vec<UntypedDependency>>,
    ) -> &mut Self {
        // Build index for efficient lookup
        self.dependency_index = Some(DependencyIndex::build(&dependencies));
        self.external_dependencies = Some(dependencies);
        self
    }

    /// Get the number of standard library calls filtered out
    pub fn filtered_count(&self) -> usize {
        self.filtered_count
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Reset the filtered count
    pub fn reset_filtered_count(&self) {
        self.filtered_count
            .store(0, std::sync::atomic::Ordering::Relaxed);
    }

    /// Build scope chain from entity parent chain.
    ///
    /// Returns an ordered list from outermost (root) to innermost (the entity).
    /// Used for scope-aware resolution to correctly handle name shadowing.
    ///
    /// `entity_map` is a parent-lookup map built once per file; callers that
    /// resolve many relations from the same file must reuse a single map instead
    /// of calling `build_scope_chain` per relation.
    #[allow(dead_code)]
    fn build_scope_chain_from_map(
        caller_id: EntityId,
        entity_map: &HashMap<EntityId, &Entity>,
    ) -> Vec<EntityId> {
        Self::build_scope_chain_from_map_with_limit(caller_id, entity_map, 100)
    }

    fn build_scope_chain_from_map_with_limit(
        caller_id: EntityId,
        entity_map: &HashMap<EntityId, &Entity>,
        max_depth: usize,
    ) -> Vec<EntityId> {
        let mut chain: Vec<EntityId> = Vec::new();
        let mut current = Some(caller_id);

        while let Some(id) = current {
            chain.push(id);
            if chain.len() > max_depth {
                break;
            }
            current = entity_map.get(&id).and_then(|e| e.parent);
        }

        chain.reverse();
        chain
    }

    /// Whether `name` is bound as a parameter of any entity in the caller's
    /// scope chain (innermost to outermost entities all count: a method, its
    /// enclosing functions, etc.).
    fn is_bound_in_scope_chain(
        name: &str,
        scope_chain: &[EntityId],
        entity_map: &HashMap<EntityId, &Entity>,
    ) -> bool {
        scope_chain.iter().any(|id| {
            entity_map
                .get(id)
                .is_some_and(|entity| entity.parameters.iter().any(|(param, _)| param == name))
        })
    }

    /// Resolve a single raw relation
    ///
    /// Attempts to resolve the callee in the global symbol table and determines
    /// if this is an external reference (standard library, external package, or unknown).
    ///
    /// # Resolution Order
    /// 1. Check if callee is a standard library entity (optional filtering)
    /// 2. Try fully qualified name (file::callee_name)
    /// 3. Fallback to simple name (simple:callee_name)
    /// 4. Fallback to local symbols
    ///
    /// # Arguments
    ///
    /// * `raw_data` - The raw relation data to resolve
    /// * `parsed` - The parsed file containing the relation
    /// * `symbol_table` - The global symbol table for cross-file resolution
    ///
    /// # Returns
    ///
    /// * `Some(ResolvedRelation)` - If relation is not filtered
    /// * `None` - If standard library filtering is enabled and callee is in stdlib
    pub fn resolve(
        &self,
        raw_data: &RawRelationData,
        parsed: &ParsedFile,
        symbol_table: &ProjectSymbolTable,
        entity_index: &RelationIndex,
    ) -> Option<ResolvedRelation> {
        // Single-entry entry point: exists to serve existing tests. Production
        // callers must go through `resolve_batch`/`resolve_with_scope_map` so
        // the per-file scope map is built once and reused across relations.
        self.resolve_batch(
            std::slice::from_ref(raw_data),
            parsed,
            symbol_table,
            entity_index,
        )
        .into_iter()
        .next()
    }

    /// Resolve a raw relation against a prebuilt per-file scope map.
    ///
    /// Callers resolving many relations from the same file should build the
    /// entity map once and reuse it here to avoid O(E) map construction per
    /// relation.
    ///
    /// # Resolution Order
    ///
    /// Symbol resolution runs FIRST; the stdlib identity is decided only
    /// AFTER resolution:
    /// 1. Try to resolve the callee to an internal entity (local scope ->
    ///    symbol table -> entity index). A resolved internal target is kept
    ///    regardless of whether the name looks like a standard library name
    ///    (a project may legitimately define `print`/`len`).
    /// 2. Only when resolution fails AND the name is detected as stdlib:
    ///    - `filter_stdlib_calls=false` keeps the edge as an external
    ///      `StandardLibrary` relation (and records `stdlib_preserved_external`);
    ///    - `filter_stdlib_calls=true` drops the edge (and records
    ///      `stdlib_filtered`).
    /// 3. Unresolved, non-stdlib callees go through external package
    ///    classification.
    pub fn resolve_with_scope_map(
        &self,
        raw_data: &RawRelationData,
        parsed: &ParsedFile,
        symbol_table: &ProjectSymbolTable,
        entity_index: &RelationIndex,
        entity_map: &HashMap<EntityId, &Entity>,
    ) -> Option<ResolvedRelation> {
        // Determine if this is a stdlib call.
        //
        // # Design: Single Detection Point (at relation extraction)
        //
        // Standard library detection happens once during relation extraction
        // (in relation_extractor.rs) and is stored in RawRelationData.stdlib_category.
        // This is the authoritative source. There is intentionally no
        // fallback detection here: the global classifier hook was never
        // registered by any caller, so any fallback would be dead code that
        // only masks missing classification at extraction time.
        //
        // See STDLIB_SUMMARY.md for detailed analysis of stdlib handling.
        //
        // detection no longer causes an early return; the stdlib identity
        // is applied only after symbol resolution.
        let is_stdlib = raw_data.stdlib_category.is_some();

        self.resolve_calls
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if let Some(metrics) = &self.metrics {
            metrics.resolve_calls_total.increment();
        }

        // Try to resolve callee using enhanced resolution with scope chain
        // Scope chain enables correct name shadowing resolution
        let scope_chain = Self::build_scope_chain_from_map_with_limit(
            raw_data.src,
            entity_map,
            symbol_table.max_scope_chain_depth(),
        );
        let resolution_context = ResolutionContext {
            file_path: parsed.path.clone(),
            module_path: Vec::new(),
            scope_chain,
        };

        // Import-alias redirect: a callee written under a local alias
        // (`import { foo as bar }` → `bar()`) is not registered in the
        // symbol table under that alias. Resolution first tries the literal
        // name (a local definition shadows any import alias), then retries
        // under the import's original symbol name so the call resolves to an
        // internal edge instead of a spurious Unknown edge.
        //
        // The per-file local→global ID remap is fetched once here so every
        // candidate resolution emits callee IDs in the index-global space
        // (see `resolve_name_candidate`).
        let file_remap =
            entity_index.entity_id_remap_for(&cce_types::normalize_project_path(&parsed.path));
        let simple_name = raw_data
            .dst_name
            .rsplit(['.', ':'])
            .next()
            .unwrap_or(&raw_data.dst_name);
        let has_overload = symbol_table.has_overload_candidates(simple_name);
        let is_bare = !raw_data.dst_name.contains('.') && !raw_data.dst_name.contains(':');
        // The inferred declared type outranks the syntactic receiver name:
        // `handler.Handle` with `handler: RequestHandlerWrapper<T>` must
        // compare the owner against the type, not the variable name. The raw
        // receiver name stays as the last resort for receivers inference
        // knows nothing about (`this`, `console`, untyped locals).
        let receiver_type = self
            .extract_promoted_receiver_type(&raw_data.dst_name, symbol_table)
            .or_else(|| {
                if has_overload || !is_bare {
                    self.extract_receiver_type(raw_data, parsed, symbol_table, entity_index)
                } else {
                    None
                }
            })
            .or_else(|| self.extract_receiver_from_name(&raw_data.dst_name))
            .or_else(|| {
                // `call.method` captures only the property identifier, so a
                // plain `this.get('env')` reaches resolution as bare `get`
                // with the receiver lost. Recover it from the source text
                // before the call span; only a bare name with no inferred
                // receiver takes this path, so existing evidence is never
                // overridden.
                if is_bare {
                    Self::recover_receiver_before_span(
                        parsed.source.as_ref(),
                        raw_data.span.start_byte,
                    )
                } else {
                    None
                }
            });
        let arg_types_vec = self.extract_argument_types(raw_data, parsed, symbol_table);
        let arg_count_via_parse =
            self.extract_call_argument_count(raw_data, parsed)
                .or_else(|| {
                    extract_call_arguments(parsed.source.as_ref(), raw_data.span).map(|v| v.len())
                });
        // A recovered self-like receiver must survive even the unambiguous
        // fast path: the single project-wide candidate may still live in
        // another file, which a self-rooted call can never target.
        let self_recovered = receiver_type
            .as_deref()
            .is_some_and(cce_parser_core::is_self_receiver);
        let overload_ctx = if is_bare && !has_overload && !self_recovered {
            OverloadContext {
                receiver_type: None,
                arg_count: None,
                arg_types: None,
                language: parsed.language,
            }
        } else {
            OverloadContext {
                receiver_type: receiver_type.clone(),
                language: parsed.language,
                arg_count: arg_count_via_parse.or({
                    if arg_types_vec.is_empty() {
                        None
                    } else {
                        Some(arg_types_vec.len())
                    }
                }),
                arg_types: if arg_types_vec.is_empty() {
                    None
                } else {
                    Some(arg_types_vec)
                },
            }
        };
        let candidate_ctx = NameCandidateContext {
            parsed,
            symbol_table,
            entity_index,
            resolution_context: &resolution_context,
            file_remap: file_remap.as_ref(),
            overload_ctx: Some(&overload_ctx),
        };
        // Local-binding termination: a bare callee name bound as a parameter
        // (or an implicit receiver like Python's `cls`/`self`) anywhere in
        // the caller's scope chain is a local binding. Such a name must never
        // fall through to project-wide symbol resolution, where an unrelated
        // same-name symbol in another file could hijack the edge.
        let locally_bound = is_bare
            && raw_data.relation_type.is_call()
            && Self::is_bound_in_scope_chain(
                &raw_data.dst_name,
                &resolution_context.scope_chain,
                entity_map,
            );
        let mut symbol_ref: Option<SymbolRef> = None;
        let mut resolved_entity_id: Option<EntityId> = None;
        let promoted_name = self.promoted_candidate_name(&raw_data.dst_name, symbol_table);
        let embedded_name = self.embedded_promoted_candidate_name(
            &raw_data.dst_name,
            receiver_type.as_deref(),
            entity_index,
            symbol_table,
        );
        let mut via_promoted = false;
        if !locally_bound {
            for name in self.ordered_resolution_names(
                &raw_data.dst_name,
                parsed,
                symbol_table,
                entity_index,
                receiver_type.as_deref(),
            ) {
                let (candidate_ref, candidate_id) =
                    self.resolve_name_candidate(&name, is_stdlib, &candidate_ctx);
                if candidate_id.is_some() {
                    via_promoted = promoted_name.as_deref().is_some_and(|p| p == name.as_str())
                        || embedded_name.as_deref().is_some_and(|p| p == name.as_str());
                    symbol_ref = candidate_ref;
                    resolved_entity_id = candidate_id;
                    break;
                }
                // Keep the closest symbol ref so the snapshot still records a
                // best-effort symbol even when every candidate fails.
                symbol_ref = symbol_ref.or(candidate_ref);
            }
        }

        // Unified post-processing filter for Rust `clone`/`clone_from` on
        // generic receivers. All string heuristics are centralized in
        // `RelationPostProcessor` so the resolver stays deterministic and
        // free of scattered `rsplit`/`split` logic.
        if raw_data.relation_type.is_call()
            && resolved_entity_id.is_some()
            && self
                .effective_post_processor()
                .should_filter_rust_clone_auto(
                    &raw_data.dst_name,
                    parsed.language,
                    parsed,
                    symbol_table,
                    receiver_type.as_deref(),
                    true,
                )
        {
            resolved_entity_id = None;
            symbol_ref = None;
        }

        let callee_symbol = symbol_ref
            .as_ref()
            .map(|symbol_ref| self.snapshot_symbol(symbol_ref, resolved_entity_id));
        let callee_id = resolved_entity_id;

        // Determine if this is an external reference
        let is_external = callee_id.is_none();

        // Unified post-processing filter for debug/log/macros on unresolved
        // calls (non-stdlib only). Stdlib calls are handled by the
        // `filter_stdlib_calls` branch below so `println` is counted as
        // `stdlib_filtered` rather than generic debug filtered.
        // Delegated to `RelationPostProcessor` for centralized control.
        if self.effective_post_processor().should_filter_relation(
            &raw_data.dst_name,
            parsed.language,
            is_external,
            is_stdlib,
            raw_data.relation_type.is_call(),
        ) {
            if let Some(metrics) = &self.metrics {
                metrics.debug_filtered.increment();
            }
            return None;
        }

        // Determine external call type if this is an external reference.
        // the stdlib decision is post-resolution — internally-resolved
        // callees are kept even when their name looks like a stdlib name.
        let mut external_type = if is_external {
            if is_stdlib {
                if self.filter_stdlib_calls {
                    self.filtered_count
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if let Some(metrics) = &self.metrics {
                        // Call and non-call stdlib edges are dropped by the
                        // same switch but have different recall consequences:
                        // a dropped `Vec::new()` is a call nobody queries,
                        // a dropped `Vec<T>` reference erases a type edge from
                        // the graph. Count them apart so the non-call volume
                        // stays auditable.
                        if raw_data.relation_type.is_call() {
                            metrics.stdlib_filtered.increment();
                        } else {
                            metrics.stdlib_filtered_non_call.increment();
                        }
                    }
                    return None;
                }
                if let Some(metrics) = &self.metrics {
                    metrics.stdlib_preserved_external.increment();
                }
                // Standard library external call
                Some(ExternalCallType::standard_library(
                    self.extract_stdlib_name(&raw_data.dst_name, &parsed.language),
                ))
            } else {
                // Use shared logic for classifying external packages
                self.classify_external_package(raw_data, parsed)
            }
        } else {
            None
        };

        if external_type
            .as_ref()
            .is_some_and(|t| matches!(t, ExternalCallType::Unknown { .. }))
        {
            if let Some(metrics) = &self.metrics {
                // Placeholder callees (`<closure>` / `<func_literal>`) are
                // anonymous syntax, not resolution failures: count them
                // under their own reason instead of `SymbolNotFound`.
                let is_literal_callee = raw_data.dst_name.len() > 2
                    && raw_data.dst_name.starts_with('<')
                    && raw_data.dst_name.ends_with('>');
                let reason = if is_literal_callee {
                    "closure_literal"
                } else {
                    cce_types::relation::UnresolvedReason::SymbolNotFound.as_str()
                };
                metrics.record_unresolved(reason);
            }
        }

        // For internally-resolved edges, the canonical callee name is the
        // resolved entity's registered (simple) name: call-chain queries match
        // by simple name, while the caller-written path text stays available
        // in `raw_target` and in the canonical snapshot. External/unresolved
        // edges keep the full written path so stdlib detection and external
        // classification stay intact.
        let mut callee_name = callee_id
            .and_then(|id| entity_index.get_function_by_entity_id(id))
            .map(|entity| entity.name.clone())
            .unwrap_or_else(|| raw_data.dst_name.clone());

        let mut effective_callee_id = callee_id;
        let mut overload_signature: Option<String> = None;
        // Function references name a value, not a call site, so normal
        // resolution often yields nothing when symbol coverage gaps; retry
        // directly against the entity-name index. Internal-linkage
        // candidates from other files can never be the target of a
        // cross-file reference, so they are excluded before the
        // largest-span choice. Unknown names and non-functions still
        // resolve to nothing and are dropped below.
        if effective_callee_id.is_none()
            && raw_data.relation_type == cce_types::relation::RelationType::FunctionReference
        {
            effective_callee_id = self.resolve_function_definition(
                &raw_data.dst_name,
                &parsed.path,
                entity_index,
                symbol_table,
            );
        }
        // A recovered internal target flips the earlier external verdict:
        // classification ran before the retry, so refresh the canonical
        // name and drop the Unknown marker the edge no longer deserves.
        if effective_callee_id.is_some() && callee_id.is_none() {
            if let Some(id) = effective_callee_id {
                if let Some(entity) = entity_index.get_function_by_entity_id(id) {
                    callee_name = entity.name.clone();
                }
            }
            external_type = None;
        }
        let is_external = effective_callee_id.is_none();
        // Determine owner_type and call_context using TypeMemberIndex
        let (owner_type, call_context, relation_type) = if let Some(callee_id) = effective_callee_id
        {
            // Try to find the owner type from TypeMemberIndex
            let global_type_index = symbol_table.global_type_index();
            let owner_type = global_type_index
                .owner_of(callee_id)
                .map(|key| key.qualified.clone());

            if let Some(ref owner) = owner_type {
                if let Some(overload) = symbol_table.get_overload_set(owner, &callee_name) {
                    if overload.candidates.len() > 1 {
                        let in_set = overload.candidates.iter().any(|c| c.entity_id == callee_id);
                        if !in_set {
                            let arg_types =
                                self.extract_argument_types(raw_data, parsed, symbol_table);
                            let arg_refs: Vec<Option<&TypeShape>> =
                                arg_types.iter().map(|opt| opt.as_ref()).collect();
                            let expected_return =
                                self.infer_expected_return_type(raw_data, parsed, symbol_table);
                            if let Some((entity_id, signature)) = overload
                                .resolve_with_score_signature(
                                    &arg_refs,
                                    expected_return.as_deref(),
                                    parsed.language,
                                )
                            {
                                effective_callee_id = Some(entity_id);
                                overload_signature = Some(signature);
                            } else if let Some(best) =
                                overload.resolve_with_inferred_generics(&arg_refs, parsed.language)
                            {
                                effective_callee_id = Some(best.entity_id);
                                overload_signature = Some(
                                    crate::type_inference::overload::format_overload_signature(
                                        &callee_name,
                                        best,
                                    ),
                                );
                            } else if let Some(best) = overload.resolve(&[]) {
                                effective_callee_id = Some(best.entity_id);
                                overload_signature = Some(
                                    crate::type_inference::overload::format_overload_signature(
                                        &callee_name,
                                        best,
                                    ),
                                );
                            }
                        }
                    }
                }
            }

            // Determine call context based on relation type and owner_type
            // A bare call that resolves to a nominal value type (struct,
            // class, enum, union, enum variant) is a value construction,
            // not a function call: upgrade the edge so the call context,
            // the stored edge type and downstream consumers (annotation
            // labels, call-chain queries, edge weights) see construction
            // semantics. The decision is driven by the resolved callee
            // kind, never by name spelling. The kind lookup goes through
            // the entity index (index-global ID space): the per-file
            // entity map carries caller-local IDs, which no longer match
            // once the callee ID has been translated to global space.
            let relation_type = match raw_data.relation_type {
                cce_types::relation::RelationType::DirectCall
                    if effective_callee_id
                        .and_then(|id| entity_index.get_function_by_entity_id(id))
                        .is_some_and(|callee| callee.kind.is_constructible_type()) =>
                {
                    cce_types::relation::RelationType::ConstructorCall
                }
                other => other,
            };
            // Function references survive only when tied to a real function:
            // initializer lists also hold macro constants and enum values
            // that are syntactically indistinguishable, and an unresolved or
            // non-function target carries no review value as an external edge.
            if relation_type == cce_types::relation::RelationType::FunctionReference
                && !effective_callee_id.is_some_and(|id| {
                    entity_index
                        .get_function_by_entity_id(id)
                        .is_some_and(|callee| callee.kind.is_function_like())
                })
            {
                return None;
            }
            let call_context = match relation_type {
                cce_types::relation::RelationType::InstanceMethodCall => {
                    if let Some(ref owner) = owner_type {
                        instance_call_context(owner)
                    } else {
                        // Fallback 1: try to extract receiver type from dst_name
                        // For patterns like "obj.method" or "Type::method"
                        if let Some(receiver) = self.extract_receiver_from_name(&raw_data.dst_name)
                        {
                            let method_name = raw_data
                                .dst_name
                                .rsplit(['.', ':'])
                                .next()
                                .unwrap_or(&raw_data.dst_name)
                                .trim();
                            let inferred = self
                                .lookup_inferred_receiver_type(
                                    &receiver,
                                    &parsed.path,
                                    symbol_table,
                                    parsed,
                                )
                                .or_else(|| {
                                    self.infer_receiver_via_use_site(
                                        method_name,
                                        symbol_table,
                                        parsed.language,
                                    )
                                });
                            let receiver_type = inferred.unwrap_or(receiver);
                            instance_call_context(&receiver_type)
                        } else {
                            CallContext::Direct
                        }
                    }
                }
                cce_types::relation::RelationType::StaticMethodCall => {
                    if let Some(ref owner) = owner_type {
                        CallContext::StaticMethod {
                            owner_type: owner.clone(),
                        }
                    } else {
                        // Fallback: try to extract owner type from dst_name
                        if let Some(owner) = self.extract_owner_from_static_call(&raw_data.dst_name)
                        {
                            CallContext::StaticMethod { owner_type: owner }
                        } else {
                            CallContext::Direct
                        }
                    }
                }
                cce_types::relation::RelationType::ConstructorCall => {
                    if let Some(ref owner) = owner_type {
                        CallContext::Constructor {
                            owner_type: owner.clone(),
                        }
                    } else {
                        // For constructor calls, the callee_name is the type being constructed
                        CallContext::Constructor {
                            owner_type: callee_name.clone(),
                        }
                    }
                }
                _ => CallContext::Direct,
            };

            (owner_type, call_context, relation_type)
        } else {
            // External call: no owner_type, default call_context
            (None, CallContext::Direct, raw_data.relation_type)
        };

        if let Some(effective) = effective_callee_id {
            let global_src = file_remap
                .as_ref()
                .and_then(|m| m.get(&raw_data.src))
                .copied()
                .unwrap_or(raw_data.src);
            let caller_entity_opt = entity_map.get(&raw_data.src);
            let caller_name = caller_entity_opt.map(|e| e.name.as_str()).unwrap_or("");
            let last_segment = raw_data
                .dst_name
                .rsplit(['.', ':'])
                .next()
                .unwrap_or(&raw_data.dst_name);
            let is_name_match = caller_name == callee_name && last_segment == caller_name;
            // Qualified and bare aliases of one function share a span under
            // different ids (`app.handle` vs `handle`): byte-identical,
            // available spans in the same file with the same simple name are
            // the same function, so they self-loop even when the entity ids
            // differ and the id comparison above misses. Unavailable spans
            // (synthetic nodes, unit-test fixtures) never qualify, and bare
            // names alone never imply aliasing.
            let is_same_span_alias = match (
                caller_entity_opt,
                entity_index.get_function_by_entity_id(effective),
            ) {
                (Some(caller_entity), Some(callee_entity)) => {
                    caller_entity.span.is_available()
                        && callee_entity.span.is_available()
                        && caller_entity.span.start_byte == callee_entity.span.start_byte
                        && caller_entity.span.end_byte == callee_entity.span.end_byte
                        && entity_index.get_file_path_by_entity(effective).is_some_and(
                            |callee_file| {
                                cce_types::path::normalized_equals(&callee_file, &parsed.path)
                            },
                        )
                        && caller_name
                            .rsplit(['.', ':'])
                            .next()
                            .is_some_and(|caller_simple| {
                                callee_name
                                    .rsplit(['.', ':'])
                                    .next()
                                    .is_some_and(|callee_simple| caller_simple == callee_simple)
                            })
                }
                _ => false,
            };
            let is_self_loop = effective == global_src || is_same_span_alias;
            let is_structural_self_loop = is_self_loop && relation_type.is_structural();
            let is_call_self_loop =
                (is_self_loop || (is_name_match && !via_promoted)) && relation_type.is_call();
            if is_structural_self_loop || is_call_self_loop {
                let is_explicit_self = raw_data.dst_name.starts_with("Self::")
                    || raw_data.dst_name.starts_with("Self.")
                    || raw_data.dst_name == "self"
                    || raw_data.dst_name == "Self";
                if !is_explicit_self {
                    let should_filter = if let Some(caller_entity) = caller_entity_opt {
                        let s = caller_entity.span;
                        raw_data.span.start_byte >= s.start_byte
                            && raw_data.span.end_byte <= s.end_byte
                    } else {
                        true
                    };
                    if should_filter {
                        if let Some(metrics) = &self.metrics {
                            metrics.relation_self_loop_filtered_total.increment();
                        }
                        return None;
                    }
                }
            }
        }

        // Unresolved function references carry no review value: the
        // initializer also holds constants, and an external edge to an
        // unknown name only bloats the graph. Resolved non-function
        // targets are dropped earlier; this covers the unresolved rest.
        if relation_type == cce_types::relation::RelationType::FunctionReference
            && effective_callee_id.is_none()
        {
            return None;
        }

        Some(ResolvedRelation {
            caller: raw_data.src,
            callee_id: effective_callee_id,
            callee_name,
            relation_type,
            span: raw_data.span,
            is_external,
            external_type,
            callee_symbol,
            stdlib_category: raw_data.stdlib_category,
            owner_type,
            call_context,
            overload_signature,
            call_frequency: 1,
            cfg_condition: entity_map
                .get(&raw_data.src)
                .and_then(|caller| caller.get_metadata(CFG_PREDICATE))
                .filter(|predicate| !predicate.is_empty())
                .cloned(),
        })
    }

    /// Resolve a function-reference name to its definition entity.
    ///
    /// Only function-like candidates qualify. A free function carrying the
    /// `static` storage class has internal linkage and is invisible outside
    /// its own file, so foreign static candidates are excluded: they can
    /// never be the target of a cross-file reference. Class-scoped `static`
    /// (C++ static members) keeps external linkage and is never excluded.
    /// Candidates from the caller's own file win over foreign ones; within
    /// one partition the largest span wins, since definitions carry bodies.
    /// Scoped names (`Scope::member`) additionally require a verified owner:
    /// exactly one candidate owned by the named scope resolves, anything
    /// ambiguous is dropped rather than guessed.
    fn resolve_function_definition(
        &self,
        name: &str,
        caller_file: &str,
        entity_index: &RelationIndex,
        symbol_table: &ProjectSymbolTable,
    ) -> Option<EntityId> {
        if let Some((scope, simple)) = Self::split_funcref_scope(name) {
            return self.resolve_scoped_funcref(
                name,
                scope,
                simple,
                caller_file,
                entity_index,
                symbol_table,
            );
        }
        let visible = entity_index
            .get_function_ids_by_name(name)
            .into_iter()
            .filter(|id| self.is_visible_funcref_candidate(*id, caller_file, entity_index));
        Self::pick_funcref_winner(visible, caller_file, entity_index)
    }

    /// Split a scoped reference (`Scope::member`) into its parts.
    ///
    /// Returns `None` for bare names, which take the visibility-partitioned
    /// largest-span path instead of scope verification.
    fn split_funcref_scope(name: &str) -> Option<(&str, &str)> {
        let (scope, simple) = name.rsplit_once("::")?;
        if scope.is_empty() || simple.is_empty() || simple.contains(':') {
            return None;
        }
        Some((scope, simple))
    }

    /// Whether an entity may be the target of a cross-file function value.
    ///
    /// Function-like kinds only; free `static` functions are confined to
    /// their own file while every other shape keeps external visibility.
    fn is_visible_funcref_candidate(
        &self,
        id: EntityId,
        caller_file: &str,
        entity_index: &RelationIndex,
    ) -> bool {
        entity_index
            .get_function_by_entity_id(id)
            .is_some_and(|entity| {
                if !entity.kind.is_function_like() {
                    return false;
                }
                if entity.kind == cce_types::EntityKind::Function
                    && entity.modifiers.iter().any(|m| m == "static")
                {
                    return entity_index
                        .get_file_path_by_entity(id)
                        .as_deref()
                        .is_some_and(|f| Self::paths_equivalent(f, caller_file));
                }
                true
            })
    }

    /// Winner among visible candidates: the caller's own file first, then
    /// the largest span. First candidate wins ties, and the index yields
    /// insertion order, so the outcome is deterministic for one build.
    fn pick_funcref_winner(
        ids: impl Iterator<Item = EntityId>,
        caller_file: &str,
        entity_index: &RelationIndex,
    ) -> Option<EntityId> {
        let mut best_same: Option<(EntityId, usize)> = None;
        let mut best_foreign: Option<(EntityId, usize)> = None;
        for id in ids {
            let size = entity_index
                .get_function_by_entity_id(id)
                .map(|entity| entity.span.end_byte.saturating_sub(entity.span.start_byte))
                .unwrap_or(0);
            let same_file = entity_index
                .get_file_path_by_entity(id)
                .as_deref()
                .is_some_and(|f| Self::paths_equivalent(f, caller_file));
            let slot = if same_file {
                &mut best_same
            } else {
                &mut best_foreign
            };
            let replace = slot.is_none_or(|(_, current)| size > current);
            if replace {
                *slot = Some((id, size));
            }
        }
        best_same.or(best_foreign).map(|(id, _)| id)
    }

    /// Resolve a scoped function reference through its owning type.
    ///
    /// Backends registering qualified names keep working through the direct
    /// hit path. Otherwise the trailing segment supplies candidates and the
    /// type-member owner decides: a single owner match resolves, zero or
    /// several matches drop the edge instead of guessing an overload.
    fn resolve_scoped_funcref(
        &self,
        name: &str,
        scope: &str,
        simple: &str,
        caller_file: &str,
        entity_index: &RelationIndex,
        symbol_table: &ProjectSymbolTable,
    ) -> Option<EntityId> {
        let direct: Vec<EntityId> = entity_index
            .get_function_ids_by_name(name)
            .into_iter()
            .filter(|id| self.is_visible_funcref_candidate(*id, caller_file, entity_index))
            .collect();
        if !direct.is_empty() {
            return Self::pick_funcref_winner(direct.into_iter(), caller_file, entity_index);
        }
        let mut owned: Vec<EntityId> = entity_index
            .get_function_ids_by_name(simple)
            .into_iter()
            .filter(|id| self.is_visible_funcref_candidate(*id, caller_file, entity_index))
            .filter(|id| self.funcref_owned_by_scope(*id, scope, entity_index, symbol_table))
            .collect();
        owned.sort();
        owned.dedup();
        if owned.len() == 1 {
            owned.into_iter().next()
        } else {
            None
        }
    }

    /// Whether the candidate entity is recorded as a member of the scope.
    ///
    /// Compares by member name, span and file path instead of entity id:
    /// the type index is built from parse-local ids while candidates arrive
    /// in index-global space, so id comparison cannot bridge the two.
    fn funcref_owned_by_scope(
        &self,
        id: EntityId,
        scope: &str,
        entity_index: &RelationIndex,
        symbol_table: &ProjectSymbolTable,
    ) -> bool {
        let (Some(candidate), Some(candidate_file)) = (
            entity_index.get_function_by_entity_id(id),
            entity_index.get_file_path_by_entity(id),
        ) else {
            return false;
        };
        let type_index = symbol_table.global_type_index();
        type_index
            .get_type_by_simple(scope)
            .iter()
            .filter(|entry| {
                entry.key.simple == scope || Self::scope_matches_owner(&entry.key.qualified, scope)
            })
            .filter_map(|entry| entry.members.get(&candidate.name))
            .flatten()
            .any(|member| {
                member.span == candidate.span
                    && Self::paths_equivalent(&member.file_path, &candidate_file)
            })
    }

    /// Whether a type-member owner satisfies a reference scope.
    ///
    /// Accepts the full qualified name as well as its trailing segment in
    /// either separator style, so `pkg::S` and `pkg.S` scopes both match an
    /// owner recorded as `pkg::S` or `pkg.S`.
    fn scope_matches_owner(owner_qualified: &str, scope: &str) -> bool {
        if owner_qualified == scope {
            return true;
        }
        let simple = owner_qualified
            .rsplit("::")
            .next()
            .unwrap_or(owner_qualified);
        if simple == scope {
            return true;
        }
        simple.rsplit('.').next().unwrap_or(simple) == scope
    }

    /// Promoted-receiver rewrite (`engine.RouterGroup.Use` ->
    /// `RouterGroup.Use`) when the trailing field type owns the method.
    fn promoted_candidate_name(
        &self,
        dst_name: &str,
        symbol_table: &ProjectSymbolTable,
    ) -> Option<String> {
        let owner = self.extract_promoted_receiver_type(dst_name, symbol_table)?;
        let method = dst_name.rsplit('.').next()?;
        let promoted = format!("{owner}.{method}");
        (promoted != dst_name).then_some(promoted)
    }

    /// Candidate resolution names with the promoted-receiver rewrites first.
    ///
    /// A dotted field-access path (`engine.RouterGroup.Use`) otherwise
    /// resolves through the raw literal, which degrades to the head
    /// variable's type (`Engine.Use`, a self-loop here) before the
    /// refined receiver can disambiguate. The rewritten `Type.Method`
    /// candidate dispatches through type-member resolution to the real
    /// owner; it only exists on a verified type-member match. The
    /// embedded variant covers the two-segment shape (`engine.Use`) where
    /// the receiver's own type lacks the method and an embedded field's
    /// type provides it.
    fn ordered_resolution_names(
        &self,
        callee_name: &str,
        parsed: &ParsedFile,
        symbol_table: &ProjectSymbolTable,
        entity_index: &RelationIndex,
        receiver_type: Option<&str>,
    ) -> Vec<String> {
        let mut names = Vec::new();
        if let Some(promoted) = self.promoted_candidate_name(callee_name, symbol_table) {
            names.push(promoted);
        } else if let Some(embedded) = self.embedded_promoted_candidate_name(
            callee_name,
            receiver_type,
            entity_index,
            symbol_table,
        ) {
            names.push(embedded);
        }
        names.extend(self.resolution_names(callee_name, parsed));
        names
    }

    /// Promoted-receiver rewrite for the two-segment shape (`engine.Use`).
    ///
    /// Fires only when the inferred receiver type is known, names a real
    /// type that lacks the method itself, and an embedded field's type owns
    /// it. The rewritten `Owner.method` candidate then dispatches through
    /// type-member resolution exactly like the explicit field-path variant.
    fn embedded_promoted_candidate_name(
        &self,
        dst_name: &str,
        receiver_type: Option<&str>,
        entity_index: &RelationIndex,
        symbol_table: &ProjectSymbolTable,
    ) -> Option<String> {
        if dst_name.contains("::") {
            return None;
        }
        let (head, method) = dst_name.split_once('.')?;
        if head.is_empty() || method.is_empty() || method.contains('.') {
            return None;
        }
        let owner =
            self.embedded_method_owner(receiver_type?, method, entity_index, symbol_table)?;
        let promoted = format!("{owner}.{method}");
        (promoted != dst_name).then_some(promoted)
    }

    /// Owner type reached through anonymous embedded fields.
    ///
    /// Returns `None` when the receiver type is unknown, names no indexed
    /// type, or owns the method itself: promotion only fills the gap the
    /// receiver's own type leaves. Embedded sources are struct field
    /// children without a type annotation (the shape of an anonymous
    /// embedded field); every name is verified against the type index
    /// before it is followed, so ordinary named fields never open a
    /// promotion path. Breadth-first with a visited set and depth cap so
    /// cyclic embeddings terminate.
    fn embedded_method_owner(
        &self,
        receiver_type: &str,
        method: &str,
        entity_index: &RelationIndex,
        symbol_table: &ProjectSymbolTable,
    ) -> Option<String> {
        const MAX_PROMOTION_DEPTH: usize = 8;
        if method.is_empty() {
            return None;
        }
        let start = Self::normalize_receiver_type_name(receiver_type);
        if start.is_empty() {
            return None;
        }
        let type_index = symbol_table.global_type_index();
        let owns_method = |name: &str| {
            type_index.get_type_by_simple(name).iter().any(|entry| {
                type_index
                    .get_members(&entry.key.qualified, method)
                    .is_some()
            })
        };
        if owns_method(&start) {
            return None;
        }
        let mut visited: HashSet<String> = HashSet::new();
        let mut frontier: Vec<String> = vec![start];
        let mut depth = 0;
        while !frontier.is_empty() && depth < MAX_PROMOTION_DEPTH {
            depth += 1;
            let mut next: Vec<String> = Vec::new();
            for name in frontier.drain(..) {
                let mut fresh = false;
                for entry in type_index.get_type_by_simple(&name) {
                    fresh |= visited.insert(entry.key.qualified.clone());
                }
                if !fresh {
                    continue;
                }
                for host_id in entity_index.get_function_ids_by_name(&name) {
                    let Some(host) = entity_index.get_function_by_entity_id(host_id) else {
                        continue;
                    };
                    if !matches!(
                        host.kind,
                        cce_types::EntityKind::Struct
                            | cce_types::EntityKind::Interface
                            | cce_types::EntityKind::TypeAlias
                            | cce_types::EntityKind::Class
                    ) {
                        continue;
                    }
                    for child_id in &host.children {
                        let Some(child) = entity_index.get_function_by_entity_id(*child_id) else {
                            continue;
                        };
                        if child.kind != cce_types::EntityKind::Field
                            || child.get_metadata("type_annotation").is_some()
                        {
                            continue;
                        }
                        let field_type = child.name.trim();
                        if field_type.is_empty()
                            || field_type == name
                            || next.iter().any(|n| n == field_type)
                        {
                            continue;
                        }
                        if owns_method(field_type) {
                            return Some(field_type.to_string());
                        }
                        if type_index.get_type_by_simple(field_type).is_empty() {
                            continue;
                        }
                        next.push(field_type.to_string());
                    }
                }
            }
            frontier = next;
        }
        None
    }

    /// Strip pointer, package and generic decorations to a simple type name.
    ///
    /// Mirrors the receiver normalization of the type-index backends so the
    /// walk keys agree with the index: `*pkg.Engine[T]` becomes `Engine`.
    fn normalize_receiver_type_name(raw: &str) -> String {
        let decorated = raw.trim().trim_start_matches(['*', ' ']).trim();
        let simple = decorated.rsplit('.').next().unwrap_or(decorated);
        let simple = simple.split('[').next().unwrap_or(simple);
        simple
            .split('<')
            .next()
            .unwrap_or(simple)
            .trim()
            .to_string()
    }

    /// Receiver type for dotted field-access paths (`engine.RouterGroup.Use`).
    ///
    /// The trailing field access refines the receiver to the embedded
    /// field's type. When that segment names a project type owning the
    /// called method, it wins over the head variable's type so
    /// promoted-method calls resolve to the real owner instead of
    /// degrading to the outer type (which self-loop filtering then
    /// drops). Only fires on a verified type-member match.
    fn extract_promoted_receiver_type(
        &self,
        dst_name: &str,
        symbol_table: &ProjectSymbolTable,
    ) -> Option<String> {
        let mut segments: Vec<&str> = dst_name.split('.').collect();
        if segments.len() < 3 {
            return None;
        }
        let method = segments.pop();
        let field_type = segments.pop();
        let (Some(method), Some(field_type)) = (method, field_type) else {
            return None;
        };
        let type_index = symbol_table.global_type_index();
        let owns_method = type_index
            .get_type_by_simple(field_type)
            .iter()
            .any(|entry| {
                type_index
                    .get_members(&entry.key.qualified, method)
                    .is_some()
            });
        owns_method.then(|| field_type.to_string())
    }

    /// Extract receiver type from instance method call name
    ///
    /// For patterns like "obj.method" or "Type::method", extracts the receiver part.
    /// Returns None if the pattern doesn't match.
    fn extract_receiver_from_name(&self, dst_name: &str) -> Option<String> {
        // Handle "obj.method" pattern
        if let Some(dot_pos) = dst_name.rfind('.') {
            let receiver = &dst_name[..dot_pos];
            if !receiver.is_empty() && receiver.chars().next().is_some_and(|c| c.is_alphabetic()) {
                return Some(receiver.to_string());
            }
        }

        // Handle "Type::method" pattern
        if let Some(colon_pos) = dst_name.rfind("::") {
            let receiver = &dst_name[..colon_pos];
            if !receiver.is_empty() && receiver.chars().next().is_some_and(|c| c.is_alphabetic()) {
                return Some(receiver.to_string());
            }
        }

        None
    }

    /// Recover a textual call receiver lost at extraction.
    ///
    /// `call.method` captures only the property identifier, so a plain
    /// `this.get('env')` reaches resolution as bare `get` with a span
    /// covering just the property name. When the source immediately before
    /// the span reads `identifier.`, that identifier is the receiver. Only
    /// a standalone identifier directly attached by a dot qualifies:
    /// longer chains (`a.this.get`) and detached forms are left alone
    /// rather than guessed.
    fn recover_receiver_before_span(source: &str, span_start: usize) -> Option<String> {
        let bytes = source.as_bytes();
        let mut end = span_start.min(bytes.len());
        if end == 0 || bytes[end - 1] != b'.' {
            return None;
        }
        end -= 1;
        while end > 0 && bytes[end - 1].is_ascii_whitespace() {
            end -= 1;
        }
        let mut start = end;
        while start > 0
            && (bytes[start - 1].is_ascii_alphanumeric()
                || bytes[start - 1] == b'_'
                || bytes[start - 1] == b'$')
        {
            start -= 1;
        }
        if start == end {
            return None;
        }
        if start > 0
            && (bytes[start - 1] == b'.'
                || bytes[start - 1].is_ascii_alphanumeric()
                || bytes[start - 1] == b'_'
                || bytes[start - 1] == b'$')
        {
            return None;
        }
        Some(source[start..end].to_string())
    }

    /// Extract owner type from static method call name
    ///
    /// For patterns like "Type::method" or "Type.method", extracts the type part.
    /// Returns None if the pattern doesn't match.
    fn extract_owner_from_static_call(&self, dst_name: &str) -> Option<String> {
        // Handle "Type::method" pattern
        if let Some(colon_pos) = dst_name.rfind("::") {
            let owner = &dst_name[..colon_pos];
            if !owner.is_empty() && owner.chars().next().is_some_and(|c| c.is_alphabetic()) {
                return Some(owner.to_string());
            }
        }

        // Handle "Type.method" pattern (less common for static calls, but possible)
        if let Some(dot_pos) = dst_name.rfind('.') {
            let owner = &dst_name[..dot_pos];
            if !owner.is_empty() && owner.chars().next().is_some_and(|c| c.is_alphabetic()) {
                // Check if it looks like a type name (starts with uppercase)
                if owner.chars().next().is_some_and(|c| c.is_uppercase()) {
                    return Some(owner.to_string());
                }
            }
        }

        None
    }

    /// Look up the inferred type for a receiver variable using type inference.
    ///
    /// When the type inference context has recorded a type for the receiver
    /// variable, this method returns it.
    /// This is used to improve CallContext determination for dynamically-typed
    /// languages (Python, JavaScript).
    fn lookup_inferred_receiver_type(
        &self,
        receiver_name: &str,
        file_path: &str,
        symbol_table: &ProjectSymbolTable,
        parsed: &ParsedFile,
    ) -> Option<String> {
        let ctx = symbol_table.get_type_inference_context(file_path)?;
        if let Some(binding) = ctx.get_variable_type(receiver_name) {
            return Some(binding.type_name.clone());
        }
        self.lookup_cross_file_receiver_type(receiver_name, parsed, symbol_table)
    }

    /// Cross-file receiver type lookup via `call_target` metadata and the
    /// global cross-file propagator.
    ///
    /// Handles `x = foo()` where `foo` is defined in another file with a
    /// known return type. The local type context may not have the propagated
    /// binding yet (especially during incremental builds), so this lazy query
    /// consults the propagator directly.
    fn lookup_cross_file_receiver_type(
        &self,
        receiver_name: &str,
        parsed: &ParsedFile,
        symbol_table: &ProjectSymbolTable,
    ) -> Option<String> {
        use std::collections::HashSet;
        const MAX_CHAIN_DEPTH: usize = 5;
        for entity in &parsed.entities {
            if entity.name == receiver_name
                && entity.kind == cce_types::entity::EntityKind::Variable
            {
                if let Some(target) = entity
                    .metadata
                    .get("call_target")
                    .or_else(|| entity.metadata.get("constructor_type"))
                {
                    let chain = crate::type_inference::cross_file::parse_call_chain(target);
                    if chain.len() > 1 && chain.len() <= MAX_CHAIN_DEPTH {
                        let mut visited = HashSet::new();
                        let mut current_type: Option<String> = None;
                        let mut cycle = false;
                        for step in &chain {
                            if !visited.insert(step.method_name.clone()) {
                                cycle = true;
                                break;
                            }
                            if let Some(binding) =
                                symbol_table.get_cross_file_return_type_by_name(&step.method_name)
                            {
                                current_type = Some(binding.type_name.clone());
                            } else if current_type.is_none() {
                                break;
                            }
                        }
                        if !cycle {
                            if let Some(ct) = current_type {
                                return Some(ct);
                            }
                        }
                    }
                    // Stored targets may carry an argument list (`foo(a)`);
                    // receiver lookup uses the stripped callee name.
                    let stripped = crate::type_inference::cross_file::split_call_target(target).0;
                    let simple = stripped
                        .rsplit(['.', ':', '/'])
                        .next()
                        .unwrap_or(&stripped)
                        .trim();
                    if simple.is_empty() {
                        continue;
                    }
                    if let Some(binding) = symbol_table.get_cross_file_return_type_by_name(simple) {
                        return Some(binding.type_name.clone());
                    }
                }
            }
        }
        None
    }

    fn infer_receiver_via_use_site(
        &self,
        method_name: &str,
        symbol_table: &ProjectSymbolTable,
        _language: cce_types::language::Language,
    ) -> Option<String> {
        let global = symbol_table.global_type_index();
        let mut candidates: Vec<String> = Vec::new();
        for type_entry in global.all_types() {
            if type_entry.members.contains_key(method_name) {
                candidates.push(type_entry.key.qualified.clone());
            }
        }
        if candidates.is_empty() {
            return None;
        }
        if candidates.len() == 1 {
            return Some(candidates.into_iter().next().unwrap());
        }
        candidates.sort();
        let mut best: Option<(String, usize)> = None;
        for owner in &candidates {
            if let Some(set) = symbol_table.get_overload_set(owner, method_name) {
                let score = set.candidates.len();
                match &best {
                    Some((_, best_score)) if *best_score >= score => {}
                    _ => best = Some((owner.clone(), score)),
                }
            }
        }
        if let Some((owner, _)) = best {
            return Some(owner);
        }
        Some(candidates[0].clone())
    }

    fn infer_expected_return_type(
        &self,
        raw_data: &RawRelationData,
        parsed: &ParsedFile,
        symbol_table: &ProjectSymbolTable,
    ) -> Option<String> {
        // Heuristic: find a variable whose span contains the call span. The variable's
        // inferred type can serve as the expected return type for overload ranking.
        let call_span = raw_data.span;
        for entity in &parsed.entities {
            if entity.kind != cce_types::entity::EntityKind::Variable {
                continue;
            }
            if entity.span.start_byte <= call_span.start_byte
                && entity.span.end_byte >= call_span.end_byte
                && entity.span.start_byte != entity.span.end_byte
            {
                if let Some(ctx) = symbol_table.get_type_inference_context(&parsed.path) {
                    if let Some(binding) = ctx.get_variable_type(&entity.name) {
                        if !binding.type_name.is_empty() && binding.type_name != "unknown" {
                            return Some(binding.type_name.clone());
                        }
                    }
                }
                if let Some(rt) = &entity.return_type {
                    if !rt.is_empty() {
                        return Some(rt.clone());
                    }
                }
            }
        }
        // Fallback: check for assignment pattern where call_target matches dst_name
        for entity in &parsed.entities {
            if entity.kind != cce_types::entity::EntityKind::Variable {
                continue;
            }
            if let Some(target) = entity
                .metadata
                .get("call_target")
                .or_else(|| entity.metadata.get("constructor_type"))
            {
                // Stored targets may carry an argument list (`foo(a)`);
                // compare against the stripped callee name.
                let stripped = crate::type_inference::cross_file::split_call_target(target).0;
                if stripped.rsplit(['.', ':', '/']).next().unwrap_or(&stripped)
                    == raw_data
                        .dst_name
                        .rsplit(['.', ':', '/'])
                        .next()
                        .unwrap_or(&raw_data.dst_name)
                {
                    if let Some(ctx) = symbol_table.get_type_inference_context(&parsed.path) {
                        if let Some(binding) = ctx.get_variable_type(&entity.name) {
                            if !binding.type_name.is_empty() && binding.type_name != "unknown" {
                                return Some(binding.type_name.clone());
                            }
                        }
                    }
                }
            }
        }
        None
    }

    /// Resolve multiple raw relations
    ///
    /// # Arguments
    ///
    /// * `raw_relations` - Slice of raw relations to resolve
    /// * `parsed` - The parsed file containing the relations
    /// * `symbol_table` - The global symbol table for cross-file resolution
    ///
    /// # Returns
    ///
    /// A vector of resolved relations (filtered ones are excluded)
    pub fn resolve_batch(
        &self,
        raw_relations: &[RawRelationData],
        parsed: &ParsedFile,
        symbol_table: &ProjectSymbolTable,
        entity_index: &RelationIndex,
    ) -> Vec<ResolvedRelation> {
        // Build the scope map once per file instead of per relation.
        let entity_map: HashMap<EntityId, &Entity> =
            parsed.entities.iter().map(|e| (e.id, e)).collect();
        let mut out: Vec<ResolvedRelation> = Vec::new();
        for raw_data in raw_relations {
            if let Some(resolved) = self.resolve_with_scope_map(
                raw_data,
                parsed,
                symbol_table,
                entity_index,
                &entity_map,
            ) {
                out.push(resolved);
                // Type-driven callback binding rides on the resolved edge.
                // Synthesized edges carry a global caller (the HOF id), so
                // they must never go through any local caller remap.
                let last = out.len() - 1;
                out.extend(self.synthesize_hof_callback_edges(
                    raw_data,
                    parsed,
                    symbol_table,
                    entity_index,
                    &entity_map,
                    &out[last],
                ));
            }
        }
        assign_call_frequency(&mut out);
        out
    }

    /// Synthesize type-driven callback edges for one resolved HOF call.
    ///
    /// For `arr.map(cb)` the primary edge is `caller -> map`; the callback
    /// definition itself is otherwise unreachable from the HOF. This pass
    /// adds `map -> cb` when — and only when — every gate is type- or
    /// syntax-driven:
    ///   1. the relation is a `HigherOrderCall` or `EventCallback` resolved
    ///      to an internal HOF;
    ///   2. the HOF's declared parameter list is available and the
    ///      corresponding formal parameter parses to `TypeShape::Function`;
    ///   3. the argument is a bare identifier (no literals, chains, or
    ///      inline closures);
    ///   4. the identifier resolves to an internal entity via the strict
    ///      candidate path;
    ///   5. the resolved target is not the HOF itself.
    ///
    /// Without the function-typed parameter the edge is simply not
    /// produced: no name-shape or convention heuristics participate. Every
    /// gate fails closed, so the pass needs no runtime toggle — disabling
    /// it would only preserve a known call-graph gap. The returned edges
    /// carry a global `caller` (the resolved HOF id), so callers must NOT
    /// run them through the per-file local caller remap.
    pub fn synthesize_hof_callback_edges(
        &self,
        raw_data: &RawRelationData,
        parsed: &ParsedFile,
        symbol_table: &ProjectSymbolTable,
        entity_index: &RelationIndex,
        entity_map: &HashMap<EntityId, &Entity>,
        resolved: &ResolvedRelation,
    ) -> Vec<ResolvedRelation> {
        // Gate 1: an HOF/event-binding call site resolved to an internal
        // entity. Library receivers (e.g. `Array.prototype.map`) never
        // resolve internally and fail closed here.
        if !matches!(
            raw_data.relation_type,
            cce_types::relation::RelationType::HigherOrderCall
                | cce_types::relation::RelationType::EventCallback
        ) {
            return Vec::new();
        }
        let Some(hof_id) = resolved.callee_id else {
            return Vec::new();
        };

        // Gate 2: the formal parameter must be a declared function type.
        let param_bindings = symbol_table.parameter_types_for_entity(&parsed.path, hof_id);
        if param_bindings.is_empty() {
            return Vec::new();
        }
        let param_shapes: Vec<TypeShape> = param_bindings
            .iter()
            .map(|binding| {
                binding
                    .shape
                    .clone()
                    .or_else(|| parse_type_shape(&binding.type_name, parsed.language))
                    .unwrap_or(TypeShape::Named("unknown".to_string()))
            })
            .collect();

        let Some(args) = extract_call_arguments(parsed.source.as_ref(), raw_data.span) else {
            return Vec::new();
        };
        if args.is_empty() {
            return Vec::new();
        }

        let scope_chain = Self::build_scope_chain_from_map_with_limit(
            raw_data.src,
            entity_map,
            symbol_table.max_scope_chain_depth(),
        );
        let resolution_context = ResolutionContext {
            file_path: parsed.path.clone(),
            module_path: Vec::new(),
            scope_chain,
        };
        let file_remap =
            entity_index.entity_id_remap_for(&cce_types::normalize_project_path(&parsed.path));
        let candidate_ctx = NameCandidateContext {
            parsed,
            symbol_table,
            entity_index,
            resolution_context: &resolution_context,
            file_remap: file_remap.as_ref(),
            overload_ctx: None,
        };

        let mut out = Vec::new();
        for (idx, arg) in args.iter().enumerate() {
            let Some(param_shape) = param_shapes.get(idx) else {
                break;
            };
            if !matches!(param_shape, TypeShape::Function { .. }) {
                continue;
            }
            // Gate 3: bare identifier only.
            let trimmed = arg.trim();
            if !is_bare_identifier(trimmed) {
                continue;
            }
            // Gate 4: strict internal resolution of the argument name.
            let (_, Some(arg_id)) = self.resolve_name_candidate(trimmed, false, &candidate_ctx)
            else {
                continue;
            };
            // Gate 5: never synthesize a self-loop.
            if arg_id == hof_id {
                continue;
            }
            let callee_name = entity_index
                .get_function_by_entity_id(arg_id)
                .map(|entity| entity.name.clone())
                .unwrap_or_else(|| trimmed.to_string());
            out.push(ResolvedRelation {
                caller: hof_id,
                callee_id: Some(arg_id),
                callee_name,
                relation_type: cce_types::relation::RelationType::CallbackCall,
                span: raw_data.span,
                is_external: false,
                external_type: None,
                callee_symbol: None,
                stdlib_category: None,
                owner_type: None,
                call_context: CallContext::Direct,
                overload_signature: None,
                call_frequency: 1,
                cfg_condition: None,
            });
        }
        out
    }

    fn extract_argument_types(
        &self,
        raw_data: &RawRelationData,
        parsed: &ParsedFile,
        symbol_table: &ProjectSymbolTable,
    ) -> Vec<Option<TypeShape>> {
        let Some(arg_texts) = extract_call_arguments(parsed.source.as_ref(), raw_data.span) else {
            return Vec::new();
        };
        arg_texts
            .iter()
            .map(|arg| self.infer_arg_type(arg, parsed, symbol_table, raw_data.span.start_byte))
            .collect()
    }

    /// Infer the shape of an identifier argument from a recorded else-branch
    /// binding when the call site falls inside the negation side.
    ///
    /// Returns `None` when the call site is outside every recorded else
    /// range or the variable carries no else binding, letting the caller
    /// fall back to the default (then-biased) lookup.
    fn infer_else_branch_arg_shape(
        parsed: &ParsedFile,
        ctx: &crate::type_inference::TypeInferenceContext,
        name: &str,
        call_site: usize,
    ) -> Option<TypeShape> {
        let owner = parsed
            .entities
            .iter()
            .filter(|entity| {
                entity.kind.is_function_like()
                    && entity.span.start_byte <= call_site
                    && call_site < entity.span.end_byte
            })
            .min_by_key(|entity| entity.span.end_byte - entity.span.start_byte)?;
        let facts = parsed.control_flow.get(owner.id)?;
        let in_else = facts.facts.iter().any(|fact| {
            fact.kind == ControlFlowFactKind::If && fact.contains_byte_in_else(call_site)
        });
        if !in_else {
            return None;
        }
        let binding = ctx.get_narrowed_in_branch(name, BranchPolarity::Else)?;
        if let Some(shape) = binding.shape.clone() {
            return Some(shape);
        }
        parse_type_shape(&binding.type_name, parsed.language)
    }

    fn infer_arg_type(
        &self,
        arg: &str,
        parsed: &ParsedFile,
        symbol_table: &ProjectSymbolTable,
        call_site: usize,
    ) -> Option<TypeShape> {
        let trimmed = arg.trim();
        if trimmed.is_empty() {
            return None;
        }
        if let Some(lit) = infer_literal_type_shape(trimmed, parsed.language) {
            return Some(lit);
        }
        if is_identifier(trimmed) {
            if let Some(ctx) = symbol_table.get_type_inference_context(&parsed.path) {
                // Calls inside a recorded else range observe the complement
                // binding; every miss falls through to the default lookup.
                if let Some(shape) =
                    Self::infer_else_branch_arg_shape(parsed, &ctx, trimmed, call_site)
                {
                    return Some(shape);
                }
                if let Some(binding) = ctx.get_variable_type(trimmed) {
                    if let Some(shape) = binding.shape.clone() {
                        return Some(shape);
                    }
                    if let Some(shape) = parse_type_shape(&binding.type_name, parsed.language) {
                        return Some(shape);
                    }
                    return Some(TypeShape::Named(binding.type_name.clone()));
                }
            }
            if let Some(binding) = symbol_table.get_cross_file_return_type_by_name(trimmed) {
                if let Some(shape) = binding.shape.clone() {
                    return Some(shape);
                }
                if let Some(shape) = parse_type_shape(&binding.type_name, parsed.language) {
                    return Some(shape);
                }
                return Some(TypeShape::Named(binding.type_name.clone()));
            }
            return None;
        }
        if let Some(paren) = trimmed.find('(') {
            let func_name = trimmed[..paren].trim();
            let simple = func_name
                .rsplit(['.', ':', '/'])
                .next()
                .unwrap_or(func_name)
                .trim();
            if !simple.is_empty() {
                if let Some(ctx) = symbol_table.get_type_inference_context(&parsed.path) {
                    // Try to find return type of the called function in the same file
                    for (eid, binding) in ctx.return_types_iter() {
                        if let Some(entity) = parsed
                            .entities
                            .iter()
                            .find(|e| e.id == *eid && e.name == simple)
                        {
                            let _ = entity;
                            if let Some(shape) = binding.shape.clone() {
                                return Some(shape);
                            }
                            if let Some(shape) =
                                parse_type_shape(&binding.type_name, parsed.language)
                            {
                                return Some(shape);
                            }
                            return Some(TypeShape::Named(binding.type_name.clone()));
                        }
                    }
                }
                if let Some(binding) = symbol_table.get_cross_file_return_type_by_name(simple) {
                    if let Some(shape) = binding.shape.clone() {
                        return Some(shape);
                    }
                    if let Some(shape) = parse_type_shape(&binding.type_name, parsed.language) {
                        return Some(shape);
                    }
                    return Some(TypeShape::Named(binding.type_name.clone()));
                }
            }
            // Constructor pattern `new Type(...)`
            if trimmed.starts_with("new ") {
                let rest = trimmed.strip_prefix("new ").unwrap().trim();
                if let Some(end) = rest.find(['(', ' ', ';']) {
                    let ty = rest[..end].trim();
                    if !ty.is_empty() {
                        return Some(TypeShape::Named(ty.to_string()));
                    }
                } else if !rest.is_empty() {
                    return Some(TypeShape::Named(rest.to_string()));
                }
            }
        }
        if trimmed.starts_with("new ") {
            let rest = trimmed.strip_prefix("new ").unwrap().trim();
            if let Some(end) = rest.find(['(', ' ', ';']) {
                let ty = rest[..end].trim();
                if !ty.is_empty() {
                    return Some(TypeShape::Named(ty.to_string()));
                }
            } else if !rest.is_empty() {
                return Some(TypeShape::Named(rest.to_string()));
            }
        }
        None
    }

    /// Extract receiver type from method call for overload resolution
    fn extract_receiver_type(
        &self,
        raw_data: &RawRelationData,
        parsed: &ParsedFile,
        symbol_table: &ProjectSymbolTable,
        entity_index: &RelationIndex,
    ) -> Option<String> {
        if !raw_data.dst_name.contains('.') {
            return None;
        }
        let receiver_name = raw_data.dst_name.split('.').next()?;
        if let Some(ctx) = symbol_table.get_type_inference_context(&parsed.path) {
            if let Some(type_binding) = ctx.get_variable_type(receiver_name) {
                return Some(type_binding.type_name.clone());
            }
        }
        if let Some(type_binding) = symbol_table.get_cross_file_return_type_by_name(receiver_name) {
            return Some(type_binding.type_name.clone());
        }
        for entity in &parsed.entities {
            if entity.name == receiver_name {
                if let Some(rt) = &entity.return_type {
                    return Some(rt.clone());
                }
            }
        }
        self.lookup_inferred_receiver_type(receiver_name, &parsed.path, symbol_table, parsed)
            .or_else(|| self.lookup_cross_file_receiver_type(receiver_name, parsed, symbol_table))
            .or_else(|| {
                // Fallback to entity_index lookup
                let _ = entity_index;
                None
            })
    }

    /// Extract call argument count from source code with proper bracket depth tracking
    fn extract_call_argument_count(
        &self,
        raw_data: &RawRelationData,
        parsed: &ParsedFile,
    ) -> Option<usize> {
        let source = &parsed.source;
        let span = &raw_data.span;
        let call_text = source.get(span.start_byte..span.end_byte)?;
        let open_pos = call_text.find('(')?;
        let args_text = &call_text[open_pos + 1..];
        let mut depth = 0;
        let mut arg_count = 0;
        let mut in_string = false;
        let mut string_char = ' ';
        for ch in args_text.chars() {
            match ch {
                '(' | '[' | '{' if !in_string => depth += 1,
                ')' | ']' | '}' if !in_string => {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                }
                '"' | '\'' | '`' if !in_string => {
                    in_string = true;
                    string_char = ch;
                }
                c if in_string && c == string_char => {
                    in_string = false;
                }
                ',' if depth == 0 && !in_string => {
                    arg_count += 1;
                }
                _ => {}
            }
        }
        if args_text.trim().is_empty() {
            return Some(0);
        }
        if !args_text.trim().is_empty() {
            arg_count += 1;
        }
        Some(arg_count)
    }
}

/// Stamp every relation with how many call sites its caller uses to reach the
/// same target.
///
/// The index keeps one edge per call site (`RelationEdgeIdentity::callsite`),
/// so a caller that invokes the same callee from five places holds five edges
/// and the repetition is otherwise invisible to any consumer that collapses
/// them. Grouping by the callsite-free identity turns that multiplicity into an
/// explicit weight, so a single representative edge still reports the full
/// frequency. Only call-class edges carry a frequency: a non-call edge has no
/// call sites to count, so it stays at one occurrence.
fn assign_call_frequency(relations: &mut [ResolvedRelation]) {
    let mut frequencies: HashMap<RelationEdgeIdentity, u64> = HashMap::new();
    for relation in relations.iter() {
        if relation.relation_type.is_call() {
            *frequencies
                .entry(callsite_free_identity(relation))
                .or_default() += 1;
        }
    }
    for relation in relations.iter_mut() {
        if relation.relation_type.is_call() {
            relation.call_frequency = frequencies[&callsite_free_identity(relation)];
        }
    }
}

/// The dedup identity of an edge with the call-site discriminator dropped, so
/// every call site of one logical relation shares a key.
fn callsite_free_identity(relation: &ResolvedRelation) -> RelationEdgeIdentity {
    RelationEdgeIdentity {
        callsite: None,
        ..relation_identity(relation)
    }
}

fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_alphanumeric() || c == '_')
}

/// Build the call context for a receiver-typed call, separating dynamic
/// dispatch from direct instance dispatch.
///
/// A call is dispatched dynamically when the receiver's *static* type does
/// not pin one implementation: a trait object (`dyn Trait`, including the
/// common `&dyn`/`Box<dyn>`/`Arc<dyn>` wrappers), an opaque `impl Trait`,
/// `Self` inside a trait or its implementors, or a bare type parameter
/// (`T`, `K`, `V`, ...). Such receivers are erased to an interface, so the
/// concrete callee is only known at runtime and the resolved edge must be
/// marked as virtual.
fn instance_call_context(receiver_type: &str) -> CallContext {
    if is_dynamic_receiver(receiver_type) {
        CallContext::VirtualDispatch {
            receiver_type: receiver_type.to_string(),
        }
    } else {
        CallContext::InstanceMethod {
            receiver_type: receiver_type.to_string(),
        }
    }
}

/// Leading reference and raw-pointer markers, longest first so `&mut ` is
/// never cut to a bare `&`.
const POINTER_PREFIXES: &[&str] = &["&mut ", "&", "*mut ", "*const ", "*", "const "];

/// Pointer and smart-pointer wrappers whose single type argument is the
/// erased type actually being received.
const DYNAMIC_RECEIVER_WRAPPERS: &[&str] = &[
    "Box", "Rc", "Arc", "Cow", "Ref", "RefCell", "Cell", "Mutex", "RwLock", "Pin",
];

/// Whether a receiver type is statically erased, so method lookup goes
/// through an interface rather than a single concrete type.
///
/// Recursive through pointer wrappers (`Arc<dyn Tr>` -> `dyn Tr`), and stops
/// at the first wrapper whose argument is a concrete type: `Arc<Concrete>`
/// pins its implementation and is not dynamic.
fn is_dynamic_receiver(receiver_type: &str) -> bool {
    let mut current = receiver_type.trim();
    loop {
        if current == "Self" || current == "this" {
            return true;
        }
        // A trait object or an opaque `impl Trait` bound names an interface,
        // never a single concrete implementation.
        if strip_prefix_any(current, &["dyn ", "impl "]).is_some() {
            return true;
        }
        if let Some(rest) = strip_prefix_any(current, POINTER_PREFIXES) {
            current = rest;
            continue;
        }
        if let Some(argument) = strip_wrapper_argument(current) {
            current = argument;
            continue;
        }
        return is_type_parameter(current);
    }
}

/// Strip the first matching prefix, if any.
fn strip_prefix_any<'a>(text: &'a str, prefixes: &[&str]) -> Option<&'a str> {
    prefixes.iter().find_map(|prefix| text.strip_prefix(prefix))
}

/// Yield the single type argument of a known wrapper generic, if the generic
/// has exactly one argument.
fn strip_wrapper_argument(type_name: &str) -> Option<&str> {
    let (base, rest) = type_name.split_once('<')?;
    let argument = rest.strip_suffix('>')?;
    if !argument.contains(',') && DYNAMIC_RECEIVER_WRAPPERS.contains(&base.trim()) {
        Some(argument)
    } else {
        None
    }
}

/// Whether a type name is a bare generic type parameter such as `T` or `K`.
fn is_type_parameter(type_name: &str) -> bool {
    is_identifier(type_name)
        && type_name
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

fn infer_literal_type_shape(text: &str, language: Language) -> Option<TypeShape> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed == "true" || trimmed == "false" {
        return Some(TypeShape::Named(
            cce_types::literal_type_name(&language, cce_types::LiteralKind::Boolean).to_string(),
        ));
    }
    if trimmed == "null" || trimmed == "nil" || trimmed == "None" {
        return Some(TypeShape::Named(
            cce_types::literal_type_name(&language, cce_types::LiteralKind::Null).to_string(),
        ));
    }
    if (trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2)
        || (trimmed.starts_with('\'') && trimmed.ends_with('\'') && trimmed.len() >= 2)
        || (trimmed.starts_with('`') && trimmed.ends_with('`') && trimmed.len() >= 2)
    {
        let kind = if trimmed.starts_with('\'') && trimmed.len() == 3 {
            cce_types::LiteralKind::Char
        } else {
            cce_types::LiteralKind::String
        };
        return Some(TypeShape::Named(
            cce_types::literal_type_name(&language, kind).to_string(),
        ));
    }
    if let Some(kind) = cce_types::classify_numeric_literal(trimmed) {
        return Some(TypeShape::Named(
            cce_types::literal_type_name(&language, kind).to_string(),
        ));
    }
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        return Some(TypeShape::Named(
            cce_types::literal_type_name(&language, cce_types::LiteralKind::Array).to_string(),
        ));
    }
    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        return Some(TypeShape::Named(
            cce_types::literal_type_name(&language, cce_types::LiteralKind::Object).to_string(),
        ));
    }
    None
}

/// Whether the text is a bare ASCII identifier (no dots, colons, brackets,
/// or quotes). Used by the callback-binding gates so only plain named
/// references — never literals, chains, or inline closures — are bound.
fn is_bare_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn extract_call_arguments(source: &str, span: cce_types::Span) -> Option<Vec<String>> {
    let bytes = source.as_bytes();
    let start = span.start_byte.min(bytes.len());
    let end = span.end_byte.min(bytes.len()).max(start);
    let mut i = start;
    while i < end && bytes[i] != b'(' {
        i += 1;
    }
    if i >= end {
        return None;
    }
    let mut depth = 1usize;
    let mut args: Vec<String> = Vec::new();
    let mut current_start = i + 1;
    let mut in_string: Option<u8> = None;
    let mut in_line_comment = false;
    let mut in_block_comment = false;
    let mut j = i + 1;
    while j < end {
        let b = bytes[j];
        if in_line_comment {
            if b == b'\n' {
                in_line_comment = false;
            }
            j += 1;
            continue;
        }
        if in_block_comment {
            if b == b'*' && j + 1 < end && bytes[j + 1] == b'/' {
                in_block_comment = false;
                j += 2;
                continue;
            }
            j += 1;
            continue;
        }
        if let Some(quote) = in_string {
            if b == b'\\' {
                j += 2;
                continue;
            }
            if b == quote {
                in_string = None;
            } else if quote == b'`' && b == b'$' && j + 1 < end && bytes[j + 1] == b'{' {
                depth += 1;
            }
            j += 1;
            continue;
        }
        if b == b'/' && j + 1 < end {
            if bytes[j + 1] == b'/' {
                in_line_comment = true;
                j += 2;
                continue;
            }
            if bytes[j + 1] == b'*' {
                in_block_comment = true;
                j += 2;
                continue;
            }
        }
        match b {
            b'"' | b'\'' | b'`' => in_string = Some(b),
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    let arg_slice = &source[current_start..j];
                    let trimmed = arg_slice.trim();
                    if !trimmed.is_empty() {
                        args.push(trimmed.to_string());
                    } else if current_start == i + 1 {
                        // No args
                    }
                    return Some(args);
                }
                if depth == 1 && b == b',' {
                    // This case is handled below for ',' at depth 1, but we already decremented
                }
            }
            b',' if depth == 1 => {
                let arg_slice = &source[current_start..j];
                args.push(arg_slice.trim().to_string());
                current_start = j + 1;
            }
            _ => {}
        }
        j += 1;
    }
    None
}

impl Default for RelationResolver {
    fn default() -> Self {
        Self::new()
    }
}

mod external;
mod name_candidates;

pub(crate) use name_candidates::NameCandidateContext;

#[cfg(test)]
mod tests;
