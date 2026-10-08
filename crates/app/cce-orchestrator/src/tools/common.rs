//! Shared utilities for tool implementations
//!
//! Provides common patterns used across multiple tools to avoid code duplication.

use std::collections::HashMap;
use std::sync::Arc;

use cce_codegraph::index::{LayeredSnapshotIndex, SnapshotFileQueryOps};
use cce_storage_metadb_sqlite::source_reader;
use cce_storage_metadb_sqlite::types::ChunkRecord;
use cce_types::Entity;

use crate::index::vector_store::RelationStore;
use crate::tools::symbol_lookup_types::SymbolLookupError;

/// Read source snippets for multiple chunks, grouped by file for efficiency.
///
/// Returns a map of chunk_id -> snippet text.
pub(crate) fn read_snippets_batch(
    project_root: Option<&std::path::Path>,
    chunks: &HashMap<String, ChunkRecord>,
) -> HashMap<String, String> {
    let mut snippets = HashMap::new();

    // Group chunks by file path to minimize file reads
    let mut file_groups: HashMap<String, Vec<(&String, &ChunkRecord)>> = HashMap::new();
    for (chunk_id, chunk) in chunks {
        file_groups
            .entry(chunk.file_path.clone())
            .or_default()
            .push((chunk_id, chunk));
    }

    for (file_path, group) in file_groups {
        // Read the full file once, then extract line ranges
        let content = source_reader::read_source_lines(project_root, &file_path, 0, u32::MAX);
        let lines: Vec<&str> = content.lines().collect();

        for (chunk_id, chunk) in group {
            let start = chunk.start_line.max(0) as usize;
            let end = (chunk.end_line.max(0) as usize + 1).min(lines.len());
            let snippet = if start < end && start < lines.len() {
                lines[start..end].join("\n")
            } else {
                String::new()
            };
            snippets.insert(chunk_id.clone(), snippet);
        }
    }

    snippets
}

/// Get a read connection from the local relation branch with consistent error
/// handling.
///
/// The single fenced downcast for the synchronous symbol tools: snippet and
/// mapping reads stay on the embedded branch until the contract carries the
/// operations they need.
pub(crate) fn get_read_connection(
    relation: &Option<RelationStore>,
) -> Result<parking_lot::MutexGuard<'_, rusqlite::Connection>, String> {
    let client = relation
        .as_ref()
        .and_then(RelationStore::as_local)
        .ok_or("local relation database not configured")?;
    client
        .read_connection()
        .map_err(|e| format!("Failed to get read connection: {e}"))
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
