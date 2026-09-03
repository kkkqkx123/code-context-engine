//! File dependency graph for tracking cross-file dependencies
//!
//! This module provides:
//! - FileDependencyGraph: Tracks forward and reverse dependencies between files
//! - Topological sorting for determining correct processing order
//! - Cycle detection for handling circular dependencies
//!
//! # Architecture Position
//!
//! This module operates at the **file level**:
//!
//! | Module | Level | Purpose |
//! |--------|-------|---------|
//! | `dependency_graph` (this) | File | Hot updates, incremental parsing |
//!
//! The file-level dependency graph is used during hot updates to:
//! 1. Determine which files need to be reprocessed when a file changes
//! 2. Establish the correct order for processing files (dependencies first)
//! 3. Detect and handle circular dependencies

pub mod entity;

pub use entity::{EntityDependencyGraph, EntityImpactAnalysis};

use dashmap::DashMap;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use thiserror::Error;

/// Error type for dependency graph operations
#[derive(Error, Debug, Clone, PartialEq)]
pub enum DependencyGraphError {
    #[error("Cycle detected in dependency graph: {0}")]
    CycleDetected(String),
    #[error("File not found: {0}")]
    FileNotFound(String),
    #[error("Invalid operation: {0}")]
    InvalidOperation(String),
}

/// Impact analysis for a file change.
#[derive(Debug, Clone)]
pub struct ImpactAnalysis {
    pub changed_file: String,
    pub direct_dependents: Vec<String>,
    pub transitive_dependents: Vec<String>,
    pub impact_score: f64,
}

/// File dependency graph
///
/// Maintains bidirectional dependency tracking between files:
/// - forward_edges: file -> files it depends on (outgoing)
/// - reverse_edges: file -> files that depend on it (incoming)
///
/// This enables efficient dependency propagation during hot updates.
#[derive(Debug)]
pub struct FileDependencyGraph {
    /// File path -> set of files it depends on (outgoing dependencies)
    forward_edges: DashMap<String, HashSet<String>>,

    /// File path -> set of files that depend on it (incoming dependencies)
    reverse_edges: DashMap<String, HashSet<String>>,

    /// Version counter for tracking changes (useful for caching)
    version_counter: AtomicU64,
}

impl Clone for FileDependencyGraph {
    fn clone(&self) -> Self {
        Self {
            forward_edges: self.forward_edges.clone(),
            reverse_edges: self.reverse_edges.clone(),
            version_counter: AtomicU64::new(self.version_counter.load(Ordering::Relaxed)),
        }
    }
}

impl FileDependencyGraph {
    /// Create a new empty dependency graph
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a dependency edge: `from` file depends on `to` file
    ///
    /// This updates both forward and reverse edges for efficient lookups.
    /// Duplicate edges are ignored so repeated registration (files can pass
    /// through `add_dependency` across spool replay passes) does not accumulate
    /// duplicates that later leak into delta dependency diffs.
    pub fn add_dependency(&self, from: &str, to: &str) {
        if from == to {
            tracing::warn!("Ignoring self-dependency for file: {}", from);
            return;
        }

        // Insert forward edge; HashSet provides O(1) dedup without a separate
        // contains check, avoiding the previous Vec::contains linear scan and
        // the non-atomic check-then-insert window.
        let inserted_forward = self
            .forward_edges
            .entry(from.to_string())
            .or_default()
            .insert(to.to_string());

        if !inserted_forward {
            return;
        }

        // Only update reverse side when forward was new
        self.reverse_edges
            .entry(to.to_string())
            .or_default()
            .insert(from.to_string());

        // Increment version
        self.version_counter.fetch_add(1, Ordering::Relaxed);
    }

    /// Add multiple dependencies at once
    pub fn add_dependencies(&self, from: &str, to_list: &[String]) {
        for to in to_list {
            self.add_dependency(from, to);
        }
    }

