//! Relation searcher for call chain and inheritance queries
//!
//! Provides a unified interface for relation queries with pagination
//! and error handling.

use cce_relation::query::QueryCache;
use cce_relation::{CallChainNode, CallChainQuery};
use cce_types::{EntityId, ResolvedRelation};
use parking_lot::RwLock;
use std::sync::Arc;

use super::error::Result;

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
}

impl Default for RelationQueryOptions {
    fn default() -> Self {
        Self {
            max_depth: 3,
            offset: 0,
            limit: 20,
            include_start: false,
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

    /// Get callees (functions called by this function)
    pub fn get_callees(&self, entity_id: EntityId) -> Vec<ResolvedRelation> {
        self.query
            .get_callees_by_entity(entity_id)
            .unwrap_or_default()
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

    /// Get callees with pagination
    pub fn get_callees_paginated(
        &self,
        entity_id: EntityId,
        options: &RelationQueryOptions,
    ) -> Vec<ResolvedRelation> {
        let callees = self.get_callees(entity_id);
        callees
            .into_iter()
            .skip(options.offset)
            .take(options.limit)
            .collect()
    }

    /// Get callers with pagination
    pub fn get_callers_paginated(
        &self,
        entity_id: EntityId,
        options: &RelationQueryOptions,
    ) -> Vec<EntityId> {
        let callers = self.get_callers(entity_id);
        callers
            .into_iter()
            .skip(options.offset)
            .take(options.limit)
            .collect()
    }

    // ========== Call Chain Queries ==========

    /// Query forward call chain (caller -> callees) with caching
    pub fn query_forward(
        &self,
        entity_id: EntityId,
        options: &RelationQueryOptions,
    ) -> Result<Vec<CallChainNode>> {
        let cache_key = (entity_id, options.max_depth, false);
        if let Some(cached) = self.cache.write().get_call_chain(cache_key).cloned() {
            return Ok(cached);
        }
        let result = self
            .query
            .query_forward_by_entity(entity_id, options.max_depth);

        let converted: std::result::Result<Vec<CallChainNode>, crate::query::error::QueryError> =
            result.map_err(Into::into);
        if let Ok(ref nodes) = converted {
            self.cache.write().put_call_chain(cache_key, nodes.clone());
        }
        converted
    }

    /// Query backward call chain (callee -> callers) with caching
    pub fn query_backward(
        &self,
        entity_id: EntityId,
        options: &RelationQueryOptions,
    ) -> Result<Vec<CallChainNode>> {
        let cache_key = (entity_id, options.max_depth, true);
        if let Some(cached) = self.cache.write().get_call_chain(cache_key).cloned() {
            return Ok(cached);
        }
        let result = self
            .query
            .query_backward_by_entity(entity_id, options.max_depth);

        let converted: std::result::Result<Vec<CallChainNode>, crate::query::error::QueryError> =
            result.map_err(Into::into);
        if let Ok(ref nodes) = converted {
            self.cache.write().put_call_chain(cache_key, nodes.clone());
        }
        converted
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
        let result = self
            .query
            .find_call_chain(start_id, end_id, options.max_depth);

        let converted: std::result::Result<
            Option<Vec<CallChainNode>>,
            crate::query::error::QueryError,
        > = result.map_err(Into::into);

        converted
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

    /// Get inheritance hierarchy (all ancestors)
    pub fn get_inheritance_hierarchy(&self, class_id: EntityId, max_depth: usize) -> Vec<EntityId> {
        self.query.get_inheritance_hierarchy(class_id, max_depth)
    }

    /// Get all derived classes (transitive closure)
    pub fn get_all_derived_classes(&self, class_id: EntityId, max_depth: usize) -> Vec<EntityId> {
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

/// Inheritance tree with ancestors and descendants.
pub struct InheritanceTree {
    pub ancestors: Vec<EntityId>,
    pub descendants: Vec<EntityId>,
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
    /// Get change impact analysis for a file.
    pub fn get_change_impact(
        &self,
        file_path: &str,
    ) -> cce_relation::dependency_graph::ImpactAnalysis {
        self.query.get_change_impact(file_path)
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
