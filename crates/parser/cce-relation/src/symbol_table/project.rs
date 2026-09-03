//! Project symbol table (project-level)
//!
//! Manages all packages in a project and external dependencies.
//! This is the top-level symbol table in the four-level hierarchy.
//!
//! # Enhanced Features
//! - Module paths and namespaces
//! - Import aliases
//! - Re-exports
//! - External package symbols
//! - Visibility rules

use crate::symbol::{SymbolMetadata, SymbolRef};
use crate::symbol_table::type_index::TypeMemberIndex;
use crate::type_inference::TypeInferenceContext;
use crate::type_inference::cross_file::CrossFilePropagator;
use crate::type_inference::overload::{OverloadCandidate, OverloadSet};
use crate::type_inference::types::{TypeShape, parse_type_shape};
use cce_metrics::domain::pipeline::RelationMetrics;
use cce_types::entity::EntityId;
use cce_types::language::Language;
use cce_types::normalize_project_path;
use dashmap::DashMap;
use lru::LruCache;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

/// Simple-name index entry.
///
/// Explicitly distinguishes the two sources of a bare-name match so
/// resolution can use the entry's own `module_path`/`entity_id` instead of
/// treating the stored file path as a package id
#[derive(Debug, Clone)]
enum SimpleNameEntry {
    /// Symbol defined in a project file (from `insert_symbol`).
    FileSymbol {
        /// Normalized file path of the defining file
        file_path: String,
        /// Module path of the defining file
        module_path: String,
        /// Entity ID (unified with runtime entity ID space)
        entity_id: EntityId,
    },
    /// Package-level public export (from `add_package`).
    PackageExport {
        /// Package id
        package_id: String,
        /// Entity ID (unified with runtime entity ID space)
        entity_id: EntityId,
    },
}

impl SimpleNameEntry {
    /// Deterministic ordering key for candidates of the same simple name.
    ///
    /// The key format is: "{call_frequency}::{file_path}::{module_path}"
    /// where lower values are preferred (higher frequency gets lower key).
    fn sort_key(&self, call_frequency: Option<u64>) -> String {
        // Invert call frequency so higher frequency gets lower sort key (preferred)
        let freq = call_frequency.map(|f| u64::MAX - f).unwrap_or(u64::MAX);
        match self {
            SimpleNameEntry::FileSymbol {
                file_path,
                module_path,
                ..
            } => {
                format!("{}::{}::{}", freq, file_path, module_path)
            }
            SimpleNameEntry::PackageExport { package_id, .. } => {
                format!("{}::{}::", freq, package_id)
            }
        }
    }

    fn entity_id(&self) -> EntityId {
        match self {
            SimpleNameEntry::FileSymbol { entity_id, .. }
            | SimpleNameEntry::PackageExport { entity_id, .. } => *entity_id,
        }
    }

    fn file_path(&self) -> Option<&str> {
        match self {
            SimpleNameEntry::FileSymbol { file_path, .. } => Some(file_path),
            SimpleNameEntry::PackageExport { .. } => None,
        }
    }
}

/// Project symbol table - manages all packages and external dependencies
#[derive(Debug)]
pub struct ProjectSymbolTable {
    /// Project root path
    pub root_path: PathBuf,

    /// Package symbol tables: package_id -> PackageSymbolTable
    ///
    /// Shared via `Arc` so resolution paths clone a refcount instead of
    /// deep-cloning the whole package table per lookup
    packages: DashMap<String, Arc<super::package::PackageSymbolTable>>,

    /// Package-name index: package_name -> PackageSymbolTable.
    ///
    /// Built at `add_package` time so `get_package_by_name` and qualified
    /// resolution no longer scan the full package set per lookup
    packages_by_name: DashMap<String, Arc<super::package::PackageSymbolTable>>,

    /// External dependency symbol tables: package_name -> ExternalSymbolTable
    ///
    /// Shared via `Arc` so resolution paths clone a refcount instead of
    /// deep-cloning the whole exports map per lookup
    external_deps: DashMap<String, Arc<ExternalSymbolTable>>,

    /// Global name index: qualified_name -> EntityId
    /// Format: "package::module::symbol" or "file::symbol"
    global_index: DashMap<String, EntityId>,

    /// Simple name index: name -> Vec<SimpleNameEntry>
    ///
    /// Entries are kept in insertion order; resolution sorts them
    /// deterministically (see [`SimpleNameEntry::sort_key`]) so the first
    /// match never depends on DashMap iteration order
    simple_name_index: DashMap<String, Vec<SimpleNameEntry>>,

