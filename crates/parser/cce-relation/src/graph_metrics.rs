//! Graph centrality and clustering metrics over the relation index.
//!
//! Provides degree centrality, betweenness centrality (Brandes algorithm),
//! local clustering coefficient, and PageRank over the resolved relation
//! edges. All metrics operate on a snapshot index and return deterministic
//! results ordered by entity id.

use std::collections::{HashMap, HashSet, VecDeque};

use cce_types::EntityId;

use crate::index::snapshot_index::LayeredSnapshotIndex;
use crate::index::view::RelationIndexView;

/// Maximum graph size for betweenness centrality (O(VE) complexity).
const MAX_BETWEENNESS_NODES: usize = 5000;

/// PageRank damping factor.
const PAGERANK_DAMPING: f64 = 0.85;

/// PageRank convergence threshold.
const PAGERANK_EPSILON: f64 = 1e-6;

/// Maximum PageRank iterations.
const PAGERANK_MAX_ITER: usize = 100;

/// Graph metrics for a single entity.
#[derive(Debug, Clone, Default)]
pub struct EntityMetrics {
    /// Normalized degree centrality (0.0 to 1.0).
    pub degree_centrality: f64,
    /// Normalized betweenness centrality (0.0 to 1.0), when computed.
    pub betweenness_centrality: Option<f64>,
    /// Local clustering coefficient (0.0 to 1.0).
    pub clustering_coefficient: f64,
    /// PageRank score.
    pub pagerank: f64,
}

/// Compute graph metrics for all entities in the index.
///
/// Betweenness centrality is only computed when the graph has at most
/// `MAX_BETWEENNESS_NODES` nodes; otherwise it is skipped (returns `None`).
pub fn compute_metrics(index: &LayeredSnapshotIndex) -> HashMap<EntityId, EntityMetrics> {
    let nodes: Vec<EntityId> = {
        let mut ids: Vec<EntityId> = Vec::new();
        index.for_each_function(|id, _| ids.push(id));
        ids.sort();
        ids
    };

    let n = nodes.len();
    if n == 0 {
        return HashMap::new();
    }

    let adjacency = build_adjacency(index, &nodes);
    let degree = compute_degree_centrality(&nodes, &adjacency);
    let betweenness = if n <= MAX_BETWEENNESS_NODES {
        Some(compute_betweenness_centrality(&nodes, &adjacency))
    } else {
        None
    };
    let clustering = compute_clustering_coefficient(&nodes, &adjacency);
    let pagerank = compute_pagerank(&nodes, &adjacency);

    let mut result = HashMap::new();
    for node in &nodes {
        result.insert(
            *node,
            EntityMetrics {
                degree_centrality: degree[node],
                betweenness_centrality: betweenness.as_ref().map(|b| b[node]),
                clustering_coefficient: clustering[node],
                pagerank: pagerank[node],
            },
        );
    }
    result
}

fn build_adjacency(
    index: &LayeredSnapshotIndex,
    nodes: &[EntityId],
) -> HashMap<EntityId, Vec<EntityId>> {
    let mut adj: HashMap<EntityId, Vec<EntityId>> = HashMap::new();
    for node in nodes {
        adj.insert(*node, Vec::new());
    }
    index.for_each_resolved_relation(|caller, relations| {
        let callee_ids: Vec<EntityId> = relations
            .iter()
            .filter_map(|r| r.callee_id)
            .filter(|id| adj.contains_key(id))
            .collect();
        if !callee_ids.is_empty() {
            if let Some(neighbors) = adj.get_mut(&caller) {
                neighbors.extend(callee_ids);
            }
        }
    });
    adj
}

fn compute_degree_centrality(
    nodes: &[EntityId],
    adjacency: &HashMap<EntityId, Vec<EntityId>>,
) -> HashMap<EntityId, f64> {
    let n = nodes.len();
    let mut result = HashMap::new();
    if n <= 1 {
        for node in nodes {
            result.insert(*node, 0.0);
        }
        return result;
    }
    let max_degree = (n - 1) as f64;
    for node in nodes {
        let degree = adjacency.get(node).map(|v| v.len()).unwrap_or(0) as f64;
        result.insert(*node, degree / max_degree);
    }
    result
}

