//! File-level parsed results

use std::collections::HashMap;
use std::sync::Arc;

use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
use serde::{Deserialize as SerdeDeserialize, Serialize as SerdeSerialize};

use super::embedded_block::{BlockRelation, EmbeddedBlock};
use super::{BehaviorStore, ControlFlowStore, Entity, EntityId};
use crate::types::import::ReexportRecord;
use crate::types::relation::{RelationLevel, RelationType};
use crate::types::stdlib_category::StdlibCategory;
use crate::types::{ImportTable, Span};

/// Parse status enumeration
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    SerdeSerialize,
    SerdeDeserialize,
    Default,
    Archive,
    RkyvDeserialize,
    RkyvSerialize,
)]
pub enum ParseStatus {
    /// Pending parsing
    #[serde(rename = "pending")]
    #[default]
    Pending,
    /// Parsing in progress
    #[serde(rename = "parsing")]
    Parsing,
    /// Parse successful
    #[serde(rename = "success")]
    Success,
    /// Parse failed
    #[serde(rename = "failed")]
    Failed,
    /// Partial success (with warnings)
    #[serde(rename = "partial")]
    Partial,
}

impl std::fmt::Display for ParseStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseStatus::Pending => write!(f, "pending"),
            ParseStatus::Parsing => write!(f, "parsing"),
            ParseStatus::Success => write!(f, "success"),
            ParseStatus::Failed => write!(f, "failed"),
            ParseStatus::Partial => write!(f, "partial"),
        }
    }
}

/// Raw relation data (for serialization)
///
/// This is a simplified version of Relation for use in ParsedFile
/// to avoid circular dependencies. Uses src/dst naming for consistency.
#[derive(
    Debug, Clone, SerdeSerialize, SerdeDeserialize, Archive, RkyvDeserialize, RkyvSerialize,
)]
pub struct RawRelationData {
    /// Source entity ID (0 when the relation is file-level)
    pub src: EntityId,
    /// Relation scope: file-level (imports, module-level calls) vs
    /// entity-level (calls/references owned by a specific entity)
    #[serde(default)]
    pub level: RelationLevel,
    /// Target name (string, not resolved yet)
    pub dst_name: String,
    /// Relation type
    pub relation_type: RelationType,
    /// Source code span
    pub span: crate::types::Span,
    /// Standard library category (if this is a stdlib relation)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdlib_category: Option<StdlibCategory>,
}

/// Parsed file result (pure parsing output)
///
/// Contains only the extracted semantic information from a source file.
/// No parsing metadata or status - only the results.
///
/// # Design Rationale
///
/// - **Pure Data**: Only contains parsing results, no metadata
/// - **Serializable**: Efficient for caching and persistence
/// - **Reusable**: Can be used independently of parsing process
/// - **Self-contained**: All needed info for downstream processing
/// - **Memory Efficient**: Uses `Arc<str>` for source to enable sharing
/// - **Conflict Aware**: `local_symbols` uses `Vec<EntityId>` to handle name conflicts
///
/// # Field Organization
///
/// Fields are organized by data source:
/// - **Basic Info**: language, path, source
/// - **Core Data**: entities, local_symbols, raw_relations (from Parser)
/// - **Control-Flow Sidecar**: control-flow facts keyed by entity ID
/// - **Behavior Sidecar**: behavior facts keyed by entity ID
/// - **SFC Data**: embedded_blocks, block_relations (from Parser for Vue/Svelte)
/// - **Documentation**: import_table, file_doc_comment, file_doc_span
///
/// Note: exports/dependencies/local_calls are NOT stored here.
/// They are derived by IndexBuilder from raw_relations and entities.
/// Imports are cached in `import_table` to avoid re-parsing the AST.
#[derive(
    Debug, Clone, SerdeSerialize, SerdeDeserialize, RkyvSerialize, RkyvDeserialize, Archive,
)]
pub struct ParsedFile {
    // === Basic Information ===
    /// Programming language
    pub language: crate::types::language::Language,
    /// File path
    pub path: String,
    /// Source code (Arc<str> for memory efficiency and sharing)
    #[serde(with = "arc_str_serde")]
    pub source: Arc<str>,

