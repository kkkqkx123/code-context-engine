//! Read-only graph service over a relation snapshot.
//!
//! The service composes the traversal primitives of `RelationSearcher`
//! into graph-shaped answers: ego neighborhoods, shortest paths,
//! induced subgraphs, connected components, and full exports.
//! It never touches semantic scoring.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use cce_relation::RelationQueryError;
use cce_relation::index::{
    RelationIndexView,
    snapshot_query::{SnapshotEntityQueryOps, SnapshotSymbolQueryOps},
};
use cce_types::{Entity, EntityId, Span};

use super::model::{Confidence, GraphEdge, GraphNode, SubGraph, confidence_of, kind_label};
use crate::query::error::{QueryError, Result};
use crate::query::relation_searcher::{PathQueryOptions, RelationSearcher};

/// Neighbor direction for ego graph queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphDirection {
    /// Follow outgoing (callee) edges only.
    Forward,
    /// Follow incoming (caller) edges only.
    Backward,
    /// Follow both directions.
    Both,
}

/// Upper bound for single graph operations.
const MAX_GRAPH_NODES: usize = 10_000;

/// Read-only service for graph traversal and materialization.
pub struct GraphService {
    searcher: Arc<RelationSearcher>,
}

impl GraphService {
    /// Wrap an existing relation searcher.
    pub fn new(searcher: Arc<RelationSearcher>) -> Self {
        Self { searcher }
    }

    /// Access the underlying searcher.
    pub fn searcher(&self) -> &RelationSearcher {
        &self.searcher
    }

    /// Ego neighborhood of one entity up to `depth` hops.
    pub fn ego_graph(
        &self,
        root: EntityId,
        depth: usize,
        direction: GraphDirection,
    ) -> Result<SubGraph> {
        let index = self.searcher.query().index();
        let mut builder = SubGraphBuilder::new(index);
        builder.insert_entity(root);

        let mut visited: HashSet<EntityId> = HashSet::from([root]);
        let mut frontier: VecDeque<(EntityId, usize)> = VecDeque::from([(root, 0)]);
        while let Some((current, hops)) = frontier.pop_front() {
            if hops >= depth || builder.len() >= MAX_GRAPH_NODES {
                continue;
            }
            for (neighbor, edge) in self.neighbors(current, direction) {
                builder.insert_edge(edge);
                if visited.insert(neighbor) {
                    builder.insert_entity(neighbor);
                    frontier.push_back((neighbor, hops + 1));
                }
            }
        }
        Ok(builder.finish())
    }

    /// Shortest path between two entities as a linear subgraph.
    ///
    /// A missing endpoint is reported as no path (`Ok(None)`) rather than an
    /// error, matching the graph contract where reachability is the question.
    pub fn shortest_path(
        &self,
        start: EntityId,
        end: EntityId,
        max_depth: usize,
    ) -> Result<Option<SubGraph>> {
        let options = PathQueryOptions::new().with_max_depth(max_depth);
        let nodes = match self.searcher.find_path(start, end, &options) {
            Ok(nodes) => nodes,
            Err(QueryError::Relation(RelationQueryError::NotFound(_))) => return Ok(None),
            Err(other) => return Err(other),
        };
        let Some(nodes) = nodes else { return Ok(None) };
        let index = self.searcher.query().index();
        let mut builder = SubGraphBuilder::new(index);
        let mut previous: Option<String> = None;
        for node in &nodes {
            let id = builder.insert_call_node(node);
            if let Some(prev) = previous.replace(id.clone()) {
                builder.insert_raw_edge(prev, id, node.relation_type.to_string());
            }
        }
        Ok(Some(builder.finish()))
    }

    /// Induced subgraph over an explicit entity set.
    pub fn subgraph(&self, ids: &[EntityId]) -> Result<SubGraph> {
        let index = self.searcher.query().index();
        let mut builder = SubGraphBuilder::new(index);
        let wanted: HashSet<EntityId> = ids.iter().copied().collect();
        for id in &wanted {
            builder.insert_entity(*id);
        }
        for id in &wanted {
            for relation in self.searcher.get_callees(*id) {
                if let Some(target) = relation.callee_id {
                    if wanted.contains(&target) {
                        builder.insert_relation(*id, &relation);
                    }
                }
            }
        }
        Ok(builder.finish())
    }

