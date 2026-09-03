//! Core relation types for semantic relationship extraction
//!
//! This module provides types for semantic relationships:
//!
//! **Classification System** (global):
//! - RelationType: 35 relation types across 5 domains
//! - RelationLevel: File-level vs entity-level relations
//! - ExternalCallType: Classification of external references
//!
//! **Parser Output**:
//! - Relation: Relations as extracted from source code
//! - RelationTarget: Relation destination (Unresolved/Resolved)
//!
//! **Resolved Relations** (after symbol resolution):
//! - ResolvedRelation: Fully resolved with EntityId or marked external
//! - RelationSymbolRecord: Symbol snapshot preservation
//! - RelationSymbolLocation: Symbol location information
//!
//! **Persistent Resolution** (stable, cross-session identification):
//! - VirtualSymbolId: Source-code-level stable symbol identification
//! - VirtualRelation: Relations using VirtualSymbolId
//! - RelationCapture: Persistent relation records
//! - RelationVerificationStatus: Symbol verification tracking

pub mod canonical;
pub mod classification;
pub mod resolution;
pub mod resolved;
pub mod snapshot_store;
pub mod target;

pub use canonical::{
    AddedEntity, CanonicalDependency, CanonicalEntity, CanonicalExport, CanonicalFile,
    CanonicalRelation, CanonicalRelationSnapshot, CanonicalRelationTarget, DependencyDiff,
    ExportDiff, FileRelationDiff, FingerprintComponents, ImportDiff, RELATION_PARSER_VERSION,
    RELATION_PATH_NORMALIZATION_VERSION, RELATION_RESOLVER_VERSION,
    RELATION_SNAPSHOT_SCHEMA_VERSION, SYMBOL_KEY_CONFLICT_SAMPLE_CAP, SnapshotBuildMetadata,
    SnapshotDelta, StableSymbolId, StableSymbolKey, SymbolKeyConflictRecord, UnresolvedReason,
    fingerprint_from_components, normalize_project_path,
};
pub use classification::{ExternalCallType, RelationLevel, RelationType};
pub use resolution::{
    RelationCapture, RelationVerificationStatus, VirtualRelation, VirtualSymbolId,
};
pub use resolved::{RelationSymbolLocation, RelationSymbolRecord, ResolvedRelation};
pub use snapshot_store::{RelationSnapshotManifest, RelationSnapshotState, RelationSnapshotStore};
pub use target::RelationTarget;

/// Call context for enhanced call graph information
///
/// This enum provides additional context about how a function is called,
/// enabling more precise call graph analysis and type-aware resolution.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    SerdeSerialize,
    SerdeDeserialize,
    Archive,
    RkyvDeserialize,
    Serialize,
    Default,
)]
pub enum CallContext {
    /// Direct function call (no receiver/owner context)
    #[default]
    Direct,
    /// Instance method call (obj.method())
    InstanceMethod {
        /// The type of the receiver object
        receiver_type: String,
    },
    /// Static method call (Class.method())
    StaticMethod {
        /// The type that owns the static method
        owner_type: String,
    },
    /// Constructor call (new Class(), Class())
    Constructor {
        /// The type being constructed
        owner_type: String,
    },
}

use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize};
use serde::{Deserialize as SerdeDeserialize, Serialize as SerdeSerialize};

use super::Span;
use super::entity::EntityId;

/// Unified relation type (Parser output)
///
/// Represents a semantic relationship between two entities as extracted
/// from source code. Uses `caller` terminology for generality across all
/// relation types (calls, dependencies, structural, references, etc.).
///
/// At this stage:
/// - Target may be unresolved (RelationTarget::Unresolved)
/// - stdlib_category is populated if detected during parsing
/// - Cross-reference resolution happens downstream in symbol resolution
#[derive(Debug, Clone, SerdeSerialize, SerdeDeserialize, Archive, RkyvDeserialize, Serialize)]
pub struct Relation {
    /// Relation level (file or entity)
    pub caller_level: RelationLevel,

    /// Caller ID (file ID or entity ID depending on caller_level)
    pub caller_id: i64,

    /// Target of the relation
    pub dst: RelationTarget,

    /// Relation type
    pub relation_type: RelationType,

    /// Source code span
    pub span: Span,

