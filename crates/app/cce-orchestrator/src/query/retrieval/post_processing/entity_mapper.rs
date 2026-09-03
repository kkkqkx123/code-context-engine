//! Entity mapping utilities
//!
//! Provides chunk-to-entity mapping and chunk record lookups,
//! eliminating SQLite query code duplication.

use std::collections::{HashMap, HashSet};

use rusqlite::Connection;

use crate::query::error::Result;
use crate::query::filter::QueryFilter;
use crate::query::types::SearchResult;
use cce_storage_sqlite::repo::ChunkRepository;
use cce_storage_sqlite::types::ChunkRecord;

/// Fetch chunk records by chunk IDs, resolving the full epoch view.
///
/// Two-stage resolution ("own first, miss → parent"): chunk IDs missing from
/// the own generation are re-queried against the inherited parent epoch, and
/// parent hits belonging to overridden files (replaced/deleted) are dropped so
/// only the visible view is returned.
///
/// Returns a map of chunk_id -> ChunkRecord.
pub(crate) fn get_chunk_records(
    conn: &Connection,
    chunk_ids: &[String],
    project_id: i64,
    query_filter: &QueryFilter,
) -> Result<Option<HashMap<String, ChunkRecord>>> {
    if chunk_ids.is_empty() {
        return Ok(Some(HashMap::new()));
    }

    match resolve_chunk_records(conn, chunk_ids, project_id, query_filter) {
        Ok(records) => Ok(Some(records)),
        Err(e) => {
            tracing::warn!("Failed to fetch chunks from SQLite: {}", e);
            Ok(None)
        }
    }
}

