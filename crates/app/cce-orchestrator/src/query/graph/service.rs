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
    snapshot_query::{SnapshotEntityQueryOps, SnapshotRelationQueryOps, SnapshotSymbolQueryOps},
};
use cce_types::{Entity, EntityId, Span};

use super::model::{
    Confidence, GraphEdge, GraphFilter, GraphNode, GraphPagination, PagedComponents, PagedSubGraph,
    SubGraph, confidence_of, kind_label, relation_domain,
};
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
    /// Edges preserve the true relation type and call context from traversal.
    ///
    /// The filter constrains traversal itself, not just the returned edges:
    /// an edge the filter rejects is never crossed, so `domains=call` yields a
    /// call-only path rather than a path trimmed after the fact.
    pub fn shortest_path(
        &self,
        start: EntityId,
        end: EntityId,
        max_depth: usize,
    ) -> Result<Option<SubGraph>> {
        self.shortest_path_with_options(start, end, max_depth, &GraphFilter::allow_all())
    }

    /// Shortest path with relation filtering.
    pub fn shortest_path_with_options(
        &self,
        start: EntityId,
        end: EntityId,
        max_depth: usize,
        filter: &GraphFilter,
    ) -> Result<Option<SubGraph>> {
        let options = PathQueryOptions::new().with_max_depth(max_depth);
        let nodes = match self
            .searcher
            .find_path_filtered(start, end, &options, filter)
        {
            Ok(nodes) => nodes,
            Err(QueryError::Relation(RelationQueryError::NotFound(_))) => return Ok(None),
            Err(other) => return Err(other),
        };
        let Some(nodes) = nodes else { return Ok(None) };
        let index = self.searcher.query().index();
        let mut builder = SubGraphBuilder::new(index);
        let mut previous: Option<EntityId> = None;
        for node in &nodes {
            builder.insert_call_node(node);
            if let Some(prev) = previous {
                builder.insert_path_edge(prev, node);
            }
            previous = Some(node.function_id);
        }
        Ok(Some(builder.finish()))
    }

    /// Induced subgraph over an explicit entity set.
    pub fn subgraph(&self, ids: &[EntityId]) -> Result<SubGraph> {
        Ok(self
            .subgraph_with_options(ids, &GraphFilter::allow_all(), GraphPagination::default())?
            .into_subgraph())
    }

    /// Induced subgraph with relation filtering and pagination.
    pub fn subgraph_with_options(
        &self,
        ids: &[EntityId],
        filter: &GraphFilter,
        pagination: GraphPagination,
    ) -> Result<PagedSubGraph> {
        let index = self.searcher.query().index();
        let mut builder = SubGraphBuilder::new(index);
        let wanted: HashSet<EntityId> = ids.iter().copied().collect();
        for id in &wanted {
            builder.insert_entity(*id);
        }
        for id in &wanted {
            for relation in self.searcher.get_callees(*id) {
                if !Self::relation_passes_filter(&relation, filter) {
                    continue;
                }
                if let Some(target) = relation.callee_id {
                    if wanted.contains(&target) {
                        builder.insert_relation(*id, &relation);
                    }
                } else if filter.include_external {
                    builder.insert_relation(*id, &relation);
                }
            }
        }
        Ok(paginate_graph(builder.finish(), filter, pagination))
    }

    /// Connected components over internal edges (union-find).
    ///
    /// Unfiltered and uncapped; prefer [`Self::connected_components_with_options`]
    /// on projects large enough for the whole component list to matter.
    pub fn connected_components(&self) -> Result<Vec<Vec<EntityId>>> {
        Ok(self
            .connected_components_with_options(
                &GraphFilter::allow_all(),
                GraphPagination::default(),
            )?
            .components)
    }

    /// Connected components with relation filtering and pagination.
    ///
    /// Only edges accepted by the filter join two entities into the same
    /// component, so `domains=call` yields the call-graph decomposition rather
    /// than the same partition with cosmetic changes.
    pub fn connected_components_with_options(
        &self,
        filter: &GraphFilter,
        pagination: GraphPagination,
    ) -> Result<PagedComponents> {
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
                if !Self::relation_passes_filter(relation, filter) {
                    continue;
                }
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
        // Largest first so a truncated page keeps the structurally significant
        // components; ties break on the smallest member for determinism.
        components.sort_by(|left, right| {
            right
                .len()
                .cmp(&left.len())
                .then_with(|| left.first().cmp(&right.first()))
        });
        let total = components.len();
        let components = components
            .into_iter()
            .skip(pagination.offset)
            .take(pagination.limit)
            .collect();
        Ok(PagedComponents {
            components,
            total_components: total,
        })
    }

    /// Ego neighborhood with relation filtering and pagination.
    pub fn ego_graph_with_options(
        &self,
        root: EntityId,
        depth: usize,
        direction: GraphDirection,
        filter: &GraphFilter,
        pagination: GraphPagination,
    ) -> Result<PagedSubGraph> {
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
                let (_, relation) = &edge;
                if !Self::relation_passes_filter(relation, filter) {
                    continue;
                }
                builder.insert_edge(edge);
                if visited.insert(neighbor) {
                    builder.insert_entity(neighbor);
                    frontier.push_back((neighbor, hops + 1));
                }
            }
        }
        Ok(paginate_graph(builder.finish(), filter, pagination))
    }

    /// Full project export capped at `limit` nodes in entity order.
    pub fn export_full(&self, limit: usize) -> Result<SubGraph> {
        Ok(self
            .export_full_with_options(limit, &GraphFilter::allow_all(), GraphPagination::default())?
            .into_subgraph())
    }

    /// Full project export with hub-first ordering, filtering and pagination.
    ///
    /// `limit` caps the hub set before filtering; `pagination` slices the
    /// materialized subgraph afterwards. Totals reflect the filtered hub set
    /// before pagination.
    pub fn export_full_with_options(
        &self,
        limit: usize,
        filter: &GraphFilter,
        pagination: GraphPagination,
    ) -> Result<PagedSubGraph> {
        let index = self.searcher.query().index();
        let mut degrees: HashMap<EntityId, usize> = HashMap::new();
        index.for_each_function(|id, _| {
            degrees.insert(id, 0);
        });
        index.for_each_resolved_relation(|caller, relations| {
            if let Some(count) = degrees.get_mut(&caller) {
                *count = relations.len();
            }
        });
        let mut ids: Vec<EntityId> = degrees.keys().copied().collect();
        ids.sort_by(|left, right| {
            degrees
                .get(right)
                .copied()
                .unwrap_or_default()
                .cmp(&degrees.get(left).copied().unwrap_or_default())
                .then_with(|| left.cmp(right))
        });
        ids.truncate(limit);
        self.subgraph_with_options(&ids, filter, pagination)
    }

    /// Compute graph centrality and clustering metrics for all entities.
    pub fn compute_metrics(
        &self,
    ) -> std::collections::HashMap<cce_types::EntityId, cce_relation::graph_metrics::EntityMetrics>
    {
        cce_relation::graph_metrics::compute_metrics(self.searcher.query().index())
    }

    /// Whether a stored relation survives the graph filter.
    fn relation_passes_filter(
        relation: &cce_types::ResolvedRelation,
        filter: &GraphFilter,
    ) -> bool {
        if !filter.include_external && relation.is_external {
            return false;
        }
        if filter.relation_domains.is_empty() {
            return true;
        }
        let domain = relation_domain(&relation.relation_type);
        filter.relation_domains.iter().any(|d| d == domain)
    }
}

