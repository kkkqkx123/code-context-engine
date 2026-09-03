//! Path tracking for symlink cycle detection
//!
//! This module provides functionality to track visited paths during
//! directory traversal to detect and prevent symlink cycles.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Tracks visited paths for symlink cycle detection
///
/// Used during directory traversal to detect and prevent infinite loops
/// caused by symbolic links that create circular directory structures.
#[derive(Debug, Default)]
pub struct PathTracker {
    visited_paths: HashSet<PathBuf>,
}

impl PathTracker {
    /// Create a new empty path tracker
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if a path has been visited
    pub fn is_visited(&self, path: &Path) -> bool {
        self.visited_paths.contains(path)
    }

    /// Mark a path as visited
    pub fn mark_visited(&mut self, path: PathBuf) {
        self.visited_paths.insert(path);
    }

    /// Clear all visited paths
    pub fn clear(&mut self) {
        self.visited_paths.clear();
    }

    /// Get the number of visited paths
    pub fn len(&self) -> usize {
        self.visited_paths.len()
    }

    /// Check if no paths have been visited
    pub fn is_empty(&self) -> bool {
        self.visited_paths.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_tracker_empty() {
        let tracker = PathTracker::new();
        let path = Path::new("/test/path");
        assert!(!tracker.is_visited(path));
        assert!(tracker.is_empty());
        assert_eq!(tracker.len(), 0);
    }

    #[test]
    fn test_path_tracker_mark_and_check() {
        let mut tracker = PathTracker::new();
        let path = PathBuf::from("/test/path");

        assert!(!tracker.is_visited(&path));
        tracker.mark_visited(path.clone());
        assert!(tracker.is_visited(&path));
        assert_eq!(tracker.len(), 1);
    }

    #[test]
    fn test_path_tracker_different_paths() {
        let mut tracker = PathTracker::new();
        let path1 = PathBuf::from("/test/path1");
        let path2 = PathBuf::from("/test/path2");

        tracker.mark_visited(path1.clone());
        assert!(tracker.is_visited(&path1));
        assert!(!tracker.is_visited(&path2));
        assert_eq!(tracker.len(), 1);

        tracker.mark_visited(path2.clone());
        assert!(tracker.is_visited(&path2));
        assert_eq!(tracker.len(), 2);
    }

    #[test]
    fn test_path_tracker_clear() {
        let mut tracker = PathTracker::new();
        let path = PathBuf::from("/test/path");

        tracker.mark_visited(path.clone());
        assert!(tracker.is_visited(&path));
        assert!(!tracker.is_empty());

        tracker.clear();
        assert!(!tracker.is_visited(&path));
        assert!(tracker.is_empty());
    }

    #[test]
    fn test_path_tracker_duplicate_mark() {
        let mut tracker = PathTracker::new();
        let path = PathBuf::from("/test/path");

        tracker.mark_visited(path.clone());
        tracker.mark_visited(path.clone()); // Mark again

        assert!(tracker.is_visited(&path));
        assert_eq!(tracker.len(), 1); // Should still be 1
    }
}
