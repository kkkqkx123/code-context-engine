//! Call chain query for function call relationships
//!
//! Provides forward and backward call chain analysis based on relation indexes.
//! Supports recursive depth queries and path finding between functions.
//!
//! # Architecture Position
//!
//! This module is a **high-level query API** that wraps the lower-level
//! operations from `index/relation_query.rs`:
//!
//! ```text
//! query.rs (this module)
//!   ├── CallChainQuery: High-level API with metrics collection
//!   ├── CallChainTraverser: Graph traversal algorithms (BFS, path finding)
//!   └── TraversalConfig: Configuration for traversal behavior
//!
//! index/relation_query.rs (lower level)
//!   ├── RelationQueryOps: Basic relation lookups
//!   ├── HierarchyQueryOps: Inheritance hierarchy queries
//!   └── FrontendQueryOps: Frontend component queries
//! ```
//!
//! # Usage
//!
//! Use `CallChainQuery` for most query operations. It provides:
//! - Metrics collection for monitoring
//! - Cycle-safe traversal
//! - Configurable depth limits
//!
//! For direct index access without metrics, use the traits from
//! `index/relation_query.rs` directly.

pub mod cache;

pub use cache::QueryCache;

use super::error::RelationQueryError;
use super::index::core::{CallChainNode, RelationIndex};
use super::index::snapshot_index::{LayeredSnapshotIndex, RelationSnapshotIndex};
use super::index::snapshot_query::{
    SnapshotEntityQueryOps, SnapshotHierarchyQueryOps, SnapshotQueryIndex, SnapshotRelationQueryOps,
};
use cce_types::relation::CallContext;
use cce_types::{EntityId, RelationType, ResolvedRelation};
use std::collections::{HashSet, VecDeque};
use std::sync::Arc;

/// Type alias for path elements in BFS queue
type PathElement = (
    EntityId,
    RelationType,
    Option<usize>,
    Option<String>,
    CallContext,
);

/// Traversal direction for call chain queries
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraversalDirection {
    /// Traverse forward (from caller to callee)
    Forward,
    /// Traverse backward (from callee to caller)
    Backward,
}

/// Traversal configuration for call chain queries
#[derive(Debug, Clone)]
pub struct TraversalConfig {
    /// Maximum depth to traverse
    pub max_depth: usize,
    /// Whether to include the starting node in results
    pub include_start_node: bool,
    /// Whether to stop traversal when cycles are detected
    pub stop_on_cycles: bool,
    /// Direction of traversal
    pub direction: TraversalDirection,
    /// Whether to enable debug logging
    pub debug: bool,
    /// Maximum number of nodes to visit (safety limit)
    pub max_nodes: usize,
    /// Keep only edges whose coarse domain is in this set. Empty means every
    /// domain.
    pub relation_domains: Vec<String>,
    /// Keep only edges whose exact relation type is in this set. Empty means
    /// no fine-grained filtering. Applied after `relation_domains`.
    pub relation_types: Vec<RelationType>,
    /// Whether edges pointing outside the indexed project may be crossed.
    pub include_external: bool,
    /// Whether to use bidirectional BFS for path finding. Defaults to true.
    pub use_bidirectional: bool,
}

impl Default for TraversalConfig {
    fn default() -> Self {
        Self {
            max_depth: usize::MAX,
            include_start_node: false,
            stop_on_cycles: true,
            direction: TraversalDirection::Forward,
            debug: false,
            max_nodes: 10000,
            relation_domains: Vec::new(),
            relation_types: Vec::new(),
            include_external: true,
            use_bidirectional: true,
        }
    }
}

impl TraversalConfig {
    /// Create a new configuration with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Set maximum depth
    pub fn with_max_depth(mut self, max_depth: usize) -> Self {
        self.max_depth = max_depth;
        self
    }

    /// Set whether to include start node
    pub fn with_include_start_node(mut self, include_start_node: bool) -> Self {
        self.include_start_node = include_start_node;
        self
    }

    /// Set whether to stop on cycles
    pub fn with_stop_on_cycles(mut self, stop_on_cycles: bool) -> Self {
        self.stop_on_cycles = stop_on_cycles;
        self
    }

    /// Set traversal direction
    pub fn with_direction(mut self, direction: TraversalDirection) -> Self {
        self.direction = direction;
        self
    }

    /// Enable debug logging
    pub fn with_debug(mut self, debug: bool) -> Self {
        self.debug = debug;
        self
    }

    /// Set maximum number of nodes to visit
    pub fn with_max_nodes(mut self, max_nodes: usize) -> Self {
        self.max_nodes = max_nodes;
        self
    }

    /// Restrict traversal to the given coarse relation domains.
    pub fn with_relation_domains(mut self, domains: Vec<String>) -> Self {
        self.relation_domains = domains;
        self
    }

    /// Set whether edges pointing outside the indexed project may be crossed.
    pub fn with_include_external(mut self, include_external: bool) -> Self {
        self.include_external = include_external;
        self
    }

    /// Restrict traversal to the given exact relation types.
    pub fn with_relation_types(mut self, types: Vec<RelationType>) -> Self {
        self.relation_types = types;
        self
    }

    /// Set whether to use bidirectional BFS for path finding.
    pub fn with_bidirectional(mut self, use_bidirectional: bool) -> Self {
        self.use_bidirectional = use_bidirectional;
        self
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<(), RelationQueryError> {
        if self.max_depth == 0 {
            return Err(RelationQueryError::config("max_depth cannot be 0"));
        }
        if self.max_nodes == 0 {
            return Err(RelationQueryError::config("max_nodes cannot be 0"));
        }
        Ok(())
    }
}

/// Whether a stored relation survives the traversal filter.
fn relation_passes_filter(relation: &ResolvedRelation, config: &TraversalConfig) -> bool {
    if !config.include_external && relation.is_external {
        return false;
    }
    if !config.relation_domains.is_empty()
        && !config
            .relation_domains
            .iter()
            .any(|domain| *domain == relation.relation_type.domain())
    {
        return false;
    }
    if !config.relation_types.is_empty() && !config.relation_types.contains(&relation.relation_type)
    {
        return false;
    }
    true
}

/// Generic graph traversal for call chains.
///
/// Works over any queryable index surface ([`RelationIndex`],
/// [`RelationSnapshotIndex`], [`LayeredSnapshotIndex`]); only read operations
/// are used.
pub struct CallChainTraverser<'a, I: SnapshotQueryIndex = RelationIndex> {
    index: &'a I,
    config: TraversalConfig,
}

