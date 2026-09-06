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
use super::index::ThreadSafeIndex;
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
}

impl Default for TraversalConfig {
    fn default() -> Self {
        Self {
            max_depth: usize::MAX,
            include_start_node: false,
            stop_on_cycles: true,
            direction: TraversalDirection::Forward,
            debug: false,
            max_nodes: 10000, // Safety limit to prevent infinite loops
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
                let callers = self.index.get_callers_by_callee_entity(start_id);

                for caller_id in callers {
                    queue.push_back((
                        caller_id,
                        1,
                        RelationType::DirectCall,
                        None,
                        None,
                        CallContext::Direct,
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
                        let callers = self.index.get_callers_by_callee_entity(current_id);
                        for caller_id in callers {
                            queue.push_back((
                                caller_id,
                                depth + 1,
                                RelationType::DirectCall,
                                None,
                                None,
                                CallContext::Direct,
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
        // Validate configuration
        self.config.validate()?;

        // Verify both nodes exist
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
    /// Returns an error if the entity doesn't exist or the query fails.
    pub fn get_callees_by_entity(
        &self,
        entity_id: EntityId,
    ) -> Result<Vec<ResolvedRelation>, RelationQueryError> {
        // Check if entity exists
        if !self.index.contains_function(entity_id) {
            return Err(RelationQueryError::not_found(format!(
                "Entity not found: {:?}",
                entity_id
            )));
        }

        // Get relations
        self.index
            .get_resolved_relations_by_caller(entity_id)
            .ok_or_else(|| {
                RelationQueryError::internal(format!(
                    "Failed to get relations for entity: {:?}",
                    entity_id
                ))
            })
    }

    /// Get callers (functions that call this function) by EntityId
    pub fn get_callers_by_entity(&self, entity_id: EntityId) -> Vec<EntityId> {
        // Get callers (this method always returns a Vec, empty if not found)
        self.index.get_callers_by_callee_entity(entity_id)
    }

    /// Get callees with detailed error logging
    ///
    /// This method provides enhanced debugging information through structured logging.
    pub fn get_callees_by_entity_with_logging(
        &self,
        entity_id: EntityId,
    ) -> Result<Vec<ResolvedRelation>, RelationQueryError> {
        let span = tracing::span!(
            tracing::Level::DEBUG,
            "get_callees",
            entity_id = ?entity_id
        );
        let _enter = span.enter();

        // Check if entity exists
        if !self.index.contains_function(entity_id) {
            tracing::warn!(
                entity_id = ?entity_id,
                "Entity not found in function index"
            );
            return Err(RelationQueryError::not_found(format!(
                "Entity not found: {:?}",
                entity_id
            )));
        }

        // Get relations
        match self.index.get_resolved_relations_by_caller(entity_id) {
            Some(relations) => Ok(relations),
            None => {
                tracing::warn!(
                    entity_id = ?entity_id,
                    "No relations found for existing entity"
                );
                Err(RelationQueryError::internal(format!(
                    "Failed to get relations for entity: {:?}",
                    entity_id
                )))
            }
        }
    }

    /// Get callees with fallback
    ///
    /// Returns an empty vector on error instead of failing, with a warning log.
    pub fn get_callees_by_entity_safe(&self, entity_id: EntityId) -> Vec<ResolvedRelation> {
        match self.get_callees_by_entity(entity_id) {
            Ok(relations) => relations,
            Err(e) => {
                tracing::warn!(
                    entity_id = ?entity_id,
                    error = %e,
                    "Failed to get callees, returning empty"
                );
                Vec::new()
            }
        }
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

    /// Get inheritance hierarchy (all ancestors)
    pub fn get_inheritance_hierarchy(&self, class_id: EntityId, max_depth: usize) -> Vec<EntityId> {
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();

        queue.push_back((class_id, 0));

        while let Some((current_id, depth)) = queue.pop_front() {
            if depth >= max_depth {
                continue;
            }

            let base_classes = self.index.get_base_classes(current_id);
            for base_id in base_classes {
                if !visited.contains(&base_id) {
                    visited.insert(base_id);
                    result.push(base_id);
                    queue.push_back((base_id, depth + 1));
                }
            }
        }

        result
    }

    /// Get all derived classes (transitive closure)
    pub fn get_all_derived_classes(&self, class_id: EntityId, max_depth: usize) -> Vec<EntityId> {
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();

        queue.push_back((class_id, 0));

        while let Some((current_id, depth)) = queue.pop_front() {
            if depth >= max_depth {
                continue;
            }

            let derived = self.index.get_derived_classes(current_id);
            for derived_id in derived {
                if !visited.contains(&derived_id) {
                    visited.insert(derived_id);
                    result.push(derived_id);
                    queue.push_back((derived_id, depth + 1));
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
    pub fn get_change_impact(&self, file_path: &str) -> crate::dependency_graph::ImpactAnalysis {
        use crate::index::view::RelationIndexView;
        let direct_dependents = self.index.dependents_of(file_path);
        let transitive_dependents = self.index.collect_transitive_dependents(file_path, 10);
        let impact_score = if transitive_dependents.is_empty() {
            0.0
        } else {
            (transitive_dependents.len() as f64 * 10.0).min(100.0)
        };
        crate::dependency_graph::ImpactAnalysis {
            changed_file: file_path.to_string(),
            direct_dependents,
            transitive_dependents,
            impact_score,
        }
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
    ) -> Result<Option<Vec<CallChainNode>>, RelationQueryError> {
        let config = TraversalConfig::new()
            .with_max_depth(max_depth)
            .with_include_start_node(true)
            .with_stop_on_cycles(true)
            .with_direction(TraversalDirection::Forward);

        let traverser = CallChainTraverser::new(self.index.as_ref(), config);
        traverser.find_path(start_id, end_id)
    }
}

impl CallChainQuery {
    /// Get callees (functions called by this function) - simplified wrapper.
    pub fn get_callees(&self, entity_id: EntityId) -> Vec<ResolvedRelation> {
        self.get_callees_by_entity(entity_id).unwrap_or_default()
    }

    /// Get callers (functions that call this function) - simplified wrapper.
    pub fn get_callers(&self, entity_id: EntityId) -> Vec<EntityId> {
        self.get_callers_by_entity(entity_id)
    }

    /// Query forward call chain (caller -> callees) with simplified return.
    pub fn query_forward(
        &self,
        entity_id: EntityId,
        max_depth: usize,
    ) -> Vec<crate::index::core::CallChainNode> {
        self.query_forward_by_entity(entity_id, max_depth)
            .unwrap_or_default()
    }

    /// Query backward call chain (callee -> callers) with simplified return.
    pub fn query_backward(
        &self,
        entity_id: EntityId,
        max_depth: usize,
    ) -> Vec<crate::index::core::CallChainNode> {
        self.query_backward_by_entity(entity_id, max_depth)
            .unwrap_or_default()
    }

    /// Build a call graph (forward traversal collecting nodes and edges).
    pub fn build_call_graph(
        &self,
        start_id: EntityId,
        max_depth: usize,
    ) -> crate::types::CallChainGraph {
        let nodes = self.query_forward(start_id, max_depth);
        let mut edges = Vec::new();
        for node in &nodes {
            if let Ok(callees) = self.get_callees_by_entity(node.function_id) {
                for callee in callees {
                    if let Some(callee_id) = callee.callee_id {
                        edges.push((node.function_id, callee_id, callee.relation_type));
                    }
                }
            }
        }
        crate::types::CallChainGraph { nodes, edges }
    }

    /// Find dependency path between two entities.
    pub fn find_dependency_path(
        &self,
        start_id: EntityId,
        target_id: EntityId,
    ) -> Option<Vec<crate::index::core::CallChainNode>> {
        self.find_call_chain(start_id, target_id, 10)
            .unwrap_or(None)
    }
}

impl Default for CallChainQuery {
    fn default() -> Self {
        Self::new()
    }
}

/// Unified call chain query over a `UnifiedSnapshotIndex`.
///
/// Mirrors `CallChainQuery` but directly uses the unified snapshot.
pub struct UnifiedCallChainQuery {
    index: Arc<crate::index::unified_snapshot::UnifiedSnapshotIndex>,
}

impl UnifiedCallChainQuery {
    pub fn new() -> Self {
        Self {
            index: Arc::new(crate::index::unified_snapshot::UnifiedSnapshotIndex::empty()),
        }
    }

    pub fn from_snapshot(index: Arc<crate::index::unified_snapshot::UnifiedSnapshotIndex>) -> Self {
        Self { index }
    }

    pub fn from_relation_index(index: &RelationIndex) -> Self {
        Self {
            index: Arc::new(
                crate::index::unified_snapshot::UnifiedSnapshotIndex::from_relation_index(index),
            ),
        }
    }

    pub fn index(&self) -> &crate::index::unified_snapshot::UnifiedSnapshotIndex {
        &self.index
    }

    pub fn get_callees_by_entity(
        &self,
        entity_id: EntityId,
    ) -> Result<Vec<ResolvedRelation>, RelationQueryError> {
        use crate::index::snapshot_query::SnapshotRelationQueryOps;
        if !self.index.contains_function(entity_id) {
            return Err(RelationQueryError::not_found(format!(
                "Entity not found: {:?}",
                entity_id
            )));
        }
        self.index
            .get_resolved_relations_by_caller(entity_id)
            .ok_or_else(|| {
                RelationQueryError::internal(format!(
                    "Failed to get relations for entity: {:?}",
                    entity_id
                ))
            })
    }

    pub fn get_callers_by_entity(&self, entity_id: EntityId) -> Vec<EntityId> {
        use crate::index::snapshot_query::SnapshotRelationQueryOps;
        self.index.get_callers_by_callee_entity(entity_id)
    }

    pub fn get_callees(&self, entity_id: EntityId) -> Vec<ResolvedRelation> {
        self.get_callees_by_entity(entity_id).unwrap_or_default()
    }

    pub fn get_callers(&self, entity_id: EntityId) -> Vec<EntityId> {
        self.get_callers_by_entity(entity_id)
    }

    pub fn query_forward(
        &self,
        entity_id: EntityId,
        max_depth: usize,
    ) -> Result<Vec<crate::index::core::CallChainNode>, RelationQueryError> {
        let config = TraversalConfig::new()
            .with_max_depth(max_depth)
            .with_include_start_node(false)
            .with_stop_on_cycles(true)
            .with_direction(TraversalDirection::Forward);
        let traverser = CallChainTraverser::new(self.index.as_ref(), config);
        traverser.traverse_from(entity_id)
    }

    pub fn query_backward(
        &self,
        entity_id: EntityId,
        max_depth: usize,
    ) -> Result<Vec<crate::index::core::CallChainNode>, RelationQueryError> {
        use crate::index::snapshot_query::SnapshotEntityQueryOps;
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
}

impl Default for UnifiedCallChainQuery {
    fn default() -> Self {
        Self::new()
    }
}

/// Thread-safe call chain query
///
/// Provides thread-safe access to call chain queries.
pub struct ThreadSafeQuery {
    index: ThreadSafeIndex,
}

impl ThreadSafeQuery {
    /// Create a new thread-safe query
    pub fn new() -> Self {
        Self {
            index: ThreadSafeIndex::new(),
        }
    }

    /// Create from an existing thread-safe index
    pub fn from_index(index: ThreadSafeIndex) -> Self {
        Self { index }
    }

    /// Get a reference to the underlying index
    pub fn index(&self) -> &ThreadSafeIndex {
        &self.index
    }

    /// Get a clone of the index for sharing
    pub fn share_index(&self) -> ThreadSafeIndex {
        self.index.clone()
    }
}

impl Default for ThreadSafeQuery {
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
        });

        index
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
            .find_call_chain(EntityId(0), EntityId(2), 5)
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
