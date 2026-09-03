//! Index capabilities for query coordination
//!
//! This module provides capability checking to ensure queries are only executed
//! against available indexes. This prevents runtime errors when an index is
//! disabled but a query attempts to use it.

use std::fmt;

/// Index capabilities - describes what indexes are available for querying
///
/// This struct is used by QueryCoordinator to check if a requested operation
/// can be performed before attempting to execute it.
///
/// # Example
///
/// ```
/// use cce_orchestrator::query::capabilities::IndexCapabilities;
///
/// // Create from indexer config
/// let caps = IndexCapabilities::new()
///     .with_vectors(true)
///     .with_bm25(true)
///     .with_summaries(false)
///     .with_relations(true);
///
/// // Check capabilities
/// assert!(caps.has_vectors());
/// assert!(!caps.has_summaries());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexCapabilities {
    /// Vector index is available
    has_vectors: bool,
    /// BM25 index is available
    has_bm25: bool,
    /// Summary index is available
    has_summaries: bool,
    /// Relation index is available
    has_relations: bool,
    /// Cross-file dependency propagation is enabled (`track_cross_file_deps`).
    /// When disabled, call-chain queries may miss cross-file edges.
    relation_propagation_enabled: bool,
    /// Maximum depth of the hot-update dependency propagation scope
    /// (0 = unlimited). A finite depth means transitive callers beyond it are
    /// not reparsed and may carry stale edges, so call chains deeper than the
    /// configured depth can lack cross-file links.
    relation_propagation_depth: usize,
}

impl Default for IndexCapabilities {
    fn default() -> Self {
        // Default to all enabled for backward compatibility
        Self {
            has_vectors: true,
            has_bm25: true,
            has_summaries: true,
            has_relations: true,
            relation_propagation_enabled: true,
            relation_propagation_depth: 0,
        }
    }
}

impl IndexCapabilities {
    /// Create new capabilities with all disabled
    pub fn new() -> Self {
        Self {
            has_vectors: false,
            has_bm25: false,
            has_summaries: false,
            has_relations: false,
            relation_propagation_enabled: false,
            relation_propagation_depth: 0,
        }
    }

    /// Create capabilities with all enabled
    pub fn all() -> Self {
        Self::default()
    }

    /// Create capabilities with nothing enabled
    pub fn none() -> Self {
        Self::new()
    }

    /// Set vector index capability
    pub fn with_vectors(mut self, enabled: bool) -> Self {
        self.has_vectors = enabled;
        self
    }

    /// Set BM25 index capability
    pub fn with_bm25(mut self, enabled: bool) -> Self {
        self.has_bm25 = enabled;
        self
    }

    /// Set summary index capability
    pub fn with_summaries(mut self, enabled: bool) -> Self {
        self.has_summaries = enabled;
        self
    }

    /// Set relation index capability
    pub fn with_relations(mut self, enabled: bool) -> Self {
        self.has_relations = enabled;
        self
    }

    /// Set relation propagation capability (`track_cross_file_deps`).
    ///
    /// When disabled, call-chain queries may miss cross-file edges because
    /// dependent files are not reparsed during hot updates.
    pub fn with_relation_propagation(mut self, enabled: bool) -> Self {
        self.relation_propagation_enabled = enabled;
        self
    }

    /// Set the relation propagation depth (0 = unlimited).
    ///
    /// A finite depth reports that hot-update propagation only reparses
    /// dependents up to that many hops, so deeper call chains may lack
    /// cross-file edges.
    pub fn with_relation_propagation_depth(mut self, depth: usize) -> Self {
        self.relation_propagation_depth = depth;
        self
    }

    /// Check if vector index is available
    pub fn has_vectors(&self) -> bool {
        self.has_vectors
    }

    /// Check if BM25 index is available
    pub fn has_bm25(&self) -> bool {
        self.has_bm25
    }

    /// Check if summary index is available
    pub fn has_summaries(&self) -> bool {
        self.has_summaries
    }

    /// Check if relation index is available
    pub fn has_relations(&self) -> bool {
        self.has_relations
    }

    /// Check if cross-file relation propagation is enabled
    pub fn has_relation_propagation(&self) -> bool {
        self.relation_propagation_enabled
    }

    /// Get the relation propagation depth (0 = unlimited).
    pub fn relation_propagation_depth(&self) -> usize {
        self.relation_propagation_depth
    }

    /// Check if any index is available
    pub fn has_any(&self) -> bool {
        self.has_vectors || self.has_bm25 || self.has_summaries || self.has_relations
    }