/// Apply relation filtering then pagination to a materialized subgraph.
///
/// Pagination addresses nodes only. The returned edges are every filtered edge
/// induced on the returned node page, so each page is a self-consistent
/// subgraph a client can render without stitching pages together. Offsetting
/// and limiting the edge list independently would both break that invariant
/// (a page could carry edges pointing at absent nodes) and silently drop
/// every edge on any page past the first.
///
/// `total_nodes` counts filtered nodes before pagination; `total_edges` counts
/// filtered edges over that whole node set, i.e. the edges the full graph
/// would yield.
fn paginate_graph(
    graph: SubGraph,
    filter: &GraphFilter,
    pagination: GraphPagination,
) -> PagedSubGraph {
    let filtered_edges: Vec<GraphEdge> = graph
        .edges
        .into_iter()
        .filter(|edge| filter.matches_edge(edge))
        .collect();
    let total_nodes = graph.nodes.len();
    let total_edges = filtered_edges.len();
    let nodes: Vec<GraphNode> = graph
        .nodes
        .into_iter()
        .skip(pagination.offset)
        .take(pagination.limit)
        .collect();
    let surviving: HashSet<&str> = nodes.iter().map(|node| node.id.as_str()).collect();
    let edges: Vec<GraphEdge> = filtered_edges
        .into_iter()
        .filter(|edge| {
            surviving.contains(edge.source.as_str()) && surviving.contains(edge.target.as_str())
        })
        .collect();
    PagedSubGraph {
        nodes,
        edges,
        total_nodes,
        total_edges,
    }
}