fn compute_betweenness_centrality(
    nodes: &[EntityId],
    adjacency: &HashMap<EntityId, Vec<EntityId>>,
) -> HashMap<EntityId, f64> {
    let n = nodes.len();
    let mut cb: HashMap<EntityId, f64> = HashMap::new();
    for node in nodes {
        cb.insert(*node, 0.0);
    }

    for s in nodes {
        let mut stack: Vec<EntityId> = Vec::new();
        let mut pred: HashMap<EntityId, Vec<EntityId>> = HashMap::new();
        let mut sigma: HashMap<EntityId, f64> = HashMap::new();
        let mut dist: HashMap<EntityId, i64> = HashMap::new();

        for node in nodes {
            pred.insert(*node, Vec::new());
            sigma.insert(*node, 0.0);
            dist.insert(*node, -1);
        }
        sigma.insert(*s, 1.0);
        dist.insert(*s, 0);

        let mut queue: VecDeque<EntityId> = VecDeque::from([*s]);
        while let Some(v) = queue.pop_front() {
            stack.push(v);
            let d_v = dist[&v];
            if let Some(neighbors) = adjacency.get(&v) {
                for w in neighbors {
                    if dist[w] < 0 {
                        dist.insert(*w, d_v + 1);
                        queue.push_back(*w);
                    }
                    if dist[w] == d_v + 1 {
                        let sigma_v = sigma[&v];
                        let entry = sigma.entry(*w).or_insert(0.0);
                        *entry += sigma_v;
                        pred.entry(*w).or_default().push(v);
                    }
                }
            }
        }

        let mut delta: HashMap<EntityId, f64> = HashMap::new();
        for node in nodes {
            delta.insert(*node, 0.0);
        }
        while let Some(w) = stack.pop() {
            for v in &pred[&w] {
                let coeff = (sigma[v] / sigma[&w]) * (1.0 + delta[&w]);
                *delta.entry(*v).or_insert(0.0) += coeff;
            }
            if w != *s {
                *cb.entry(w).or_insert(0.0) += delta[&w];
            }
        }
    }

    if n > 2 {
        let normalizer = ((n - 1) * (n - 2)) as f64;
        for value in cb.values_mut() {
            *value /= normalizer;
        }
    } else {
        for value in cb.values_mut() {
            *value = 0.0;
        }
    }
    cb
}

fn compute_clustering_coefficient(
    nodes: &[EntityId],
    adjacency: &HashMap<EntityId, Vec<EntityId>>,
) -> HashMap<EntityId, f64> {
    let mut result = HashMap::new();
    for node in nodes {
        let neighbors = match adjacency.get(node) {
            Some(n) if n.len() >= 2 => n,
            _ => {
                result.insert(*node, 0.0);
                continue;
            }
        };
        let neighbor_set: HashSet<EntityId> = neighbors.iter().copied().collect();
        let k = neighbors.len();
        let mut edges = 0usize;
        for neighbor in neighbors {
            if let Some(neighbor_neighbors) = adjacency.get(neighbor) {
                for nn in neighbor_neighbors {
                    if neighbor_set.contains(nn) {
                        edges += 1;
                    }
                }
            }
        }
        let possible = (k * (k - 1)) as f64;
        result.insert(*node, edges as f64 / possible);
    }
    result
}