    /// Connected components over internal edges (union-find).
    pub fn connected_components(&self) -> Result<Vec<Vec<EntityId>>> {
        let index = self.searcher.query().index();
        let mut parent: HashMap<EntityId, EntityId> = HashMap::new();
        index.for_each_function(|id, _| {
            parent.insert(id, id);
        });
        fn find(parent: &mut HashMap<EntityId, EntityId>, mut node: EntityId) -> EntityId {
            while parent[&node] != node {
                let next = parent[&node];
                parent.insert(node, parent[&next]);
                node = next;
            }
            node
        }
        index.for_each_resolved_relation(|caller, relations| {
            for relation in relations {
                if let Some(callee) = relation.callee_id {
                    if parent.contains_key(&caller) && parent.contains_key(&callee) {
                        let a = find(&mut parent, caller);
                        let b = find(&mut parent, callee);
                        if a != b {
                            parent.insert(a, b);
                        }
                    }
                }
            }
        });
        let mut groups: HashMap<EntityId, Vec<EntityId>> = HashMap::new();
        let members: Vec<EntityId> = parent.keys().copied().collect();
        for member in members {
            let root = find(&mut parent, member);
            groups.entry(root).or_default().push(member);
        }
        let mut components: Vec<Vec<EntityId>> = groups.into_values().collect();
        for component in &mut components {
            component.sort();
        }
        components.sort_by_key(|component| component.first().copied());
        Ok(components)
    }

    /// Full project export capped at `limit` nodes in entity order.
    pub fn export_full(&self, limit: usize) -> Result<SubGraph> {
        let index = self.searcher.query().index();
        let mut ids: Vec<EntityId> = Vec::new();
        index.for_each_function(|id, _| ids.push(id));
        ids.sort();
        ids.truncate(limit);
        self.subgraph(&ids)
    }

    /// Direct neighbors of one entity with their edges.
    fn neighbors(
        &self,
        entity: EntityId,
        direction: GraphDirection,
    ) -> Vec<(EntityId, (EntityId, cce_types::ResolvedRelation))> {
        let mut out = Vec::new();
        if direction == GraphDirection::Forward || direction == GraphDirection::Both {
            for relation in self.searcher.get_callees(entity) {
                if let Some(target) = relation.callee_id {
                    out.push((target, (entity, relation)));
                } else {
                    // External targets become synthetic nodes keyed by name.
                    out.push((entity, (entity, relation)));
                }
            }
        }
        if direction == GraphDirection::Backward || direction == GraphDirection::Both {
            for caller in self.searcher.get_callers(entity) {
                for relation in self.searcher.get_callees(caller) {
                    if relation.callee_id == Some(entity) {
                        out.push((caller, (caller, relation)));
                        break;
                    }
                }
            }
        }
        out
    }
}

/// Incremental builder that deduplicates nodes by id.
struct SubGraphBuilder<'a> {
    index: &'a cce_relation::index::snapshot_index::LayeredSnapshotIndex,
    nodes: Vec<GraphNode>,
    seen: HashSet<String>,
    edges: Vec<GraphEdge>,
}

impl<'a> SubGraphBuilder<'a> {
    fn new(index: &'a cce_relation::index::snapshot_index::LayeredSnapshotIndex) -> Self {
        Self {
            index,
            nodes: Vec::new(),
            seen: HashSet::new(),
            edges: Vec::new(),
        }
    }

    fn len(&self) -> usize {
        self.nodes.len()
    }

    fn finish(self) -> SubGraph {
        SubGraph {
            nodes: self.nodes,
            edges: self.edges,
        }
    }

    fn insert_entity(&mut self, id: EntityId) {
        let node_id = self.node_id(id);
        if !self.seen.insert(node_id.clone()) {
            return;
        }
        let (label, kind, file, location) = match self.entity_metadata(id) {
            Some(entity) => {
                let file = self.index.get_file_path_by_entity(id).unwrap_or_default();
                (
                    entity.name.clone(),
                    kind_label(&entity.kind),
                    file,
                    location_of(&entity.span),
                )
            }
            None => (
                node_id.clone(),
                "unknown".to_string(),
                String::new(),
                String::new(),
            ),
        };
        self.nodes.push(GraphNode {
            id: node_id,
            label,
            kind,
            source_file: file,
            source_location: location,
        });
    }

