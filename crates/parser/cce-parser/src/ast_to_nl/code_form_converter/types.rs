//! Core data types for code form conversion

use cce_types::entity::{EntityId, EntityKind};
use std::collections::HashMap;

/// Structured code form representation of a single entity
///
/// This is a lightweight structure that captures the essential information about
/// an entity's code form, suitable for consumption by summary generation, export,
/// and other code-understanding modules.
#[derive(Debug, Clone)]
pub struct CodeFormEntity {
    /// Unique entity ID (file-local)
    pub id: EntityId,

    /// Entity name
    pub name: String,

    /// Entity kind (cross-language unified)
    pub kind: EntityKind,

    /// Modifiers (e.g., "pub", "private", "static", "async", "abstract")
    pub modifiers: Vec<String>,

    /// Type annotation for fields/returns
    /// - For functions: return type
    /// - For fields: field type
    /// - For parameters: parameter types (stored separately in signature)
    pub type_annotation: Option<String>,

    /// Documentation comment (if present)
    pub doc_comment: Option<String>,

    /// Brief description or summary hint (auto-generated or from first line of doc)
    pub summary_hint: String,

    /// Function signature (if applicable)
    pub signature: Option<String>,

    /// Parameters (name, type) pairs - for functions/methods
    pub parameters: Vec<(String, Option<String>)>,

    /// Semantic depth (0 = top-level)
    pub depth: usize,
}

impl CodeFormEntity {
    /// Check if this entity is a type definition (class, struct, etc.)
    pub fn is_type_definition(&self) -> bool {
        matches!(
            self.kind,
            EntityKind::Class
                | EntityKind::Struct
                | EntityKind::Enum
                | EntityKind::Interface
                | EntityKind::Trait
                | EntityKind::Union
                | EntityKind::TypeAlias
        )
    }

    /// Check if this entity is a function-like entity
    pub fn is_function_like(&self) -> bool {
        matches!(
            self.kind,
            EntityKind::Function
                | EntityKind::Method
                | EntityKind::Constructor
                | EntityKind::Destructor
                | EntityKind::Operator
        )
    }
}

/// Structured code form representation of an entity group
///
/// An entity group typically represents a semantic grouping like:
/// - Class with its methods
/// - Trait with its implementations
/// - Module with its exports
/// - Test suite with test cases
#[derive(Debug, Clone)]
pub struct CodeFormGroup {
    /// Header entity (the main entity of the group, e.g., class)
    pub header: CodeFormEntity,

    /// Member entities (e.g., methods of a class)
    pub members: Vec<CodeFormEntity>,

    /// Nested groups (for hierarchical structures)
    pub nested_groups: Vec<CodeFormGroup>,

    /// Group type name (informational)
    pub group_type: String,
}

impl CodeFormGroup {
    /// Get the total entity count (header + members + all nested)
    pub fn entity_count(&self) -> usize {
        1 + self.members.len()
            + self
                .nested_groups
                .iter()
                .map(|g| g.entity_count())
                .sum::<usize>()
    }

    /// Get all top-level members (not including nested groups)
    pub fn all_members(&self) -> Vec<&CodeFormEntity> {
        self.members.iter().collect()
    }

    /// Get all functions/methods
    pub fn all_functions(&self) -> Vec<&CodeFormEntity> {
        self.members
            .iter()
            .filter(|m| m.is_function_like())
            .collect()
    }

    /// Get all type definitions (nested classes, etc.)
    pub fn all_type_definitions(&self) -> Vec<&CodeFormEntity> {
        self.members
            .iter()
            .filter(|m| m.is_type_definition())
            .collect()
    }
}

/// Context information for code form conversion
///
/// Carries contextual information during the conversion process,
/// such as entity lookup maps, configuration, etc.
#[derive(Debug, Clone)]
pub struct CodeFormContext {
    /// Entity ID to entity map for quick lookup
    pub entity_lookup: HashMap<EntityId, CodeFormEntity>,

    /// File language
    pub language: String,

    /// File path
    pub file_path: String,

    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

impl CodeFormContext {
    /// Create a new context
    pub fn new(language: String, file_path: String) -> Self {
        Self {
            entity_lookup: HashMap::new(),
            language,
            file_path,
            metadata: HashMap::new(),
        }
    }

    /// Add entity to lookup
    pub fn add_entity(&mut self, entity: CodeFormEntity) {
        self.entity_lookup.insert(entity.id, entity);
    }

    /// Get entity by ID
    pub fn get_entity(&self, id: EntityId) -> Option<&CodeFormEntity> {
        self.entity_lookup.get(&id)
    }
}
