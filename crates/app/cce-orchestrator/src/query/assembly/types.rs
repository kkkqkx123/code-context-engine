//! SPSR-Graph assembly types
//!
//! This module provides type definitions for SPSR-Graph (Structure-Preserving
//! and Semantically-Reordered Code Graph) assembly operations.

use std::collections::HashSet;

use cce_types::EntityId;

/// Search result input for assembly
///
/// Encapsulates all parameters needed for assembling a search result.
#[derive(Debug, Clone)]
pub struct SearchResultInput {
    /// Result ID
    pub id: String,
    /// Optional entity ID for relation queries
    pub entity_id: Option<EntityId>,
    /// Entity name
    pub name: String,
    /// Entity kind (function, class, etc.)
    pub kind: String,
    /// File path
    pub file_path: String,
    /// Start line
    pub start_line: u32,
    /// End line
    pub end_line: u32,
    /// Original content
    pub content: String,
    /// Relevance score
    pub score: f32,
}

/// Deduplication strategy
pub use cce_config::modules::search::DedupStrategy;

/// Truncation strategy for assembled content
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum TruncationStrategy {
    /// Hard cut at character limit (current behavior)
    #[default]
    HardCut,
    /// Cut at semantic boundaries (function/class boundaries)
    SemanticBoundary,
    /// Remove low-priority units first when approaching limit
    PriorityBased,
    /// Dynamically reduce expansion depth based on budget
    Progressive,
}

/// SPSR-Graph assembly configuration
pub use cce_config::modules::search::SPSRGraphConfig;

/// Semantic unit type
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticUnitType {
    /// Function
    Function,
    /// Method
    Method,
    /// Class
    Class,
    /// Struct
    Struct,
    /// Interface/Trait
    Interface,
    /// Enum
    Enum,
    /// Module
    Module,
    /// Unknown
    Unknown,
}

impl std::fmt::Display for SemanticUnitType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Function => write!(f, "function"),
            Self::Method => write!(f, "method"),
            Self::Class => write!(f, "class"),
            Self::Struct => write!(f, "struct"),
            Self::Interface => write!(f, "interface"),
            Self::Enum => write!(f, "enum"),
            Self::Module => write!(f, "module"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

/// Origin of an expanded unit relative to the primary result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpansionOrigin {
    /// The primary search-result unit itself.
    Primary,
    /// A callee of the primary unit (forward call-graph edge).
    Forward,
    /// A caller of the primary unit (backward call-graph edge).
    Backward,
}

/// Expanded semantic unit
#[derive(Debug, Clone)]
pub struct ExpandedUnit {
    /// Entity ID
    pub entity_id: Option<EntityId>,
    /// Complete code content
    pub code: String,
    /// File path
    pub file_path: String,
    /// Start line
    pub start_line: u32,
    /// End line
    pub end_line: u32,
    /// Entity name
    pub name: String,
    /// Semantic unit type
    pub unit_type: SemanticUnitType,
    /// Origin of this unit relative to the primary result
    pub origin: ExpansionOrigin,
    /// Relation label rendered in the expansion marker (e.g. "calls")
    pub edge_label: String,
    /// Depth in the expansion tree
    pub depth: u32,
}

impl ExpandedUnit {
    /// Create a new expanded unit
    pub fn new(
        code: String,
        file_path: String,
        start_line: u32,
        end_line: u32,
        name: String,
    ) -> Self {
        Self {
            entity_id: None,
            code,
            file_path,
            start_line,
            end_line,
            name,
            unit_type: SemanticUnitType::Unknown,
            origin: ExpansionOrigin::Primary,
            edge_label: String::new(),
            depth: 0,
        }
    }

    /// Set origin and edge label as an expansion unit
    pub fn with_expansion(
        mut self,
        origin: ExpansionOrigin,
        edge_label: impl Into<String>,
    ) -> Self {
        self.origin = origin;
        self.edge_label = edge_label.into();
        self
    }

    /// Set entity ID
    pub fn with_entity_id(mut self, id: EntityId) -> Self {
        self.entity_id = Some(id);
        self
    }

    /// Set unit type
    pub fn with_unit_type(mut self, unit_type: SemanticUnitType) -> Self {
        self.unit_type = unit_type;
        self
    }

    /// Get content hash for deduplication
    pub fn content_hash(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        self.code.hash(&mut hasher);
        hasher.finish()
    }

    /// Check if this unit is from the same file as another
    pub fn is_same_file(&self, other: &ExpandedUnit) -> bool {
        self.file_path == other.file_path
    }
}

/// File information
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FileInfo {
    /// File path
    pub path: String,
    /// Number of units from this file
    pub unit_count: usize,
    /// Total lines
    pub total_lines: u32,
}

impl FileInfo {
    /// Create a new file info
    pub fn new(path: String) -> Self {
        Self {
            path,
            unit_count: 0,
            total_lines: 0,
        }
    }
}

/// Assembly metadata
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AssemblyMetadata {
    /// Whether relation expansion attached any unit
    pub expanded: bool,
    /// Number of expanded nodes (forward + backward)
    pub expanded_nodes: usize,
    /// Number of forward (callee) expansion units
    pub forward_nodes: usize,
    /// Number of backward (caller) expansion units
    pub backward_nodes: usize,
    /// Number of involved files
    pub file_count: usize,
    /// Original content length
    pub original_length: usize,
    /// Assembled content length
    pub assembled_length: usize,
    /// Whether content was truncated
    pub truncated: bool,
}