impl<'a, I: SnapshotQueryIndex> CallChainTraverser<'a, I> {
    /// Create a new traverser with the given index and configuration
    pub fn new(index: &'a I, config: TraversalConfig) -> Self {
        Self { index, config }
    }

    /// Traverse from a starting entity ID
    pub fn traverse_from(
        &self,
        start_id: EntityId,
    ) -> Result<Vec<CallChainNode>, RelationQueryError> {
        // Validate configuration
        self.config.validate()?;

        let mut result = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut visited_count = 0;

        // Initialize queue based on direction
        match self.config.direction {
            TraversalDirection::Forward => {
                queue.push_back((
                    start_id,
                    0,
                    RelationType::DirectCall,
                    None,
                    None,
                    CallContext::Direct,
                ));
            }
            TraversalDirection::Backward => {
                for relation in self.index.get_relations_to_entity(start_id) {
                    let call_line = relation.span.line_range_opt().map(|(s, _)| s).unwrap_or(0);
                    queue.push_back((
                        relation.caller,
                        1,
                        relation.relation_type,
                        Some(call_line),
                        relation.owner_type.clone(),
                        relation.call_context.clone(),
                    ));
                }
            }
        }

        while let Some((current_id, depth, relation_type, call_line, owner_type, call_context)) =
            queue.pop_front()
        {
            // Safety check: limit total nodes visited
            visited_count += 1;
            if visited_count > self.config.max_nodes {
                return Err(RelationQueryError::traversal(format!(
                    "Exceeded maximum node limit ({}) during traversal",
                    self.config.max_nodes
                )));
            }

            if depth > self.config.max_depth {
                continue;
            }

            // Handle cycle detection
            if self.config.stop_on_cycles {
                if depth > 0 && visited.contains(&current_id) {
                    continue;
                }
                if depth > 0 {
                    visited.insert(current_id);
                }
            }

            // Get function info
            let func_info = self.index.get_function_by_entity_id(current_id);
            let file_path_opt = self.index.get_file_path_by_entity(current_id);
            let (func_name, file_path) = if let Some(info) = func_info {
                (info.name.clone(), file_path_opt.unwrap_or_default())
            } else {
                (format!("{}", current_id.0), String::new())
            };

            // Add to result based on configuration
            let should_add = match self.config.direction {
                TraversalDirection::Forward => depth > 0 || self.config.include_start_node,
                TraversalDirection::Backward => true, // Backward traversal always includes nodes
            };

            if should_add {
                result.push(CallChainNode {
                    function_id: current_id,
                    function_name: func_name.clone(),
                    file_path,
                    depth,
                    relation_type,
                    call_line,
                    owner_type: owner_type.clone(),
                    call_context: call_context.clone(),
                });
            }

            // Get next nodes based on direction
            if depth < self.config.max_depth {
                match self.config.direction {
                    TraversalDirection::Forward => {
                        if let Some(relations) =
                            self.index.get_resolved_relations_by_caller(current_id)
                        {
                            for relation in relations.iter() {
                                if !relation_passes_filter(relation, &self.config) {
                                    continue;
                                }
                                if let Some(callee_id) = relation.callee_id {
                                    let call_line =
                                        relation.span.line_range_opt().map(|(s, _)| s).unwrap_or(0);
                                    queue.push_back((
                                        callee_id,
                                        depth + 1,
                                        relation.relation_type,
                                        Some(call_line),
                                        relation.owner_type.clone(),
                                        relation.call_context.clone(),
                                    ));
                                }
                            }
                        }
                    }
                    TraversalDirection::Backward => {
                        for relation in self.index.get_relations_to_entity(current_id) {
                            if !relation_passes_filter(&relation, &self.config) {
                                continue;
                            }
                            let call_line =
                                relation.span.line_range_opt().map(|(s, _)| s).unwrap_or(0);
                            queue.push_back((
                                relation.caller,
                                depth + 1,
                                relation.relation_type,
                                Some(call_line),
                                relation.owner_type.clone(),
                                relation.call_context.clone(),
                            ));
                        }
                    }
                }
            }
        }

        // Validate that start node exists (for forward traversal)
        if result.is_empty()
            && self.config.direction == TraversalDirection::Forward
            && self.config.max_depth > 0
            && self.index.get_function_by_entity_id(start_id).is_none()
        {
            return Err(RelationQueryError::not_found(format!(
                "EntityId: {:?}",
                start_id
            )));
        }

        Ok(result)
    }