    /// Positive resolution cache keyed by (file_path, name).
    ///
    /// Uses an LRU cache with a fixed capacity to automatically evict
    /// least-recently-used entries when the cache is full. This replaces
    /// the previous manual batch-eviction logic with deterministic LRU
    /// eviction.
    ///
    /// Contextualized per file so a result resolved for file A cannot short-
    /// circuit file B's resolution. Only positive results are cached:
    /// caching misses would shadow symbols added later by incremental builds.
    /// Bounded by `SymbolResolutionConfig::resolution_cache_size`.
    resolution_cache: Mutex<LruCache<(String, String), Option<SymbolRef>>>,

    /// Negative resolution cache keyed by (file_path, qualified_name).
    ///
    /// Stores multi-segment qualified misses only, so repeat lookups of the
    /// same absent name skip the package walk. Bounded like the positive cache
    /// and cleared on any symbol-table mutation (`insert_symbol`, `add_package`,
    /// `clear_cache`) so incremental additions can never be shadowed
    negative_cache: DashMap<(String, String), ()>,

    /// Stable entity-id cache: "{name}\0{file}\0{module}" -> EntityId
    ///
    /// Resolution paths allocate through [`Self::entity_ref_for`] so the same
    /// target symbol returns the same `EntityId` on every resolution,
    /// making `RelationSymbolRecord.entity_id` stable across relations and
    /// rebuilds
    entity_id_cache: DashMap<String, EntityId>,

    /// Per-file file-symbol contribution: normalized file_path -> set of simple names
    /// that were inserted as file symbols (qualified `file::symbol` entries).
    /// Enables O(affected) incremental removal without scanning all indexes.
    file_symbol_contrib: DashMap<String, HashSet<String>>,

    /// Cached sorted package list for `resolve_qualified_internal` etc.
    /// Invalidated on `add_package` / `apply_package_delta` / `rebuild_indices`.
    sorted_packages_cache:
        Arc<std::sync::RwLock<Option<Vec<Arc<super::package::PackageSymbolTable>>>>>,

    /// Cached sorted external deps for `resolve_external`.
    sorted_external_cache: Arc<std::sync::RwLock<Option<Vec<Arc<ExternalSymbolTable>>>>>,

    /// Optional metrics sink for observability (sort cache hit/miss).
    metrics_sink: Arc<std::sync::RwLock<Option<Arc<RelationMetrics>>>>,

    /// Global aggregated type-member index (merged from all ModuleSymbolTable type indexes)
    global_type_index: Arc<std::sync::RwLock<TypeMemberIndex>>,

    /// Per-file type contribution for incremental pruning
    file_type_contrib: DashMap<String, HashSet<String>>,

    /// Per-file type inference contexts for lightweight type inference.
    /// Keyed by normalized file path.
    type_inference_contexts: DashMap<String, TypeInferenceContext>,

    /// Wildcard import expansion cache: (file_path, source_module) -> Vec<SymbolRef>
    ///
    /// Caches expanded symbols from wildcard imports to avoid re-expanding
    /// the same wildcard on every lookup. Invalidated when modules are added
    /// or removed.
    wildcard_expansion_cache: DashMap<(String, String), Vec<SymbolRef>>,

    /// Cross-file type propagator for return-type caching.
    ///
    /// Stores `High`/`Medium` confidence return types from all files so
    /// callers in other files can infer variable types from
    /// `x = callee()` patterns. Populated during symbol-table build and
    /// kept consistent via incremental updates.
    cross_file_propagator: CrossFilePropagator,

    /// Overload sets indexed by (owner_type, method_name)
    overload_sets: DashMap<(String, String), OverloadSet>,

    /// Overload sets indexed by simple name (for bare name resolution)
    overload_by_name: DashMap<String, Vec<OverloadSet>>,

    /// Type inference cache: file_path -> TypeInferenceContext
    inference_cache: DashMap<String, TypeInferenceContext>,

    /// Call frequency for symbols: entity_id -> count
    /// Used for ranking candidates by usage frequency
    call_frequency: DashMap<EntityId, u64>,

    /// File path -> package_id mapping for O(1) package lookup.
    ///
    /// Avoids linear scans over all packages in hot resolution paths
    /// (`resolve_via_local_scope`, `resolve_via_module_import`).
    file_to_package: DashMap<String, String>,

    /// Namespace-qualified index: "package::namespace::symbol" -> EntityId
    namespace_index: DashMap<String, EntityId>,

    /// Symbol resolution tuning knobs (replaces hard-coded constants).
    symbol_resolution_config: Arc<std::sync::RwLock<cce_config::SymbolResolutionConfig>>,
}