    fn insert_call_node(&mut self, node: &cce_relation::CallChainNode) -> String {
        let node_id = self.node_id(node.function_id);
        if self.seen.insert(node_id.clone()) {
            let (kind, location) = match self.entity_metadata(node.function_id) {
                Some(entity) => (
                    kind_label(&entity.kind),
                    location_of(&entity.span),
                ),
                None => ("unknown".to_string(), String::new()),
            };
            let location = match node.call_line {
                Some(line) if location.is_empty() => format!("L{line}"),
                _ => location,
            };
            self.nodes.push(GraphNode {
                id: node_id.clone(),
                label: node.function_name.clone(),
                kind,
                source_file: node.file_path.clone(),
                source_location: location,
            });
        }
        node_id
    }

    fn insert_relation(&mut self, caller: EntityId, relation: &cce_types::ResolvedRelation) {
        if relation.is_external {
            let target = self.external_id(&relation.callee_name);
            self.insert_external(&target, &relation.callee_name);
            self.insert_raw_edge(
                self.node_id(caller),
                target,
                relation.relation_type.to_string(),
            );
            return;
        }
        if let Some(target) = relation.callee_id {
            self.insert_entity(target);
            self.edges.push(GraphEdge {
                source: self.node_id(caller),
                target: self.node_id(target),
                relation: relation.relation_type.to_string(),
                confidence: confidence_of(relation),
            });
        }
    }

    fn insert_edge(&mut self, edge: (EntityId, cce_types::ResolvedRelation)) {
        let (caller, relation) = edge;
        self.insert_relation(caller, &relation);
    }

    fn insert_raw_edge(&mut self, source: String, target: String, relation: String) {
        self.edges.push(GraphEdge {
            source,
            target,
            relation,
            confidence: Confidence::Inferred,
        });
    }

    fn insert_external(&mut self, id: &str, name: &str) {
        if self.seen.insert(id.to_string()) {
            self.nodes.push(GraphNode {
                id: id.to_string(),
                label: name.to_string(),
                kind: "external".to_string(),
                source_file: String::new(),
                source_location: String::new(),
            });
        }
    }

    fn node_id(&self, id: EntityId) -> String {
        self.index
            .get_symbol_key_by_entity_id(id)
            .map(|key| key.stable_id().0)
            .unwrap_or_else(|| format!("entity:{}", id.0))
    }

    fn external_id(&self, name: &str) -> String {
        format!("external::{name}")
    }

    fn entity_metadata(&self, id: EntityId) -> Option<Entity> {
        self.index.get_function_by_entity_id(id)
    }
}

/// Render a span as a one-based line marker.
fn location_of(span: &Span) -> String {
    if span.start_position.row == usize::MAX {
        String::new()
    } else {
        format!("L{}", span.start_position.row + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_relation::CallChainQuery;

    fn empty_service() -> GraphService {
        GraphService::new(Arc::new(RelationSearcher::new(Arc::new(
            CallChainQuery::new(),
        ))))
    }

    #[test]
    fn test_ego_graph_unknown_entity_is_singleton() {
        let service = empty_service();
        let graph = service
            .ego_graph(EntityId(42), 2, GraphDirection::Both)
            .expect("ego");
        assert_eq!(graph.nodes.len(), 1);
        assert!(graph.edges.is_empty());
    }

    #[test]
    fn test_subgraph_empty_is_empty() {
        let service = empty_service();
        let graph = service.subgraph(&[]).expect("subgraph");
        assert!(graph.nodes.is_empty());
        assert!(graph.edges.is_empty());
    }

    #[test]
    fn test_components_empty_is_empty() {
        let service = empty_service();
        let components = service.connected_components().expect("components");
        assert!(components.is_empty());
    }

    #[test]
    fn test_shortest_path_missing_is_none() {
        let service = empty_service();
        let path = service
            .shortest_path(EntityId(1), EntityId(2), 3)
            .expect("path");
        assert!(path.is_none());
    }

    #[test]
    fn test_location_of_unavailable_span() {
        assert_eq!(location_of(&Span::unavailable()), String::new());
        let span = Span::new(0, 10, 41, 0, 41, 10);
        assert_eq!(location_of(&span), "L42");
    }
}