    // === Core Data (from Parser) ===
    /// Entity table (core output, replaces AST)
    pub entities: Vec<Entity>,
    /// Local symbol table (name -> Vec<EntityId>, handles name conflicts like overloading)
    pub local_symbols: HashMap<String, Vec<EntityId>>,
    /// Raw relations (callee uses string, deferred to IndexBuilder for resolution)
    pub raw_relations: Vec<RawRelationData>,
    /// Behavior sidecar indexed by entity ID.
    #[serde(default)]
    pub behavior: BehaviorStore,
    /// Control-flow sidecar indexed by entity ID.
    #[serde(default)]
    pub control_flow: ControlFlowStore,
    // === SFC Data (from Parser) ===
    /// Embedded code blocks (for Vue/Svelte SFC files)
    pub embedded_blocks: Vec<EmbeddedBlock>,
    /// Cross-block relations (template→script→style)
    pub block_relations: Vec<BlockRelation>,

    // === Documentation ===
    /// Cached import table (extracted from AST, avoids re-parsing)
    pub import_table: Option<ImportTable>,
    /// Named re-exports extracted from the AST (e.g. Rust `pub use`, JS/TS
    /// `export { x } from`). Empty when the language has no re-export
    /// construct or the extractor produced none.
    #[serde(default)]
    pub reexports: Vec<ReexportRecord>,
    /// File-level documentation comment (module/package documentation)
    pub file_doc_comment: Option<String>,
    /// Source range of the file-level documentation comment, when it originates from source.
    pub file_doc_span: Option<Span>,
    /// Full-content SHA-256 hex hash computed once at parse time (the source
    /// is already in hand); the relation build reuses it instead of hashing
    /// the source again.
    #[serde(default)]
    pub file_hash: Option<String>,
}

/// Helper module for serializing/deserializing Arc<str>
mod arc_str_serde {
    use serde::{Deserialize, Deserializer, Serialize as SerdeSerialize, Serializer};
    use std::sync::Arc;

    pub fn serialize<S>(value: &Arc<str>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        value.as_ref().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Arc<str>, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer).map(|s| s.into())
    }
}

mod rkyv_arc_str {}

impl Default for ParsedFile {
    fn default() -> Self {
        Self {
            language: crate::types::language::Language::Unknown,
            path: String::new(),
            source: Arc::from(""),
            entities: Vec::new(),
            local_symbols: HashMap::new(),
            raw_relations: Vec::new(),
            behavior: BehaviorStore::default(),
            control_flow: ControlFlowStore::default(),
            embedded_blocks: Vec::new(),
            block_relations: Vec::new(),
            import_table: None,
            reexports: Vec::new(),
            file_doc_comment: None,
            file_doc_span: None,
            file_hash: None,
        }
    }
}

impl ParsedFile {
    /// Create a new parsed file
    ///
    /// `path` must be in the canonical project-relative form (forward slashes,
    /// no redundant `.`/empty segments). Callers are responsible for normalizing
    /// the path before calling this constructor (e.g., via `normalize_project_path`).
    pub fn new(
        language: crate::types::language::Language,
        path: String,
        source: impl Into<Arc<str>>,
    ) -> Self {
        Self {
            language,
            path,
            source: source.into(),
            ..Default::default()
        }
    }

    /// Add an entity to the file
    pub fn add_entity(&mut self, entity: Entity) {
        // Add to local symbol table (handles name conflicts)
        self.local_symbols
            .entry(entity.name.clone())
            .or_default()
            .push(entity.id);
        // Add to entity list
        self.entities.push(entity);
    }

    /// Add a raw relation
    pub fn add_relation(&mut self, relation: RawRelationData) {
        self.raw_relations.push(relation);
    }

    /// Get entity by ID
    pub fn get_entity(&self, id: EntityId) -> Option<&Entity> {
        self.entities.iter().find(|e| e.id == id)
    }

    /// Get entity by ID (mutable)
    pub fn get_entity_mut(&mut self, id: EntityId) -> Option<&mut Entity> {
        self.entities.iter_mut().find(|e| e.id == id)
    }

    /// Get entities by name (returns all entities with the same name)
    pub fn get_entities_by_name(&self, name: &str) -> Vec<&Entity> {
        self.local_symbols
            .get(name)
            .map(|ids| ids.iter().filter_map(|id| self.get_entity(*id)).collect())
            .unwrap_or_default()
    }