/// External dependency symbol information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalSymbolTable {
    /// Package name
    pub package_name: String,

    /// Version (if known)
    pub version: Option<String>,

    /// Content hash for cache validation
    pub content_hash: Option<String>,

    /// Exported symbols
    exports: HashMap<String, SymbolMetadata>,

    /// Language
    pub language: Language,
}

impl ExternalSymbolTable {
    /// Create a new external symbol table
    pub fn new(package_name: String, version: Option<String>, language: Language) -> Self {
        Self {
            package_name,
            version,
            content_hash: None,
            exports: HashMap::new(),
            language,
        }
    }

    /// Set content hash for cache validation
    pub fn set_content_hash(&mut self, hash: String) {
        self.content_hash = Some(hash);
    }

    /// Get content hash
    pub fn content_hash(&self) -> Option<&str> {
        self.content_hash.as_deref()
    }

    /// Add an export
    pub fn add_export(&mut self, name: String, metadata: SymbolMetadata) {
        self.exports.insert(name, metadata);
    }

    /// Get an export
    pub fn get_export(&self, name: &str) -> Option<&SymbolMetadata> {
        self.exports.get(name)
    }

    /// Get all exports
    pub fn all_exports(&self) -> &HashMap<String, SymbolMetadata> {
        &self.exports
    }
}

impl ProjectSymbolTable {
    /// Create a new project symbol table with default resolution config.
    pub fn new(root_path: PathBuf) -> Self {
        Self::new_with_config(root_path, cce_config::SymbolResolutionConfig::default())
    }

    /// Create a new project symbol table with explicit resolution config.
    pub fn new_with_config(root_path: PathBuf, config: cce_config::SymbolResolutionConfig) -> Self {
        let cache_capacity = NonZeroUsize::new(config.resolution_cache_size)
            .unwrap_or(NonZeroUsize::new(4096).expect("4096 is non-zero"));
        Self {
            root_path,
            packages: DashMap::new(),
            packages_by_name: DashMap::new(),
            external_deps: DashMap::new(),
            global_index: DashMap::new(),
            simple_name_index: DashMap::new(),
            resolution_cache: Mutex::new(LruCache::new(cache_capacity)),
            negative_cache: DashMap::new(),
            entity_id_cache: DashMap::new(),
            file_symbol_contrib: DashMap::new(),
            sorted_packages_cache: Arc::new(std::sync::RwLock::new(None)),
            sorted_external_cache: Arc::new(std::sync::RwLock::new(None)),
            metrics_sink: Arc::new(std::sync::RwLock::new(None)),
            global_type_index: Arc::new(std::sync::RwLock::new(TypeMemberIndex::new())),
            file_type_contrib: DashMap::new(),
            type_inference_contexts: DashMap::new(),
            wildcard_expansion_cache: DashMap::new(),
            cross_file_propagator: CrossFilePropagator::new(),
            overload_sets: DashMap::new(),
            overload_by_name: DashMap::new(),
            inference_cache: DashMap::new(),
            call_frequency: DashMap::new(),
            file_to_package: DashMap::new(),
            namespace_index: DashMap::new(),
            symbol_resolution_config: Arc::new(std::sync::RwLock::new(config)),
        }
    }

    /// Access the current symbol resolution config.
    pub fn symbol_resolution_config(&self) -> cce_config::SymbolResolutionConfig {
        self.symbol_resolution_config
            .read()
            .map(|g| g.clone())
            .unwrap_or_default()
    }

    /// Replace the symbol resolution config at runtime.
    pub fn set_symbol_resolution_config(&self, config: cce_config::SymbolResolutionConfig) {
        if let Ok(mut guard) = self.symbol_resolution_config.write() {
            *guard = config;
        }
    }

    /// Effective re-export chain depth limit.
    pub fn max_reexport_chain_depth(&self) -> usize {
        self.symbol_resolution_config().max_reexport_chain_depth
    }

    /// Effective scope chain depth limit.
    pub fn max_scope_chain_depth(&self) -> usize {
        self.symbol_resolution_config().max_scope_chain_depth
    }

    /// Effective resolution cache capacity.
    pub fn resolution_cache_capacity(&self) -> usize {
        self.symbol_resolution_config().resolution_cache_size
    }

    /// Effective wildcard expansion limit (0 = unlimited).
    pub fn max_wildcard_expansion_size(&self) -> usize {
        self.symbol_resolution_config().max_wildcard_expansion_size
    }

    /// Whether wildcard expansion is disabled.
    pub fn is_wildcard_expansion_disabled(&self) -> bool {
        self.symbol_resolution_config().disable_wildcard_expansion
    }

