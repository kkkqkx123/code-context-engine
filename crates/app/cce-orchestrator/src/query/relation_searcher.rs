//! Relation searcher for call chain and inheritance queries
//!
//! Provides a unified interface for relation queries with pagination
//! and error handling.

use cce_relation::index::{
    SnapshotEntityQueryOps, SnapshotHierarchyQueryOps, SnapshotRelationQueryOps,
};
use cce_relation::query::QueryCache;
use cce_relation::{CallChainNode, CallChainQuery};
use cce_types::{EntityId, RelationType, ResolvedRelation, TestInfo, language::LanguageInfo};
use parking_lot::RwLock;
use std::collections::HashSet;
use std::sync::Arc;

use super::error::Result;
use super::graph::GraphFilter;
use super::types::ExcludableContentType;

/// Relation query options
#[derive(Debug, Clone)]
pub struct RelationQueryOptions {
    /// Maximum depth for traversal
    pub max_depth: usize,
    /// Pagination offset
    pub offset: usize,
    /// Pagination limit
    pub limit: usize,
    /// Include start node in results
    pub include_start: bool,
    /// Keep only entities whose file path is under this directory prefix
    pub directory_prefix: Option<String>,
    /// Content types to exclude (mirrors the main query path)
    pub exclude_content_types: Vec<ExcludableContentType>,
    /// Exact file paths to exclude (normalized project paths)
    pub excluded_files: Vec<String>,
    /// Keep only relations in these coarse domains
    /// (`call`, `dependency`, `structural`, `reference`, `template`, `other`).
    /// Empty means no domain filtering.
    pub relation_domains: Vec<String>,
    /// Whether to keep edges pointing outside the indexed project.
    pub include_external: bool,
}

impl Default for RelationQueryOptions {
    fn default() -> Self {
        Self {
            max_depth: 3,
            offset: 0,
            limit: 20,
            include_start: false,
            directory_prefix: None,
            exclude_content_types: Vec::new(),
            excluded_files: Vec::new(),
            relation_domains: Vec::new(),
            include_external: true,
        }
    }
}

impl RelationQueryOptions {
    /// Create new options with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Set maximum depth
    pub fn with_max_depth(mut self, max_depth: usize) -> Self {
        self.max_depth = max_depth;
        self
    }

    /// Set pagination offset
    pub fn with_offset(mut self, offset: usize) -> Self {
        self.offset = offset;
        self
    }

    /// Set pagination limit
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    /// Set whether to include start node
    pub fn with_include_start(mut self, include_start: bool) -> Self {
        self.include_start = include_start;
        self
    }

    /// Set directory prefix filter
    pub fn with_directory_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.directory_prefix = Some(prefix.into());
        self
    }

    /// Exclude test files from results
    pub fn with_exclude_tests(mut self, exclude: bool) -> Self {
        if exclude
            && !self
                .exclude_content_types
                .contains(&ExcludableContentType::Test)
        {
            self.exclude_content_types.push(ExcludableContentType::Test);
        }
        self
    }

    /// Set exact file paths to exclude
    pub fn with_excluded_files(mut self, files: Vec<String>) -> Self {
        self.excluded_files = files;
        self
    }

    /// Keep only relations in the given coarse domains.
    pub fn with_relation_domains(mut self, domains: Vec<String>) -> Self {
        self.relation_domains = domains;
        self
    }

    /// Set whether edges pointing outside the project are kept.
    pub fn with_include_external(mut self, include: bool) -> Self {
        self.include_external = include;
        self
    }
}

/// Coarse domain name for a relation type, mirroring the graph model.
fn relation_domain_name(relation_type: &cce_types::RelationType) -> &'static str {
    if relation_type.is_call() {
        "call"
    } else if relation_type.is_dependency() {
        "dependency"
    } else if relation_type.is_structural() {
        "structural"
    } else if relation_type.is_reference() {
        "reference"
    } else if relation_type.is_template_relation() {
        "template"
    } else {
        "other"
    }
}