    /// Find a path between two entity IDs
    pub fn find_path(
        &self,
        start_id: EntityId,
        end_id: EntityId,
    ) -> Result<Option<Vec<CallChainNode>>, RelationQueryError> {
        self.config.validate()?;

        let start_func = self
            .index
            .get_function_by_entity_id(start_id)
            .ok_or_else(|| {
                RelationQueryError::not_found(format!(
                    "Start function not found for EntityId: {:?}",
                    start_id
                ))
            })?;

        let end_func = self
            .index
            .get_function_by_entity_id(end_id)
            .ok_or_else(|| {
                RelationQueryError::not_found(format!(
                    "Target function not found for EntityId: {:?}",
                    end_id
                ))
            })?;

        let mut queue: VecDeque<Vec<PathElement>> = VecDeque::new();
        let mut visited = HashSet::new();
        let mut visited_count = 0;

        queue.push_back(vec![(
            start_id,
            RelationType::DirectCall,
            None,
            None,
            CallContext::Direct,
        )]);
        visited.insert(start_id);

        while let Some(current_path) = queue.pop_front() {
            // Safety check: limit total nodes visited
            visited_count += 1;
            if visited_count > self.config.max_nodes {
                return Err(RelationQueryError::traversal(format!(
                    "Exceeded maximum node limit ({}) while searching for path from {:?} to {:?}",
                    self.config.max_nodes, start_id, end_id
                )));
            }

            if current_path.len() > self.config.max_depth + 1 {
                continue;
            }

            let (last_id, _, _, _, _) = current_path
                .last()
                .ok_or_else(|| RelationQueryError::invalid("Empty path in BFS queue"))?;

            // Check if we reached the target
            if *last_id == end_id {
                // Found a path, build nodes
                let nodes = self.build_path_nodes_from_entities(&current_path)?;
                return Ok(Some(nodes));
            }

            // Expand path with calls from current function
            if let Some(relations) = self.index.get_resolved_relations_by_caller(*last_id) {
                for relation in relations.iter() {
                    if !relation_passes_filter(relation, &self.config) {
                        continue;
                    }
                    if let Some(callee_id) = relation.callee_id {
                        // Skip if already visited in this search (prevents cycles)
                        if visited.contains(&callee_id) {
                            continue;
                        }

                        visited.insert(callee_id);
                        let mut new_path = current_path.clone();
                        let call_line = relation.span.line_range_opt().map(|(s, _)| s).unwrap_or(0);
                        new_path.push((
                            callee_id,
                            relation.relation_type,
                            Some(call_line),
                            relation.owner_type.clone(),
                            relation.call_context.clone(),
                        ));
                        queue.push_back(new_path);
                    }
                }
            }
        }

        // Return path not found error if debug is enabled
        if self.config.debug {
            let start_name = start_func.name.clone();
            let end_name = end_func.name.clone();
            return Err(RelationQueryError::path_not_found(
                format!("{} ({:?})", start_name, start_id),
                format!("{} ({:?})", end_name, end_id),
                self.config.max_depth,
            ));
        }

        Ok(None)
    }

    /// Build path nodes from entity path
    fn build_path_nodes_from_entities(
        &self,
        path: &[PathElement],
    ) -> Result<Vec<CallChainNode>, RelationQueryError> {
        let mut nodes = Vec::new();

        for (i, (entity_id, relation_type, call_line, owner_type, call_context)) in
            path.iter().enumerate()
        {
            let func_info = self.index.get_function_by_entity_id(*entity_id);
            let file_path_opt = self.index.get_file_path_by_entity(*entity_id);
            let (func_name, file_path) = if let Some(info) = func_info {
                (info.name.clone(), file_path_opt.unwrap_or_default())
            } else {
                (format!("{}", entity_id.0), String::new())
            };

            nodes.push(CallChainNode {
                function_id: *entity_id,
                function_name: func_name,
                file_path,
                depth: i,
                relation_type: *relation_type,
                call_line: *call_line,
                owner_type: owner_type.clone(),
                call_context: call_context.clone(),
            });
        }

        Ok(nodes)
    }
}

/// Call chain query
///
/// Query-only facade over an immutable snapshot ([`LayeredSnapshotIndex`]).
/// Construction from a published snapshot is an O(1) `Arc` clone; no index
/// data is ever copied per query.
pub struct CallChainQuery {
    /// Reference to the layered snapshot index (base + optional delta)
    index: Arc<LayeredSnapshotIndex>,
}

impl CallChainQuery {
    /// Create a new call chain query with an empty snapshot
    pub fn new() -> Self {
        Self {
            index: Arc::new(LayeredSnapshotIndex::empty()),
        }
    }

    /// Create from a published snapshot (zero-copy `Arc` clone).
    pub fn from_snapshot(index: Arc<LayeredSnapshotIndex>) -> Self {
        Self { index }
    }

    /// Create from a mutable relation index.
    ///
    /// The index is deep-snapshotted at construction time; mutating the
    /// source afterwards never affects this query. Uses `snapshot_take`
    /// for an O(1)-per-map drain instead of O(entries) deep copy.
    pub fn from_index(mut index: RelationIndex) -> Self {
        Self {
            index: Arc::new(LayeredSnapshotIndex::new(Arc::new(
                RelationSnapshotIndex::from_index_owned(&mut index),
            ))),
        }
    }

    /// Get a reference to the underlying snapshot index
    pub fn index(&self) -> &LayeredSnapshotIndex {
        &self.index
    }

    // ========== EntityId-based Query Methods (New Architecture) ==========

    /// Get callees (functions called by this function) by EntityId
    ///
    /// Missing entities report not found. Existing entities without outgoing
    /// edges return an empty list.
    pub fn get_callees_by_entity(
        &self,
        entity_id: EntityId,
    ) -> Result<Vec<ResolvedRelation>, RelationQueryError> {
        if !self.index.contains_function(entity_id) {
            return Err(RelationQueryError::not_found(format!(
                "Entity not found: {:?}",
                entity_id
            )));
        }

        Ok(self
            .index
            .get_resolved_relations_by_caller(entity_id)
            .unwrap_or_default())
    }

    /// Get callers (functions that call this function) by EntityId
    pub fn get_callers_by_entity(&self, entity_id: EntityId) -> Vec<EntityId> {
        self.index.get_callers_by_callee_entity(entity_id)
    }

    // ========== Inheritance and Implementation Query Methods ==========

    /// Get base classes (classes this class extends)
    pub fn get_base_classes(&self, class_id: EntityId) -> Vec<EntityId> {
        self.index.get_base_classes(class_id)
    }

    /// Get derived classes (classes that extend this class)
    pub fn get_derived_classes(&self, class_id: EntityId) -> Vec<EntityId> {
        self.index.get_derived_classes(class_id)
    }

    /// Get implemented interfaces
    pub fn get_implemented_interfaces(&self, class_id: EntityId) -> Vec<EntityId> {
        self.index.get_implemented_interfaces(class_id)
    }

