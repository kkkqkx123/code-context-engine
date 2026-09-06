//! Indexer phase relation representation
//!
//! This module defines fully resolved relation types used in the Indexer phase.
//! At this stage, targets have been resolved to EntityIds or marked as external.

use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize};
use serde::{Deserialize as SerdeDeserialize, Serialize as SerdeSerialize};

use super::super::Span;
use super::super::entity::{EntityId, EntityKind};
use super::CallContext;
use super::classification::{ExternalCallType, RelationType};

/// Symbol location snapshot kept together with a resolved relation.
///
/// This is a lightweight, serializable copy of the symbol location data that
/// is sufficient for presentation and symbol lookup scenarios.
#[derive(Debug, Clone, SerdeSerialize, SerdeDeserialize, Archive, RkyvDeserialize, Serialize)]
pub struct RelationSymbolLocation {
    /// File path that defines the symbol
    pub file_path: String,

    /// Package path, if available
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_path: Option<String>,

    /// Module path, if available
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module_path: Option<String>,

    /// Definition span
    pub span: Span,
}

/// Resolved symbol snapshot attached to a relation.
///
/// This preserves the symbol-level chain even when the relation is later
/// linked to a different entity ID or when entity resolution fails.
#[derive(Debug, Clone, SerdeSerialize, SerdeDeserialize, Archive, RkyvDeserialize, Serialize)]
pub struct RelationSymbolRecord {
    /// Symbol ID from the symbol table
    pub symbol_id: u64,

    /// Entity ID when the symbol can be associated with an entity
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<EntityId>,

    /// Symbol name
    pub name: String,

    /// Symbol kind
    pub kind: EntityKind,

    /// Symbol location snapshot
    pub location: RelationSymbolLocation,

    /// Optional source module for cross-module references
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_module: Option<String>,
}

/// Resolved relation (after symbol resolution)
///
/// Represents a fully resolved relation where the target has been
/// linked to an EntityId (if found in project) or marked as external.
///
/// This is the output of the symbol resolution process.
#[derive(Debug, Clone, SerdeSerialize, SerdeDeserialize, Archive, RkyvDeserialize, Serialize)]
pub struct ResolvedRelation {
    /// Source entity (caller)
    pub caller: EntityId,
    /// Target entity ID (Some if found in project, None if external)
    pub callee_id: Option<EntityId>,
    /// Target name (kept for debugging and external references)
    pub callee_name: String,
    /// Relation type
    pub relation_type: RelationType,
    /// Source code span
    pub span: Span,
    /// Whether this is an external reference
    pub is_external: bool,
    /// External call type (only relevant when is_external is true)
    ///
    /// This field provides more granular classification of external references:
    /// - StandardLibrary: Built-in or standard library entities
    /// - ExternalLibrary: Third-party packages from package managers
    /// - DevDependency: Development dependencies
    /// - LocalDependency: Local path dependencies
    /// - Unknown: Classification failed or not supported
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_type: Option<ExternalCallType>,

    /// Resolved symbol snapshot, preserved for symbol-level recall and presentation
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub callee_symbol: Option<RelationSymbolRecord>,

    /// Standard library category (if this is a stdlib relation)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdlib_category: Option<super::super::stdlib_category::StdlibCategory>,

    /// Owner type for method/constructor calls (if resolved)
    ///
    /// This field provides the qualified type name that owns the callee method/constructor.
    /// For example, in `obj.method()`, this would be the type of `obj`.
    /// For direct function calls, this is `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_type: Option<String>,

    /// Call context providing additional information about how the call is made
    ///
    /// This field enables more precise call graph analysis by distinguishing between
    /// different call patterns (direct, instance method, static method, constructor).
    #[serde(default)]
    pub call_context: CallContext,

    /// Dispatched overload signature for the call site.
    ///
    /// Populated when overload disambiguation selects among multiple
    /// candidates (`name(params) -> return`); `None` keeps single-candidate
    /// and unresolved behavior unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overload_signature: Option<String>,
}

impl ResolvedRelation {
    /// Get the callee EntityId if available
    pub fn callee(&self) -> Option<EntityId> {
        self.callee_id
    }

    /// Get the caller EntityId
    pub fn caller(&self) -> EntityId {
        self.caller
    }

    pub fn is_internal(&self) -> bool {
        self.callee_id.is_some()
    }

    pub fn is_external_inferred(&self) -> bool {
        self.callee_id.is_none()
    }

    pub fn is_consistent(&self) -> bool {
        self.is_external == self.is_external_inferred()
    }

    /// Get the callee name from RelationIndex if available
    ///
    /// This method provides a way to get the callee name without storing it redundantly.
    /// For internal relations (callee_id is Some), the name should be fetched from the Entity.
    /// For external relations (callee_id is None), the stored `callee_name` is used.
    ///
    /// Get the callee name, resolving from a lookup function when needed.
    ///
    /// # Type Parameters
    ///
    /// * `F` - A lookup function that takes an `EntityId` and returns an optional name
    pub fn callee_name_from_index<F>(&self, lookup: F) -> Option<String>
    where
        F: Fn(EntityId) -> Option<String>,
    {
        if let Some(callee_id) = self.callee_id {
            lookup(callee_id)
        } else {
            Some(self.callee_name.clone())
        }
    }

    /// Get the preserved symbol snapshot, if available.
    pub fn callee_symbol(&self) -> Option<&RelationSymbolRecord> {
        self.callee_symbol.as_ref()
    }

    /// Check if this is a standard library call
    ///
    /// Determines if the relation targets a standard library entity by checking
    /// the external_type field.
    ///
    /// # Returns
    ///
    /// `true` if this is a standard library call, `false` otherwise
    pub fn is_stdlib(&self) -> bool {
        matches!(
            self.external_type,
            Some(ExternalCallType::StandardLibrary { .. })
        )
    }

    /// Get the standard library category if this is a stdlib relation
    ///
    /// Returns the pre-computed stdlib category that was determined during
    /// parsing. This avoids redundant detection in downstream consumers.
    ///
    /// Note: This field is populated during relation extraction based on both
    /// the relation type and target name. For relations detected as stdlib
    /// before resolution, this field contains the computed category.
    pub fn get_stdlib_category(&self) -> Option<super::super::stdlib_category::StdlibCategory> {
        self.stdlib_category
    }

    /// Check if stdlib information is available
    ///
    /// Returns true if either stdlib_category is set or external_type indicates stdlib.
    /// This is useful for detecting whether stdlib detection was performed.
    pub fn has_stdlib_info(&self) -> bool {
        self.stdlib_category.is_some() || self.is_stdlib()
    }
}
