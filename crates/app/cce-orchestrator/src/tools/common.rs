//! Shared utilities for tool implementations
//!
//! Provides common patterns used across multiple tools to avoid code duplication.

use std::collections::HashMap;
use std::sync::Arc;

use cce_codegraph::index::{LayeredSnapshotIndex, SnapshotFileQueryOps};
use cce_storage_common::metadb::ChunkRecord;
use cce_storage_common::source;
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
        let content = source::read_source_lines(project_root, &file_path, 0, u32::MAX);
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

/// Resolve the project root directory through the relation contract.
///
/// Query contexts only know `project_id`; chunk file paths are stored
/// relative to the project root, so the root is recovered from the project
/// registry. Returns `None` when the relation handle is missing or the
/// project row carries no root path.
pub(crate) async fn resolve_project_root(
    relation: &Option<RelationStore>,
    project_id: i64,
) -> Option<std::path::PathBuf> {
    use cce_storage_common::RelationStorage;

    relation
        .as_ref()?
        .project_record(project_id)
        .await
        .ok()?
        .map(|record| std::path::PathBuf::from(record.root_path))
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
