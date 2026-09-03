//! Read-only query operations shared by every queryable relation index type.
//!
//! This is the query-side counterpart of the write-side extension traits
//! (`EntityIndexOps`, `RelationQueryOps`, ...). It contains only the pure
//! read methods of those traits and is implemented on all three index
//! surfaces:
//!
//! - [`RelationIndex`]: the mutable build-time index (delegates to the
//!   write-side traits' maps);
//! - [`RelationSnapshotIndex`]: the immutable published snapshot — every
//!   lookup is a direct `&self` read of the shared maps, zero-copy;
//! - [`LayeredSnapshotIndex`]: base + delta — each method merges the delta
//!   at read time (removed entries hidden, added entries visible) without
//!   materializing a merged index.
//!
//! Write methods (`add_*`, `remove_*`, ...) deliberately stay on the
//! write-side traits; the snapshot types never expose them. This is enforced
//! by the type system (snapshots hold only `Arc` maps) rather than by
//! convention.

use cce_types::{
    Entity, EntityId, ExternalCallType, FileInfo, ImportTable, RelationType, ResolvedRelation,
};

use std::collections::HashMap;

use super::super::types::ExportInfo;
use super::core::SymbolKey;

use crate::error::IndexError;

/// Read-only entity lookups.
pub trait SnapshotEntityQueryOps {
    /// Get a function entity by EntityId (owned clone).
    fn get_function_by_entity_id(&self, entity_id: EntityId) -> Option<Entity>;

    /// Get function IDs by name.
    fn get_function_ids_by_name(&self, name: &str) -> Vec<EntityId>;

    /// Check if a function exists.
    fn contains_function(&self, entity_id: EntityId) -> bool;

    /// Get total number of functions.
    fn function_count(&self) -> usize;

    /// Get file path for an entity.
    fn get_file_path_by_entity(&self, entity_id: EntityId) -> Option<String>;

    /// Get entities overlapping a line range in a file.
    fn get_entities_in_line_range(
        &self,
        file_path: &str,
        start_line: usize,
        end_line: usize,
    ) -> Vec<EntityId>;
}

/// Read-only relation lookups (forward/reverse indexes).
pub trait SnapshotRelationQueryOps {
    /// Get resolved relations by caller EntityId (owned clone).
    fn get_resolved_relations_by_caller(
        &self,
        caller_id: EntityId,
    ) -> Option<Vec<ResolvedRelation>>;

    /// Get resolved relations by caller with validation.
    fn get_resolved_relations_by_caller_checked(
        &self,
        caller_id: EntityId,
    ) -> Result<Vec<ResolvedRelation>, IndexError>;

    /// Get callers by callee EntityId (uses the reverse index).
    fn get_callers_by_callee_entity(&self, callee_id: EntityId) -> Vec<EntityId>;

    /// Get callers by callee EntityId with validation.
    fn get_callers_by_callee_entity_checked(
        &self,
        callee_id: EntityId,
    ) -> Result<Vec<EntityId>, IndexError>;

    /// Get callers by callee EntityId and relation type.
    fn get_callers_by_callee_and_type(
        &self,
        callee_id: EntityId,
        relation_type: RelationType,
    ) -> Vec<EntityId>;

    /// Get relations targeting a specific entity.
    fn get_relations_to_entity(&self, callee_id: EntityId) -> Vec<ResolvedRelation>;

    /// Get relations targeting a specific entity by type.
    fn get_relations_to_entity_by_type(
        &self,
        callee_id: EntityId,
        relation_type: RelationType,
    ) -> Vec<ResolvedRelation>;

    /// Get relations from a specific entity by type.
    fn get_relations_from_entity_by_type(
        &self,
        caller_id: EntityId,
        relation_type: RelationType,
    ) -> Vec<ResolvedRelation>;

    /// Get total number of resolved relations.
    fn resolved_relation_count(&self) -> usize;

    /// Get total number of call relations.
    fn call_count(&self) -> usize;

    /// Get all resolved relations matching a given external call classification.
    fn get_relations_by_classification(
        &self,
        classification: &ExternalCallType,
    ) -> Vec<ResolvedRelation>;

    /// Get counts of resolved relations grouped by external call classification.
    fn get_classification_stats(&self) -> HashMap<ExternalCallType, usize>;
}

/// Read-only hierarchy queries (inheritance / implementation / trait bounds).
pub trait SnapshotHierarchyQueryOps {
    /// Get derived classes (classes that extend this class).
    fn get_derived_classes(&self, class_id: EntityId) -> Vec<EntityId>;

    /// Get implementing classes (classes that implement this interface).
    fn get_implementing_classes(&self, interface_id: EntityId) -> Vec<EntityId>;

    /// Get types with this trait bound (for Rust trait bounds).
    fn get_types_with_trait_bound(&self, trait_id: EntityId) -> Vec<EntityId>;