/// File-level post-filter derived from `RelationQueryOptions`.
///
/// The relation snapshot is a pure in-memory index (no storage-layer filter
/// pushdown like Qdrant/tantivy), so filtering is applied on query output
/// and direct-neighbor results. Test-file detection reuses the single
/// authoritative rule set (`cce_types::TestInfo`), shared with the main
/// query path.
#[derive(Debug, Default, Clone)]
struct RelationFileFilter {
    directory_prefix: Option<String>,
    exclude_tests: bool,
    excluded_files: HashSet<String>,
    relation_domains: HashSet<String>,
    include_external: bool,
}

impl RelationFileFilter {
    /// Deterministic fingerprint for cache keys covering all filter state.
    fn fingerprint(options: &RelationQueryOptions) -> String {
        let mut domains: Vec<&str> = options
            .relation_domains
            .iter()
            .map(|s| s.as_str())
            .collect();
        domains.sort_unstable();
        let mut excluded: Vec<&str> = options.excluded_files.iter().map(|s| s.as_str()).collect();
        excluded.sort_unstable();
        let mut content_types: Vec<String> = options
            .exclude_content_types
            .iter()
            .map(|c| format!("{c:?}"))
            .collect();
        content_types.sort();
        format!(
            "dir={:?}|tests={}|excl={:?}|domains={:?}|ext={}",
            options.directory_prefix,
            content_types.join(","),
            excluded,
            domains,
            options.include_external
        )
    }

    fn from_options(options: &RelationQueryOptions) -> Self {
        Self {
            directory_prefix: options
                .directory_prefix
                .as_deref()
                .map(|p| cce_types::normalize_project_path(p.trim_matches('/'))),
            exclude_tests: options
                .exclude_content_types
                .contains(&ExcludableContentType::Test),
            excluded_files: options
                .excluded_files
                .iter()
                .map(|f| cce_types::normalize_project_path(f))
                .collect(),
            relation_domains: options.relation_domains.iter().cloned().collect(),
            include_external: options.include_external,
        }
    }

    fn is_empty(&self) -> bool {
        self.directory_prefix.is_none()
            && !self.exclude_tests
            && self.excluded_files.is_empty()
            && self.relation_domains.is_empty()
            && self.include_external
    }

    fn matches_relation(&self, relation: &ResolvedRelation) -> bool {
        if !self.include_external && relation.is_external {
            return false;
        }
        if self.relation_domains.is_empty() {
            return true;
        }
        self.relation_domains
            .contains(relation_domain_name(&relation.relation_type))
    }

    /// Whether a file path passes the filter. An unknown path is only kept
    /// when no directory constraint is active, so a scoped query never
    /// reports nodes it cannot place.
    fn matches_path(&self, path: Option<&str>) -> bool {
        let Some(path) = path else {
            return self.directory_prefix.is_none();
        };
        let normalized = cce_types::normalize_project_path(path);
        if let Some(prefix) = &self.directory_prefix {
            let inside = normalized == *prefix || normalized.starts_with(&format!("{prefix}/"));
            if !inside {
                return false;
            }
        }
        if self.exclude_tests {
            let info = LanguageInfo::detect_from_path(&normalized);
            if TestInfo::from_path(Some(&info.language), &normalized).is_test() {
                return false;
            }
        }
        if self.excluded_files.contains(&normalized) {
            return false;
        }
        true
    }
}

/// Path query options
#[derive(Debug, Clone)]
pub struct PathQueryOptions {
    /// Maximum depth for path search
    pub max_depth: usize,
    /// Maximum nodes to visit (safety limit)
    pub max_nodes: usize,
}

impl Default for PathQueryOptions {
    fn default() -> Self {
        Self {
            max_depth: 10,
            max_nodes: 10000,
        }
    }
}

