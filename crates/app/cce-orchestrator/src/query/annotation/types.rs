//! Relation annotation types
//!
//! This module provides type definitions for relation annotation operations.

use cce_types::{EntityId, RelationType};

/// Search result input for annotation
///
/// Encapsulates all parameters needed for annotating a search result.
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

/// Relation annotation configuration
pub use cce_config::modules::search::RelationAnnotationConfig;

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
    /// Relation type of the edge that produced this unit, when known.
    /// Primary units carry `None`. A `None` on an expansion unit means the
    /// caller did not classify the edge; it is treated as call-domain so
    /// legacy call-only inputs keep working.
    pub relation_type: Option<RelationType>,
    /// Relevance score. Primary units inherit the search hit score; expansion
    /// units carry caller-resolved scores used for direction-internal ordering
    /// and budget selection.
    pub score: f32,
    /// True when the target is a standard-library symbol.
    pub is_stdlib: bool,
    /// True when the target has no in-workspace source.
    pub is_external: bool,
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
            relation_type: None,
            score: 0.0,
            is_stdlib: false,
            is_external: false,
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

    /// Set relevance score
    pub fn with_score(mut self, score: f32) -> Self {
        self.score = score;
        self
    }

    /// Set the relation type of the producing edge
    pub fn with_relation_type(mut self, relation_type: RelationType) -> Self {
        self.relation_type = Some(relation_type);
        self
    }

    /// Mark the target as a standard-library symbol (or not)
    pub fn with_stdlib(mut self, is_stdlib: bool) -> Self {
        self.is_stdlib = is_stdlib;
        self
    }

    /// Mark the target as external to the workspace (or not)
    pub fn with_external(mut self, is_external: bool) -> Self {
        self.is_external = is_external;
        self
    }

    /// Whether this unit belongs to the call domain and may be auto-attached.
    /// Units with an unknown edge (`None`) count as call-domain so legacy
    /// call-only inputs keep working; only a known non-call edge is rejected.
    pub fn is_call_domain(&self) -> bool {
        self.relation_type.as_ref().is_none_or(|t| t.is_call())
    }

    /// Get content hash for deduplication
    pub fn content_hash(&self) -> u64 {
        // Stable FNV-1a over identity plus body so equal units hash equally
        // across runs. Covers file path and line range, not just the text.
        let mut hash: u64 = 0xcbf29ce484222325;
        let mut mix = |bytes: &[u8]| {
            for byte in bytes {
                hash ^= u64::from(*byte);
                hash = hash.wrapping_mul(0x100_0000_01b3);
            }
        };
        mix(self.file_path.as_bytes());
        mix(&self.start_line.to_le_bytes());
        mix(&self.end_line.to_le_bytes());
        mix(self.code.as_bytes());
        hash
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

/// Annotation metadata
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AnnotationMetadata {
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
    /// Annotated content length
    pub annotated_length: usize,
    /// Whether content was truncated
    pub truncated: bool,
}

impl Default for AnnotationMetadata {
    fn default() -> Self {
        Self {
            expanded: false,
            expanded_nodes: 0,
            forward_nodes: 0,
            backward_nodes: 0,
            file_count: 1,
            original_length: 0,
            annotated_length: 0,
            truncated: false,
        }
    }
}

/// Annotated result
#[derive(Debug, Clone)]
pub struct AnnotatedResult {
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
    /// Annotated content
    pub annotated_content: String,
    /// Involved files
    pub involved_files: Vec<FileInfo>,
    /// Annotation metadata
    pub metadata: AnnotationMetadata,
    /// Original content (before annotation)
    pub original_content: String,
}

impl AnnotatedResult {
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
            annotated_content: unit.code.clone(),
            involved_files: vec![FileInfo::new(unit.file_path)],
            metadata: AnnotationMetadata {
                expanded: false,
                expanded_nodes: 0,
                forward_nodes: 0,
                backward_nodes: 0,
                file_count: 1,
                original_length,
                annotated_length: original_length,
                truncated: false,
            },
            original_content: unit.code,
        }
    }

    /// Check if caller-supplied relation units were expanded into the result.
    pub fn has_expanded_relations(&self) -> bool {
        self.metadata.expanded
    }

    /// Get total content length
    pub fn total_length(&self) -> usize {
        self.annotated_content.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relation_annotation_config_default() {
        let config = RelationAnnotationConfig::default();
        assert!(!config.enable_annotation);
        assert_eq!(config.max_annotated_length, 8000);
    }

    #[test]
    fn test_relation_annotation_config_builder() {
        let config = RelationAnnotationConfig::new()
            .enable(true)
            .with_max_length(3000);

        assert!(config.enable_annotation);
        assert_eq!(config.max_annotated_length, 3000);
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
        assert_eq!(unit.origin, ExpansionOrigin::Primary);
    }

    #[test]
    fn test_token_estimation() {
        let config = RelationAnnotationConfig {
            max_annotated_length: 1000,
            ..Default::default()
        };

        assert_eq!(config.get_max_length(), 1000);

        let test_content = "fn hello() { println!(\"world\"); }";
        let tokens = config.estimate_content_tokens(test_content);
        assert!(tokens > 0, "Should estimate some tokens");

        assert!(config.check_content_limit(tokens));
    }
}
