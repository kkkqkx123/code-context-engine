//! Entity-level dependency graph.
//!
//! Tracks dependencies between entities (functions, classes, methods) rather
//! than files. Enables precise impact analysis: when an entity changes, which
//! other entities are affected.

use std::collections::{HashMap, HashSet, VecDeque};

use cce_types::{EntityId, RelationType};

/// Impact analysis for an entity change.
#[derive(Debug, Clone)]
pub struct EntityImpactAnalysis {
    pub changed_entity: EntityId,
    pub direct_dependents: Vec<EntityId>,
    pub transitive_dependents: Vec<EntityId>,
    pub impact_score: f64,
}

/// Entity-level dependency graph.
#[derive(Debug, Clone, Default)]
pub struct EntityDependencyGraph {
    forward_edges: HashMap<EntityId, HashSet<EntityId>>,
    reverse_edges: HashMap<EntityId, HashSet<EntityId>>,
    edge_types: HashMap<(EntityId, EntityId), HashSet<RelationType>>,
}

impl EntityDependencyGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_dependency(&mut self, from: EntityId, to: EntityId, rel_type: RelationType) {
        if from == to {
            return;
        }
        self.forward_edges.entry(from).or_default().insert(to);
        self.reverse_edges.entry(to).or_default().insert(from);
        self.edge_types
            .entry((from, to))
            .or_default()
            .insert(rel_type);
    }

    pub fn get_dependencies(&self, entity_id: EntityId) -> Vec<EntityId> {
        self.forward_edges
            .get(&entity_id)
            .map(|v| v.iter().copied().collect())
            .unwrap_or_default()
    }

    pub fn get_dependents(&self, entity_id: EntityId) -> Vec<EntityId> {
        self.reverse_edges
            .get(&entity_id)
            .map(|v| v.iter().copied().collect())
            .unwrap_or_default()
    }

    pub fn get_dependency_types(&self, from: EntityId, to: EntityId) -> HashSet<RelationType> {
        self.edge_types
            .get(&(from, to))
            .cloned()
            .unwrap_or_default()
    }

    pub fn has_dependency(&self, from: EntityId, to: EntityId) -> bool {
        self.forward_edges
            .get(&from)
            .is_some_and(|s| s.contains(&to))
    }

    pub fn remove_dependency(&mut self, from: EntityId, to: EntityId) {
        if let Some(set) = self.forward_edges.get_mut(&from) {
            set.remove(&to);
            if set.is_empty() {
                self.forward_edges.remove(&from);
            }
        }
        if let Some(set) = self.reverse_edges.get_mut(&to) {
            set.remove(&from);
            if set.is_empty() {
                self.reverse_edges.remove(&to);
            }
        }
        self.edge_types.remove(&(from, to));
    }

    pub fn remove_entity(&mut self, entity_id: EntityId) {
        if let Some(deps) = self.forward_edges.remove(&entity_id) {
            for dep in deps {
                if let Some(rev) = self.reverse_edges.get_mut(&dep) {
                    rev.remove(&entity_id);
                    if rev.is_empty() {
                        self.reverse_edges.remove(&dep);
                    }
                }
                self.edge_types.remove(&(entity_id, dep));
            }
        }
        if let Some(dependents) = self.reverse_edges.remove(&entity_id) {
            for dep in dependents {
                if let Some(fwd) = self.forward_edges.get_mut(&dep) {
                    fwd.remove(&entity_id);
                    if fwd.is_empty() {
                        self.forward_edges.remove(&dep);
                    }
                }
                self.edge_types.remove(&(dep, entity_id));
            }
        }
    }

    pub fn analyze_impact(&self, entity_id: EntityId, max_depth: usize) -> EntityImpactAnalysis {
        let direct_dependents = self.get_dependents(entity_id);
        let transitive_dependents = self.collect_transitive_dependents(entity_id, max_depth);
        let impact_score = self.calculate_impact_score(&transitive_dependents);
        EntityImpactAnalysis {
            changed_entity: entity_id,
            direct_dependents,
            transitive_dependents,
            impact_score,
        }
    }

    fn collect_transitive_dependents(
        &self,
        entity_id: EntityId,
        max_depth: usize,
    ) -> Vec<EntityId> {
        let mut dependents = HashSet::new();
        let mut queue: VecDeque<(EntityId, usize)> = VecDeque::new();
        let mut visited: HashSet<EntityId> = HashSet::new();
        for dep in self.get_dependents(entity_id) {
            if visited.insert(dep) {
                queue.push_back((dep, 1));
            }
        }
        while let Some((current, depth)) = queue.pop_front() {
            if max_depth > 0 && depth > max_depth {
                continue;
            }
            dependents.insert(current);
            for dep in self.get_dependents(current) {
                if visited.insert(dep) {
                    queue.push_back((dep, depth + 1));
                }
            }
        }
        dependents.into_iter().collect()
    }

    fn calculate_impact_score(&self, entities: &[EntityId]) -> f64 {
        if entities.is_empty() {
            return 0.0;
        }
        let base = entities.len() as f64;
        (base * 10.0).min(100.0)
    }

    pub fn edge_count(&self) -> usize {
        self.forward_edges.values().map(|s| s.len()).sum()
    }

    pub fn entity_count(&self) -> usize {
        let mut ids = HashSet::new();
        for (k, v) in &self.forward_edges {
            ids.insert(*k);
            for id in v {
                ids.insert(*id);
            }
        }
        for (k, v) in &self.reverse_edges {
            ids.insert(*k);
            for id in v {
                ids.insert(*id);
            }
        }
        ids.len()
    }

    pub fn clear(&mut self) {
        self.forward_edges.clear();
        self.reverse_edges.clear();
        self.edge_types.clear();
    }

    pub fn has_cycle(&self) -> bool {
        let mut visited = HashSet::new();
        let mut stack = HashSet::new();
        for &node in self.forward_edges.keys() {
            if !visited.contains(&node) && self.has_cycle_dfs(node, &mut visited, &mut stack) {
                return true;
            }
        }
        false
    }

    fn has_cycle_dfs(
        &self,
        node: EntityId,
        visited: &mut HashSet<EntityId>,
        stack: &mut HashSet<EntityId>,
    ) -> bool {
        visited.insert(node);
        stack.insert(node);
        if let Some(neighbors) = self.forward_edges.get(&node) {
            for &n in neighbors {
                if !visited.contains(&n) {
                    if self.has_cycle_dfs(n, visited, stack) {
                        return true;
                    }
                } else if stack.contains(&n) {
                    return true;
                }
            }
        }
        stack.remove(&node);
        false
    }

    pub fn find_all_cycles(&self) -> Vec<Vec<EntityId>> {
        let mut cycles = Vec::new();
        let mut visited = HashSet::new();
        for &node in self.forward_edges.keys() {
            if !visited.contains(&node) {
                let mut path = Vec::new();
                self.find_cycles_dfs(node, &mut visited, &mut path, &mut cycles);
            }
        }
        cycles
    }

    fn find_cycles_dfs(
        &self,
        node: EntityId,
        visited: &mut HashSet<EntityId>,
        path: &mut Vec<EntityId>,
        cycles: &mut Vec<Vec<EntityId>>,
    ) {
        if let Some(pos) = path.iter().position(|&x| x == node) {
            cycles.push(path[pos..].to_vec());
            return;
        }
        if visited.contains(&node) {
            return;
        }
        path.push(node);
        if let Some(neighbors) = self.forward_edges.get(&node) {
            for &n in neighbors.clone().iter() {
                self.find_cycles_dfs(n, visited, path, cycles);
            }
        }
        path.pop();
        visited.insert(node);
    }

    pub fn collect_transitive_dependencies(
        &self,
        entity_id: EntityId,
        max_depth: usize,
    ) -> Vec<EntityId> {
        let mut deps = HashSet::new();
        let mut queue: VecDeque<(EntityId, usize)> = VecDeque::new();
        let mut visited = HashSet::new();
        for dep in self.get_dependencies(entity_id) {
            if visited.insert(dep) {
                queue.push_back((dep, 1));
            }
        }
        while let Some((cur, depth)) = queue.pop_front() {
            if max_depth > 0 && depth > max_depth {
                continue;
            }
            deps.insert(cur);
            for d in self.get_dependencies(cur) {
                if visited.insert(d) {
                    queue.push_back((d, depth + 1));
                }
            }
        }
        deps.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::RelationType;

    #[test]
    fn test_add_and_query() {
        let mut g = EntityDependencyGraph::new();
        g.add_dependency(EntityId(1), EntityId(2), RelationType::DirectCall);
        assert!(g.has_dependency(EntityId(1), EntityId(2)));
        assert_eq!(g.get_dependencies(EntityId(1)), vec![EntityId(2)]);
        assert_eq!(g.get_dependents(EntityId(2)), vec![EntityId(1)]);
    }

    #[test]
    fn test_impact_analysis() {
        let mut g = EntityDependencyGraph::new();
        g.add_dependency(EntityId(2), EntityId(1), RelationType::DirectCall);
        g.add_dependency(EntityId(3), EntityId(2), RelationType::DirectCall);
        let impact = g.analyze_impact(EntityId(1), 10);
        assert_eq!(impact.direct_dependents.len(), 1);
        assert_eq!(impact.transitive_dependents.len(), 2);
        assert!(impact.impact_score > 0.0);
    }

    #[test]
    fn test_cycle_detection() {
        let mut g = EntityDependencyGraph::new();
        g.add_dependency(EntityId(1), EntityId(2), RelationType::DirectCall);
        g.add_dependency(EntityId(2), EntityId(3), RelationType::DirectCall);
        g.add_dependency(EntityId(3), EntityId(1), RelationType::DirectCall);
        assert!(g.has_cycle());
        assert!(!g.find_all_cycles().is_empty());
    }

    #[test]
    fn test_remove_entity() {
        let mut g = EntityDependencyGraph::new();
        g.add_dependency(EntityId(1), EntityId(2), RelationType::DirectCall);
        g.remove_entity(EntityId(1));
        assert!(!g.has_dependency(EntityId(1), EntityId(2)));
        assert!(g.get_dependents(EntityId(2)).is_empty());
    }
}