impl PathQueryOptions {
    /// Create new options with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Set maximum depth
    pub fn with_max_depth(mut self, max_depth: usize) -> Self {
        self.max_depth = max_depth;
        self
    }

    /// Set maximum nodes
    pub fn with_max_nodes(mut self, max_nodes: usize) -> Self {
        self.max_nodes = max_nodes;
        self
    }
}

/// Relation searcher
///
/// Provides unified interface for relation queries with:
/// - Pagination support
/// - Error handling
/// - LRU caching for hot queries
///
/// This is a thin wrapper over `CallChainQuery` as per the architecture
/// simplification plan (merged `RelationSearcher` + `CallChainQuery`).
pub struct RelationSearcher {
    query: Arc<CallChainQuery>,
    cache: Arc<RwLock<QueryCache>>,
}

impl RelationSearcher {
    /// Create a new relation searcher
    pub fn new(query: Arc<CallChainQuery>) -> Self {
        Self {
            query,
            cache: Arc::new(RwLock::new(QueryCache::new(128))),
        }
    }

    /// Create from an existing query
    pub fn from_query(query: CallChainQuery) -> Self {
        Self::new(Arc::new(query))
    }

    /// Get a reference to the underlying query
    pub fn query(&self) -> &CallChainQuery {
        &self.query
    }

    /// Access the query cache
    pub fn cache(&self) -> &RwLock<QueryCache> {
        &self.cache
    }

    // ========== Direct Relation Queries ==========

    /// Get callees for traversal expansion.
    ///
    /// Missing entities yield an empty neighbor list so graph expansion can
    /// proceed without special-casing unknown ids. Direct lookups that need
    /// to distinguish missing from empty must use `get_callees_checked`.
    pub fn get_callees(&self, entity_id: EntityId) -> Vec<ResolvedRelation> {
        self.get_callees_checked(entity_id).unwrap_or_default()
    }

    /// Get callees with explicit missing-entity errors.
    pub fn get_callees_checked(
        &self,
        entity_id: EntityId,
    ) -> std::result::Result<Vec<ResolvedRelation>, cce_relation::RelationQueryError> {
        self.query.get_callees_by_entity(entity_id)
    }

    /// Get callers (functions that call this function) with caching
    pub fn get_callers(&self, entity_id: EntityId) -> Vec<EntityId> {
        if let Some(cached) = self.cache.write().get_callers(entity_id).cloned() {
            return cached;
        }
        let callers = self.query.get_callers_by_entity(entity_id);
        self.cache.write().put_callers(entity_id, callers.clone());
        callers
    }

    /// Get callees with pagination (file-filtered per options)
    pub fn get_callees_paginated(
        &self,
        entity_id: EntityId,
        options: &RelationQueryOptions,
    ) -> Vec<ResolvedRelation> {
        self.filter_callees(entity_id, options)
            .into_iter()
            .skip(options.offset)
            .take(options.limit)
            .collect()
    }

    /// Get callees after applying file and relation filters (pre-pagination).
    pub fn filter_callees(
        &self,
        entity_id: EntityId,
        options: &RelationQueryOptions,
    ) -> Vec<ResolvedRelation> {
        let filter = RelationFileFilter::from_options(options);
        let callees = self.get_callees(entity_id);
        if filter.is_empty() {
            return callees;
        }
        callees
            .into_iter()
            .filter(|relation| {
                if !filter.matches_relation(relation) {
                    return false;
                }
                match relation.callee_id {
                    Some(callee_id) => filter.matches_path(
                        self.query
                            .index()
                            .get_file_path_by_entity(callee_id)
                            .as_deref(),
                    ),
                    None => filter.matches_path(None),
                }
            })
            .collect()
    }

    /// Get callers with pagination (file-filtered per options)
    pub fn get_callers_paginated(
        &self,
        entity_id: EntityId,
        options: &RelationQueryOptions,
    ) -> Vec<EntityId> {
        self.filter_callers(entity_id, options)
            .into_iter()
            .skip(options.offset)
            .take(options.limit)
            .collect()
    }