    /// Get implementing classes (classes that implement this interface)
    pub fn get_implementing_classes(&self, interface_id: EntityId) -> Vec<EntityId> {
        self.index.get_implementing_classes(interface_id)
    }

    /// Get inheritance hierarchy (all ancestors), each tagged with its
    /// distance in hops from `class_id`.
    pub fn get_inheritance_hierarchy(
        &self,
        class_id: EntityId,
        max_depth: usize,
    ) -> Vec<(EntityId, usize)> {
        self.hierarchy_closure(class_id, max_depth, |index, id| index.get_base_classes(id))
    }

    /// Get all derived classes (transitive closure), each tagged with its
    /// distance in hops from `class_id`.
    pub fn get_all_derived_classes(
        &self,
        class_id: EntityId,
        max_depth: usize,
    ) -> Vec<(EntityId, usize)> {
        self.hierarchy_closure(class_id, max_depth, |index, id| {
            index.get_derived_classes(id)
        })
    }

    /// Breadth-first closure over a hierarchy direction, tagging each reached
    /// entity with its hop distance. The starting entity is never reported.
    fn hierarchy_closure(
        &self,
        class_id: EntityId,
        max_depth: usize,
        next: impl Fn(&LayeredSnapshotIndex, EntityId) -> Vec<EntityId>,
    ) -> Vec<(EntityId, usize)> {
        let mut result = Vec::new();
        let mut visited: HashSet<EntityId> = HashSet::from([class_id]);
        let mut queue: VecDeque<(EntityId, usize)> = VecDeque::from([(class_id, 0)]);

        while let Some((current_id, depth)) = queue.pop_front() {
            if depth >= max_depth {
                continue;
            }
            for related in next(&self.index, current_id) {
                if visited.insert(related) {
                    result.push((related, depth + 1));
                    queue.push_back((related, depth + 1));
                }
            }
        }

        result
    }

    /// Get callers by callee EntityId and relation type
    pub fn get_callers_by_callee_and_type(
        &self,
        callee_id: EntityId,
        relation_type: RelationType,
    ) -> Vec<EntityId> {
        self.index
            .get_callers_by_callee_and_type(callee_id, relation_type)
    }

    // ========== File-level Query Methods ==========

    /// Get file-level import/use relations for a file.
    pub fn get_file_imports(&self, file_path: &str) -> Vec<ResolvedRelation> {
        use crate::index::view::RelationIndexView;
        self.index
            .file_relations_of(file_path)
            .into_iter()
            .filter(|r| r.relation_type.is_import() || r.relation_type == RelationType::Use)
            .collect()
    }

    /// Get file paths that have a file-level relation targeting the given entity.
    pub fn get_file_callers_of(&self, callee_id: EntityId) -> Vec<String> {
        use crate::index::view::RelationIndexView;
        self.index.file_callers_of(callee_id)
    }

    /// Get exported entity IDs for a file.
    pub fn get_file_exports(&self, file_path: &str) -> Vec<EntityId> {
        use crate::index::view::RelationIndexView;
        self.index
            .exports_of(file_path)
            .map(|exports| exports.into_iter().map(|e| e.function_id).collect())
            .unwrap_or_default()
    }

    /// Get file paths that depend on (call into) the given file.
    pub fn get_file_callers_of_file(&self, file_path: &str) -> Vec<String> {
        use crate::index::view::RelationIndexView;
        // Collect via file-level callee reverse index for all entities in the file.
        let mut callers = std::collections::HashSet::new();
        for entity in self.index.entities_of_file(file_path) {
            for caller_file in self.index.file_callers_of(entity.id) {
                if caller_file != file_path {
                    callers.insert(caller_file);
                }
            }
        }
        // Also include dependency graph dependents for completeness.
        for dep in self.index.dependents_of(file_path) {
            callers.insert(dep);
        }
        callers.into_iter().collect()
    }

    /// Get quality report for the relation index.
    pub fn get_quality_report(&self) -> crate::index::core::QualityReport {
        self.index.quality_report()
    }

    /// Get diagnostic summary.
    pub fn get_diagnostic_summary(&self) -> crate::index::stores::diagnostics::DiagnosticSummary {
        self.index.base.diagnostics().summary()
    }

    /// Get change impact analysis for a file.
    ///
    /// Derived from the file dependency graph. `direct_dependents` and
    /// `indirect_dependents` are disjoint, so their sizes can be summed
    /// without double counting.
    pub fn get_change_impact(
        &self,
        file_path: &str,
        max_depth: usize,
    ) -> crate::dependency_graph::ImpactAnalysis<String> {
        use crate::index::view::RelationIndexView;
        let direct: HashSet<String> = self.index.dependents_of(file_path).into_iter().collect();
        let indirect: Vec<String> = self
            .index
            .collect_transitive_dependents(file_path, max_depth)
            .into_iter()
            .filter(|dependent| !direct.contains(dependent))
            .collect();
        crate::dependency_graph::ImpactAnalysis::new(
            file_path.to_string(),
            direct.into_iter().collect(),
            indirect,
        )
    }

