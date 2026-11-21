//! Call-chain graph traversal primitives.
//!
//! Holds the traversal configuration and the generic BFS traverser used by
//! [`super::CallChainQuery`]. Kept separate from the query facade so the
//! traversal algorithm has a single, reviewable responsibility.

use crate::error::RelationQueryError;
use crate::index::core::{CallChainNode, RelationIndex};
use crate::index::snapshot_query::SnapshotQueryIndex;
use cce_types::relation::CallContext;
use cce_types::{EntityId, RelationType, ResolvedRelation};
use std::collections::{HashSet, VecDeque};

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
/// Works over any queryable index surface (`RelationIndex`,
/// `RelationSnapshotIndex`, `LayeredSnapshotIndex`); only read operations
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
