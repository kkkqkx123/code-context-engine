//! Shared utilities for tool implementations
//!
//! Provides common patterns used across multiple tools to avoid code duplication.

use std::sync::Arc;

use cce_codegraph::index::{LayeredSnapshotIndex, SnapshotFileQueryOps};
use cce_types::Entity;

use cce_storage_metadb::RelationStore;

pub use crate::store::{
    get_chunk_records, read_snippets_batch, resolve_project_root as resolve_store_root,
};
use crate::symbol_lookup_types::SymbolLookupError;

/// Resolve the project root directory for a tool's optional relation handle.
///
/// Returns `None` when the relation handle is missing or the project row
/// carries no root path.
pub(crate) async fn resolve_project_root(
    relation: &Option<RelationStore>,
    project_id: i64,
) -> Option<std::path::PathBuf> {
    resolve_store_root(relation.as_ref()?, project_id).await
}

/// Get entities by file path from the snapshot index.
///
/// Shared by GotoDefinition and FindReferences tools.
pub(crate) fn get_entities_by_file(
    index: &Arc<LayeredSnapshotIndex>,
    path: &str,
) -> Result<Vec<Entity>, SymbolLookupError> {
    let entities: Vec<Entity> = index
        .get_entities_by_file(path)
        .into_iter()
        .map(|(_, entity)| entity)
        .collect();

    if entities.is_empty() {
        Err(SymbolLookupError::FileNotFound(path.to_string()))
    } else {
        Ok(entities)
    }
}