    /// Get change impact analysis for an entity, derived from the resolved
    /// relation edges rather than a separately maintained entity graph.
    ///
    /// Direct dependents are the callers of the entity; indirect dependents are
    /// the callers of those callers, so the two sets are disjoint.
    ///
    /// `scope` selects the analysis scope: `"entity"` (default) uses entity-level
    /// edges; `"file"` locates the entity's file and uses the file-level
    /// dependency graph, returning file paths as dependents.
    pub fn get_entity_impact(
        &self,
        entity_id: EntityId,
        max_depth: usize,
        scope: &str,
    ) -> crate::dependency_graph::ImpactAnalysis<String> {
        use crate::index::view::RelationIndexView;

        if scope == "file" {
            let file_path = self
                .index
                .get_file_path_by_entity(entity_id)
                .unwrap_or_default();
            if file_path.is_empty() {
                return crate::dependency_graph::ImpactAnalysis::new(
                    entity_id.to_string(),
                    Vec::new(),
                    Vec::new(),
                );
            }
            let direct: HashSet<String> =
                self.index.dependents_of(&file_path).into_iter().collect();
            let indirect: Vec<String> = self
                .index
                .collect_transitive_dependents(&file_path, max_depth)
                .into_iter()
                .filter(|dep| !direct.contains(dep))
                .collect();
            return crate::dependency_graph::ImpactAnalysis::new(
                file_path,
                direct.into_iter().collect(),
                indirect,
            );
        }

        let direct: HashSet<String> = self
            .index
            .get_callers_by_callee_entity(entity_id)
            .into_iter()
            .map(|id| id.to_string())
            .collect();
        let mut indirect: Vec<String> = Vec::new();
        let mut visited: HashSet<EntityId> = self
            .index
            .get_callers_by_callee_entity(entity_id)
            .into_iter()
            .collect();
        let mut frontier: Vec<EntityId> = visited.iter().copied().collect();
        let mut depth = 1usize;
        while !frontier.is_empty() && (max_depth == 0 || depth < max_depth) {
            let mut next: Vec<EntityId> = Vec::new();
            for current in &frontier {
                for caller in self.index.get_callers_by_callee_entity(*current) {
                    if visited.insert(caller) {
                        next.push(caller);
                    }
                }
            }
            indirect.extend(next.iter().map(|id| id.to_string()));
            frontier = next;
            depth += 1;
        }
        crate::dependency_graph::ImpactAnalysis::new(
            entity_id.to_string(),
            direct.into_iter().collect(),
            indirect,
        )
    }