    /// Get callers after applying the file and relation filters (pre-pagination).
    pub fn filter_callers(
        &self,
        entity_id: EntityId,
        options: &RelationQueryOptions,
    ) -> Vec<EntityId> {
        let filter = RelationFileFilter::from_options(options);
        let callers = self.get_callers(entity_id);
        if filter.is_empty() {
            return callers;
        }
        callers
            .into_iter()
            .filter(|caller_id| {
                if !filter.matches_path(
                    self.query
                        .index()
                        .get_file_path_by_entity(*caller_id)
                        .as_deref(),
                ) {
                    return false;
                }
                // Domain/external filtering needs the edge, not just the id.
                // Keep the caller when any edge to this callee passes.
                if filter.relation_domains.is_empty() && filter.include_external {
                    return true;
                }
                self.query
                    .index()
                    .get_resolved_relations_by_caller(*caller_id)
                    .is_some_and(|relations| {
                        relations.iter().any(|relation| {
                            relation.callee_id == Some(entity_id)
                                && filter.matches_relation(relation)
                        })
                    })
            })
            .collect()
    }

    // ========== Call Chain Queries ==========

    /// Query forward call chain (caller -> callees) with caching.
    ///
    /// The cache key includes the filter fingerprint so filtered queries never
    /// reuse unfiltered results.
    pub fn query_forward(
        &self,
        entity_id: EntityId,
        options: &RelationQueryOptions,
    ) -> Result<Vec<CallChainNode>> {
        let filter = RelationFileFilter::from_options(options);
        let fingerprint = RelationFileFilter::fingerprint(options);
        let cache_key = (entity_id, options.max_depth, false, fingerprint);
        if let Some(cached) = self
            .cache
            .write()
            .get_call_chain(cache_key.clone())
            .cloned()
        {
            return Ok(cached);
        }
        let nodes = self
            .query
            .query_forward_by_entity(entity_id, options.max_depth)
            .map_err(crate::query::error::QueryError::from)?;
        let filtered: Vec<CallChainNode> = if filter.is_empty() {
            nodes
        } else {
            nodes
                .into_iter()
                .filter(|node| {
                    if !filter.relation_domains.is_empty()
                        && !filter
                            .relation_domains
                            .contains(relation_domain_name(&node.relation_type))
                    {
                        return false;
                    }
                    filter.matches_path(Some(&node.file_path))
                })
                .collect()
        };
        self.cache
            .write()
            .put_call_chain(cache_key, filtered.clone());
        Ok(filtered)
    }

    /// Query backward call chain (callee -> callers) with caching.
    ///
    /// The cache key includes the filter fingerprint (see `query_forward`).
    pub fn query_backward(
        &self,
        entity_id: EntityId,
        options: &RelationQueryOptions,
    ) -> Result<Vec<CallChainNode>> {
        let filter = RelationFileFilter::from_options(options);
        let fingerprint = RelationFileFilter::fingerprint(options);
        let cache_key = (entity_id, options.max_depth, true, fingerprint);
        if let Some(cached) = self
            .cache
            .write()
            .get_call_chain(cache_key.clone())
            .cloned()
        {
            return Ok(cached);
        }
        let nodes = self
            .query
            .query_backward_by_entity(entity_id, options.max_depth)
            .map_err(crate::query::error::QueryError::from)?;
        let filtered: Vec<CallChainNode> = if filter.is_empty() {
            nodes
        } else {
            nodes
                .into_iter()
                .filter(|node| {
                    if !filter.relation_domains.is_empty()
                        && !filter
                            .relation_domains
                            .contains(relation_domain_name(&node.relation_type))
                    {
                        return false;
                    }
                    filter.matches_path(Some(&node.file_path))
                })
                .collect()
        };
        self.cache
            .write()
            .put_call_chain(cache_key, filtered.clone());
        Ok(filtered)
    }