impl GraphService {
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
            let index = self.searcher.query().index();
            for relation in index.get_relations_to_entity(entity) {
                out.push((relation.caller, (relation.caller, relation)));
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
        let (label, kind, file, location, scoped_name, signature) = match self.entity_metadata(id) {
            Some(entity) => {
                let file = self.index.get_file_path_by_entity(id).unwrap_or_default();
                (
                    entity.name.clone(),
                    kind_label(&entity.kind),
                    file,
                    location_of(&entity.span),
                    Some(entity.name.clone()),
                    (!entity.signature.is_empty()).then_some(entity.signature.clone()),
                )
            }
            None => (
                node_id.clone(),
                "unknown".to_string(),
                String::new(),
                String::new(),
                None,
                None,
            ),
        };
        self.nodes.push(GraphNode {
            id: node_id,
            label,
            kind,
            source_file: file,
            source_location: location,
            scoped_name,
            signature,
        });
    }

    fn insert_call_node(&mut self, node: &cce_relation::CallChainNode) -> String {
        let node_id = self.node_id(node.function_id);
        if self.seen.insert(node_id.clone()) {
            let (kind, location) = match self.entity_metadata(node.function_id) {
                Some(entity) => (kind_label(&entity.kind), location_of(&entity.span)),
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
                scoped_name: Some(node.function_name.clone()),
                signature: None,
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
                domain: relation_domain(&relation.relation_type).to_string(),
                confidence: confidence_of(relation),
                call_context: Some(relation.call_context.tag().to_string()),
                is_external: false,
                weight: edge_weight(&relation.relation_type)
                    * call_frequency_weight(relation.call_frequency),
                cfg_condition: relation.cfg_condition.clone(),
            });
        }
    }

    fn insert_edge(&mut self, edge: (EntityId, cce_types::ResolvedRelation)) {
        let (caller, relation) = edge;
        self.insert_relation(caller, &relation);
    }
    fn insert_path_edge(&mut self, source: EntityId, target: &cce_relation::CallChainNode) {
        let confidence = if target.relation_type.is_call()
            && matches!(
                target.call_context,
                cce_types::relation::CallContext::Direct
            ) {
            Confidence::Extracted
        } else {
            Confidence::Inferred
        };
        self.edges.push(GraphEdge {
            source: self.node_id(source),
            target: self.node_id(target.function_id),
            relation: target.relation_type.to_string(),
            domain: relation_domain(&target.relation_type).to_string(),
            confidence,
            call_context: Some(target.call_context.tag().to_string()),
            is_external: false,
            weight: edge_weight(&target.relation_type),
            cfg_condition: None,
        });
    }

    fn insert_raw_edge(&mut self, source: String, target: String, relation: String) {
        self.edges.push(GraphEdge {
            source,
            target,
            relation,
            // Raw string edges (external/plugin relations without a parsed
            // RelationType) cannot be classified further.
            domain: "other".to_string(),
            confidence: Confidence::Inferred,
            call_context: None,
            is_external: true,
            weight: 1.0,
            cfg_condition: None,
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
                scoped_name: None,
                signature: None,
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

/// Compute edge weight based on relation type.
///
/// Direct calls have the strongest weight (1.0), method calls slightly less
/// (0.9), field access moderate (0.5), and type references weakest (0.3).
pub fn edge_weight(relation_type: &cce_types::RelationType) -> f32 {
    use cce_types::RelationType::*;
    match relation_type {
        DirectCall => 1.0,
        InstanceMethodCall | StaticMethodCall | ChainedMethodCall => 0.9,
        ConstructorCall | PointerCall | CallbackCall | GenericCall | MacroCall => 0.8,
        GoroutineCall | DeferredCall | AsyncCall | HigherOrderCall => 0.7,
        FieldAccess => 0.5,
        TypeReference => 0.3,
        _ => 0.6,
    }
}

/// Hotness multiplier derived from how many call sites a caller uses to reach
/// the target.
///
/// Logarithmic in the frequency so a heavily repeated edge outranks a
/// single-site one without letting one hub swamp every weighted traversal:
/// each doubling of the call-site count adds one to the factor. A frequency of
/// one leaves the type weight untouched, so edges with no repetition carry
/// exactly the weight their relation type assigns.
pub fn call_frequency_weight(call_frequency: u64) -> f32 {
    if call_frequency <= 1 {
        return 1.0;
    }
    1.0 + call_frequency.max(2).ilog2() as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_relation::CallChainQuery;
    use cce_types::{Entity, EntityKind, RelationType, Span};
    use std::collections::HashMap;

    fn empty_service() -> GraphService {
        GraphService::new(Arc::new(RelationSearcher::new(Arc::new(
            CallChainQuery::new(),
        ))))
    }

    fn entity(id: u64, name: &str) -> Entity {
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

    /// Service over a chain `a -> b -> c -> d` of direct calls.
    fn chain_service() -> GraphService {
        use cce_relation::index::EntityIndexOps;

        let index = cce_relation::CallChainQuery::from_index({
            let base = cce_relation::RelationIndex::new();
            for (id, name) in [(1u64, "a"), (2, "b"), (3, "c"), (4, "d")] {
                base.add_function_with_path(EntityId(id), entity(id, name), "src/lib.rs".into());
            }
            for (from, to) in [(1u64, 2u64), (2, 3), (3, 4)] {
                base.add_resolved_relation(cce_types::ResolvedRelation {
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
                    call_context: cce_types::relation::CallContext::Direct,
                    overload_signature: None,
                    call_frequency: 1,
                    cfg_condition: None,
                });
            }
            base
        });
        GraphService::new(Arc::new(RelationSearcher::new(Arc::new(index))))
    }

    #[test]
    fn pagination_addresses_nodes_only() {
        // Two independent edges over four nodes.
        let graph = SubGraph {
            nodes: (0..4)
                .map(|index| GraphNode {
                    id: format!("n{index}"),
                    label: format!("n{index}"),
                    kind: "function".to_string(),
                    source_file: String::new(),
                    source_location: String::new(),
                    scoped_name: None,
                    signature: None,
                })
                .collect(),
            edges: vec![
                edge("n0", "n1", "call.direct"),
                edge("n2", "n3", "call.direct"),
            ],
        };

        let page = paginate_graph(
            graph,
            &GraphFilter::allow_all(),
            GraphPagination {
                offset: 0,
                limit: 2,
            },
        );
        assert_eq!(page.total_nodes, 4);
        assert_eq!(page.total_edges, 2);
        assert_eq!(page.nodes.len(), 2);
        // Every edge on the page is retained: the edge list follows the node
        // page instead of being offset independently.
        assert_eq!(page.edges.len(), 1);
        assert_eq!(page.edges[0].source, "n0");

        let page = paginate_graph(
            SubGraph {
                nodes: (0..4)
                    .map(|index| GraphNode {
                        id: format!("n{index}"),
                        label: format!("n{index}"),
                        kind: "function".to_string(),
                        source_file: String::new(),
                        source_location: String::new(),
                        scoped_name: None,
                        signature: None,
                    })
                    .collect(),
                edges: vec![
                    edge("n0", "n1", "call.direct"),
                    edge("n2", "n3", "call.direct"),
                ],
            },
            &GraphFilter::allow_all(),
            GraphPagination {
                offset: 2,
                limit: 2,
            },
        );
        assert_eq!(page.nodes.len(), 2);
        assert_eq!(page.edges.len(), 1);
        assert_eq!(page.edges[0].source, "n2");
    }

    #[test]
    fn pagination_never_yields_dangling_edges() {
        let graph = SubGraph {
            nodes: (0..3)
                .map(|index| GraphNode {
                    id: format!("n{index}"),
                    label: format!("n{index}"),
                    kind: "function".to_string(),
                    source_file: String::new(),
                    source_location: String::new(),
                    scoped_name: None,
                    signature: None,
                })
                .collect(),
            edges: vec![
                edge("n0", "n1", "call.direct"),
                edge("n1", "n2", "call.direct"),
            ],
        };
        let page = paginate_graph(
            graph,
            &GraphFilter::allow_all(),
            GraphPagination {
                offset: 0,
                limit: 1,
            },
        );
        let present: HashSet<&str> = page.nodes.iter().map(|node| node.id.as_str()).collect();
        for edge in &page.edges {
            assert!(present.contains(edge.source.as_str()));
            assert!(present.contains(edge.target.as_str()));
        }
    }

    #[test]
    fn filtered_path_refuses_to_cross_filtered_domains() {
        let service = chain_service();
        // The chain is call-only, so restricting to the structural domain
        // leaves no traversable edge and no path.
        let structural_only = GraphFilter {
            relation_domains: vec!["structural".to_string()],
            relation_types: Vec::new(),
            include_external: true,
        };
        assert!(
            service
                .shortest_path_with_options(EntityId(1), EntityId(4), 5, &structural_only)
                .expect("path")
                .is_none()
        );
        // Unfiltered, the same query resolves.
        assert!(
            service
                .shortest_path(EntityId(1), EntityId(4), 5)
                .expect("path")
                .is_some()
        );
    }

    #[test]
    fn components_are_ordered_largest_first() {
        let service = chain_service();
        let paged = service
            .connected_components_with_options(
                &GraphFilter::allow_all(),
                GraphPagination {
                    offset: 0,
                    limit: 10,
                },
            )
            .expect("components");
        assert_eq!(paged.total_components, 1);
        assert_eq!(paged.components.len(), 1);
        assert_eq!(paged.components[0].len(), 4);
    }

    #[test]
    fn components_pagination_reports_totals() {
        let service = chain_service();
        let paged = service
            .connected_components_with_options(
                &GraphFilter::allow_all(),
                GraphPagination {
                    offset: 5,
                    limit: 10,
                },
            )
            .expect("components");
        assert_eq!(paged.total_components, 1);
        assert!(paged.components.is_empty());
    }

    #[test]
    fn domain_filter_splits_components() {
        // An inheritance edge between two of the chain's functions must not
        // join them into one component once `domains=call` is applied.
        let service = {
            use cce_relation::index::EntityIndexOps;

            let base = cce_relation::RelationIndex::new();
            for (id, name) in [(1u64, "a"), (2, "b")] {
                base.add_function_with_path(EntityId(id), entity(id, name), "src/lib.rs".into());
            }
            base.add_resolved_relation(cce_types::ResolvedRelation {
                caller: EntityId(1),
                callee_id: Some(EntityId(2)),
                callee_name: "b".to_string(),
                relation_type: RelationType::Inheritance,
                span: Span::default(),
                is_external: false,
                external_type: None,
                callee_symbol: None,
                stdlib_category: None,
                owner_type: None,
                call_context: cce_types::relation::CallContext::Direct,
                overload_signature: None,
                call_frequency: 1,
                cfg_condition: None,
            });
            GraphService::new(Arc::new(RelationSearcher::new(Arc::new(
                CallChainQuery::from_index(base),
            ))))
        };
        let all = GraphFilter::allow_all();
        assert_eq!(
            service
                .connected_components_with_options(&all, GraphPagination::default())
                .expect("components")
                .total_components,
            1
        );
        let calls_only = GraphFilter {
            relation_domains: vec!["call".to_string()],
            relation_types: Vec::new(),
            include_external: true,
        };
        assert_eq!(
            service
                .connected_components_with_options(&calls_only, GraphPagination::default())
                .expect("components")
                .total_components,
            2
        );
    }

    #[test]
    fn test_repeated_call_sites_raise_edge_weight() {
        assert_eq!(call_frequency_weight(0), 1.0);
        assert_eq!(call_frequency_weight(1), 1.0);
        assert_eq!(call_frequency_weight(2), 2.0);
        assert_eq!(call_frequency_weight(4), 3.0);
        assert_eq!(call_frequency_weight(8), 4.0);
    }

    #[test]
    fn test_edge_weight_scales_with_call_frequency() {
        let once = edge_weight(&RelationType::DirectCall) * call_frequency_weight(1);
        let thrice = edge_weight(&RelationType::DirectCall) * call_frequency_weight(3);
        assert_eq!(once, 1.0);
        assert!(
            thrice > once,
            "a repeated call site must outweigh a single one"
        );
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

    fn edge(source: &str, target: &str, relation: &str) -> GraphEdge {
        GraphEdge {
            source: source.to_string(),
            target: target.to_string(),
            relation: relation.to_string(),
            domain: "call".to_string(),
            confidence: Confidence::Extracted,
            call_context: Some("direct".to_string()),
            is_external: false,
            weight: 1.0,
            cfg_condition: None,
        }
    }
}