    /// Dependency cycles among entities, each listing its members in
    /// traversal order.
    ///
    /// Derived from the forward relation edges by iterative DFS with an
    /// explicit stack: a recursive formulation would overflow the stack on
    /// deep call chains.
    pub fn find_entity_cycles(&self, max_cycles: usize) -> Vec<Vec<EntityId>> {
        use crate::index::view::RelationIndexView;

        /// DFS frame: node being expanded, plus the position within its
        /// successors.
        struct Frame {
            node: EntityId,
            successors: Vec<EntityId>,
            cursor: usize,
        }

        let mut cycles: Vec<Vec<EntityId>> = Vec::new();
        // `done` marks nodes whose outgoing edges are fully expanded; `on_path`
        // marks nodes on the current DFS path.
        let mut done: HashSet<EntityId> = HashSet::new();
        let mut roots: Vec<EntityId> = Vec::new();
        self.index.for_each_function(|id, _| roots.push(id));
        roots.sort();

        for root in roots {
            if done.contains(&root) || cycles.len() >= max_cycles {
                continue;
            }
            let mut path: Vec<EntityId> = vec![root];
            let mut on_path: HashSet<EntityId> = HashSet::from([root]);
            let mut stack: Vec<Frame> = vec![Frame {
                node: root,
                successors: self
                    .index
                    .get_resolved_relations_by_caller(root)
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|relation| relation.callee_id)
                    .collect(),
                cursor: 0,
            }];

            while let Some(frame) = stack.last_mut() {
                if frame.cursor >= frame.successors.len() {
                    let node = frame.node;
                    stack.pop();
                    on_path.remove(&node);
                    done.insert(node);
                    continue;
                }
                let next = frame.successors[frame.cursor];
                frame.cursor += 1;
                if done.contains(&next) {
                    continue;
                }
                if on_path.contains(&next) {
                    if let Some(start) = path.iter().position(|node| *node == next) {
                        cycles.push(path[start..].to_vec());
                        if cycles.len() >= max_cycles {
                            break;
                        }
                    }
                    continue;
                }
                on_path.insert(next);
                path.push(next);
                stack.push(Frame {
                    node: next,
                    successors: self
                        .index
                        .get_resolved_relations_by_caller(next)
                        .unwrap_or_default()
                        .into_iter()
                        .filter_map(|relation| relation.callee_id)
                        .collect(),
                    cursor: 0,
                });
            }
        }
        cycles
    }

    /// File-level dependency cycles from the file dependency graph.
    pub fn find_file_cycles(&self, max_cycles: usize) -> Vec<Vec<String>> {
        use crate::index::view::RelationIndexView;

        /// DFS frame over file dependencies.
        struct Frame {
            file: String,
            dependencies: Vec<String>,
            cursor: usize,
        }

        let mut cycles: Vec<Vec<String>> = Vec::new();
        let mut done: HashSet<String> = HashSet::new();
        let mut files: Vec<String> = self.index.dependency_files();
        files.sort();

        for root in files {
            if done.contains(&root) || cycles.len() >= max_cycles {
                continue;
            }
            let mut path: Vec<String> = vec![root.clone()];
            let mut on_path: HashSet<String> = HashSet::from([root.clone()]);
            let mut stack: Vec<Frame> = vec![Frame {
                file: root.clone(),
                dependencies: self.index.dependencies_of(&root),
                cursor: 0,
            }];

            while let Some(frame) = stack.last_mut() {
                if frame.cursor >= frame.dependencies.len() {
                    let file = frame.file.clone();
                    stack.pop();
                    on_path.remove(&file);
                    done.insert(file);
                    continue;
                }
                let next = frame.dependencies[frame.cursor].clone();
                frame.cursor += 1;
                if next == root && path.len() == 1 {
                    continue;
                }
                if done.contains(&next) {
                    continue;
                }
                if on_path.contains(&next) {
                    if let Some(start) = path.iter().position(|file| *file == next) {
                        cycles.push(path[start..].to_vec());
                        if cycles.len() >= max_cycles {
                            break;
                        }
                    }
                    continue;
                }
                on_path.insert(next.clone());
                path.push(next.clone());
                stack.push(Frame {
                    file: next.clone(),
                    dependencies: self.index.dependencies_of(&next),
                    cursor: 0,
                });
            }
        }
        cycles
    }

    /// Query forward call chain by EntityId
    ///
    /// Uses cycle-safe traversal with visited set to prevent infinite loops.
    pub fn query_forward_by_entity(
        &self,
        entity_id: EntityId,
        max_depth: usize,
    ) -> Result<Vec<CallChainNode>, RelationQueryError> {
        let config = TraversalConfig::new()
            .with_max_depth(max_depth)
            .with_include_start_node(false)
            .with_stop_on_cycles(true)
            .with_direction(TraversalDirection::Forward);

        let traverser = CallChainTraverser::new(self.index.as_ref(), config);
        traverser.traverse_from(entity_id)
    }

    /// Query backward call chain by EntityId
    ///
    /// Uses cycle-safe traversal with visited set to prevent infinite loops.
    pub fn query_backward_by_entity(
        &self,
        entity_id: EntityId,
        max_depth: usize,
    ) -> Result<Vec<CallChainNode>, RelationQueryError> {
        // Verify the target function exists
        self.index
            .get_function_by_entity_id(entity_id)
            .ok_or_else(|| {
                RelationQueryError::not_found(format!(
                    "Function not found for EntityId: {:?}",
                    entity_id
                ))
            })?;

        let config = TraversalConfig::new()
            .with_max_depth(max_depth)
            .with_include_start_node(false)
            .with_stop_on_cycles(true)
            .with_direction(TraversalDirection::Backward);

        let traverser = CallChainTraverser::new(self.index.as_ref(), config);
        traverser.traverse_from(entity_id)
    }

    /// Find call chain path between two EntityIds
    pub fn find_call_chain(
        &self,
        start_id: EntityId,
        end_id: EntityId,
        max_depth: usize,
        max_nodes: usize,
    ) -> Result<Option<Vec<CallChainNode>>, RelationQueryError> {
        let config = TraversalConfig::new()
            .with_max_depth(max_depth)
            .with_max_nodes(max_nodes)
            .with_include_start_node(true)
            .with_stop_on_cycles(true)
            .with_direction(TraversalDirection::Forward);

        let traverser = CallChainTraverser::new(self.index.as_ref(), config);
        traverser.find_path(start_id, end_id)
    }

    /// Find call chain path between two EntityIds, crossing only edges in the
    /// given coarse relation domains.
    pub fn find_call_chain_in_domains(
        &self,
        start_id: EntityId,
        end_id: EntityId,
        max_depth: usize,
        max_nodes: usize,
        relation_domains: Vec<String>,
        include_external: bool,
    ) -> Result<Option<Vec<CallChainNode>>, RelationQueryError> {
        let config = TraversalConfig::new()
            .with_max_depth(max_depth)
            .with_max_nodes(max_nodes)
            .with_include_start_node(true)
            .with_stop_on_cycles(true)
            .with_direction(TraversalDirection::Forward)
            .with_relation_domains(relation_domains)
            .with_include_external(include_external);

        let traverser = CallChainTraverser::new(self.index.as_ref(), config);
        traverser.find_path(start_id, end_id)
    }

    /// Find call chain path between two EntityIds, crossing only edges whose
    /// exact relation type is in the given set.
    pub fn find_call_chain_with_types(
        &self,
        start_id: EntityId,
        end_id: EntityId,
        max_depth: usize,
        max_nodes: usize,
        relation_types: Vec<RelationType>,
        include_external: bool,
    ) -> Result<Option<Vec<CallChainNode>>, RelationQueryError> {
        let config = TraversalConfig::new()
            .with_max_depth(max_depth)
            .with_max_nodes(max_nodes)
            .with_include_start_node(true)
            .with_stop_on_cycles(true)
            .with_direction(TraversalDirection::Forward)
            .with_relation_types(relation_types)
            .with_include_external(include_external);

        let traverser = CallChainTraverser::new(self.index.as_ref(), config);
        traverser.find_path(start_id, end_id)
    }

    /// Find up to `k` shortest paths between two EntityIds using Yen's algorithm.
    ///
    /// Returns at most `k` paths, sorted by length (number of edges). Each path
    /// is a sequence of `CallChainNode` from `start_id` to `end_id`.
    pub fn find_k_shortest_paths(
        &self,
        start_id: EntityId,
        end_id: EntityId,
        k: usize,
        max_depth: usize,
        max_nodes: usize,
    ) -> Result<Vec<Vec<CallChainNode>>, RelationQueryError> {
        if k == 0 {
            return Ok(Vec::new());
        }

        let config = TraversalConfig::new()
            .with_max_depth(max_depth)
            .with_max_nodes(max_nodes)
            .with_include_start_node(true)
            .with_stop_on_cycles(true)
            .with_direction(TraversalDirection::Forward);

        let traverser = CallChainTraverser::new(self.index.as_ref(), config);

        let first = traverser.find_path(start_id, end_id)?;
        let Some(first_path) = first else {
            return Ok(Vec::new());
        };

        let mut result: Vec<Vec<CallChainNode>> = vec![first_path];
        let mut candidates: Vec<Vec<CallChainNode>> = Vec::new();

        for i in 1..k {
            let prev_path = &result[i - 1];
            for j in 0..prev_path.len().saturating_sub(1) {
                let spur_node = prev_path[j].function_id;
                let root_path: Vec<EntityId> =
                    prev_path[..=j].iter().map(|n| n.function_id).collect();

                let removed_edges: Vec<(EntityId, EntityId)> = result
                    .iter()
                    .filter(|p| {
                        p.len() > j
                            && p[..=j]
                                .iter()
                                .map(|n| n.function_id)
                                .eq(root_path.iter().copied())
                    })
                    .filter_map(|p| p.get(j + 1).map(|n| (p[j].function_id, n.function_id)))
                    .collect();

                let spur_path = self.find_spur_path(
                    spur_node,
                    end_id,
                    &root_path,
                    &removed_edges,
                    max_depth,
                    max_nodes,
                )?;

                if let Some(spur) = spur_path {
                    let mut total_path: Vec<CallChainNode> = prev_path[..=j].to_vec();
                    total_path.extend(spur);
                    if !result.contains(&total_path) && !candidates.contains(&total_path) {
                        candidates.push(total_path);
                    }
                }
            }

            if candidates.is_empty() {
                break;
            }
            candidates.sort_by_key(|p| p.len());
            result.push(candidates.remove(0));
        }

        Ok(result)
    }

    fn find_spur_path(
        &self,
        spur_node: EntityId,
        end_id: EntityId,
        root_path: &[EntityId],
        removed_edges: &[(EntityId, EntityId)],
        max_depth: usize,
        max_nodes: usize,
    ) -> Result<Option<Vec<CallChainNode>>, RelationQueryError> {
        let config = TraversalConfig::new()
            .with_max_depth(max_depth)
            .with_max_nodes(max_nodes)
            .with_include_start_node(true)
            .with_stop_on_cycles(true)
            .with_direction(TraversalDirection::Forward);

        let traverser = CallChainTraverser::new(self.index.as_ref(), config);
        let path = traverser.find_path(spur_node, end_id)?;

        if let Some(nodes) = path {
            let has_removed_edge = nodes
                .windows(2)
                .any(|w| removed_edges.contains(&(w[0].function_id, w[1].function_id)));
            let has_root_node = nodes[1..]
                .iter()
                .any(|n| root_path.contains(&n.function_id));
            if has_removed_edge || has_root_node {
                return Ok(None);
            }
            return Ok(Some(nodes));
        }
        Ok(None)
    }
}