impl Default for AssemblyMetadata {
    fn default() -> Self {
        Self {
            expanded: false,
            expanded_nodes: 0,
            forward_nodes: 0,
            backward_nodes: 0,
            file_count: 1,
            original_length: 0,
            assembled_length: 0,
            truncated: false,
        }
    }
}

/// Assembled result
#[derive(Debug, Clone)]
pub struct AssembledResult {
    /// Primary search result ID
    pub id: String,
    /// Primary entity ID
    pub entity_id: Option<EntityId>,
    /// Primary entity name
    pub name: String,
    /// Primary entity type
    pub kind: String,
    /// Primary file path
    pub file_path: String,
    /// Primary score
    pub score: f32,
    /// Primary start line
    pub start_line: u32,
    /// Primary end line
    pub end_line: u32,
    /// Assembled content
    pub assembled_content: String,
    /// Involved files
    pub involved_files: Vec<FileInfo>,
    /// Assembly metadata
    pub metadata: AssemblyMetadata,
    /// Original content (before assembly)
    pub original_content: String,
}

impl AssembledResult {
    /// Create from a primary unit
    pub fn from_primary(unit: ExpandedUnit, score: f32, id: String, kind: String) -> Self {
        let original_length = unit.code.len();
        Self {
            id,
            entity_id: unit.entity_id,
            name: unit.name.clone(),
            kind,
            file_path: unit.file_path.clone(),
            score,
            start_line: unit.start_line,
            end_line: unit.end_line,
            assembled_content: unit.code.clone(),
            involved_files: vec![FileInfo::new(unit.file_path)],
            metadata: AssemblyMetadata {
                expanded: false,
                expanded_nodes: 0,
                forward_nodes: 0,
                backward_nodes: 0,
                file_count: 1,
                original_length,
                assembled_length: original_length,
                truncated: false,
            },
            original_content: unit.code,
        }
    }

    /// Check if assembly was performed
    pub fn is_assembled(&self) -> bool {
        self.metadata.expanded
    }

    /// Get total content length
    pub fn total_length(&self) -> usize {
        self.assembled_content.len()
    }
}

/// Unit deduplicator
#[derive(Debug, Default)]
pub struct UnitDeduplicator {
    seen_entity_ids: HashSet<EntityId>,
    seen_hashes: HashSet<u64>,
}

impl UnitDeduplicator {
    /// Create a new deduplicator
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if a unit should be kept (not duplicate)
    pub fn should_keep(&mut self, unit: &ExpandedUnit, strategy: DedupStrategy) -> bool {
        match strategy {
            DedupStrategy::None => true,
            DedupStrategy::ByEntityId => {
                if let Some(id) = unit.entity_id {
                    self.seen_entity_ids.insert(id)
                } else {
                    true
                }
            }
            DedupStrategy::ByContentHash => self.seen_hashes.insert(unit.content_hash()),
        }
    }

    /// Reset the deduplicator
    pub fn reset(&mut self) {
        self.seen_entity_ids.clear();
        self.seen_hashes.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spsr_graph_config_default() {
        let config = SPSRGraphConfig::default();
        assert!(!config.enable_assembly);
        assert_eq!(config.max_assembled_length, 2500);
    }

    #[test]
    fn test_spsr_graph_config_builder() {
        let config = SPSRGraphConfig::new().enable(true).with_max_length(3000);

        assert!(config.enable_assembly);
        assert_eq!(config.max_assembled_length, 3000);
    }

    #[test]
    fn test_expanded_unit() {
        let unit = ExpandedUnit::new(
            "fn add(a: i32, b: i32) -> i32 { a + b }".to_string(),
            "src/math.rs".to_string(),
            1,
            3,
            "add".to_string(),
        );

        assert_eq!(unit.name, "add");
        assert_eq!(unit.file_path, "src/math.rs");
        assert_eq!(unit.start_line, 1);
        assert_eq!(unit.end_line, 3);
        assert_eq!(unit.unit_type, SemanticUnitType::Unknown);
    }

    #[test]
    fn test_unit_deduplicator() {
        let mut dedup = UnitDeduplicator::new();

        let unit1 = ExpandedUnit::new(
            "fn foo() {}".to_string(),
            "src/a.rs".to_string(),
            1,
            2,
            "foo".to_string(),
        );

        let unit2 = ExpandedUnit::new(
            "fn foo() {}".to_string(),
            "src/a.rs".to_string(),
            1,
            2,
            "foo".to_string(),
        );

        // By content hash
        assert!(dedup.should_keep(&unit1, DedupStrategy::ByContentHash));
        assert!(!dedup.should_keep(&unit2, DedupStrategy::ByContentHash));

        dedup.reset();

        // None strategy
        assert!(dedup.should_keep(&unit1, DedupStrategy::None));
        assert!(dedup.should_keep(&unit2, DedupStrategy::None));
    }

    #[test]
    fn test_token_estimation() {
        let config = SPSRGraphConfig {
            max_assembled_length: 1000, // 1000 tokens
            ..Default::default()
        };

        assert_eq!(config.get_max_length(), 1000);

        // Test token estimation
        let test_content = "fn hello() { println!(\"world\"); }";
        let tokens = config.estimate_content_tokens(test_content);
        assert!(tokens > 0, "Should estimate some tokens");

        // Test token limit check
        assert!(config.check_content_limit(tokens));
    }
}