    /// Standard library category (if this is a stdlib call)
    ///
    /// Set during Parser phase when a call relation is identified as a stdlib call.
    /// This eliminates the need for duplicate detection in the Grouper phase.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdlib_category: Option<super::stdlib_category::StdlibCategory>,

    /// Pre-computed argument count at the call site.
    ///
    /// Populated during relation extraction (when the AST tree is available)
    /// and used downstream in overload disambiguation. Storing it here avoids
    /// re-scanning source text in `local_call_resolver`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub argument_count: Option<usize>,
}

impl Relation {
    /// Create a file-level relation
    pub fn file_relation(
        caller_id: i64,
        dst: RelationTarget,
        relation_type: RelationType,
        span: Span,
    ) -> Self {
        Self {
            caller_level: RelationLevel::File,
            caller_id,
            dst,
            relation_type,
            span,
            stdlib_category: None,
            argument_count: None,
        }
    }

    /// Create an entity-level relation
    pub fn entity_relation(
        caller_id: i64,
        dst: RelationTarget,
        relation_type: RelationType,
        span: Span,
    ) -> Self {
        Self {
            caller_level: RelationLevel::Entity,
            caller_id,
            dst,
            relation_type,
            span,
            stdlib_category: None,
            argument_count: None,
        }
    }

    /// Create a new relation with unresolved target (backward compatibility)
    /// Note: This creates an entity-level relation by default
    pub fn new(
        src: EntityId,
        dst: RelationTarget,
        relation_type: RelationType,
        span: Span,
    ) -> Self {
        Self {
            caller_level: RelationLevel::Entity,
            caller_id: src.0 as i64,
            dst,
            relation_type,
            span,
            stdlib_category: None,
            argument_count: None,
        }
    }

    /// Create a direct function call relation
    pub fn direct_call(src: EntityId, dst_name: String, span: Span) -> Self {
        Self::entity_relation(
            src.0 as i64,
            RelationTarget::unresolved(dst_name),
            RelationType::DirectCall,
            span,
        )
    }

    /// Create an instance method call relation
    pub fn instance_method_call(src: EntityId, dst_name: String, span: Span) -> Self {
        Self::entity_relation(
            src.0 as i64,
            RelationTarget::unresolved(dst_name),
            RelationType::InstanceMethodCall,
            span,
        )
    }

    /// Create a static method call relation
    pub fn static_method_call(src: EntityId, dst_name: String, span: Span) -> Self {
        Self::entity_relation(
            src.0 as i64,
            RelationTarget::unresolved(dst_name),
            RelationType::StaticMethodCall,
            span,
        )
    }

    /// Create a constructor call relation
    pub fn constructor_call(src: EntityId, dst_name: String, span: Span) -> Self {
        Self::entity_relation(
            src.0 as i64,
            RelationTarget::unresolved(dst_name),
            RelationType::ConstructorCall,
            span,
        )
    }

    /// Create a generic/template call relation
    pub fn generic_call(src: EntityId, dst_name: String, span: Span) -> Self {
        Self::entity_relation(
            src.0 as i64,
            RelationTarget::unresolved(dst_name),
            RelationType::GenericCall,
            span,
        )
    }

    /// Create a macro call relation
    pub fn macro_call(src: EntityId, dst_name: String, span: Span) -> Self {
        Self::entity_relation(
            src.0 as i64,
            RelationTarget::unresolved(dst_name),
            RelationType::MacroCall,
            span,
        )
    }

    /// Create an import relation (standard import)
    pub fn import(src: EntityId, dst_name: String, span: Span) -> Self {
        Self::entity_relation(
            src.0 as i64,
            RelationTarget::unresolved(dst_name),
            RelationType::ImportStandard,
            span,
        )
    }

    /// Create a use relation (Rust use statement)
    pub fn use_statement(src: EntityId, dst_name: String, span: Span) -> Self {
        Self::entity_relation(
            src.0 as i64,
            RelationTarget::unresolved(dst_name),
            RelationType::Use,
            span,
        )
    }

    /// Create an include relation (C/C++ include)
    pub fn include(src: EntityId, dst_name: String, _is_system: bool, span: Span) -> Self {
        Self::entity_relation(
            src.0 as i64,
            RelationTarget::unresolved(dst_name),
            RelationType::IncludeLocal,
            span,
        )
    }