    /// Check if all indexes are available
    pub fn has_all(&self) -> bool {
        self.has_vectors && self.has_bm25 && self.has_summaries && self.has_relations
    }

    /// Get count of enabled indexes
    pub fn enabled_count(&self) -> usize {
        let mut count = 0;
        if self.has_vectors {
            count += 1;
        }
        if self.has_bm25 {
            count += 1;
        }
        if self.has_summaries {
            count += 1;
        }
        if self.has_relations {
            count += 1;
        }
        count
    }

    /// Merge with another capabilities (union)
    pub fn merge(&self, other: &Self) -> Self {
        Self {
            has_vectors: self.has_vectors || other.has_vectors,
            has_bm25: self.has_bm25 || other.has_bm25,
            has_summaries: self.has_summaries || other.has_summaries,
            has_relations: self.has_relations || other.has_relations,
            relation_propagation_enabled: self.relation_propagation_enabled
                || other.relation_propagation_enabled,
            // Union: the stronger scope wins (0 = unlimited is the strongest).
            relation_propagation_depth: {
                let a = self.relation_propagation_depth;
                let b = other.relation_propagation_depth;
                if a == 0 || b == 0 { 0 } else { a.max(b) }
            },
        }
    }

    /// Intersect with another capabilities
    pub fn intersect(&self, other: &Self) -> Self {
        Self {
            has_vectors: self.has_vectors && other.has_vectors,
            has_bm25: self.has_bm25 && other.has_bm25,
            has_summaries: self.has_summaries && other.has_summaries,
            has_relations: self.has_relations && other.has_relations,
            relation_propagation_enabled: self.relation_propagation_enabled
                && other.relation_propagation_enabled,
            // Intersection: the weaker scope wins (0 = unlimited is the strongest).
            relation_propagation_depth: {
                let a = self.relation_propagation_depth;
                let b = other.relation_propagation_depth;
                if a == 0 {
                    b
                } else if b == 0 {
                    a
                } else {
                    a.min(b)
                }
            },
        }
    }
}

impl fmt::Display for IndexCapabilities {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = Vec::new();
        if self.has_vectors {
            parts.push("vectors".to_string());
        }
        if self.has_bm25 {
            parts.push("bm25".to_string());
        }
        if self.has_summaries {
            parts.push("summaries".to_string());
        }
        if self.has_relations {
            if self.relation_propagation_enabled {
                if self.relation_propagation_depth == 0 {
                    parts.push("relations".to_string());
                } else {
                    parts.push(format!(
                        "relations_with_propagation_depth({})",
                        self.relation_propagation_depth
                    ));
                }
            } else {
                parts.push("relations_without_propagation".to_string());
            }
        }
        if parts.is_empty() {
            write!(f, "none")
        } else {
            write!(f, "{}", parts.join(", "))
        }
    }
}

/// Create IndexCapabilities from IndexOptions
impl From<&crate::index::IndexOptions> for IndexCapabilities {
    fn from(options: &crate::index::IndexOptions) -> Self {
        Self {
            has_vectors: options.store_vectors,
            has_bm25: options.store_bm25,
            has_summaries: options.store_summaries,
            has_relations: options.build_relations,
            relation_propagation_enabled: true,
            // IndexOptions carries no propagation depth; report unlimited so
            // callers fall back to the relation-config depth on merge.
            relation_propagation_depth: 0,
        }
    }
}

/// Create IndexCapabilities from IndexerConfig
impl From<&cce_config::IndexerConfig> for IndexCapabilities {
    fn from(config: &cce_config::IndexerConfig) -> Self {
        Self {
            has_vectors: config.store_vectors,
            has_bm25: config.store_bm25,
            has_summaries: config.store_summaries,
            has_relations: config.build_relations,
            relation_propagation_enabled: true,
            relation_propagation_depth: 0,
        }
    }
}