impl Default for CallChainQuery {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::entity_index::EntityIndexOps;
    use cce_types::Span;
    use cce_types::{Entity, EntityId, EntityKind, RelationType, ResolvedRelation};
    use std::collections::HashMap;

    /// Minimal entity record for graph-shape tests.
    fn test_entity(id: u64, name: &str) -> Entity {
        Entity {
            id: EntityId(id),
            kind: EntityKind::Function,
            name: name.to_string(),
            signature: String::new(),
            parameters: Vec::new(),
            return_type: None,
            span: Span::default(),
            depth: 0,
            parent: None,
            children: Vec::new(),
            doc_comment: None,
            modifiers: Vec::new(),
            attributes: HashMap::new(),
            metadata: HashMap::new(),
            is_stdlib: false,
            subtype: None,
            stdlib_category: None,
        }
    }

    /// Helper function to create a test function entity
    fn create_test_function_entity(
        id: u64,
        name: &str,
        file_path: &str,
    ) -> (EntityId, Entity, String) {
        let entity = Entity {
            id: EntityId(id),
            kind: EntityKind::Function,
            name: name.to_string(),
            signature: format!("fn {}()", name),
            parameters: Vec::new(),
            return_type: None,
            span: Span::default(),
            depth: 0,
            parent: None,
            children: Vec::new(),
            doc_comment: None,
            modifiers: Vec::new(),
            attributes: HashMap::new(),
            metadata: HashMap::new(),
            is_stdlib: false,
            subtype: None,
            stdlib_category: None,
        };
        (EntityId(id), entity, file_path.to_string())
    }

