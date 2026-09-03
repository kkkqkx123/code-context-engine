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

/// Expansion strategy for call chain traversal
pub use cce_config::modules::search::ExpansionStrategy;

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

/// Unit priority for smart truncation
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum UnitPriority {
    /// Primary result - highest priority
    Primary = 0,
    /// Callers - high priority
    Caller = 1,
    /// Callees - medium priority
    Callee = 2,
    /// Sibling units - low priority
    Sibling = 3,
    /// Deep expansions - lowest priority
    DeepExpansion = 4,
}

impl UnitPriority {
    /// Get priority from relation type and depth
    pub fn from_relation_and_depth(relation: RelationType, depth: usize) -> Self {
        match relation {
            RelationType::Primary => Self::Primary,
            RelationType::Caller => Self::Caller,
            RelationType::Callee => {
                if depth <= 1 {
                    Self::Callee
                } else {
                    Self::DeepExpansion
                }
            }
            RelationType::Sibling => Self::Sibling,
            RelationType::BaseClass | RelationType::DerivedClass => Self::Callee,
        }
    }

    /// Check if this priority is lower than another
    pub fn is_lower_than(&self, other: &Self) -> bool {
        *self as u8 > *other as u8
    }
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

/// Relation type between units
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationType {
    /// Primary result
    Primary,
    /// Caller (calls the primary)
    Caller,
    /// Callee (called by the primary)
    Callee,
    /// Sibling (same scope)
    Sibling,
    /// Base class
    BaseClass,
    /// Derived class
    DerivedClass,
}

impl std::fmt::Display for RelationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Primary => write!(f, "primary"),
            Self::Caller => write!(f, "caller"),
            Self::Callee => write!(f, "callee"),
            Self::Sibling => write!(f, "sibling"),
            Self::BaseClass => write!(f, "base_class"),
            Self::DerivedClass => write!(f, "derived_class"),
        }
    }
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
    /// Relation to primary result
    pub relation: RelationType,
    /// Depth from primary result
    pub depth: usize,
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
            relation: RelationType::Primary,
            depth: 0,
        }
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

    /// Set relation type
    pub fn with_relation(mut self, relation: RelationType) -> Self {
        self.relation = relation;
        self
    }

    /// Set depth
    pub fn with_depth(mut self, depth: usize) -> Self {
        self.depth = depth;
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

    /// Get unit priority based on relation and depth
    pub fn priority(&self) -> UnitPriority {
        UnitPriority::from_relation_and_depth(self.relation, self.depth)
    }

    /// Check if this unit is from the same file as another
    pub fn is_same_file(&self, other: &ExpandedUnit) -> bool {
        self.file_path == other.file_path
    }
}

/// Call chain assembly
#[derive(Debug, Clone, Default)]
pub struct CallChainAssembly {
    /// Forward expansion (callees)
    pub forward_expansion: Vec<ExpandedUnit>,
    /// Backward expansion (callers)
    pub backward_expansion: Vec<ExpandedUnit>,
    /// Maximum depth reached
    pub max_depth: usize,
    /// Total nodes expanded
    pub total_nodes: usize,
}

impl CallChainAssembly {
    /// Create a new empty assembly
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.forward_expansion.is_empty() && self.backward_expansion.is_empty()
    }

    /// Get all units
    pub fn all_units(&self) -> Vec<&ExpandedUnit> {
        let mut units = Vec::new();
        units.extend(self.forward_expansion.iter());
        units.extend(self.backward_expansion.iter());
        units
    }

    /// Get total count
    pub fn total_count(&self) -> usize {
        self.forward_expansion.len() + self.backward_expansion.len()
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
    /// Whether expansion was performed
    pub expanded: bool,
    /// Number of expanded nodes
    pub expanded_nodes: usize,
    /// Number of involved files
    pub file_count: usize,
    /// Expansion strategy used
    pub strategy: ExpansionStrategy,
    /// Maximum depth reached
    pub max_depth: usize,
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
            file_count: 1,
            strategy: ExpansionStrategy::None,
            max_depth: 0,
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
    /// Call chain assembly
    pub call_chain: CallChainAssembly,
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
            call_chain: CallChainAssembly::new(),
            assembled_content: unit.code.clone(),
            involved_files: vec![FileInfo::new(unit.file_path)],
            metadata: AssemblyMetadata {
                expanded: false,
                expanded_nodes: 0,
                file_count: 1,
                strategy: ExpansionStrategy::None,
                max_depth: 0,
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
        assert_eq!(config.expansion_strategy, ExpansionStrategy::ForwardOnly);
        assert_eq!(config.max_expansion_depth, 2);
        assert_eq!(config.max_expanded_nodes, 5);
    }

    #[test]
    fn test_spsr_graph_config_builder() {
        let config = SPSRGraphConfig::new()
            .enable(true)
            .with_expansion_strategy(ExpansionStrategy::Bidirectional)
            .with_max_depth(3)
            .with_max_nodes(10);

        assert!(config.enable_assembly);
        assert_eq!(config.expansion_strategy, ExpansionStrategy::Bidirectional);
        assert_eq!(config.max_expansion_depth, 3);
        assert_eq!(config.max_expanded_nodes, 10);
    }

    #[test]
    fn test_expansion_strategy_display() {
        assert_eq!(format!("{}", ExpansionStrategy::None), "none");
        assert_eq!(
            format!("{}", ExpansionStrategy::ForwardOnly),
            "forward_only"
        );
        assert_eq!(
            format!("{}", ExpansionStrategy::BackwardOnly),
            "backward_only"
        );
        assert_eq!(
            format!("{}", ExpansionStrategy::Bidirectional),
            "bidirectional"
        );
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
        assert_eq!(unit.relation, RelationType::Primary);
    }

    #[test]
    fn test_call_chain_assembly() {
        let mut assembly = CallChainAssembly::new();
        assert!(assembly.is_empty());

        assembly.forward_expansion.push(ExpandedUnit::new(
            "fn foo() {}".to_string(),
            "src/a.rs".to_string(),
            1,
            2,
            "foo".to_string(),
        ));

        assert!(!assembly.is_empty());
        assert_eq!(assembly.total_count(), 1);
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