    /// Create an inheritance relation
    pub fn inheritance(src: EntityId, dst_name: String, span: Span) -> Self {
        Self::entity_relation(
            src.0 as i64,
            RelationTarget::unresolved(dst_name),
            RelationType::Inheritance,
            span,
        )
    }

    /// Create an implementation relation
    pub fn implementation(src: EntityId, dst_name: String, span: Span) -> Self {
        Self::entity_relation(
            src.0 as i64,
            RelationTarget::unresolved(dst_name),
            RelationType::Implementation,
            span,
        )
    }

    /// Create a trait bound relation
    pub fn trait_bound(src: EntityId, dst_name: String, span: Span) -> Self {
        Self::entity_relation(
            src.0 as i64,
            RelationTarget::unresolved(dst_name),
            RelationType::TraitBound,
            span,
        )
    }

    /// Create a type reference relation
    pub fn type_reference(src: EntityId, dst_name: String, span: Span) -> Self {
        Self::entity_relation(
            src.0 as i64,
            RelationTarget::unresolved(dst_name),
            RelationType::TypeReference,
            span,
        )
    }

    /// Create a field access relation
    pub fn field_access(src: EntityId, dst_name: String, span: Span) -> Self {
        Self::entity_relation(
            src.0 as i64,
            RelationTarget::unresolved(dst_name),
            RelationType::FieldAccess,
            span,
        )
    }

    /// Create an element contains relation
    pub fn element_contains(src: EntityId, dst_name: String, span: Span) -> Self {
        Self::entity_relation(
            src.0 as i64,
            RelationTarget::unresolved(dst_name),
            RelationType::ElementContains,
            span,
        )
    }

    /// Create a template reference relation
    pub fn template_reference(src: EntityId, dst_name: String, span: Span) -> Self {
        Self::entity_relation(
            src.0 as i64,
            RelationTarget::unresolved(dst_name),
            RelationType::TemplateReference,
            span,
        )
    }

    /// Create a parameter binding relation (props, attributes)
    pub fn parameter_binding(src: EntityId, dst_name: String, span: Span) -> Self {
        Self::entity_relation(
            src.0 as i64,
            RelationTarget::unresolved(dst_name),
            RelationType::ParameterBinding,
            span,
        )
    }

    /// Create an event callback relation
    pub fn event_callback(src: EntityId, dst_name: String, span: Span) -> Self {
        Self::entity_relation(
            src.0 as i64,
            RelationTarget::unresolved(dst_name),
            RelationType::EventCallback,
            span,
        )
    }

    /// Resolve the target of this relation
    ///
    /// Converts an unresolved target to a resolved one with the given EntityId.
    /// If the target is already resolved, it will be updated with the new ID.
    pub fn resolve(mut self, dst_id: Option<EntityId>, _is_external: bool) -> Self {
        let name = self.dst.name().to_string();
        self.dst = RelationTarget::resolved(dst_id, name);
        self
    }

    /// Check if this relation is resolved
    pub fn is_resolved(&self) -> bool {
        self.dst.is_resolved()
    }

    /// Get the caller ID
    pub fn caller_id(&self) -> i64 {
        self.caller_id
    }

    /// Get the caller ID as EntityId (for backward compatibility)
    pub fn src_id(&self) -> EntityId {
        EntityId(self.caller_id as u64)
    }

    /// Get the target entity ID if resolved
    pub fn dst_id(&self) -> Option<EntityId> {
        self.dst.id()
    }

    /// Get the target name
    pub fn dst_name(&self) -> &str {
        self.dst.name()
    }

    /// Set the standard library category for this relation
    ///
    /// This is populated during parsing when a call is identified as a stdlib call.
    pub fn with_stdlib_category(
        mut self,
        category: Option<super::stdlib_category::StdlibCategory>,
    ) -> Self {
        self.stdlib_category = category;
        self
    }

    /// Check if this is a standard library call
    pub fn is_stdlib_call(&self) -> bool {
        self.stdlib_category.is_some()
    }

    /// Set the pre-computed argument count for this call relation.
    pub fn with_argument_count(mut self, count: Option<usize>) -> Self {
        self.argument_count = count;
        self
    }
}