    /// Query forward call chain with pagination
    pub fn query_forward_paginated(
        &self,
        entity_id: EntityId,
        options: &RelationQueryOptions,
    ) -> Result<Vec<CallChainNode>> {
        let nodes = self.query_forward(entity_id, options)?;
        Ok(nodes
            .into_iter()
            .skip(options.offset)
            .take(options.limit)
            .collect())
    }

    /// Query backward call chain with pagination
    pub fn query_backward_paginated(
        &self,
        entity_id: EntityId,
        options: &RelationQueryOptions,
    ) -> Result<Vec<CallChainNode>> {
        let nodes = self.query_backward(entity_id, options)?;
        Ok(nodes
            .into_iter()
            .skip(options.offset)
            .take(options.limit)
            .collect())
    }

    // ========== Path Finding ==========

    /// Find call chain path between two functions
    pub fn find_path(
        &self,
        start_id: EntityId,
        end_id: EntityId,
        options: &PathQueryOptions,
    ) -> Result<Option<Vec<CallChainNode>>> {
        let result =
            self.query
                .find_call_chain(start_id, end_id, options.max_depth, options.max_nodes);

        let converted: std::result::Result<
            Option<Vec<CallChainNode>>,
            crate::query::error::QueryError,
        > = result.map_err(Into::into);

        converted
    }

    /// Find call chain path crossing only edges accepted by the filter.
    pub fn find_path_filtered(
        &self,
        start_id: EntityId,
        end_id: EntityId,
        options: &PathQueryOptions,
        filter: &GraphFilter,
    ) -> Result<Option<Vec<CallChainNode>>> {
        if filter.relation_types.is_empty() {
            self.query
                .find_call_chain_in_domains(
                    start_id,
                    end_id,
                    options.max_depth,
                    options.max_nodes,
                    filter.relation_domains.clone(),
                    filter.include_external,
                )
                .map_err(Into::into)
        } else {
            self.query
                .find_call_chain_with_types(
                    start_id,
                    end_id,
                    options.max_depth,
                    options.max_nodes,
                    filter
                        .relation_types
                        .iter()
                        .filter_map(|t| serde_json::from_str(&format!("\"{t}\"")).ok())
                        .collect(),
                    filter.include_external,
                )
                .map_err(Into::into)
        }
    }

    // ========== Inheritance Queries ==========

    /// Get base classes (classes this class extends)
    pub fn get_base_classes(&self, class_id: EntityId) -> Vec<EntityId> {
        self.query.get_base_classes(class_id)
    }

    /// Get derived classes (classes that extend this class)
    pub fn get_derived_classes(&self, class_id: EntityId) -> Vec<EntityId> {
        self.query.get_derived_classes(class_id)
    }

    /// Get implemented interfaces
    pub fn get_implemented_interfaces(&self, class_id: EntityId) -> Vec<EntityId> {
        self.query.get_implemented_interfaces(class_id)
    }

    /// Get implementing classes (classes that implement this interface)
    pub fn get_implementing_classes(&self, interface_id: EntityId) -> Vec<EntityId> {
        self.query.get_implementing_classes(interface_id)
    }

    /// Get inheritance hierarchy (all ancestors), each with its hop distance.
    pub fn get_inheritance_hierarchy(
        &self,
        class_id: EntityId,
        max_depth: usize,
    ) -> Vec<(EntityId, usize)> {
        self.query.get_inheritance_hierarchy(class_id, max_depth)
    }

    /// Get all derived classes (transitive closure), each with its hop distance.
    pub fn get_all_derived_classes(
        &self,
        class_id: EntityId,
        max_depth: usize,
    ) -> Vec<(EntityId, usize)> {
        self.query.get_all_derived_classes(class_id, max_depth)
    }

    // ========== Module Relations ==========

