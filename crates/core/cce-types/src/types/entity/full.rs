//! Full Entity - complete entity with parent/children relationships (used during parsing)

use std::collections::HashMap;

use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize};
use serde::{Deserialize as SerdeDeserialize, Serialize as SerdeSerialize};

use super::{EntityId, EntityKind};
use crate::types::Span;
use crate::types::StdlibCategory;

/// rkyv-safe snapshot of [`Entity`] with `HashMap` fields replaced by `Vec`
/// tuples to satisfy rkyv 0.8 trait bounds (`ArchivedHashMap` does not
/// implement `Hash` / `Eq`).
#[derive(Debug, Clone, Archive, RkyvDeserialize, Serialize)]
pub struct EntitySnapshot {
    pub id: EntityId,
    pub kind: EntityKind,
    pub name: String,
    pub signature: String,
    pub parameters: Vec<(String, Option<String>)>,
    pub return_type: Option<String>,
    pub span: Span,
    pub depth: usize,
    pub parent: Option<EntityId>,
    pub children: Vec<EntityId>,
    pub doc_comment: Option<String>,
    pub modifiers: Vec<String>,
    pub attributes: Vec<(String, String)>,
    pub metadata: Vec<(String, String)>,
    pub is_stdlib: bool,
    pub stdlib_category: Option<StdlibCategory>,
    pub subtype: Option<String>,
}

impl From<&Entity> for EntitySnapshot {
    fn from(e: &Entity) -> Self {
        Self {
            id: e.id,
            kind: e.kind,
            name: e.name.clone(),
            signature: e.signature.clone(),
            parameters: e.parameters.clone(),
            return_type: e.return_type.clone(),
            span: e.span,
            depth: e.depth,
            parent: e.parent,
            children: e.children.clone(),
            doc_comment: e.doc_comment.clone(),
            modifiers: e.modifiers.clone(),
            attributes: e
                .attributes
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            metadata: e
                .metadata
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            is_stdlib: e.is_stdlib,
            stdlib_category: e.stdlib_category,
            subtype: e.subtype.clone(),
        }
    }
}

impl From<EntitySnapshot> for Entity {
    fn from(s: EntitySnapshot) -> Self {
        Self {
            id: s.id,
            kind: s.kind,
            name: s.name,
            signature: s.signature,
            parameters: s.parameters,
            return_type: s.return_type,
            span: s.span,
            depth: s.depth,
            parent: s.parent,
            children: s.children,
            doc_comment: s.doc_comment,
            modifiers: s.modifiers,
            attributes: s.attributes.into_iter().collect(),
            metadata: s.metadata.into_iter().collect(),
            is_stdlib: s.is_stdlib,
            stdlib_category: s.stdlib_category,
            subtype: s.subtype,
        }
    }
}

/// Semantic entity (not AST node wrapper)
///
/// Entity represents a cross-language unified semantic concept.
/// It contains all information needed for downstream processing without AST dependency.
///
/// Use this during parsing when parent/children relationships need to be tracked.
/// After the grouper stage, consider using GroupedEntity which has flattened relationships.
#[derive(
    Debug, Clone, SerdeSerialize, SerdeDeserialize, Default, Archive, RkyvDeserialize, Serialize,
)]
pub struct Entity {
    /// Entity ID (file-local)
    pub id: EntityId,
    /// Entity kind (cross-language unified)
    pub kind: EntityKind,
    /// Entity name
    pub name: String,

    /// Derived info: signature (extracted from AST and formatted)
    pub signature: String,

    /// Structured info: parameter list [(name, type), ...]
    pub parameters: Vec<(String, Option<String>)>,

    /// Structured info: return type
    pub return_type: Option<String>,

    /// Source code location (supports splitting and positioning)
    pub span: Span,

    /// Semantic nesting depth (0 = top-level definition)
    pub depth: usize,

    /// Semantic parent entity (e.g., class contains method, not AST parent-child)
    pub parent: Option<EntityId>,

    /// Semantic child entities
    pub children: Vec<EntityId>,

    /// Doc comment (extracted text)
    pub doc_comment: Option<String>,

    /// Modifiers (e.g., "pub", "public", "export", "static", "async")
    pub modifiers: Vec<String>,

    /// Element attributes (e.g., class, id, for HTML/Vue elements)
    pub attributes: HashMap<String, String>,

    /// Extension metadata for language-specific or user-defined attributes
    /// Allows storing additional information without modifying the core structure
    pub metadata: HashMap<String, String>,

    /// Standard library entity marker
    /// Set during parsing if this entity is a standard library type/function
    pub is_stdlib: bool,

    /// Standard library category (if is_stdlib is true)
    /// Uses StdlibCategory enum for type safety and semantic optimization
    pub stdlib_category: Option<StdlibCategory>,

    /// Entity subtype (e.g., "generator" for function.generator,
    /// "class" for style_selector.class, "media" for at-rule.media).
    /// Preserves subcategory information from capture names.
    pub subtype: Option<String>,
}

impl Entity {
    /// Create a new entity with basic info
    pub fn new(id: EntityId, kind: EntityKind, name: String, span: Span) -> Self {
        Self {
            id,
            kind,
            name,
            span,
            ..Default::default()
        }
    }

    /// Set the signature
    pub fn with_signature(mut self, signature: String) -> Self {
        self.signature = signature;
        self
    }

    /// Set the parameters
    pub fn with_parameters(mut self, params: Vec<(String, Option<String>)>) -> Self {
        self.parameters = params;
        self
    }

    /// Set the return type
    pub fn with_return_type(mut self, ret_type: Option<String>) -> Self {
        self.return_type = ret_type;
        self
    }

    /// Set the parent entity
    pub fn with_parent(mut self, parent: Option<EntityId>) -> Self {
        self.parent = parent;
        self
    }

    /// Set the depth
    pub fn with_depth(mut self, depth: usize) -> Self {
        self.depth = depth;
        self
    }

    /// Set the doc comment
    pub fn with_doc_comment(mut self, doc: Option<String>) -> Self {
        self.doc_comment = doc;
        self
    }

    /// Add a child entity
    pub fn add_child(&mut self, child_id: EntityId) {
        if !self.children.contains(&child_id) {
            self.children.push(child_id);
        }
    }

    /// Get the source code length
    pub fn source_len(&self) -> usize {
        self.span.len()
    }

    /// Check if this entity is top-level (depth == 0)
    pub fn is_top_level(&self) -> bool {
        self.depth == 0
    }

    /// Add a metadata entry
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Get a metadata value
    pub fn get_metadata(&self, key: &str) -> Option<&String> {
        self.metadata.get(key)
    }

    /// Set a metadata value
    pub fn set_metadata(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.metadata.insert(key.into(), value.into());
    }
}
