//! Relation resolver for converting raw relations to resolved relations
//!
//! Provides functionality to resolve raw relations by looking up symbols in
//! a global symbol table and classifying external calls (standard library,
//! external packages, or unknown).

use super::core::RelationIndex;
use super::dependency_index::DependencyIndex;
use crate::config_parser::UntypedDependency;
use crate::index::EntityIndexOps;
use crate::stdlib_classifier::with_stdlib_classifier;
use crate::symbol::SymbolRef;
use crate::symbol_table::ProjectSymbolTable;
use crate::symbol_table::ResolutionContext;
use crate::symbol_table::project::OverloadContext;
use crate::type_inference::types::{BranchPolarity, TypeShape, parse_type_shape};
use cce_metrics::domain::pipeline::RelationMetrics;
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
        // This is the authoritative source and should always be set.
        //
        // The fallback detection here is only for backward compatibility with
        // cached relations that may not have this field set. In a clean system,
        // this fallback should never be needed.
        //
        // See STDLIB_SUMMARY.md for detailed analysis of stdlib handling.
        //
        // detection no longer causes an early return; the stdlib identity
        // is applied only after symbol resolution.
        let is_stdlib = if let Some(_category) = raw_data.stdlib_category {
            true
        } else {
            let detected = with_stdlib_classifier(|c| {
                c.is_stdlib_by_type(
                    &raw_data.dst_name,
                    &raw_data.relation_type,
                    &parsed.language,
                )
            })
            .unwrap_or(false);
            if detected {
                tracing::warn!(
                    "Fallback stdlib detection for: {} -> {} (relation type: {:?})",
                    raw_data.src,
                    raw_data.dst_name,
                    raw_data.relation_type
                );
            }
            detected
        };

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
        let receiver_type = self
            .extract_receiver_from_name(&raw_data.dst_name)
            .or_else(|| {
                if has_overload || !is_bare {
                    self.extract_receiver_type(raw_data, parsed, symbol_table, entity_index)
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
        let overload_ctx = if is_bare && !has_overload {
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
        let mut symbol_ref: Option<SymbolRef> = None;
        let mut resolved_entity_id: Option<EntityId> = None;
        for name in self.resolution_names(&raw_data.dst_name, parsed) {
            let (candidate_ref, candidate_id) =
                self.resolve_name_candidate(&name, is_stdlib, &candidate_ctx);
            if candidate_id.is_some() {
                symbol_ref = candidate_ref;
                resolved_entity_id = candidate_id;
                break;
            }
            // Keep the closest symbol ref so the snapshot still records a
            // best-effort symbol even when every candidate fails.
            symbol_ref = symbol_ref.or(candidate_ref);
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
            return None;
        }

        // Determine external call type if this is an external reference.
        // the stdlib decision is post-resolution — internally-resolved
        // callees are kept even when their name looks like a stdlib name.
        let external_type = if is_external {
            if is_stdlib {
                if self.filter_stdlib_calls {
                    self.filtered_count
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if let Some(metrics) = &self.metrics {
                        metrics.stdlib_filtered.increment();
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
                metrics.record_unresolved(
                    cce_types::relation::UnresolvedReason::SymbolNotFound.as_str(),
                );
            }
        }

        // For internally-resolved edges, the canonical callee name is the
        // resolved entity's registered (simple) name: call-chain queries match
        // by simple name, while the caller-written path text stays available
        // in `raw_target` and in the canonical snapshot. External/unresolved
        // edges keep the full written path so stdlib detection and external
        // classification stay intact.
        let callee_name = callee_id
            .and_then(|id| entity_index.get_function_by_entity_id(id))
            .map(|entity| entity.name.clone())
            .unwrap_or_else(|| raw_data.dst_name.clone());

        let mut effective_callee_id = callee_id;
        let mut overload_signature: Option<String> = None;
        // Determine owner_type and call_context using TypeMemberIndex
        let (owner_type, call_context) = if let Some(callee_id) = effective_callee_id {
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
            let call_context = match raw_data.relation_type {
                cce_types::relation::RelationType::InstanceMethodCall => {
                    if let Some(ref owner) = owner_type {
                        CallContext::InstanceMethod {
                            receiver_type: owner.clone(),
                        }
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
                            CallContext::InstanceMethod { receiver_type }
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

            (owner_type, call_context)
        } else {
            // External call: no owner_type, default call_context
            (None, CallContext::Direct)
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
            if (effective == global_src || is_name_match) && raw_data.relation_type.is_call() {
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

        Some(ResolvedRelation {
            caller: raw_data.src,
            callee_id: effective_callee_id,
            callee_name,
            relation_type: raw_data.relation_type,
            span: raw_data.span,
            is_external,
            external_type,
            callee_symbol,
            stdlib_category: raw_data.stdlib_category,
            owner_type,
            call_context,
            overload_signature,
        })
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
        raw_relations
            .iter()
            .filter_map(|raw_data| {
                self.resolve_with_scope_map(
                    raw_data,
                    parsed,
                    symbol_table,
                    entity_index,
                    &entity_map,
                )
            })
            .collect()
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

fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_alphanumeric() || c == '_')
}

fn infer_literal_type_shape(text: &str, language: Language) -> Option<TypeShape> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed == "true" || trimmed == "false" {
        return Some(TypeShape::Named(match language {
            Language::Java => "boolean".to_string(),
            Language::TypeScript | Language::JavaScript | Language::Tsx | Language::Jsx => {
                "boolean".to_string()
            }
            _ => "bool".to_string(),
        }));
    }
    if trimmed == "null" || trimmed == "nil" || trimmed == "None" {
        return Some(TypeShape::Named("null".to_string()));
    }
    if (trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2)
        || (trimmed.starts_with('\'') && trimmed.ends_with('\'') && trimmed.len() >= 2)
        || (trimmed.starts_with('`') && trimmed.ends_with('`') && trimmed.len() >= 2)
    {
        return Some(TypeShape::Named(match language {
            Language::Java | Language::CSharp | Language::Scala | Language::Kotlin => {
                "String".to_string()
            }
            Language::Python => "str".to_string(),
            Language::Go => "string".to_string(),
            Language::Rust => "String".to_string(),
            _ => "str".to_string(),
        }));
    }
    if trimmed.parse::<i64>().is_ok() {
        return Some(TypeShape::Named(match language {
            Language::Java | Language::CSharp | Language::Go => "int".to_string(),
            Language::Python => "int".to_string(),
            Language::Rust => "i32".to_string(),
            Language::TypeScript | Language::JavaScript | Language::Tsx | Language::Jsx => {
                "number".to_string()
            }
            _ => "int".to_string(),
        }));
    }
    if trimmed.parse::<f64>().is_ok() {
        return Some(TypeShape::Named(match language {
            Language::Java => "double".to_string(),
            Language::CSharp => "double".to_string(),
            Language::Python => "float".to_string(),
            Language::Go => "float64".to_string(),
            Language::Rust => "f64".to_string(),
            Language::TypeScript | Language::JavaScript | Language::Tsx | Language::Jsx => {
                "number".to_string()
            }
            _ => "float".to_string(),
        }));
    }
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        return Some(TypeShape::Named("array".to_string()));
    }
    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        return Some(TypeShape::Named("object".to_string()));
    }
    None
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