    /// Resolve a symbol within a specific namespace.
    pub fn resolve_in_namespace(
        &self,
        package: &str,
        namespace: &str,
        symbol_name: &str,
    ) -> Option<SymbolRef> {
        let key = format!("{}::{}::{}", package, namespace, symbol_name);
        if let Some(entity_id) = self.namespace_index.get(&key).map(|id| *id) {
            // Try to find metadata via package modules
            if let Some(pkg) = self.get_package_by_name(package) {
                for module in pkg.modules_in_namespace(namespace) {
                    if let Some(meta) = module.get_export(symbol_name) {
                        if self
                            .global_index
                            .iter()
                            .any(|e| *e.value() == entity_id && e.key().ends_with(symbol_name))
                        {
                            return Some(self.stable_symbol_ref(meta));
                        }
                        return Some(self.stable_symbol_ref(meta));
                    }
                }
                // Fallback: search any module's exports
                for module in pkg.all_modules() {
                    if let Some(meta) = module.get_export(symbol_name) {
                        // Check if this module belongs to the namespace
                        if let Some(ns) = module.namespace_path.namespace_prefix() {
                            if ns == namespace {
                                return Some(self.stable_symbol_ref(meta));
                            }
                        }
                    }
                }
            }
            // If we have entity_id but no module found, try global fallback
            for pkg in self.sorted_packages() {
                if pkg.package_name == package {
                    if let Some(meta) = pkg.get_public_export(symbol_name) {
                        return Some(self.stable_symbol_ref(&meta));
                    }
                }
            }
            // As last resort, construct via global_index lookup
            if let Some(metadata) = self.metadata_for_global_key(&key, None) {
                return Some(self.stable_symbol_ref(&metadata));
            }
            let _ = entity_id;
        }
        // fallback: traverse namespace modules
        if let Some(pkg) = self.get_package_by_name(package) {
            for module in pkg.modules_in_namespace(namespace) {
                if let Some(meta) = module.get_export(symbol_name) {
                    return Some(self.stable_symbol_ref(meta));
                }
            }
        }
        None
    }

    /// Expand wildcard import with visibility filtering.
    pub fn expand_wildcard_import(
        &self,
        source_module: &str,
        caller_scope: &crate::symbol::ScopeContext,
    ) -> Vec<crate::symbol::SymbolRef> {
        if self.is_wildcard_expansion_disabled() {
            return Vec::new();
        }
        let cache_key = (caller_scope.file_path.clone(), source_module.to_string());
        if let Some(cached) = self.wildcard_expansion_cache.get(&cache_key) {
            return cached.clone();
        }

        // Resolve source module through packages
        let mut source_module_table: Option<Arc<super::package::PackageSymbolTable>> = None;
        let mut target_module: Option<Arc<super::module::ModuleSymbolTable>> = None;
        for pkg in self.sorted_packages() {
            if let Some(m) = pkg.resolve_module_path(source_module, Some(&caller_scope.file_path)) {
                source_module_table = Some(pkg);
                target_module = Some(m);
                break;
            }
            if let Some(m) = pkg.get_module_by_path(source_module) {
                source_module_table = Some(pkg);
                target_module = Some(m);
                break;
            }
        }
        let Some(target) = target_module else {
            return Vec::new();
        };

        let max_size = self.max_wildcard_expansion_size();
        let expanded: Vec<crate::symbol::SymbolRef> = target
            .exports_visible_from(caller_scope)
            .into_iter()
            .take(max_size)
            .map(|(_, metadata)| self.stable_symbol_ref(metadata))
            .collect();

        if expanded.len() >= max_size {
            tracing::warn!(
                "Wildcard import from {} reached limit ({}), results may be incomplete",
                source_module,
                max_size
            );
        }

        self.wildcard_expansion_cache
            .insert(cache_key, expanded.clone());
        let _ = source_module_table;
        expanded
    }

    /// Register a file to package mapping for O(1) lookup.
    pub fn register_file_package(&self, file_path: &str, package_id: &str) {
        let normalized = normalize_project_path(file_path);
        self.file_to_package
            .insert(normalized, package_id.to_string());
    }

    /// Get the package containing the given file in O(1).
    pub fn get_package_for_file(
        &self,
        file_path: &str,
    ) -> Option<Arc<super::package::PackageSymbolTable>> {
        let normalized = normalize_project_path(file_path);
        let package_id = self.file_to_package.get(&normalized)?;
        self.packages
            .get(package_id.value())
            .map(|p| p.value().clone())
    }

    /// Remove stale file->package entries not present in `valid_files`.
    pub fn prune_file_package_mappings(&self, valid_files: &std::collections::HashSet<String>) {
        let normalized_valid: std::collections::HashSet<String> = valid_files
            .iter()
            .map(|p| normalize_project_path(p))
            .collect();
        self.file_to_package
            .retain(|k, _| normalized_valid.contains(k));
    }