    /// Get the first entity by name (for backward compatibility)
    pub fn get_entity_by_name(&self, name: &str) -> Option<&Entity> {
        self.local_symbols
            .get(name)
            .and_then(|ids| ids.first())
            .and_then(|id| self.get_entity(*id))
    }

    /// Get top-level entities (depth == 0)
    pub fn top_level_entities(&self) -> Vec<&Entity> {
        self.entities.iter().filter(|e| e.is_top_level()).collect()
    }

    /// Get entities by kind
    pub fn entities_by_kind(&self, kind: super::EntityKind) -> Vec<&Entity> {
        self.entities.iter().filter(|e| e.kind == kind).collect()
    }

    /// Get function/method entities
    pub fn functions(&self) -> Vec<&Entity> {
        self.entities
            .iter()
            .filter(|e| e.kind.is_function_like())
            .collect()
    }

    /// Get type definition entities
    pub fn type_definitions(&self) -> Vec<&Entity> {
        self.entities
            .iter()
            .filter(|e| e.kind.is_type_definition())
            .collect()
    }

    /// Get source code as string slice
    pub fn source_str(&self) -> &str {
        &self.source
    }

    /// Resolve scoped name for an entity ID
    ///
    /// Computes the fully-qualified name from root to target entity.
    /// For example: "MyModule::MyStruct::my_method"
    ///
    /// This is used as the source-level logical address for the entity,
    /// independent of runtime EntityId allocation. Stable across parse runs
    /// as long as source code structure doesn't change.
    ///
    /// Anonymous entities (closures/lambdas) use special naming:
    /// <anonymous@{line}:{col}> to avoid EntityId dependency.
    pub fn resolve_scoped_name(&self, entity_id: EntityId) -> Option<String> {
        self.resolve_scoped_name_from_map(
            entity_id,
            &self.entities.iter().map(|e| (e.id, e)).collect(),
        )
    }

    /// Resolve scoped name using a prebuilt id -> entity map.
    ///
    /// Callers resolving many names from the same file should build the map
    /// once  instead of relying on the linear `get_entity` scan.
    fn resolve_scoped_name_from_map(
        &self,
        entity_id: EntityId,
        entity_map: &HashMap<EntityId, &Entity>,
    ) -> Option<String> {
        entity_map.get(&entity_id)?;

        let mut names = Vec::new();
        let mut current_id = Some(entity_id);

        while let Some(id) = current_id {
            let entity = entity_map.get(&id)?;
            let name = if entity.name.is_empty() || entity.name == "<anonymous>" {
                format!(
                    "<anonymous@{}:{}>",
                    entity.span.start_position.row, entity.span.start_position.column
                )
            } else {
                entity.name.clone()
            };
            names.push(name);
            current_id = entity.parent;
        }

        names.reverse();
        Some(names.join("::"))
    }

    /// Resolve scoped names for all entities
    ///
    /// Returns a mapping of EntityId to scoped_name for verification purposes.
    ///
    /// Builds the id -> entity lookup map once (O(E)) and walks each parent
    /// chain through it, avoiding the previous O(E) linear scan per entity.
    pub fn resolve_all_scoped_names(&self) -> std::collections::HashMap<EntityId, String> {
        let entity_map: HashMap<EntityId, &Entity> =
            self.entities.iter().map(|e| (e.id, e)).collect();
        let mut result = std::collections::HashMap::with_capacity(self.entities.len());
        for entity in &self.entities {
            if let Some(scoped_name) = self.resolve_scoped_name_from_map(entity.id, &entity_map) {
                result.insert(entity.id, scoped_name);
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The constructor is the identity boundary: whatever path spelling a
    /// caller passes, the stored form must be the canonical project-relative
    /// one so chunk IDs and storage keys stay stable.
    #[test]
    fn new_preserves_path_as_provided() {
        // ParsedFile::new() now expects pre-normalized paths.
        // Callers are responsible for normalizing via normalize_project_path().
        let parsed = ParsedFile::new(
            crate::types::language::Language::Rust,
            "src/lib.rs".to_string(),
            "fn main() {}",
        );
        assert_eq!(parsed.path, "src/lib.rs");

        let windows = ParsedFile::new(
            crate::types::language::Language::Python,
            "scripts/tool.py".to_string(),
            "",
        );
        assert_eq!(windows.path, "scripts/tool.py");
    }
}