fn resolve_chunk_records(
    conn: &Connection,
    chunk_ids: &[String],
    project_id: i64,
    query_filter: &QueryFilter,
) -> std::result::Result<HashMap<String, ChunkRecord>, cce_types::StorageError> {
    let own_records = ChunkRepository::get_by_chunk_ids(
        conn,
        chunk_ids,
        project_id,
        Some(query_filter.epoch_value()),
    )?;
    let mut records: HashMap<String, ChunkRecord> = own_records
        .into_iter()
        .map(|chunk| (chunk.chunk_id.clone(), chunk))
        .collect();

    let Some(parent_epoch) = query_filter.parent_epoch() else {
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

    let excluded: Option<HashSet<&str>> = if query_filter.excluded_files().is_empty() {
        None
    } else {
        Some(
            query_filter
                .excluded_files()
                .iter()
                .map(String::as_str)
                .collect(),
        )
    };
    let parent_records =
        ChunkRepository::get_by_chunk_ids(conn, &missing, project_id, Some(parent_epoch))?;
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

/// Enrich a single search result with chunk record data.
///
/// Fills in snippet, content, start_line, end_line, kind, and name fields
/// from the chunk record if available. Also falls back to SQLite entity IDs
/// when the result's entity_ids is not already populated. Snippet/content are
/// lazy-loaded from the source file on disk (chunks no longer persist raw
/// code), so `project_root` must be resolved by the caller.
pub(crate) fn enrich_from_chunk(
    result: &mut SearchResult,
    chunk_records: &HashMap<String, ChunkRecord>,
    project_root: Option<&std::path::Path>,
) {
    if let Some(chunk) = chunk_records.get(&result.id) {
        let source_text = cce_storage_sqlite::source_reader::read_source_lines(
            project_root,
            &chunk.file_path,
            chunk.start_line.max(0) as u32,
            chunk.end_line.max(0) as u32,
        );
        result.snippet = Some(source_text.clone());
        result.content = source_text;
        result.file_path = chunk.file_path.clone();
        result.start_line = chunk.start_line as u32;
        result.end_line = chunk.end_line as u32;
        result.kind = chunk.chunk_type.clone();

        let entity_names: Vec<String> =
            serde_json::from_str(&chunk.entity_names).unwrap_or_default();
        if !entity_names.is_empty() {
            // After hybrid expansion a result carries at most one entity; name
            // it with that entity's own display name when available. Otherwise
            // fall back to the first named entry (the group title).
            result.name = choose_entity_name(&chunk.entity_ids, &entity_names, &result.entity_ids);
        }

        let sqlite_entity_ids: Vec<i64> =
            serde_json::from_str(&chunk.entity_ids).unwrap_or_default();

        if result.entity_ids.is_empty() {
            // Fallback: populate entity_ids from SQLite if the payload/BM25
            // index didn't carry them.
            if !sqlite_entity_ids.is_empty() {
                result.entity_ids = sqlite_entity_ids
                    .iter()
                    .map(|&id| cce_types::EntityId(id as u64))
                    .collect();
            }
        }
    }
}

/// Pick the display name for an enriched hit.
///
/// `stored_ids`/`names` are positionally aligned lists persisted with the
/// chunk. When the hit represents exactly one entity (the post-expansion
/// contract), the matching entry wins; entries are otherwise scanned in order
/// and empty strings (unknown names) are skipped.
fn choose_entity_name(
    stored_ids: &str,
    names: &[String],
    hit_entity_ids: &[cce_types::EntityId],
) -> String {
    let stored_ids: Vec<i64> = serde_json::from_str(stored_ids).unwrap_or_default();
    if let [only] = hit_entity_ids {
        if let Some(index) = stored_ids.iter().position(|&id| id as u64 == only.0) {
            if let Some(name) = names.get(index).filter(|name| !name.is_empty()) {
                return name.clone();
            }
        }
    }
    names
        .iter()
        .find(|name| !name.is_empty())
        .cloned()
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_storage_sqlite::{ChunkRepository, SqliteClient};
    use cce_types::EntityId;

    fn chunk_record(entity_ids: &[i64]) -> ChunkRecord {
        ChunkRecord::new(
            "chunk_x".to_string(),
            "src/main.rs".to_string(),
            "code".to_string(),
            1,
            2,
        )
        .with_entity_ids(entity_ids)
    }

    /// Seed chunks in two generations:
    /// - `chunk_own` exists only in the own epoch (5)
    /// - `chunk_parent` exists only in the parent epoch (4)
    /// - `chunk_excluded` exists only in the parent epoch but belongs to an
    ///   overridden file, so it must never surface
    fn seed_two_generations() -> SqliteClient {
        let client = SqliteClient::in_memory().expect("in-memory database");
        let chunk = |id: &str, path: &str, epoch: i64| {
            ChunkRecord::new(id.to_string(), path.to_string(), "code".to_string(), 1, 2)
                .with_epoch(epoch)
                .with_project_id(1)
        };
        client
            .with_transaction(|tx| {
                tx.execute(
                    "INSERT INTO projects (id, name, root_path, config_file_path, created_at, updated_at)
                     VALUES (1, 'test', '/tmp/test', '.cce/config.json', 1, 1)",
                    [],
                )
                .map(|_| ())
                .map_err(|error| cce_types::StorageError::insert(error.to_string()))?;
                ChunkRepository::insert_batch(
                    tx,
                    &[
                        chunk("chunk_own", "src/new.rs", 5),
                        chunk("chunk_parent", "src/old.rs", 4),
                        chunk("chunk_excluded", "gone.rs", 4),
                    ],
                )
                .map(|_| ())
            })
            .expect("chunks should be inserted");
        client
    }

    #[test]
    fn get_chunk_records_resolves_parent_misses_and_drops_overridden_files() {
        let client = seed_two_generations();
        let conn = client.read_connection().expect("connection should open");
        let view =
            QueryFilter::inherited(5, Some(4), vec!["gone.rs".to_string()]).expect("valid view");

        let records = get_chunk_records(
            &conn,
            &[
                "chunk_own".to_string(),
                "chunk_parent".to_string(),
                "chunk_excluded".to_string(),
            ],
            1,
            &view,
        )
        .expect("lookup should succeed")
        .expect("record map should be present");

        assert!(records.contains_key("chunk_own"));
        assert!(
            records.contains_key("chunk_parent"),
            "own-generation miss must resolve against the parent"
        );
        assert!(
            !records.contains_key("chunk_excluded"),
            "parent rows of overridden files must stay hidden"
        );
    }

    #[test]
    fn get_chunk_records_full_generation_ignores_other_epochs() {
        let client = seed_two_generations();
        let conn = client.read_connection().expect("connection should open");
        let view = QueryFilter::new(5).expect("full view");

        let records = get_chunk_records(&conn, &["chunk_parent".to_string()], 1, &view)
            .expect("lookup should succeed")
            .expect("record map should be present");
        assert!(
            !records.contains_key("chunk_parent"),
            "a full generation must not see foreign epochs"
        );
    }

    #[test]
    fn test_enrich_populates_entity_ids_when_payload_empty() {
        let mut result = SearchResult {
            id: "chunk_x".to_string(),
            entity_ids: Vec::new(),
            ..Default::default()
        };
        let records = HashMap::from([("chunk_x".to_string(), chunk_record(&[7, 8]))]);
        enrich_from_chunk(&mut result, &records, None);

        assert_eq!(result.entity_ids, vec![EntityId(7), EntityId(8)]);
    }

    #[test]
    fn test_enrich_keeps_payload_when_populated() {
        let mut result = SearchResult {
            id: "chunk_x".to_string(),
            entity_ids: vec![EntityId(7), EntityId(8)],
            ..Default::default()
        };
        let records = HashMap::from([("chunk_x".to_string(), chunk_record(&[7, 8]))]);
        enrich_from_chunk(&mut result, &records, None);

        assert_eq!(result.entity_ids, vec![EntityId(7), EntityId(8)]);
    }

    #[test]
    fn test_enrich_names_single_entity_hit_by_its_own_name() {
        let mut record = chunk_record(&[7, 8]);
        record.entity_names = serde_json::to_string(&["alpha".to_string(), "beta".to_string()])
            .expect("serialize names");
        let records = HashMap::from([("chunk_x".to_string(), record)]);

        // Expanded hit for entity 8 must carry "beta", not the group title.
        let mut result = SearchResult {
            id: "chunk_x".to_string(),
            entity_ids: vec![EntityId(8)],
            name: "stale".to_string(),
            ..Default::default()
        };
        enrich_from_chunk(&mut result, &records, None);
        assert_eq!(result.name, "beta");
    }

    #[test]
    fn test_choose_entity_name_falls_back_to_first_named_entry() {
        assert_eq!(
            choose_entity_name("[1,2]", &["a".to_string()], &[EntityId(2)]),
            "a"
        );
        assert_eq!(choose_entity_name("[1,2]", &[], &[EntityId(1)]), "");
    }
}