    /// Access the cross-file type propagator.
    pub fn cross_file_propagator(&self) -> &CrossFilePropagator {
        &self.cross_file_propagator
    }

    /// Update call frequency for a symbol
    pub fn update_call_frequency(&self, entity_id: EntityId) {
        *self.call_frequency.entry(entity_id).or_insert(0) += 1;
    }

    /// Get call frequency for a symbol
    pub fn get_call_frequency(&self, entity_id: EntityId) -> u64 {
        self.call_frequency.get(&entity_id).map(|r| *r).unwrap_or(0)
    }

    /// Get an overload set for a given owner and method.
    pub fn get_overload_set(&self, owner_type: &str, method_name: &str) -> Option<OverloadSet> {
        self.overload_sets
            .get(&(owner_type.to_string(), method_name.to_string()))
            .map(|s| s.clone())
    }

    /// Build overload sets from the global type index.
    ///
    /// For each type with multiple members sharing the same name, creates an
    /// `OverloadSet` containing `OverloadCandidate` entries with populated
    /// `parameter_types` (from the type inference context) and `return_type`.
    pub fn rebuild_overload_sets(&self) {
        self.overload_sets.clear();
        self.overload_by_name.clear();
        let global = self
            .global_type_index
            .read()
            .expect("global_type_index lock");
        for type_entry in global.all_types() {
            let owner = type_entry.key.qualified.clone();
            for (method_name, members) in &type_entry.members {
                if members.len() <= 1 {
                    continue;
                }
                let mut set = OverloadSet::new(method_name.clone(), owner.clone());
                for member in members {
                    let (parameter_types, return_type) = self.load_overload_candidate_types(member);
                    let candidate = OverloadCandidate {
                        entity_id: member.entity_id,
                        parameter_types,
                        return_type,
                        specificity: members.len() as u32,
                    };
                    set.add_candidate(candidate);
                }
                if set.candidates.len() > 1 {
                    self.overload_sets
                        .insert((owner.clone(), method_name.clone()), set.clone());
                    self.overload_by_name
                        .entry(method_name.clone())
                        .or_default()
                        .push(set);
                }
            }
        }
    }

    /// Get overload sets by simple name
    pub fn get_overload_sets_by_name(&self, name: &str) -> Vec<OverloadSet> {
        self.overload_by_name
            .get(name)
            .map(|sets| sets.clone())
            .unwrap_or_default()
    }

    /// Check if a name has multiple overload candidates
    pub fn has_overload_candidates(&self, name: &str) -> bool {
        self.overload_by_name
            .get(name)
            .map(|sets| sets.iter().any(|s| s.candidates.len() > 1))
            .unwrap_or(false)
    }

    /// Get cached type inference context for a file
    pub fn get_inference_cache(&self, file_path: &str) -> Option<TypeInferenceContext> {
        let normalized = normalize_project_path(file_path);
        self.inference_cache.get(&normalized).map(|ctx| ctx.clone())
    }

    /// Cache type inference context for a file
    pub fn set_inference_cache(&self, file_path: &str, ctx: TypeInferenceContext) {
        let normalized = normalize_project_path(file_path);
        self.inference_cache.insert(normalized, ctx);
    }

    /// Invalidate inference cache for a file
    pub fn invalidate_inference_cache(&self, file_path: &str) {
        let normalized = normalize_project_path(file_path);
        self.inference_cache.remove(&normalized);
    }

    /// Clear all inference cache
    pub fn clear_inference_cache(&self) {
        self.inference_cache.clear();
    }

    /// Load parameter and return types for an overload candidate.
    ///
    /// Looks up the type inference context for the member's file and extracts
    /// parameter types. Falls back to parsing the entity's parameter type
    /// annotations if the type inference context is not available.
    fn load_overload_candidate_types(
        &self,
        member: &crate::symbol_table::type_index::MemberEntry,
    ) -> (Vec<TypeShape>, TypeShape) {
        let normalized = normalize_project_path(&member.file_path);

        // Try type inference context first (has parsed TypeShape)
        if let Some(type_ctx) = self.type_inference_contexts.get(&normalized) {
            if let Some(param_bindings) = type_ctx.get_parameter_types(member.entity_id) {
                let parameter_types: Vec<TypeShape> = param_bindings
                    .iter()
                    .filter_map(|b| {
                        b.shape
                            .clone()
                            .or_else(|| parse_type_shape(&b.type_name, Language::Unknown))
                    })
                    .collect();
                if !parameter_types.is_empty() {
                    let return_type = type_ctx
                        .get_return_type(member.entity_id)
                        .and_then(|b| {
                            b.shape
                                .clone()
                                .or_else(|| parse_type_shape(&b.type_name, Language::Unknown))
                        })
                        .unwrap_or(TypeShape::Named("unknown".to_string()));
                    return (parameter_types, return_type);
                }
            }
        }

        // Fallback: no parameter types available
        (vec![], TypeShape::Named("unknown".to_string()))
    }