    fn create_test_index() -> RelationIndex {
        let index = RelationIndex::new();

        // Add functions using EntityId-based API
        let (id0, entity0, path0) = create_test_function_entity(0, "function_a", "test.c");
        let (id1, entity1, path1) = create_test_function_entity(1, "function_b", "test.c");
        let (id2, entity2, path2) = create_test_function_entity(2, "function_c", "test.c");
        index.add_function_with_path(id0, entity0, path0);
        index.add_function_with_path(id1, entity1, path1);
        index.add_function_with_path(id2, entity2, path2);

        // Add resolved relations using EntityId-based API
        index.add_resolved_relation(ResolvedRelation {
            caller: EntityId(0),
            callee_id: Some(EntityId(1)),
            callee_name: "function_b".to_string(),
            relation_type: RelationType::DirectCall,
            span: Span::default(),
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
        index.add_resolved_relation(ResolvedRelation {
            caller: EntityId(1),
            callee_id: Some(EntityId(2)),
            callee_name: "function_c".to_string(),
            relation_type: RelationType::DirectCall,
            span: Span::default(),
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

        index
    }

    fn impact_chain_query() -> CallChainQuery {
        let base = RelationIndex::new();
        base.add_function_with_path(EntityId(1), test_entity(1, "target"), "src/lib.rs".into());
        for id in 2u64..=4 {
            base.add_function_with_path(
                EntityId(id),
                test_entity(id, &format!("fn{id}")),
                "src/lib.rs".into(),
            );
        }
        // 2 -> 1, 3 -> 2, 4 -> 3.
        for (from, to) in [(2u64, 1u64), (3, 2), (4, 3)] {
            base.add_resolved_relation(ResolvedRelation {
                caller: EntityId(from),
                callee_id: Some(EntityId(to)),
                callee_name: format!("fn{to}"),
                relation_type: RelationType::DirectCall,
                span: Span::default(),
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
        CallChainQuery::from_index(base)
    }

    #[test]
    fn entity_impact_direct_and_indirect_are_disjoint() {
        let query = impact_chain_query();
        let impact = query.get_entity_impact(EntityId(1), 10, "entity");
        assert_eq!(impact.changed, EntityId(1).to_string());
        assert_eq!(impact.direct_dependents, vec![EntityId(2).to_string()]);
        // 3 and 4 are reachable only after leaving the first hop.
        assert_eq!(
            impact.indirect_dependents,
            vec![EntityId(3).to_string(), EntityId(4).to_string()]
        );
        for indirect in &impact.indirect_dependents {
            assert!(!impact.direct_dependents.contains(indirect));
        }
        assert!(impact.impact_score > 0.0);
    }

    #[test]
    fn entity_impact_respects_max_depth() {
        let query = impact_chain_query();
        let impact = query.get_entity_impact(EntityId(1), 1, "entity");
        assert_eq!(impact.direct_dependents, vec![EntityId(2).to_string()]);
        assert!(impact.indirect_dependents.is_empty());
    }

    #[test]
    fn entity_impact_of_leaf_has_no_dependents() {
        let query = impact_chain_query();
        let impact = query.get_entity_impact(EntityId(4), 10, "entity");
        assert!(impact.direct_dependents.is_empty());
        assert!(impact.indirect_dependents.is_empty());
        assert_eq!(impact.impact_score, 0.0);
    }

    #[test]
    fn finds_call_cycles() {
        let base = RelationIndex::new();
        for id in 1u64..=3 {
            base.add_function_with_path(
                EntityId(id),
                test_entity(id, &format!("fn{id}")),
                "src/lib.rs".into(),
            );
        }
        // 1 -> 2 -> 3 -> 1, plus a disjoint acyclic tail 4 -> 1.
        for (from, to) in [(1u64, 2u64), (2, 3), (3, 1), (4, 1)] {
            base.add_resolved_relation(ResolvedRelation {
                caller: EntityId(from),
                callee_id: Some(EntityId(to)),
                callee_name: format!("fn{to}"),
                relation_type: RelationType::DirectCall,
                span: Span::default(),
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
        let query = CallChainQuery::from_index(base);
        let cycles = query.find_entity_cycles(10);
        assert_eq!(cycles.len(), 1);
        let cycle = &cycles[0];
        assert_eq!(cycle.len(), 3);
        // Every member is a distinct participant in the cycle.
        assert_eq!(cycle.iter().copied().collect::<HashSet<_>>().len(), 3);
        // 4 calls into the cycle but is not part of it.
        assert!(!cycle.contains(&EntityId(4)));
    }

    #[test]
    fn acyclic_graph_has_no_cycles() {
        let query = impact_chain_query();
        assert!(query.find_entity_cycles(10).is_empty());
    }

    #[test]
    fn cycle_search_honors_its_cap() {
        let base = RelationIndex::new();
        for id in 1u64..=4 {
            base.add_function_with_path(
                EntityId(id),
                test_entity(id, &format!("fn{id}")),
                "src/lib.rs".into(),
            );
        }
        // Two disjoint two-node cycles.
        for (from, to) in [(1u64, 2u64), (2, 1), (3, 4), (4, 3)] {
            base.add_resolved_relation(ResolvedRelation {
                caller: EntityId(from),
                callee_id: Some(EntityId(to)),
                callee_name: format!("fn{to}"),
                relation_type: RelationType::DirectCall,
                span: Span::default(),
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
        let query = CallChainQuery::from_index(base);
        assert_eq!(query.find_entity_cycles(1).len(), 1);
        assert_eq!(query.find_entity_cycles(10).len(), 2);
    }

    #[test]
    fn file_impact_direct_and_indirect_are_disjoint() {
        let base = RelationIndex::new();
        // b depends on a, c on b, d on c: changing a.rs reaches all three.
        base.dependency_graph.add_dependency("b.rs", "a.rs");
        base.dependency_graph.add_dependency("c.rs", "b.rs");
        base.dependency_graph.add_dependency("d.rs", "c.rs");
        let query = CallChainQuery::from_index(base);
        let impact = query.get_change_impact("a.rs", 10);
        assert_eq!(impact.changed, "a.rs");
        assert_eq!(impact.direct_dependents, vec!["b.rs".to_string()]);
        assert_eq!(
            impact.indirect_dependents,
            vec!["c.rs".to_string(), "d.rs".to_string()]
        );
    }

    #[test]
    fn test_query_forward() {
        let index = create_test_index();
        let query = CallChainQuery::from_index(index);

        // Query forward from func_a (EntityId(0)) with depth 1
        let result = query
            .query_forward_by_entity(EntityId(0), 1)
            .expect("Query failed");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].function_name, "function_b");

        // Query forward from func_a with depth 2
        let result = query
            .query_forward_by_entity(EntityId(0), 2)
            .expect("Query failed");
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_query_backward() {
        let index = create_test_index();
        let query = CallChainQuery::from_index(index);

        // Query backward from func_c (EntityId(2))
        let result = query
            .query_backward_by_entity(EntityId(2), 1)
            .expect("Query failed");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].function_name, "function_b");
    }

    #[test]
    fn test_find_call_chain() {
        let index = create_test_index();
        let query = CallChainQuery::from_index(index);

        // Find path from func_a (EntityId(0)) to func_c (EntityId(2))
        let path = query
            .find_call_chain(EntityId(0), EntityId(2), 5, 10_000)
            .expect("Query failed");
        assert!(path.is_some());
        let path = path.expect("Path should not be None");
        assert_eq!(path.len(), 3); // func_a -> func_b -> func_c
    }

    #[test]
    fn test_get_callees_by_entity() {
        let index = create_test_index();
        let query = CallChainQuery::from_index(index);

        let callees = query
            .get_callees_by_entity(EntityId(0))
            .expect("Query failed");
        assert_eq!(callees.len(), 1);
        assert_eq!(callees[0].callee_name, "function_b");
    }

    #[test]
    fn test_get_callers_by_entity() {
        let index = create_test_index();
        let query = CallChainQuery::from_index(index);

        let callers = query.get_callers_by_entity(EntityId(2));
        assert_eq!(callers.len(), 1);
        assert_eq!(callers[0], EntityId(1));
    }

    #[test]
    fn test_thread_safe_query() {
        let index = create_test_index();
        let ts_index = index; // ThreadSafeIndex is now a type alias for RelationIndex

        // Test EntityId-based API through ThreadSafeIndex directly
        assert!(SnapshotEntityQueryOps::contains_function(
            &ts_index,
            EntityId(0)
        ));
        assert!(!SnapshotEntityQueryOps::contains_function(
            &ts_index,
            EntityId(999)
        ));

        // Test get function
        let func = SnapshotEntityQueryOps::get_function_by_entity_id(&ts_index, EntityId(0));
        assert!(func.is_some());
        assert_eq!(func.expect("Failed to get function").name, "function_a");
    }
}