    /// Get module relations (imports, exports, callers) for a file.
    pub fn get_module_relations(&self, file_path: &str) -> ModuleRelations {
        ModuleRelations {
            imports: self.query.get_file_imports(file_path),
            exports: self.query.get_file_exports(file_path),
            callers: self.query.get_file_callers_of_file(file_path),
        }
    }

    /// Get file-level import relations for a file.
    pub fn get_file_imports(&self, file_path: &str) -> Vec<ResolvedRelation> {
        self.query.get_file_imports(file_path)
    }

    /// Get file-level callers of an entity.
    pub fn get_file_callers(&self, callee_id: EntityId) -> Vec<String> {
        self.query.get_file_callers_of(callee_id)
    }

    /// Get exports for a file.
    pub fn get_file_exports(&self, file_path: &str) -> Vec<EntityId> {
        self.query.get_file_exports(file_path)
    }

    // ========== Inheritance Tree ==========

    /// Get complete inheritance tree (ancestors + descendants).
    pub fn get_inheritance_tree(&self, class_id: EntityId, max_depth: usize) -> InheritanceTree {
        InheritanceTree {
            ancestors: self.query.get_inheritance_hierarchy(class_id, max_depth),
            descendants: self.query.get_all_derived_classes(class_id, max_depth),
        }
    }

    /// Default traversal depth for inheritance closures.
    pub const INHERITANCE_MAX_DEPTH: usize = 10;

    /// Get interface implementation hierarchy.
    pub fn get_interface_hierarchy(&self, interface_id: EntityId) -> InterfaceHierarchy {
        InterfaceHierarchy {
            interface_id,
            implementors: self.query.get_implementing_classes(interface_id),
        }
    }
}

/// Module-level relations aggregated for a file.
pub struct ModuleRelations {
    pub imports: Vec<ResolvedRelation>,
    pub exports: Vec<EntityId>,
    pub callers: Vec<String>,
}

/// Inheritance tree with ancestors and descendants, each tagged with its hop
/// distance from the queried class.
pub struct InheritanceTree {
    pub ancestors: Vec<(EntityId, usize)>,
    pub descendants: Vec<(EntityId, usize)>,
}

/// Interface hierarchy with implementors.
pub struct InterfaceHierarchy {
    pub interface_id: EntityId,
    pub implementors: Vec<EntityId>,
}

// ========== Diagnostics ==========

impl RelationSearcher {
    /// Get quality report for the relation index.
    pub fn get_quality_report(&self) -> cce_relation::index::core::QualityReport {
        self.query.get_quality_report()
    }

    /// Get diagnostic summary.
    pub fn get_diagnostic_summary(
        &self,
    ) -> cce_relation::index::stores::diagnostics::DiagnosticSummary {
        self.query.get_diagnostic_summary()
    }
}

// ========== Impact Analysis ==========

impl RelationSearcher {
    /// Default traversal depth for impact analysis.
    const IMPACT_MAX_DEPTH: usize = 10;

    /// Get change impact analysis for a file.
    pub fn get_change_impact(&self, file_path: &str) -> cce_relation::ImpactAnalysis<String> {
        self.query
            .get_change_impact(file_path, Self::IMPACT_MAX_DEPTH)
    }

    /// Get change impact analysis for an entity.
    ///
    /// `scope` selects the analysis scope: `"entity"` (default) uses entity-level
    /// edges; `"file"` uses the file-level dependency graph.
    pub fn get_entity_impact(
        &self,
        entity_id: EntityId,
        max_depth: usize,
        scope: &str,
    ) -> cce_relation::ImpactAnalysis<String> {
        self.query.get_entity_impact(entity_id, max_depth, scope)
    }

    /// Dependency cycles among entities.
    pub fn find_entity_cycles(&self, max_cycles: usize) -> Vec<Vec<EntityId>> {
        self.query.find_entity_cycles(max_cycles)
    }

    /// Dependency cycles among files.
    pub fn find_file_cycles(&self, max_cycles: usize) -> Vec<Vec<String>> {
        self.query.find_file_cycles(max_cycles)
    }
}