    /// Attach a metrics sink for observability.
    pub fn set_metrics(&self, metrics: Arc<RelationMetrics>) {
        if let Ok(mut guard) = self.metrics_sink.write() {
            *guard = Some(metrics);
        }
    }

    /// Synthetic EntityId marker bit (high bit 63).
    pub const SYNTHETIC_MARK: u64 = 1u64 << 63;

    /// Whether the given EntityId is a synthetic symbol-table ID.
    pub fn is_synthetic_id(id: EntityId) -> bool {
        id.0 & Self::SYNTHETIC_MARK != 0
    }

    /// Extract counter from a synthetic EntityId.
    pub fn synthetic_counter(id: EntityId) -> u64 {
        id.0 & !Self::SYNTHETIC_MARK
    }

    /// Build a `SymbolRef` for a target symbol with a stable entity id.
    ///
    /// The id is cached per `(name, defining file, module path)` so repeated
    /// resolution of the same target (across relations and rebuilds) returns
    /// the same `EntityId`
    pub fn symbol_ref_for(&self, metadata: &SymbolMetadata, module_path: &str) -> SymbolRef {
        let cache_key = format!(
            "{}\0{}\0{}",
            metadata.name_str(),
            metadata.location.file_path,
            module_path
        );
        // Compute the counter BEFORE acquiring the entry lock to avoid
        // self-deadlock: DashMap::len() acquires read locks on all shards,
        // which would conflict with the write lock held by entry().
        let counter = self.entity_id_cache.len() as u64;
        // For new entries, we need to generate a unique EntityId.
        // We use a high-bit prefix to avoid collision with real entity IDs.
        let entity_id = *self.entity_id_cache.entry(cache_key).or_insert_with(|| {
            // Use a counter in the high bits to generate unique IDs
            // that won't collide with real EntityIds from file processing.
            // Real EntityIds start from 0 and increment; synthetic ones
            // use bit 63 as a flag.
            EntityId(Self::SYNTHETIC_MARK | counter)
        });
        SymbolRef::new(entity_id, metadata.clone())
    }

    /// Insert a symbol into the global index
    ///
    /// This is used for building the symbol table from parsed files
    /// during batch processing in hot update scenarios.
    pub fn insert_symbol(
        &self,
        qualified_name: String,
        entity_id: EntityId,
        file_path: String,
        module_path: String,
    ) {
        // both registration and query sides use the canonical normalized
        // path form, so `./`/relative/absolute spellings of the same file
        // resolve to the same key.
        let file_path = normalize_project_path(&file_path);
        // the simple-name index must be keyed by the bare name (the
        // last `::` segment); callers query it with `get_by_simple_name`.
        // Indexing the full qualified name made file symbols permanently
        // invisible to simple-name lookups.
        let simple_name = qualified_name
            .rsplit("::")
            .next()
            .map(|segment| segment.to_string())
            .unwrap_or_else(|| qualified_name.clone());
        // re-adding the same file must not accumulate duplicate
        // entries; keep the newest registration for the same file+module.
        if let Some(mut entries) = self.simple_name_index.get_mut(&simple_name) {
            entries.retain(|entry| entry.file_path() != Some(file_path.as_str()));
        }
        self.insert_simple_name_entry(
            &simple_name,
            SimpleNameEntry::FileSymbol {
                file_path: file_path.clone(),
                module_path: module_path.clone(),
                entity_id,
            },
        );
        self.file_symbol_contrib
            .entry(file_path.clone())
            .or_default()
            .insert(simple_name.clone());
        // namespace-level index: "package::namespace::symbol"
        {
            use cce_types::NamespacePath;
            let ns_path = NamespacePath::parse(&qualified_name);
            if let Some(ns_prefix) = ns_path.namespace_prefix() {
                if let Some(pkg_name) = module_path.split("::").next() {
                    if !pkg_name.is_empty() {
                        let ns_key = format!("{}::{}::{}", pkg_name, ns_prefix, simple_name);
                        self.namespace_index.insert(ns_key, entity_id);
                    }
                }
            }
        }
        // Update global index for this specific symbol (maintaining cache consistency)
        let file_qualified = format!("{}::{}", file_path, simple_name);
        self.global_index.insert(file_qualified, entity_id);
        // Use fine-grained invalidation: only entries that could be affected
        // by this specific symbol are evicted, preserving unrelated cache hits
        // during hot-update prefill where many symbols are inserted.
        self.invalidate_cache_for_symbol(&simple_name, &file_path);
    }

