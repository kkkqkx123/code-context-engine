//! Relation-store reads shared by the tools.
//!
//! Two-stage epoch-view resolution ("own first, miss → parent") so inherited
//! generations stay readable while parent rows of overridden files stay
//! hidden.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use cce_storage_common::MetadataStorage;
use cce_storage_common::metadb::ChunkRecord;
pub use cce_storage_common::source::{SourceFileCache, read_source_lines_cached};
use cce_storage_metadb_sqlite::SqliteClient;
use std::sync::Arc;

use crate::epoch_view::EpochView;

/// Fetch chunk records by chunk IDs through the backend-neutral relation
/// contract, resolving the full epoch view.
pub async fn get_chunk_records(
    store: &Arc<SqliteClient>,
    chunk_ids: &[String],
    project_id: i64,
    view: &EpochView,
) -> Option<HashMap<String, ChunkRecord>> {
    if chunk_ids.is_empty() {
        return Some(HashMap::new());
    }

    match resolve_chunk_records(store, chunk_ids, project_id, view).await {
        Ok(records) => Some(records),
        Err(e) => {
            tracing::warn!("Failed to fetch chunks from relation store: {}", e);
            None
        }
    }
}

async fn resolve_chunk_records(
    store: &Arc<SqliteClient>,
    chunk_ids: &[String],
    project_id: i64,
    view: &EpochView,
) -> std::result::Result<HashMap<String, ChunkRecord>, cce_types::StorageError> {
    let own_records = store
        .chunks_by_ids(project_id, chunk_ids, &[view.epoch_value()])
        .await?;
    let mut records: HashMap<String, ChunkRecord> = own_records
        .into_iter()
        .map(|chunk| (chunk.chunk_id.clone(), chunk))
        .collect();

    let Some(parent_epoch) = view.parent_epoch() else {
        return Ok(records);
    };
    let missing: Vec<String> = chunk_ids
        .iter()
        .filter(|id| !records.contains_key(*id))
        .cloned()
        .collect();
    if missing.is_empty() {
        return Ok(records);
    }

    let excluded: Option<HashSet<&str>> = if view.excluded_files().is_empty() {
        None
    } else {
        Some(view.excluded_files().iter().map(String::as_str).collect())
    };
    let parent_records = store
        .chunks_by_ids(project_id, &missing, &[parent_epoch])
        .await?;
    for chunk in parent_records {
        if let Some(ref excluded) = excluded
            && excluded.contains(chunk.file_path.as_str())
        {
            continue;
        }
        records.entry(chunk.chunk_id.clone()).or_insert(chunk);
    }
    Ok(records)
}

/// Resolve a project root directory through the relation store.
///
/// Query contexts only know `project_id`; chunk file paths are stored relative
/// to the project root, so the root is recovered from the project registry.
/// Returns `None` when the relation handle is missing or the project row
/// carries no root path.
pub async fn resolve_project_root(store: &Arc<SqliteClient>, project_id: i64) -> Option<PathBuf> {
    store
        .project_record(project_id)
        .await
        .ok()?
        .map(|record| PathBuf::from(record.root_path))
}

/// Read source snippets for multiple chunks, grouped by file for efficiency.
///
/// Returns a map of chunk_id -> snippet text.
pub fn read_snippets_batch(
    project_root: Option<&std::path::Path>,
    chunks: &HashMap<String, ChunkRecord>,
) -> HashMap<String, String> {
    let mut snippets = HashMap::new();

    let mut file_groups: HashMap<String, Vec<(&String, &ChunkRecord)>> = HashMap::new();
    for (chunk_id, chunk) in chunks {
        file_groups
            .entry(chunk.file_path.clone())
            .or_default()
            .push((chunk_id, chunk));
    }

    for (file_path, group) in file_groups {
        let content =
            cce_storage_common::source::read_source_lines(project_root, &file_path, 0, u32::MAX);
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