fn compute_pagerank(
    nodes: &[EntityId],
    adjacency: &HashMap<EntityId, Vec<EntityId>>,
) -> HashMap<EntityId, f64> {
    let n = nodes.len();
    let mut pr: HashMap<EntityId, f64> = HashMap::new();
    let initial = 1.0 / n as f64;
    for node in nodes {
        pr.insert(*node, initial);
    }

    let mut out_degree: HashMap<EntityId, usize> = HashMap::new();
    for node in nodes {
        out_degree.insert(*node, adjacency.get(node).map(|v| v.len()).unwrap_or(0));
    }

    for _ in 0..PAGERANK_MAX_ITER {
        let mut new_pr: HashMap<EntityId, f64> = HashMap::new();
        let mut dangling_sum = 0.0;
        for node in nodes {
            if out_degree[node] == 0 {
                dangling_sum += pr[node];
            }
        }
        let dangling_contrib = dangling_sum / n as f64;

        for node in nodes {
            let mut rank =
                (1.0 - PAGERANK_DAMPING) / n as f64 + PAGERANK_DAMPING * dangling_contrib;
            for other in nodes {
                if let Some(neighbors) = adjacency.get(other) {
                    if neighbors.contains(node) {
                        let out = out_degree[other] as f64;
                        if out > 0.0 {
                            rank += PAGERANK_DAMPING * (pr[other] / out);
                        }
                    }
                }
            }
            new_pr.insert(*node, rank);
        }

        let mut max_diff = 0.0;
        for node in nodes {
            let diff = (new_pr[node] - pr[node]).abs();
            if diff > max_diff {
                max_diff = diff;
            }
        }
        pr = new_pr;
        if max_diff < PAGERANK_EPSILON {
            break;
        }
    }
    pr
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::EntityIndexOps;
    use crate::index::core::RelationIndex;
    use cce_types::{Entity, EntityKind, RelationType, ResolvedRelation, Span};
    use std::collections::HashMap;
    use std::sync::Arc;

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

    fn relation(from: u64, to: u64) -> ResolvedRelation {
        ResolvedRelation {
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
        }
    }

    #[test]
    fn empty_graph_has_no_metrics() {
        let index = RelationIndex::new();
        let snapshot = LayeredSnapshotIndex::new(Arc::new(
            crate::index::snapshot_index::RelationSnapshotIndex::from_index_owned(
                &mut index.clone(),
            ),
        ));
        let metrics = compute_metrics(&snapshot);
        assert!(metrics.is_empty());
    }

    #[test]
    fn single_node_has_zero_metrics() {
        let mut index = RelationIndex::new();
        index.add_function_with_path(EntityId(1), test_entity(1, "a"), "a.rs".into());
        let snapshot = LayeredSnapshotIndex::new(Arc::new(
            crate::index::snapshot_index::RelationSnapshotIndex::from_index_owned(&mut index),
        ));
        let metrics = compute_metrics(&snapshot);
        assert_eq!(metrics.len(), 1);
        let m = &metrics[&EntityId(1)];
        assert_eq!(m.degree_centrality, 0.0);
        assert_eq!(m.clustering_coefficient, 0.0);
        assert!((m.pagerank - 1.0).abs() < 1e-6);
    }

    #[test]
    fn chain_graph_degree_centrality() {
        let mut index = RelationIndex::new();
        for id in 1u64..=3 {
            index.add_function_with_path(
                EntityId(id),
                test_entity(id, &format!("fn{id}")),
                "lib.rs".into(),
            );
        }
        index.add_resolved_relation(relation(1, 2));
        index.add_resolved_relation(relation(2, 3));
        let snapshot = LayeredSnapshotIndex::new(Arc::new(
            crate::index::snapshot_index::RelationSnapshotIndex::from_index_owned(&mut index),
        ));
        let metrics = compute_metrics(&snapshot);
        assert_eq!(metrics[&EntityId(1)].degree_centrality, 0.5);
        assert_eq!(metrics[&EntityId(2)].degree_centrality, 1.0);
        assert_eq!(metrics[&EntityId(3)].degree_centrality, 0.5);
    }

    #[test]
    fn betweenness_centrality_of_middle_node() {
        let mut index = RelationIndex::new();
        for id in 1u64..=3 {
            index.add_function_with_path(
                EntityId(id),
                test_entity(id, &format!("fn{id}")),
                "lib.rs".into(),
            );
        }
        index.add_resolved_relation(relation(1, 2));
        index.add_resolved_relation(relation(2, 3));
        let snapshot = LayeredSnapshotIndex::new(Arc::new(
            crate::index::snapshot_index::RelationSnapshotIndex::from_index_owned(&mut index),
        ));
        let metrics = compute_metrics(&snapshot);
        let b = metrics[&EntityId(2)]
            .betweenness_centrality
            .expect("betweenness");
        assert!(b > 0.0);
        let b1 = metrics[&EntityId(1)]
            .betweenness_centrality
            .expect("betweenness");
        assert_eq!(b1, 0.0);
    }

    #[test]
    fn pagerank_sums_to_one() {
        let mut index = RelationIndex::new();
        for id in 1u64..=4 {
            index.add_function_with_path(
                EntityId(id),
                test_entity(id, &format!("fn{id}")),
                "lib.rs".into(),
            );
        }
        index.add_resolved_relation(relation(1, 2));
        index.add_resolved_relation(relation(2, 3));
        index.add_resolved_relation(relation(3, 4));
        index.add_resolved_relation(relation(4, 1));
        let snapshot = LayeredSnapshotIndex::new(Arc::new(
            crate::index::snapshot_index::RelationSnapshotIndex::from_index_owned(&mut index),
        ));
        let metrics = compute_metrics(&snapshot);
        let sum: f64 = metrics.values().map(|m| m.pagerank).sum();
        assert!((sum - 1.0).abs() < 1e-6);
    }
}