/// Create IndexCapabilities from RelationConfig
///
/// Relation-specific flags only; storage flags default to disabled so
/// callers merge this with the indexer-derived capabilities.
impl From<&cce_config::RelationConfig> for IndexCapabilities {
    fn from(config: &cce_config::RelationConfig) -> Self {
        Self {
            has_vectors: false,
            has_bm25: false,
            has_summaries: false,
            has_relations: config.index.enabled,
            relation_propagation_enabled: config.track_cross_file_deps,
            relation_propagation_depth: config.max_propagation_depth,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_capabilities() {
        let caps = IndexCapabilities::default();
        assert!(caps.has_vectors());
        assert!(caps.has_bm25());
        assert!(caps.has_summaries());
        assert!(caps.has_relations());
        assert!(caps.has_relation_propagation());
        assert!(caps.has_all());
    }

    #[test]
    fn test_new_capabilities() {
        let caps = IndexCapabilities::new();
        assert!(!caps.has_vectors());
        assert!(!caps.has_bm25());
        assert!(!caps.has_summaries());
        assert!(!caps.has_relations());
        assert!(!caps.has_relation_propagation());
        assert!(!caps.has_any());
    }

    #[test]
    fn test_builder_pattern() {
        let caps = IndexCapabilities::new().with_vectors(true).with_bm25(true);
        assert!(caps.has_vectors());
        assert!(caps.has_bm25());
        assert!(!caps.has_summaries());
        assert!(!caps.has_relations());
        assert_eq!(caps.enabled_count(), 2);
    }

    #[test]
    fn test_relation_propagation_flag() {
        let caps = IndexCapabilities::new()
            .with_relations(true)
            .with_relation_propagation(false);
        assert!(caps.has_relations());
        assert!(!caps.has_relation_propagation());

        let merged = caps.merge(&IndexCapabilities::new().with_relation_propagation(true));
        assert!(merged.has_relation_propagation());

        let intersected = IndexCapabilities::all().intersect(&caps);
        assert!(!intersected.has_relation_propagation());
    }

    #[test]
    fn test_relation_propagation_depth() {
        let unlimited = IndexCapabilities::new().with_relation_propagation_depth(0);
        let depth_3 = IndexCapabilities::new().with_relation_propagation_depth(3);
        let depth_5 = IndexCapabilities::new().with_relation_propagation_depth(5);

        assert_eq!(unlimited.relation_propagation_depth(), 0);
        assert_eq!(depth_3.relation_propagation_depth(), 3);

        // Union: the stronger scope wins (0 = unlimited is the strongest).
        assert_eq!(depth_3.merge(&depth_5).relation_propagation_depth(), 5);
        assert_eq!(unlimited.merge(&depth_3).relation_propagation_depth(), 0);
        // Intersection: the weaker scope wins.
        assert_eq!(depth_3.intersect(&depth_5).relation_propagation_depth(), 3);
        assert_eq!(
            unlimited.intersect(&depth_3).relation_propagation_depth(),
            3
        );
    }

    #[test]
    fn test_display_with_propagation_depth() {
        let limited = IndexCapabilities::new()
            .with_relations(true)
            .with_relation_propagation(true)
            .with_relation_propagation_depth(4);
        assert_eq!(
            format!("{}", limited),
            "relations_with_propagation_depth(4)"
        );

        let unlimited = IndexCapabilities::new()
            .with_relations(true)
            .with_relation_propagation(true)
            .with_relation_propagation_depth(0);
        assert_eq!(format!("{}", unlimited), "relations");
    }

    #[test]
    fn test_merge() {
        let caps1 = IndexCapabilities::new().with_vectors(true);
        let caps2 = IndexCapabilities::new().with_bm25(true);
        let merged = caps1.merge(&caps2);
        assert!(merged.has_vectors());
        assert!(merged.has_bm25());
    }

    #[test]
    fn test_intersect() {
        let caps1 = IndexCapabilities::all().with_summaries(false);
        let caps2 = IndexCapabilities::all().with_relations(false);
        let intersected = caps1.intersect(&caps2);
        assert!(intersected.has_vectors());
        assert!(intersected.has_bm25());
        assert!(!intersected.has_summaries());
        assert!(!intersected.has_relations());
    }

    #[test]
    fn test_display() {
        let caps = IndexCapabilities::new().with_vectors(true).with_bm25(true);
        assert_eq!(format!("{}", caps), "vectors, bm25");

        let none = IndexCapabilities::none();
        assert_eq!(format!("{}", none), "none");

        let all = IndexCapabilities::all();
        assert_eq!(format!("{}", all), "vectors, bm25, summaries, relations");

        let limited = IndexCapabilities::new()
            .with_relations(true)
            .with_relation_propagation(false);
        assert_eq!(format!("{}", limited), "relations_without_propagation");
    }

    #[test]
    fn test_from_relation_config() {
        let config = cce_config::RelationConfig::default();
        let caps = IndexCapabilities::from(&config);
        assert!(caps.has_relations());
        assert!(caps.has_relation_propagation());

        let limited = cce_config::RelationConfig {
            track_cross_file_deps: false,
            index: cce_config::IndexConfig::default(),
            ..config
        };
        let caps = IndexCapabilities::from(&limited);
        assert!(caps.has_relations());
        assert!(!caps.has_relation_propagation());
    }
}
