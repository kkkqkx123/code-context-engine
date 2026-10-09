//! Entity mapping utilities
//!
//! Provides chunk-to-entity mapping and chunk record lookups,
//! eliminating SQLite query code duplication.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use rusqlite::Connection;

use crate::index::vector_store::RelationStore;
use crate::query::error::Result;
use crate::query::filter::QueryFilter;
use crate::query::types::SearchResult;
use crate::query::types::content_reference::{
    ContentState, DowngradeReason, file_level_reference, reference_content,
};
use cce_storage_common::RelationStorage;
use cce_storage_metadb_sqlite::repo::ChunkRepository;
use cce_storage_metadb_sqlite::source_reader::{SourceFileCache, read_source_lines_cached};
use cce_storage_metadb_sqlite::types::ChunkRecord;
use cce_utils::token_estimation::estimate_tokens;

/// Fetch chunk records by chunk IDs, resolving the full epoch view.
///
/// Two-stage resolution ("own first, miss → parent"): chunk IDs missing from
/// the own generation are re-queried against the inherited parent epoch, and
/// parent hits belonging to overridden files (replaced/deleted) are dropped so
/// only the visible view is returned.
///
/// Returns a map of chunk_id -> ChunkRecord.
pub fn get_chunk_records(
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

/// Fetch chunk records by chunk IDs through the backend-neutral
/// [`RelationStorage`] contract, resolving the full epoch view.
///
/// Same two-stage resolution as [`get_chunk_records`] ("own first,
/// miss → parent", dropping parent hits of overridden files); the epoch
/// filtering runs inside the branch implementation while the
/// excluded-file decision stays with the caller view.
pub async fn get_chunk_records_from_store(
    store: &RelationStore,
    chunk_ids: &[String],
    project_id: i64,
    query_filter: &QueryFilter,
) -> Result<Option<HashMap<String, ChunkRecord>>> {
    if chunk_ids.is_empty() {
        return Ok(Some(HashMap::new()));
    }

    match resolve_chunk_records_from_store(store, chunk_ids, project_id, query_filter).await {
        Ok(records) => Ok(Some(records)),
        Err(e) => {
            tracing::warn!("Failed to fetch chunks from relation store: {}", e);
            Ok(None)
        }
    }
}

async fn resolve_chunk_records_from_store(
    store: &RelationStore,
    chunk_ids: &[String],
    project_id: i64,
    query_filter: &QueryFilter,
) -> std::result::Result<HashMap<String, ChunkRecord>, cce_types::StorageError> {
    let own_records = store
        .chunks_by_ids(project_id, chunk_ids, &[query_filter.epoch_value()])
        .await?;
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
/// Returns `None` when the project row or its root path is unavailable.
pub async fn resolve_project_root_from_store(
    store: &RelationStore,
    project_id: i64,
) -> Option<PathBuf> {
    store
        .project_record(project_id)
        .await
        .ok()?
        .map(|record| PathBuf::from(record.root_path))
}

/// Batch-enrich results sharing one file-content cache.
///
/// Materialization is mandatory: every result either carries its full body
/// (within the token budget) or a file-and-range reference. Metadata (file
/// path, line range, kind) is taken from the chunk record; an unreadable file
/// becomes a reference rather than an empty body.
///
/// After enrichment, results that downgraded to file references for the same
/// file are deduplicated by file_path, keeping the highest-scoring one.
pub fn enrich_results(
    results: &mut Vec<SearchResult>,
    chunk_records: &HashMap<String, ChunkRecord>,
    project_root: Option<&std::path::Path>,
    max_content_tokens: usize,
) {
    let mut cache = SourceFileCache::new();
    for result in results.iter_mut() {
        materialize(
            result,
            chunk_records,
            project_root,
            &mut cache,
            max_content_tokens,
        );
    }
    dedup_file_references(results);
}

/// Deduplicate file-reference results by file_path, keeping the highest-scoring one.
///
/// After enrichment, multiple chunks from the same file may downgrade to file
/// references (over budget, missing file, or file-level hits). This collapses
/// them to a single reference per file.
fn dedup_file_references(results: &mut Vec<SearchResult>) {
    use std::collections::HashMap;
    let mut best_by_file: HashMap<&str, usize> = HashMap::new();
    let mut keep = vec![true; results.len()];
    for (i, result) in results.iter().enumerate() {
        if !result.content_state.is_reference() {
            continue;
        }
        let path = result.file_path.as_str();
        if let Some(&best_idx) = best_by_file.get(path) {
            if result.score > results[best_idx].score {
                keep[best_idx] = false;
                best_by_file.insert(path, i);
            } else {
                keep[i] = false;
            }
        } else {
            best_by_file.insert(path, i);
        }
    }
    let mut write_idx = 0;
    for (read_idx, &keep_item) in keep.iter().enumerate() {
        if keep_item {
            results.swap(write_idx, read_idx);
            write_idx += 1;
        }
    }
    results.truncate(write_idx);
}

fn materialize(
    result: &mut SearchResult,
    chunk_records: &HashMap<String, ChunkRecord>,
    project_root: Option<&std::path::Path>,
    cache: &mut SourceFileCache,
    max_content_tokens: usize,
) {
    let Some(chunk) = chunk_records.get(&result.id) else {
        // A file-level hit carries no chunk record by construction; point at
        // the file instead of returning an empty body.
        if result.content.is_empty() && result.kind == "summary" {
            result.content_state = ContentState::Reference(DowngradeReason::FileLevel);
            result.content = file_level_reference(&result.file_path, DowngradeReason::FileLevel);
            result.score *= 0.8;
        } else if result.content.is_empty() {
            // A chunk hit without a record means the index and query metadata
            // stores diverged. Downgrade honestly instead of keeping the
            // recall-stage Full state with an empty body.
            result.content_state = ContentState::Reference(DowngradeReason::ChunkMissing);
            if result.start_line == 0 && result.end_line == 0 {
                result.content =
                    file_level_reference(&result.file_path, DowngradeReason::ChunkMissing);
            } else {
                result.content = reference_content(
                    &result.file_path,
                    result.start_line,
                    result.end_line,
                    0,
                    DowngradeReason::ChunkMissing,
                );
            }
            result.score *= 0.8;
        }
        return;
    };

    // Metadata is authoritative from the chunk record. Chunk records store
    // tree-sitter's zero-based rows; this type exposes one-based line numbers,
    // so the boundary conversion happens here and nowhere downstream.
    result.file_path = chunk.file_path.clone();
    result.start_line = to_one_based_line(chunk.start_line);
    result.end_line = to_one_based_line(chunk.end_line);
    result.kind = chunk.chunk_type.clone();
    result.truncated = chunk.truncated != 0;

    let entity_names: Vec<String> = serde_json::from_str(&chunk.entity_names).unwrap_or_default();
    if !entity_names.is_empty() {
        // After hybrid expansion a result carries at most one entity; name
        // it with that entity's own display name when available. Otherwise
        // fall back to the first named entry (the group title).
        result.name = choose_entity_name(&chunk.entity_ids, &entity_names, &result.entity_ids);
    }

    let sqlite_entity_ids: Vec<i64> = serde_json::from_str(&chunk.entity_ids).unwrap_or_default();
    if result.entity_ids.is_empty() && !sqlite_entity_ids.is_empty() {
        // Fallback: populate entity_ids from SQLite if the payload/BM25
        // index didn't carry them.
        result.entity_ids = sqlite_entity_ids
            .iter()
            .map(|&id| cce_types::EntityId(id as u64))
            .collect();
    }

    // Body materialization: read the live source, then apply the budget. The
    // reader indexes by zero-based row, so it keeps the raw chunk values while
    // `result` carries the one-based translation for presentation.
    match read_source_lines_cached(
        cache,
        project_root,
        &chunk.file_path,
        chunk.start_line.max(0) as u32,
        chunk.end_line.max(0) as u32,
    ) {
        None => {
            result.content_state = ContentState::Reference(DowngradeReason::FileMissing);
            result.content = reference_content(
                &chunk.file_path,
                result.start_line,
                result.end_line,
                0,
                DowngradeReason::FileMissing,
            );
            result.score *= 0.8;
        }
        Some(source_text) => {
            let tokens = estimate_tokens(&source_text);
            if tokens > max_content_tokens {
                result.content_state = ContentState::Reference(DowngradeReason::OverLimit);
                result.content = reference_content(
                    &chunk.file_path,
                    result.start_line,
                    result.end_line,
                    tokens,
                    DowngradeReason::OverLimit,
                );
                result.score *= 0.8;
            } else {
                result.content_state = ContentState::Full;
                result.content = source_text;
            }
        }
    }
}

/// Translate a stored zero-based row index into a one-based line number.
///
/// Chunk records persist tree-sitter rows, whose zero encodes the first line.
/// Presentation layers — reference lines, search hits, judgment expectations —
/// all speak one-based line numbers, so the conversion is centralized here.
/// A negative stored value is treated as line one rather than clamped away,
/// keeping the result a valid one-based line for any input.
fn to_one_based_line(row: i64) -> u32 {
    if row < 0 { 1 } else { (row as u64 + 1) as u32 }
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
    use cce_storage_metadb_sqlite::{ChunkRepository, SqliteClient};
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
                .map_err(|error| cce_types::StorageError::insert("projects", error.to_string()))?;
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
        let result = SearchResult {
            id: "chunk_x".to_string(),
            entity_ids: Vec::new(),
            ..Default::default()
        };
        let records = HashMap::from([("chunk_x".to_string(), chunk_record(&[7, 8]))]);
        let mut results = vec![result];
        enrich_results(&mut results, &records, None, 2000);

        assert_eq!(results[0].entity_ids, vec![EntityId(7), EntityId(8)]);
    }

    #[test]
    fn test_enrich_keeps_payload_when_populated() {
        let result = SearchResult {
            id: "chunk_x".to_string(),
            entity_ids: vec![EntityId(7), EntityId(8)],
            ..Default::default()
        };
        let records = HashMap::from([("chunk_x".to_string(), chunk_record(&[7, 8]))]);
        let mut results = vec![result];
        enrich_results(&mut results, &records, None, 2000);

        assert_eq!(results[0].entity_ids, vec![EntityId(7), EntityId(8)]);
    }

    #[test]
    fn test_enrich_names_single_entity_hit_by_its_own_name() {
        let mut record = chunk_record(&[7, 8]);
        record.entity_names = serde_json::to_string(&["alpha".to_string(), "beta".to_string()])
            .expect("serialize names");
        let records = HashMap::from([("chunk_x".to_string(), record)]);

        // Expanded hit for entity 8 must carry "beta", not the group title.
        let result = SearchResult {
            id: "chunk_x".to_string(),
            entity_ids: vec![EntityId(8)],
            name: "stale".to_string(),
            ..Default::default()
        };
        let mut results = vec![result];
        enrich_results(&mut results, &records, None, 2000);
        assert_eq!(results[0].name, "beta");
    }

    #[test]
    fn test_choose_entity_name_falls_back_to_first_named_entry() {
        assert_eq!(
            choose_entity_name("[1,2]", &["a".to_string()], &[EntityId(2)]),
            "a"
        );
        assert_eq!(choose_entity_name("[1,2]", &[], &[EntityId(1)]), "");
    }

    fn write_source(dir: &std::path::Path, rel: &str, body: &str) {
        let file = dir.join(rel);
        std::fs::create_dir_all(file.parent().expect("parent")).expect("create dir");
        std::fs::write(&file, body).expect("write source");
    }

    #[test]
    fn materialize_downgrades_over_budget_body_to_reference() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_source(
            dir.path(),
            "src/big.rs",
            &"let value = compute();\n".repeat(400),
        );

        let result = SearchResult {
            id: "chunk_big".to_string(),
            ..Default::default()
        };
        let records = HashMap::from([(
            "chunk_big".to_string(),
            ChunkRecord::new(
                "chunk_big".to_string(),
                "src/big.rs".to_string(),
                "code".to_string(),
                0,
                399,
            ),
        )]);
        let mut results = vec![result];
        enrich_results(&mut results, &records, Some(dir.path()), 50);

        assert_eq!(
            results[0].content_state,
            ContentState::Reference(DowngradeReason::OverLimit)
        );
        assert!(results[0].content.contains("[reference] src/big.rs:1-400"));
        assert!(results[0].content.contains("over budget"));
        assert_eq!(results[0].start_line, 1);
        assert_eq!(results[0].end_line, 400);
    }

    #[test]
    fn materialize_missing_file_becomes_reference() {
        let dir = tempfile::tempdir().expect("tempdir");
        let result = SearchResult {
            id: "chunk_gone".to_string(),
            ..Default::default()
        };
        let records = HashMap::from([(
            "chunk_gone".to_string(),
            ChunkRecord::new(
                "chunk_gone".to_string(),
                "src/gone.rs".to_string(),
                "code".to_string(),
                3,
                9,
            ),
        )]);
        let mut results = vec![result];
        enrich_results(&mut results, &records, Some(dir.path()), 2000);

        assert_eq!(
            results[0].content_state,
            ContentState::Reference(DowngradeReason::FileMissing)
        );
        assert!(results[0].content.contains("[reference] src/gone.rs:4-10"));
        assert!(results[0].content.contains("not found"));
    }

    #[test]
    fn materialize_keeps_body_within_budget() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_source(dir.path(), "src/small.rs", "fn a() {}\nfn b() {}\n");

        let result = SearchResult {
            id: "chunk_small".to_string(),
            ..Default::default()
        };
        let records = HashMap::from([(
            "chunk_small".to_string(),
            ChunkRecord::new(
                "chunk_small".to_string(),
                "src/small.rs".to_string(),
                "code".to_string(),
                0,
                1,
            ),
        )]);
        let mut results = vec![result];
        enrich_results(&mut results, &records, Some(dir.path()), 2000);

        assert_eq!(results[0].content_state, ContentState::Full);
        assert_eq!(results[0].content, "fn a() {}\nfn b() {}");
        assert_eq!(results[0].start_line, 1);
        assert_eq!(results[0].end_line, 2);
    }

    #[test]
    fn materialize_summary_without_record_gets_file_level_reference() {
        let result = SearchResult {
            id: "summary::src/lib.rs".to_string(),
            kind: "summary".to_string(),
            file_path: "src/lib.rs".to_string(),
            ..Default::default()
        };
        let records = HashMap::new();
        let mut results = vec![result];
        enrich_results(&mut results, &records, None, 2000);

        assert_eq!(
            results[0].content_state,
            ContentState::Reference(DowngradeReason::FileLevel)
        );
        assert!(results[0].content.contains("[reference] src/lib.rs"));
    }

    #[test]
    fn materialize_missing_chunk_record_downgrades_honestly() {
        let score = 2.0;
        let result = SearchResult {
            id: "group_1_bm25_0".to_string(),
            file_path: "src/lib.rs".to_string(),
            score,
            original_score: score,
            ..Default::default()
        };
        let records = HashMap::new();
        let mut results = vec![result];
        enrich_results(&mut results, &records, None, 2000);

        assert_eq!(
            results[0].content_state,
            ContentState::Reference(DowngradeReason::ChunkMissing)
        );
        assert!(!results[0].content.is_empty());
        assert!(results[0].content.contains("[reference] src/lib.rs"));
        assert!((results[0].score - score * 0.8).abs() < f32::EPSILON);
    }

    #[test]
    fn to_one_based_line_translates_stored_rows() {
        assert_eq!(to_one_based_line(0), 1);
        assert_eq!(to_one_based_line(3), 4);
        assert_eq!(to_one_based_line(9), 10);
        assert_eq!(to_one_based_line(-1), 1);
    }

    #[test]
    fn materialize_missing_chunk_record_keeps_existing_body() {
        let result = SearchResult {
            id: "group_1_bm25_0".to_string(),
            file_path: "src/lib.rs".to_string(),
            content: "already present".to_string(),
            content_state: ContentState::Full,
            ..Default::default()
        };
        let records = HashMap::new();
        let mut results = vec![result];
        enrich_results(&mut results, &records, None, 2000);

        assert_eq!(results[0].content_state, ContentState::Full);
        assert_eq!(results[0].content, "already present");
    }
}