    pub fn register_workspace_member(
        &self,
        member_name: &str,
        member_path: &Path,
        exported_symbols: Vec<(String, EntityId)>,
    ) {
        for (symbol_name, entity_id) in exported_symbols {
            self.insert_symbol(
                format!("{}::{}", member_name, symbol_name),
                entity_id,
                member_path.to_string_lossy().to_string(),
                member_name.to_string(),
            );
        }
    }

    /// Remove stale file symbols for `file_path` that are not in
    /// `new_simple_names`. Keeps `simple_name_index` and
    /// `file_symbol_contrib` consistent in O(affected) time.
    pub fn prune_file_symbols(&self, file_path: &str, new_simple_names: &HashSet<String>) {
        let normalized = normalize_project_path(file_path);
        let old_set = self
            .file_symbol_contrib
            .get(&normalized)
            .map(|s| s.clone())
            .unwrap_or_default();
        for name in old_set.difference(new_simple_names) {
            if let Some(mut entries) = self.simple_name_index.get_mut(name) {
                entries.retain(|e| e.file_path() != Some(normalized.as_str()));
                if entries.is_empty() {
                    drop(entries);
                    self.simple_name_index.remove(name);
                }
            }
        }
        // Overwrite the contribution set so the following `insert_symbol`
        // calls see the new baseline (idempotent re-inserts are fine).
        self.file_symbol_contrib
            .insert(normalized, new_simple_names.clone());
        // Use fine-grained cache invalidation: only invalidate entries for this file
        // since we're only removing symbols for this specific file.
        self.invalidate_cache_for_file(file_path);
        // Rebuild global index to keep it consistent after symbol removal
        self.rebuild_global_indices();
    }

    // === TypeMemberIndex Management ===