// ========== Structural and Frontend Relations ==========

/// Direction of a structural relation family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructuralDirection {
    /// Counterparties this entity points at.
    Outgoing,
    /// Counterparties pointing at this entity.
    Incoming,
}

/// One resolved structural or frontend relation.
#[derive(Debug, Clone)]
pub struct StructuralRelation {
    pub entity_id: EntityId,
    pub label: String,
    pub relation_type: RelationType,
}

/// The structural / frontend relation families reachable from an entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructuralKind {
    /// Types carrying this entity as a trait bound.
    TraitBound,
    /// Child elements of a component or element.
    ChildElements,
    /// Parent of an element.
    ParentElement,
    /// Event handlers bound by an element.
    EventHandlers,
    /// Elements bound to an event handler.
    HandlerElements,
    /// Parameter (prop) bindings declared by a component.
    ParameterBindings,
    /// Template references issued by an element.
    TemplateReferences,
    /// Components issuing a template reference to this entity.
    TemplateRefOwners,
    /// Types with this entity as a trait bound (alias for TraitBound,
    /// exposed under a separate label for API completeness).
    TraitBounds,
}

impl StructuralKind {
    /// Label used on the wire.
    pub fn label(self) -> &'static str {
        match self {
            Self::TraitBound => "trait_bound",
            Self::ChildElements => "child_elements",
            Self::ParentElement => "parent_element",
            Self::EventHandlers => "event_handlers",
            Self::HandlerElements => "handler_elements",
            Self::ParameterBindings => "parameter_bindings",
            Self::TemplateReferences => "template_references",
            Self::TemplateRefOwners => "template_ref_owners",
            Self::TraitBounds => "trait_bounds",
        }
    }

    /// Parse a wire label.
    pub fn parse(label: &str) -> Option<Self> {
        match label {
            "trait_bound" => Some(Self::TraitBound),
            "child_elements" => Some(Self::ChildElements),
            "parent_element" => Some(Self::ParentElement),
            "event_handlers" => Some(Self::EventHandlers),
            "handler_elements" => Some(Self::HandlerElements),
            "parameter_bindings" => Some(Self::ParameterBindings),
            "template_references" => Some(Self::TemplateReferences),
            "template_ref_owners" => Some(Self::TemplateRefOwners),
            "trait_bounds" => Some(Self::TraitBounds),
            _ => None,
        }
    }

    /// All accepted labels, in documentation order.
    pub fn labels() -> &'static [&'static str] {
        &[
            "trait_bound",
            "trait_bounds",
            "child_elements",
            "parent_element",
            "event_handlers",
            "handler_elements",
            "parameter_bindings",
            "template_references",
            "template_ref_owners",
        ]
    }
}

