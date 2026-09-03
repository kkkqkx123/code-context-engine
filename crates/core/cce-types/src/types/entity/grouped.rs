//! GroupedEntity - flattened entity representation for use after grouper stage

use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use std::collections::HashMap;

use super::{Entity, EntityId, EntityKind};
use crate::types::StdlibCategory;

/// GroupedEntity is a flattened entity representation used after the grouper stage.
///
/// It omits parent/children relationships since these are now managed by EntityGroup.
/// - Use `Entity` during parsing (has children/parent relationships)
/// - Use `GroupedEntity` after grouping (belongs to EntityGroup.header/members)
///
/// Design rationale:
/// - Reduces memory footprint in EntityGroup
/// - Eliminates data duplication (parent/children stored in Group structure)
/// - Keeps essential info: id, name, kind, signature, parameters, docs
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GroupedEntity {
    /// Entity ID (retained for correlation)
    pub id: EntityId,

    /// Entity name
    pub name: String,

    /// Entity kind
    pub kind: EntityKind,

    /// Signature (simplified from Entity::signature)
    pub signature: String,

    /// Parameters [(name, type), ...]
    /// SmallVec with inline capacity of 4 to avoid heap allocation for small parameter lists
    pub parameters: SmallVec<[(CompactString, Option<CompactString>); 4]>,

    /// Return type
    pub return_type: Option<String>,

    /// Doc comment (extracted text)
    pub doc_comment: Option<String>,

    /// Modifiers (e.g., "pub", "public", "static", "async")
    /// Preserved from Entity for template usage
    #[serde(default)]
    pub modifiers: Vec<String>,

    /// Element attributes (e.g., class, id, for HTML/Vue elements)
    /// Preserved from Entity for template usage
    #[serde(default)]
    pub attributes: HashMap<String, String>,

    /// Entity subtype (e.g., "generator" for function.generator)
    /// Preserved from Entity for template usage
    #[serde(default)]
    pub subtype: Option<String>,

    /// Standard library entity marker
    pub is_stdlib: bool,

    /// Standard library category (if is_stdlib is true)
    /// Uses StdlibCategory enum for type safety and semantic optimization
    pub stdlib_category: Option<StdlibCategory>,

    /// Extension metadata (framework-specific info, annotations, etc.)
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

impl GroupedEntity {
    /// Create a new GroupedEntity from a full Entity
    pub fn from_entity(entity: &Entity) -> Self {
        // Convert parameters to CompactString format
        let parameters: SmallVec<[(CompactString, Option<CompactString>); 4]> = entity
            .parameters
            .iter()
            .map(|(name, ty)| {
                (
                    CompactString::from(name.as_str()),
                    ty.as_ref().map(|t| CompactString::from(t.as_str())),
                )
            })
            .collect();

        Self {
            id: entity.id,
            name: entity.name.clone(),
            kind: entity.kind,
            signature: entity.signature.clone(),
            parameters,
            return_type: entity.return_type.clone(),
            doc_comment: entity.doc_comment.clone(),
            modifiers: entity.modifiers.clone(),
            attributes: entity.attributes.clone(),
            subtype: entity.subtype.clone(),
            is_stdlib: entity.is_stdlib,
            stdlib_category: entity.stdlib_category,
            metadata: entity.metadata.clone(),
        }
    }

    /// Create a GroupedEntity with basic info
    pub fn new(id: EntityId, kind: EntityKind, name: String, signature: String) -> Self {
        Self {
            id,
            kind,
            name,
            signature,
            parameters: SmallVec::new(),
            return_type: None,
            doc_comment: None,
            modifiers: Vec::new(),
            attributes: HashMap::new(),
            subtype: None,
            is_stdlib: false,
            stdlib_category: None,
            metadata: HashMap::new(),
        }
    }

    /// Set the parameters
    pub fn with_parameters(
        mut self,
        params: SmallVec<[(CompactString, Option<CompactString>); 4]>,
    ) -> Self {
        self.parameters = params;
        self
    }

    /// Set the return type
    pub fn with_return_type(mut self, ret_type: Option<String>) -> Self {
        self.return_type = ret_type;
        self
    }

    /// Set the doc comment
    pub fn with_doc_comment(mut self, doc: Option<String>) -> Self {
        self.doc_comment = doc;
        self
    }
}