    /// Get base classes (classes this class extends).
    fn get_base_classes(&self, class_id: EntityId) -> Vec<EntityId>;

    /// Get implemented interfaces.
    fn get_implemented_interfaces(&self, class_id: EntityId) -> Vec<EntityId>;
}

/// Read-only frontend/markup queries.
pub trait SnapshotFrontendQueryOps {
    /// Get child elements (via ElementContains relation).
    fn get_child_elements(&self, parent_id: EntityId) -> Vec<EntityId>;

    /// Get parent element (via reverse ElementContains relation).
    fn get_parent_element(&self, child_id: EntityId) -> Vec<EntityId>;

    /// Get event handlers bound to an element/component.
    fn get_event_handlers(&self, element_id: EntityId) -> Vec<ResolvedRelation>;

    /// Get elements that use a specific event handler.
    fn get_elements_by_handler(&self, handler_id: EntityId) -> Vec<EntityId>;

    /// Get parameter bindings (props) of a component.
    fn get_parameter_bindings(&self, component_id: EntityId) -> Vec<ResolvedRelation>;

    /// Get template references (ref/bind:this) of an element.
    fn get_template_references(&self, element_id: EntityId) -> Vec<ResolvedRelation>;

    /// Get components/elements that reference a specific entity via template reference.
    fn get_elements_by_template_ref(&self, target_id: EntityId) -> Vec<EntityId>;
}

/// Read-only file / import / export lookups and file-level aggregations.
pub trait SnapshotFileQueryOps {
    /// Get file info by ID.
    fn get_file(&self, file_id: &str) -> Option<FileInfo>;

    /// Check if a file exists.
    fn contains_file(&self, file_id: &str) -> bool;

    /// Get total number of files.
    fn file_count(&self) -> usize;

    /// Get import table by file ID.
    fn get_import_table(&self, file_id: &str) -> Option<ImportTable>;

    /// Check if a file has imports.
    fn has_imports(&self, file_id: &str) -> bool;

    /// Get total number of import tables.
    fn import_count(&self) -> usize;

    /// Get exports by file ID.
    fn get_exports(&self, file_id: &str) -> Option<Vec<ExportInfo>>;

    /// Find export by function name in a file.
    fn find_export_by_name(&self, file_id: &str, function_name: &str) -> Option<ExportInfo>;

    /// Get entity IDs belonging to a file.
    fn get_entity_ids_by_file(&self, file_id: &str) -> Vec<EntityId>;

    /// Get all entities belonging to a file.
    fn get_entities_by_file(&self, file_id: &str) -> Vec<(EntityId, Entity)>;

    /// Get all resolved relations belonging to a file.
    fn get_resolved_relations_by_file(
        &self,
        file_id: &str,
    ) -> Vec<(EntityId, Vec<ResolvedRelation>)>;
}

/// Read-only stable-symbol lookups.
pub trait SnapshotSymbolQueryOps {
    /// Look up EntityId by SymbolKey.
    fn get_entity_id_by_symbol_key(&self, key: &SymbolKey) -> Option<EntityId>;

    /// Look up EntityId by stable symbol ID.
    fn get_entity_id_by_stable_symbol_id(&self, stable_id: &str) -> Option<EntityId>;

    /// Look up SymbolKey by EntityId.
    fn get_symbol_key_by_entity_id(&self, entity_id: EntityId) -> Option<SymbolKey>;

    /// Snapshot all registered stable symbol keys.
    fn stable_symbol_keys(&self) -> Vec<SymbolKey>;
}

/// Transitive file dependency queries.
pub trait TransitiveFileQueryOps {
    /// Get all transitive dependents (files that depend on `file_path` directly or indirectly).
    fn transitive_dependents_of(&self, file_path: &str) -> Vec<String>;
    /// Get all transitive dependencies (files that `file_path` depends on directly or indirectly).
    fn transitive_dependencies_of(&self, file_path: &str) -> Vec<String>;
}

/// Marker supertrait: a queryable index exposes every read-only operation.
pub trait SnapshotQueryIndex:
    SnapshotEntityQueryOps
    + SnapshotRelationQueryOps
    + SnapshotHierarchyQueryOps
    + SnapshotFrontendQueryOps
    + SnapshotFileQueryOps
    + SnapshotSymbolQueryOps
{
}

impl<T> SnapshotQueryIndex for T where
    T: SnapshotEntityQueryOps
        + SnapshotRelationQueryOps
        + SnapshotHierarchyQueryOps
        + SnapshotFrontendQueryOps
        + SnapshotFileQueryOps
        + SnapshotSymbolQueryOps
{
}

mod layered_impls;
mod mutable_impls;
mod snapshot_impls;
#[cfg(test)]
mod tests;
mod unified_impls;