    /// Get all files that `file` depends on (outgoing dependencies)
    pub fn get_dependencies(&self, file: &str) -> Vec<String> {
        self.forward_edges
            .get(file)
            .map(|v| v.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Get all files that depend on `file` (incoming dependencies / dependents)
    pub fn get_dependents(&self, file: &str) -> Vec<String> {
        self.reverse_edges
            .get(file)
            .map(|v| v.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Check if `from` depends on `to` (direct dependency only)
    pub fn has_dependency(&self, from: &str, to: &str) -> bool {
        self.forward_edges
            .get(from)
            .is_some_and(|deps| deps.contains(to))
    }

    /// Check if `file` has any dependencies
    pub fn has_dependencies(&self, file: &str) -> bool {
        self.forward_edges
            .get(file)
            .is_some_and(|deps| !deps.is_empty())
    }

    /// Check if `file` has any dependents
    pub fn has_dependents(&self, file: &str) -> bool {
        self.reverse_edges
            .get(file)
            .is_some_and(|deps| !deps.is_empty())
    }

    /// Remove a single dependency edge.
    pub fn remove_dependency(&self, from: &str, to: &str) {
        let mut removed = false;
        if let Some(mut deps) = self.forward_edges.get_mut(from) {
            removed = deps.remove(to);
            if deps.is_empty() {
                drop(deps);
                self.forward_edges.remove(from);
            }
        }
        if let Some(mut deps) = self.reverse_edges.get_mut(to) {
            let r = deps.remove(from);
            removed = removed || r;
            if deps.is_empty() {
                drop(deps);
                self.reverse_edges.remove(to);
            }
        }
        if removed {
            self.version_counter.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Remove a file and all its associated edges from the graph
    ///
    /// This removes:
    /// - All forward edges from this file
    /// - All reverse edges to this file
    /// - This file from other files' dependency lists
    pub fn remove_file(&self, file: &str) {
        let mut changed = false;
        // Remove forward edges and clean up reverse edges
        if let Some((_, deps)) = self.forward_edges.remove(file) {
            changed = true;
            for dep in deps {
                if let Some(mut reverse_deps) = self.reverse_edges.get_mut(&dep) {
                    reverse_deps.remove(file);
                    if reverse_deps.is_empty() {
                        drop(reverse_deps);
                        self.reverse_edges.remove(dep.as_str());
                    }
                }
            }
        }

        // Remove reverse edges and clean up forward edges
        if let Some((_, dependents)) = self.reverse_edges.remove(file) {
            changed = true;
            for dependent in dependents {
                if let Some(mut deps) = self.forward_edges.get_mut(&dependent) {
                    deps.remove(file);
                    if deps.is_empty() {
                        drop(deps);
                        self.forward_edges.remove(dependent.as_str());
                    }
                }
            }
        }

        if changed {
            self.version_counter.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Get all files in the graph
    pub fn get_all_files(&self) -> Vec<String> {
        let mut files: HashSet<String> = HashSet::new();
        for entry in self.forward_edges.iter() {
            files.insert(entry.key().clone());
            for dep in entry.value().iter() {
                files.insert(dep.clone());
            }
        }
        for entry in self.reverse_edges.iter() {
            files.insert(entry.key().clone());
            for dep in entry.value().iter() {
                files.insert(dep.clone());
            }
        }
        files.into_iter().collect()
    }

    /// Get the current version number
    pub fn version(&self) -> u64 {
        self.version_counter.load(Ordering::Relaxed)
    }

    /// Clear all edges from the graph
    pub fn clear(&self) {
        self.forward_edges.clear();
        self.reverse_edges.clear();
        self.version_counter.store(0, Ordering::Relaxed);
    }

    /// Get dependency count (total number of edges)
    pub fn edge_count(&self) -> usize {
        self.forward_edges.iter().map(|e| e.value().len()).sum()
    }

    /// Get file count (files with at least one edge)
    pub fn file_count(&self) -> usize {
        let mut files: HashSet<String> = HashSet::new();
        for entry in self.forward_edges.iter() {
            files.insert(entry.key().clone());
            for dep in entry.value().iter() {
                files.insert(dep.clone());
            }
        }
        files.len()
    }

    /// Analyze the impact of a file change.
    pub fn analyze_impact(&self, changed_file: &str) -> ImpactAnalysis {
        const MAX_IMPACT_DEPTH: usize = 10;
        let direct_dependents = self.get_dependents(changed_file);
        let transitive_dependents =
            self.collect_transitive_dependents(changed_file, MAX_IMPACT_DEPTH);
        let impact_score = self.calculate_impact_score(&transitive_dependents);
        ImpactAnalysis {
            changed_file: changed_file.to_string(),
            direct_dependents,
            transitive_dependents,
            impact_score,
        }
    }

    fn calculate_impact_score(&self, files: &[String]) -> f64 {
        if files.is_empty() {
            return 0.0;
        }
        let base = files.len() as f64;
        // Weight by depth: deeper dependents contribute less? For now simple linear.
        // Cap at 100.
        (base * 10.0).min(100.0)
    }

    /// Perform topological sort on a subset of files
    ///
    /// Uses Kahn's algorithm to produce a valid processing order.
    /// Files with no dependencies come first, followed by files whose
    /// dependencies have all been processed.
    ///
    /// # Arguments
    /// * `files` - The subset of files to sort
    ///
    /// # Returns
    /// * `Ok(Vec<String>)` - Sorted file paths in processing order
    /// * `Err(DependencyGraphError)` - If a cycle is detected
    pub fn topological_sort(&self, files: &[String]) -> Result<Vec<String>, DependencyGraphError> {
        if files.is_empty() {
            return Ok(Vec::new());
        }

        let file_set: HashSet<String> = files.iter().cloned().collect();

        // Build adjacency list and in-degree map for the subgraph
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();

        // Initialize all files with in-degree 0
        for file in files {
            in_degree.insert(file.clone(), 0);
            adjacency.insert(file.clone(), Vec::new());
        }

        // Build the subgraph with filtered dependencies
        for file in files {
            let deps = self.get_dependencies(file);
            for dep in deps {
                if file_set.contains(&dep) {
                    // Add edge: dep -> file (file depends on dep, so dep comes first)
                    adjacency.entry(dep.clone()).or_default().push(file.clone());
                    *in_degree.entry(file.clone()).or_insert(0) += 1;
                }
            }
        }

        // Kahn's algorithm
        let mut queue: VecDeque<String> = in_degree
            .iter()
            .filter(|&(_, deg)| *deg == 0)
            .map(|(k, _)| k.clone())
            .collect();

        let mut result = Vec::new();

        while let Some(node) = queue.pop_front() {
            result.push(node.clone());

            if let Some(neighbors) = adjacency.get(&node) {
                for neighbor in neighbors {
                    if let Some(deg) = in_degree.get_mut(neighbor) {
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push_back(neighbor.clone());
                        }
                    }
                }
            }
        }

        // Check for cycles
        if result.len() != files.len() {
            // Find files involved in cycles
            let remaining: Vec<String> = files
                .iter()
                .filter(|f| !result.contains(f))
                .cloned()
                .collect();

            return Err(DependencyGraphError::CycleDetected(format!(
                "Files involved in cycle: {}",
                remaining.join(", ")
            )));
        }

        Ok(result)
    }

    /// Perform topological sort on all files in the graph
    pub fn topological_sort_all(&self) -> Result<Vec<String>, DependencyGraphError> {
        let all_files = self.get_all_files();
        self.topological_sort(&all_files)
    }

    /// Collect all transitive dependents of a file (BFS traversal)
    ///
    /// Returns all files that directly or indirectly depend on the given file.
    /// This is used for dependency propagation during hot updates.
    ///
    /// # Arguments
    /// * `file` - The starting file
    /// * `max_depth` - Maximum traversal depth (0 = unlimited)
    ///
    /// # Returns
    /// * `Vec<String>` - All transitive dependents
    pub fn collect_transitive_dependents(&self, file: &str, max_depth: usize) -> Vec<String> {
        let mut dependents = HashSet::new();
        let mut queue: VecDeque<(String, usize)> = VecDeque::new();
        let mut visited: HashSet<String> = HashSet::new();

        // Start with direct dependents
        for dep in self.get_dependents(file) {
            if !visited.contains(&dep) {
                visited.insert(dep.clone());
                queue.push_back((dep, 1));
            }
        }

        // BFS traversal
        while let Some((current, depth)) = queue.pop_front() {
            if max_depth > 0 && depth > max_depth {
                continue;
            }

            dependents.insert(current.clone());

            // Add next level dependents
            for dep in self.get_dependents(&current) {
                if !visited.contains(&dep) {
                    visited.insert(dep.clone());
                    queue.push_back((dep, depth + 1));
                }
            }
        }

        dependents.into_iter().collect()
    }

    /// Collect all transitive dependencies of a file (BFS traversal)
    ///
    /// Returns all files that the given file directly or indirectly depends on.
    pub fn collect_transitive_dependencies(&self, file: &str, max_depth: usize) -> Vec<String> {
        let mut dependencies = HashSet::new();
        let mut queue: VecDeque<(String, usize)> = VecDeque::new();
        let mut visited: HashSet<String> = HashSet::new();

        // Start with direct dependencies
        for dep in self.get_dependencies(file) {
            if !visited.contains(&dep) {
                visited.insert(dep.clone());
                queue.push_back((dep, 1));
            }
        }

        // BFS traversal
        while let Some((current, depth)) = queue.pop_front() {
            if max_depth > 0 && depth > max_depth {
                continue;
            }

            dependencies.insert(current.clone());

            // Add next level dependencies
            for dep in self.get_dependencies(&current) {
                if !visited.contains(&dep) {
                    visited.insert(dep.clone());
                    queue.push_back((dep, depth + 1));
                }
            }
        }

        dependencies.into_iter().collect()
    }

    /// Detect if there's a cycle in the dependency graph
    ///
    /// Uses DFS-based cycle detection.
    pub fn has_cycle(&self) -> bool {
        let files = self.get_all_files();
        let mut visited: HashSet<String> = HashSet::new();
        let mut recursion_stack: HashSet<String> = HashSet::new();

        for file in files {
            if self.has_cycle_dfs(&file, &mut visited, &mut recursion_stack) {
                return true;
            }
        }

        false
    }

    fn has_cycle_dfs(
        &self,
        file: &str,
        visited: &mut HashSet<String>,
        recursion_stack: &mut HashSet<String>,
    ) -> bool {
        visited.insert(file.to_string());
        recursion_stack.insert(file.to_string());

        for dep in self.get_dependencies(file) {
            if !visited.contains(&dep) {
                if self.has_cycle_dfs(&dep, visited, recursion_stack) {
                    return true;
                }
            } else if recursion_stack.contains(&dep) {
                return true;
            }
        }

        recursion_stack.remove(file);
        false
    }

    /// Find all cycles in the dependency graph
    ///
    /// Returns a list of cycles, where each cycle is a list of file paths.
    pub fn find_all_cycles(&self) -> Vec<Vec<String>> {
        let mut cycles = Vec::new();
        let files = self.get_all_files();
        let mut visited: HashSet<String> = HashSet::new();

        for file in files {
            if !visited.contains(&file) {
                let mut path = Vec::new();
                self.find_cycles_dfs(&file, &mut visited, &mut path, &mut cycles);
            }
        }

        cycles
    }

    fn find_cycles_dfs(
        &self,
        file: &str,
        visited: &mut HashSet<String>,
        path: &mut Vec<String>,
        cycles: &mut Vec<Vec<String>>,
    ) {
        if path.contains(&file.to_string()) {
            // Found a cycle. `file` is guaranteed to be present in `path`
            // (checked above), so the lookup cannot fail; skip defensively
            // instead of unwrapping in case the invariant is ever violated.
            if let Some(cycle_start) = path.iter().position(|f| f == file) {
                let cycle: Vec<String> = path[cycle_start..].to_vec();
                cycles.push(cycle);
            }
            return;
        }

        if visited.contains(file) {
            return;
        }

        path.push(file.to_string());

        for dep in self.get_dependencies(file) {
            self.find_cycles_dfs(&dep, visited, path, cycles);
        }

        path.pop();
        visited.insert(file.to_string());
    }

    /// Perform topological sort with fallback for cycles
    ///
    /// This method attempts to perform a topological sort. If a cycle is detected,
    /// it logs a warning and falls back to a heuristic ordering (alphabetical by path)
    /// to ensure processing can continue.
    ///
    /// # Arguments
    /// * `files` - The subset of files to sort
    ///
    /// # Returns
    /// * `Vec<String>` - Sorted file paths in processing order
    pub fn topological_sort_with_fallback(&self, files: &[String]) -> Vec<String> {
        match self.topological_sort(files) {
            Ok(sorted) => sorted,
            Err(DependencyGraphError::CycleDetected(msg)) => {
                tracing::warn!(
                    "Cycle detected in dependency graph, using fallback ordering: {}",
                    msg
                );

                // Fallback strategy: sort alphabetically by path
                // This ensures deterministic ordering even with cycles
                let mut sorted = files.to_vec();
                sorted.sort();

                tracing::info!("Fallback ordering applied to {} files", sorted.len());

                sorted
            }
            Err(e) => {
                tracing::error!("Unexpected error in topological sort: {}", e);
                // Return original order as last resort
                files.to_vec()
            }
        }
    }
}

impl Default for FileDependencyGraph {
    fn default() -> Self {
        Self {
            forward_edges: DashMap::new(),
            reverse_edges: DashMap::new(),
            version_counter: AtomicU64::new(0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_dependency() {
        let graph = FileDependencyGraph::new();

        graph.add_dependency("a.rs", "b.rs");

        assert!(graph.has_dependency("a.rs", "b.rs"));
        assert!(!graph.has_dependency("b.rs", "a.rs"));

        let deps = graph.get_dependencies("a.rs");
        assert_eq!(deps, vec!["b.rs"]);

        let dependents = graph.get_dependents("b.rs");
        assert_eq!(dependents, vec!["a.rs"]);
    }

    #[test]
    fn test_remove_file() {
        let graph = FileDependencyGraph::new();

        graph.add_dependency("a.rs", "b.rs");
        graph.add_dependency("c.rs", "b.rs");

        graph.remove_file("b.rs");

        assert!(!graph.has_dependencies("a.rs"));
        assert!(!graph.has_dependencies("c.rs"));
        assert!(!graph.has_dependents("a.rs"));
    }

    #[test]
    fn test_topological_sort() {
        let graph = FileDependencyGraph::new();

        // Create a simple dependency chain: c -> b -> a
        graph.add_dependency("b.rs", "a.rs");
        graph.add_dependency("c.rs", "b.rs");

        let files = vec!["a.rs".to_string(), "b.rs".to_string(), "c.rs".to_string()];
        let sorted = graph.topological_sort(&files).unwrap();

        // a should come before b, and b before c
        let a_pos = sorted.iter().position(|f| f == "a.rs").unwrap();
        let b_pos = sorted.iter().position(|f| f == "b.rs").unwrap();
        let c_pos = sorted.iter().position(|f| f == "c.rs").unwrap();

        assert!(a_pos < b_pos);
        assert!(b_pos < c_pos);
    }

    #[test]
    fn test_topological_sort_cycle() {
        let graph = FileDependencyGraph::new();

        // Create a cycle: a -> b -> c -> a
        graph.add_dependency("a.rs", "b.rs");
        graph.add_dependency("b.rs", "c.rs");
        graph.add_dependency("c.rs", "a.rs");

        let files = vec!["a.rs".to_string(), "b.rs".to_string(), "c.rs".to_string()];
        let result = graph.topological_sort(&files);

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DependencyGraphError::CycleDetected(_)
        ));
    }

    #[test]
    fn test_transitive_dependents() {
        let graph = FileDependencyGraph::new();

        // Create chain: d -> c -> b -> a
        graph.add_dependency("b.rs", "a.rs");
        graph.add_dependency("c.rs", "b.rs");
        graph.add_dependency("d.rs", "c.rs");

        let dependents = graph.collect_transitive_dependents("a.rs", 0);

        assert_eq!(dependents.len(), 3);
        assert!(dependents.contains(&"b.rs".to_string()));
        assert!(dependents.contains(&"c.rs".to_string()));
        assert!(dependents.contains(&"d.rs".to_string()));
    }

    #[test]
    fn test_transitive_dependents_with_max_depth() {
        let graph = FileDependencyGraph::new();

        // Create chain: d -> c -> b -> a
        graph.add_dependency("b.rs", "a.rs");
        graph.add_dependency("c.rs", "b.rs");
        graph.add_dependency("d.rs", "c.rs");

        let dependents = graph.collect_transitive_dependents("a.rs", 2);

        assert_eq!(dependents.len(), 2);
        assert!(dependents.contains(&"b.rs".to_string()));
        assert!(dependents.contains(&"c.rs".to_string()));
        assert!(!dependents.contains(&"d.rs".to_string()));
    }

    #[test]
    fn test_has_cycle() {
        let graph = FileDependencyGraph::new();

        assert!(!graph.has_cycle());

        // Create a cycle
        graph.add_dependency("a.rs", "b.rs");
        graph.add_dependency("b.rs", "c.rs");
        graph.add_dependency("c.rs", "a.rs");

        assert!(graph.has_cycle());
    }

    #[test]
    fn test_self_dependency_ignored() {
        let graph = FileDependencyGraph::new();

        graph.add_dependency("a.rs", "a.rs");

        assert!(!graph.has_dependency("a.rs", "a.rs"));
    }

    #[test]
    fn test_version_increment() {
        let graph = FileDependencyGraph::new();

        let v1 = graph.version();
        graph.add_dependency("a.rs", "b.rs");
        let v2 = graph.version();

        assert!(v2 > v1);
    }
}