    pub fn global_type_index(&self) -> std::sync::RwLockReadGuard<'_, TypeMemberIndex> {
        self.global_type_index
            .read()
            .expect("global_type_index lock")
    }

    pub fn global_type_index_mut(&self) -> std::sync::RwLockWriteGuard<'_, TypeMemberIndex> {
        self.global_type_index
            .write()
            .expect("global_type_index lock")
    }

    pub fn rebuild_global_type_index(&self) {
        let mut global = self
            .global_type_index
            .write()
            .expect("global_type_index lock");
        global.clear();
        self.file_type_contrib.clear();
        // Collect per-module type indexes for placeholder completion
        let mut module_indexes: Vec<TypeMemberIndex> = Vec::new();
        let mut total_dups: u64 = 0;
        for package in self.packages.iter() {
            for module in package.all_modules() {
                let idx = module.type_index();
                if !idx.is_empty() {
                    // track contribution for incremental
                    let mut set = HashSet::new();
                    for key in idx.type_keys() {
                        set.insert(key.qualified.clone());
                    }
                    self.file_type_contrib.insert(module.file_path.clone(), set);
                    module_indexes.push(idx.clone());
                    let dups = global.merge_from(idx);
                    total_dups += dups.len() as u64;
                }
            }
        }
        // Complete placeholders: merge placeholder types into real types
        // that were defined in other files
        for module_idx in &module_indexes {
            global.complete_placeholders_from(module_idx);
        }
        if total_dups > 0 {
            if let Some(metrics) = self.metrics_sink.read().ok().and_then(|g| g.clone()) {
                metrics.type_member_duplicate_total.increment_by(total_dups);
            }
        }
        self.clear_cache();
    }

    pub fn prune_file_types(&self, file_path: &str, new_qualified: &HashSet<String>) {
        let normalized = normalize_project_path(file_path);
        let _old_set = self
            .file_type_contrib
            .get(&normalized)
            .map(|s| s.clone())
            .unwrap_or_default();
        // Remove stale types via global index file-based removal
        {
            let mut global = self
                .global_type_index
                .write()
                .expect("global_type_index lock");
            global.remove_file_contribution(&normalized);
            // Re-merge remaining modules' type indexes that still contribute to removed qualified names?
            // For simplicity, after removal we already cleared contributions for that file.
            // The remaining global still has correct state because other files' contributions remain.
        }
        self.file_type_contrib
            .insert(normalized, new_qualified.clone());
        // Use fine-grained cache invalidation: only invalidate entries for this file
        // since we're only removing type information for this specific file.
        self.invalidate_cache_for_file(file_path);
    }

    pub fn apply_type_delta_for_file(&self, file_path: &str, new_index: &TypeMemberIndex) {
        let normalized = normalize_project_path(file_path);
        let dup_count = {
            let mut global = self
                .global_type_index
                .write()
                .expect("global_type_index lock");
            global.remove_file_contribution(&normalized);
            if !new_index.is_empty() {
                let dups = global.merge_from(new_index);
                // Complete any placeholders that now have a real type target
                global.complete_placeholders_from(new_index);
                dups.len() as u64
            } else {
                0
            }
        };
        if dup_count > 0 {
            if let Some(metrics) = self.metrics_sink.read().ok().and_then(|g| g.clone()) {
                metrics.type_member_duplicate_total.increment_by(dup_count);
            }
        }
        let mut set = HashSet::new();
        for k in new_index.type_keys() {
            set.insert(k.qualified.clone());
        }
        self.file_type_contrib.insert(normalized, set);
        // Use fine-grained cache invalidation: only invalidate entries for this file
        // since we're only updating type information for this specific file.
        self.invalidate_cache_for_file(file_path);
    }

    // === Package Management ===

    /// Insert a simple-name entry keeping the candidate vector deterministically
    /// sorted, so lookup paths can iterate the first matching entry without
    /// re-sorting on every resolution
    fn insert_simple_name_entry(&self, name: &str, entry: SimpleNameEntry) {
        let entity_id = entry.entity_id();
        let call_frequency = self.get_call_frequency(entity_id);
        let mut entries = self.simple_name_index.entry(name.to_string()).or_default();
        let pos = entries
            .binary_search_by(|existing| {
                let existing_freq = self.get_call_frequency(existing.entity_id());
                existing
                    .sort_key(Some(existing_freq))
                    .cmp(&entry.sort_key(Some(call_frequency)))
            })
            .unwrap_or_else(|pos| pos);
        entries.insert(pos, entry);
    }

    /// Rebuild the global index from simple_name_index and packages.
    ///
    /// This method reconstructs the flat global_index from the hierarchical
    /// simple_name_index and package exports, ensuring consistency after
    /// symbol removal or addition.
    pub fn rebuild_global_indices(&self) {
        self.global_index.clear();

        // Rebuild from file symbols (simple_name_index)
        for entries in self.simple_name_index.iter() {
            let simple_name = entries.key();
            for entry in entries.value() {
                match entry {
                    SimpleNameEntry::FileSymbol {
                        file_path,
                        module_path: _,
                        entity_id,
                    } => {
                        let qualified_name = format!("{}::{}", file_path, simple_name);
                        self.global_index.insert(qualified_name, *entity_id);
                    }
                    SimpleNameEntry::PackageExport {
                        package_id,
                        entity_id,
                    } => {
                        let qualified_name = format!("{}::{}", package_id, simple_name);
                        self.global_index.insert(qualified_name, *entity_id);
                    }
                }
            }
        }
    }

    /// Resolve a qualified name through the hierarchical structure.
    ///
    /// This method provides an alternative to direct global_index lookup,
    /// querying the hierarchical structure for qualified name resolution.
    pub fn resolve_global_qualified(&self, qualified_name: &str) -> Option<EntityId> {
        // First try the global index (fast path)
        if let Some(entity_id) = self.global_index.get(qualified_name) {
            return Some(*entity_id);
        }

        // Fallback: try to resolve through hierarchical structure
        // For qualified names like "package::module::symbol", try different segments
        let parts: Vec<&str> = qualified_name.split("::").collect();
        if parts.len() < 2 {
            return None;
        }

        // Try to find in packages
        for package in self.packages.iter() {
            if package.package_name == parts[0] {
                // Try to find in package exports
                if let Some(metadata) = package.get_public_export(&parts[1..].join("::")) {
                    // This is a simplified resolution - in practice would need full module path resolution
                    // For now, return None and let the normal resolution path handle it
                    let _ = metadata;
                }
            }
        }

        None
    }
}

/// Resolution context for enhanced symbol resolution
#[derive(Debug, Clone)]
pub struct ResolutionContext {
    /// Current file path
    pub file_path: String,

    /// Current module path
    pub module_path: Vec<String>,

    /// Scope chain (innermost to outermost) for local scope resolution
    pub scope_chain: Vec<EntityId>,
}

/// Project statistics
#[derive(Debug, Clone)]
pub struct ProjectStats {
    /// Number of packages
    pub package_count: usize,

    /// Number of external dependencies
    pub external_dep_count: usize,

    /// Total number of symbols
    pub total_symbols: usize,
}

mod enhanced_resolution;
pub mod qualified_resolution;
mod registry;
mod stats_and_indices;
pub use qualified_resolution::OverloadContext;
#[cfg(test)]
mod tests;
