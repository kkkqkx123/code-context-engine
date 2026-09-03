//! Support helpers extracted from `RelationUpdateProcessor`.
//!
//! These pure functions are intentionally free of async/IO so they can be
//! unit-tested independently. The processor remains the orchestration facade;
//! this module holds the algorithmic core for fingerprint scoping and
//! candidate-dependents collection.

use std::collections::{HashMap, HashSet, VecDeque};

use cce_relation::index::RelationIndexView;
use cce_types::normalize_project_path;

/// Whether the symbol-fingerprint scope is too large relative to the project.
pub fn scope_exceeds_ratio(scope_len: usize, project_files: usize, ratio: f64) -> bool {
    ratio > 0.0 && project_files > 0 && scope_len as f64 > project_files as f64 * ratio
}

/// Files whose symbols the candidate resolution could consult, bounded
/// by the dependency graph: the replaced set plus its transitive
/// dependents and dependencies in both the old and the new graph.
pub fn symbol_fingerprint_scope<O: RelationIndexView, N: RelationIndexView>(
    old_index: &O,
    new_index: &N,
    replaced_files: &HashSet<String>,
    max_depth: usize,
) -> HashSet<String> {
    let unlimited = max_depth == 0;
    let mut scope = HashSet::new();
    for file in replaced_files {
        // Union BFS for dependents
        {
            let mut visited: HashSet<String> = HashSet::new();
            let mut queue: VecDeque<(String, usize)> = VecDeque::new();
            visited.insert(file.clone());
            queue.push_back((file.clone(), 0));
            while let Some((cur, depth)) = queue.pop_front() {
                if !unlimited && depth >= max_depth {
                    continue;
                }
                let mut neighbors: HashSet<String> = HashSet::new();
                for nb in old_index.dependents_of(&cur) {
                    neighbors.insert(normalize_project_path(&nb));
                }
                for nb in new_index.dependents_of(&cur) {
                    neighbors.insert(normalize_project_path(&nb));
                }
                for nb in neighbors {
                    if visited.insert(nb.clone()) {
                        if !replaced_files.contains(&nb) {
                            scope.insert(nb.clone());
                        }
                        queue.push_back((nb, depth + 1));
                    }
                }
            }
        }
        // Union BFS for dependencies
        {
            let mut visited: HashSet<String> = HashSet::new();
            let mut queue: VecDeque<(String, usize)> = VecDeque::new();
            visited.insert(file.clone());
            queue.push_back((file.clone(), 0));
            while let Some((cur, depth)) = queue.pop_front() {
                if !unlimited && depth >= max_depth {
                    continue;
                }
                let mut neighbors: HashSet<String> = HashSet::new();
                for nb in old_index.dependencies_of(&cur) {
                    neighbors.insert(normalize_project_path(&nb));
                }
                for nb in new_index.dependencies_of(&cur) {
                    neighbors.insert(normalize_project_path(&nb));
                }
                for nb in neighbors {
                    if visited.insert(nb.clone()) {
                        if !replaced_files.contains(&nb) {
                            scope.insert(nb.clone());
                        }
                        queue.push_back((nb, depth + 1));
                    }
                }
            }
        }
    }
    scope
}

/// Collect the files that must be rebuilt alongside the changed set:
/// transitive dependents over import edges (cost 1) and caller-derived edges (cost 2).
pub fn collect_candidate_dependents<V: RelationIndexView>(
    index: &V,
    changed_files: &HashSet<String>,
    max_depth: usize,
) -> HashSet<String> {
    let unlimited = max_depth == 0;
    let mut best: HashMap<String, usize> = HashMap::new();
    let mut dependents: HashSet<String> = HashSet::new();
    let mut queue: Vec<(String, usize)> = Vec::new();
    for path in changed_files {
        best.insert(path.clone(), 0);
        queue.push((path.clone(), 0));
    }

    while let Some((path, cost)) = queue.pop() {
        if !unlimited && cost > max_depth {
            continue;
        }
        // Strong edges: import dependents.
        let import_cost = cost + 1;
        for dependent in index.dependents_of(&path) {
            let dependent = normalize_project_path(&dependent);
            if import_cost < best.get(&dependent).copied().unwrap_or(usize::MAX) {
                best.insert(dependent.clone(), import_cost);
                dependents.insert(dependent.clone());
                if unlimited || import_cost <= max_depth {
                    queue.push((dependent, import_cost));
                }
            }
        }
        // Weak edges: files whose entities call an entity of `path`.
        let call_cost = cost + 2;
        if unlimited || call_cost <= max_depth {
            for entity in index.entities_of_file(&path) {
                for caller in index.callers_of(entity.id) {
                    let Some(caller_file) = index.entity_file_of(caller) else {
                        continue;
                    };
                    let caller_file = normalize_project_path(&caller_file);
                    if call_cost < best.get(&caller_file).copied().unwrap_or(usize::MAX) {
                        best.insert(caller_file.clone(), call_cost);
                        dependents.insert(caller_file.clone());
                        queue.push((caller_file, call_cost));
                    }
                }
                for caller_file in index.file_callers_of(entity.id) {
                    let caller_file = normalize_project_path(&caller_file);
                    if call_cost < best.get(&caller_file).copied().unwrap_or(usize::MAX) {
                        best.insert(caller_file.clone(), call_cost);
                        dependents.insert(caller_file.clone());
                        queue.push((caller_file, call_cost));
                    }
                }
            }
        }
    }
    dependents
}