impl RelationSearcher {
    /// Resolve one structural / frontend relation family for an entity.
    ///
    /// Families with several edges per pair (`event_handlers`,
    /// `parameter_bindings`, `template_references`) keep one entry per edge;
    /// the rest collapse to distinct counterparties.
    pub fn structural_relations(
        &self,
        entity_id: EntityId,
        kind: StructuralKind,
        direction: StructuralDirection,
    ) -> Vec<StructuralRelation> {
        use cce_relation::index::snapshot_query::SnapshotFrontendQueryOps;

        let index = self.query.index();
        let outgoing = |relations: Vec<cce_types::ResolvedRelation>| {
            relations
                .into_iter()
                .filter_map(|relation| {
                    relation.callee_id.map(|callee| StructuralRelation {
                        entity_id: callee,
                        label: relation.callee_name,
                        relation_type: relation.relation_type,
                    })
                })
                .collect::<Vec<_>>()
        };
        let incoming = |ids: Vec<EntityId>, relation_type: RelationType| {
            ids.into_iter()
                .map(|id| StructuralRelation {
                    entity_id: id,
                    label: index
                        .get_function_by_entity_id(id)
                        .map(|entity| entity.name)
                        .unwrap_or_default(),
                    relation_type,
                })
                .collect::<Vec<_>>()
        };

        match (kind, direction) {
            (StructuralKind::TraitBound, _) => incoming(
                index.get_types_with_trait_bound(entity_id),
                RelationType::TraitBound,
            ),
            (StructuralKind::ChildElements, StructuralDirection::Outgoing) => incoming(
                index.get_child_elements(entity_id),
                RelationType::ElementContains,
            ),
            (StructuralKind::ChildElements, StructuralDirection::Incoming) => outgoing(
                index.get_relations_from_entity_by_type(entity_id, RelationType::ElementContains),
            ),
            (StructuralKind::ParentElement, StructuralDirection::Outgoing) => incoming(
                index.get_parent_element(entity_id),
                RelationType::ElementContains,
            ),
            (StructuralKind::ParentElement, StructuralDirection::Incoming) => outgoing(
                index.get_relations_to_entity_by_type(entity_id, RelationType::ElementContains),
            ),
            (StructuralKind::EventHandlers, StructuralDirection::Outgoing) => {
                outgoing(index.get_event_handlers(entity_id))
            }
            (StructuralKind::EventHandlers, StructuralDirection::Incoming) => incoming(
                index.get_elements_by_handler(entity_id),
                RelationType::EventCallback,
            ),
            (StructuralKind::HandlerElements, StructuralDirection::Outgoing) => incoming(
                index.get_elements_by_handler(entity_id),
                RelationType::EventCallback,
            ),
            (StructuralKind::HandlerElements, StructuralDirection::Incoming) => {
                outgoing(index.get_event_handlers(entity_id))
            }
            (StructuralKind::ParameterBindings, StructuralDirection::Outgoing) => {
                outgoing(index.get_parameter_bindings(entity_id))
            }
            (StructuralKind::ParameterBindings, StructuralDirection::Incoming) => outgoing(
                index.get_relations_to_entity_by_type(entity_id, RelationType::ParameterBinding),
            ),
            (StructuralKind::TemplateReferences, StructuralDirection::Outgoing) => {
                outgoing(index.get_template_references(entity_id))
            }
            (StructuralKind::TemplateReferences, StructuralDirection::Incoming) => incoming(
                index.get_elements_by_template_ref(entity_id),
                RelationType::TemplateReference,
            ),
            (StructuralKind::TemplateRefOwners, StructuralDirection::Outgoing) => incoming(
                index.get_elements_by_template_ref(entity_id),
                RelationType::TemplateReference,
            ),
            (StructuralKind::TemplateRefOwners, StructuralDirection::Incoming) => {
                outgoing(index.get_template_references(entity_id))
            }
            (StructuralKind::TraitBounds, _) => incoming(
                index.get_types_with_trait_bound(entity_id),
                RelationType::TraitBound,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relation_query_options_default() {
        let options = RelationQueryOptions::default();
        assert_eq!(options.max_depth, 3);
        assert_eq!(options.offset, 0);
        assert_eq!(options.limit, 20);
        assert!(!options.include_start);
    }

    #[test]
    fn test_relation_query_options_builder() {
        let options = RelationQueryOptions::new()
            .with_max_depth(5)
            .with_offset(10)
            .with_limit(50)
            .with_include_start(true);

        assert_eq!(options.max_depth, 5);
        assert_eq!(options.offset, 10);
        assert_eq!(options.limit, 50);
        assert!(options.include_start);
    }

    #[test]
    fn test_path_query_options_default() {
        let options = PathQueryOptions::default();
        assert_eq!(options.max_depth, 10);
        assert_eq!(options.max_nodes, 10000);
    }

    #[test]
    fn test_path_query_options_builder() {
        let options = PathQueryOptions::new()
            .with_max_depth(20)
            .with_max_nodes(5000);

        assert_eq!(options.max_depth, 20);
        assert_eq!(options.max_nodes, 5000);
    }
}
