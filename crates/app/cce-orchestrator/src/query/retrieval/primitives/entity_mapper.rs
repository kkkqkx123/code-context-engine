//! Entity mapping utilities
//!
//! Provides chunk-to-entity mapping and chunk record lookups,
//! eliminating SQLite query code duplication.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::index::vector_store::MetadataStore;
use crate::query::error::Result;
use crate::query::filter::QueryFilter;
use crate::query::types::SearchResult;
use crate::query::types::content_reference::{
    ContentState, DowngradeReason, file_level_reference, reference_content,
};
use cce_storage_common::metadb::ChunkRecord;
use cce_storage_common::source::{SourceFileCache, read_source_lines_cached};
use cce_utils::token_estimation::estimate_tokens;

/// Fetch chunk records by chunk IDs through the backend-neutral
/// [`MetadataStorage`] contract, resolving the full epoch view.
///
/// Same two-stage resolution as [`get_chunk_records`] ("own first,
/// miss → parent", dropping parent hits of overridden files); the epoch
/// filtering runs inside the branch implementation while the
/// excluded-file decision stays with the caller view.
pub async fn get_chunk_records_from_store(
    store: &MetadataStore,
    chunk_ids: &[String],
    project_id: i64,
    query_filter: &QueryFilter,
) -> Result<Option<HashMap<String, ChunkRecord>>> {
    if chunk_ids.is_empty() {
        return Ok(Some(HashMap::new()));
    }

    let view = match cce_tools::EpochView::inherited(
        query_filter.epoch_value(),
        query_filter.parent_epoch(),
        query_filter.excluded_files().to_vec(),
    ) {
        Ok(view) => view,
        Err(e) => {
            tracing::warn!("Invalid epoch view for chunk lookup: {e}");
            return Ok(None);
        }
    };
    Ok(cce_tools::store::get_chunk_records(store, chunk_ids, project_id, &view).await)
}

/// Downgrade empty bodies to honest references when no store is available.
///
/// Enrichment without a relation store cannot materialize content; hits with
/// an empty body become file-level or range references instead of keeping a
/// full state with no content.
pub fn downgrade_empty_bodies(results: &mut Vec<SearchResult>) {
    let empty: HashMap<String, ChunkRecord> = HashMap::new();
    enrich_results(results, &empty, None, usize::MAX);
}

/// Resolve a project root directory through the relation store.
///
/// Query contexts only know `project_id`; chunk file paths are stored relative
/// to the project root, so the root is recovered from the project registry.
/// Returns `None` when the project row or its root path is unavailable.
pub async fn resolve_project_root_from_store(
    store: &MetadataStore,
    project_id: i64,
) -> Option<PathBuf> {
    cce_tools::store::resolve_project_root(store, project_id).await
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

/// Deduplicate file-reference results by file path plus line interval.
///
/// Non-overlapping intervals from the same file are kept as separate
/// references; overlapping intervals keep the highest score. Full bodies
/// never participate here.
fn dedup_file_references(results: &mut Vec<SearchResult>) {
    let mut keep = vec![true; results.len()];
    for i in 0..results.len() {
        if !keep[i] || !results[i].content_state.is_reference() {
            continue;
        }
        for j in (i + 1)..results.len() {
            if !keep[j] || !results[j].content_state.is_reference() {
                continue;
            }
            if results[i].file_path != results[j].file_path {
                continue;
            }
            if intervals_overlap(
                results[i].start_line,
                results[i].end_line,
                results[j].start_line,
                results[j].end_line,
            ) {
                if results[j].score > results[i].score {
                    keep[i] = false;
                    break;
                } else {
                    keep[j] = false;
                }
            }
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

/// Whether two closed line intervals overlap or touch at an endpoint.
fn intervals_overlap(a_start: u32, a_end: u32, b_start: u32, b_end: u32) -> bool {
    let (a_start, a_end) = (a_start.min(a_end), a_start.max(a_end));
    let (b_start, b_end) = (b_start.min(b_end), b_start.max(b_end));
    a_start <= b_end + 1 && b_start <= a_end + 1
}

fn materialize(
    result: &mut SearchResult,
    chunk_records: &HashMap<String, ChunkRecord>,
    project_root: Option<&std::path::Path>,
    cache: &mut SourceFileCache,
    max_content_tokens: usize,
) {
    let Some(chunk) = chunk_records.get(&result.id) else {
        if result.content.is_empty() {
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
            result.metadata.insert(
                "downgrade".to_string(),
                DowngradeReason::ChunkMissing.note().to_string(),
            );
        }
        return;
    };

    // Metadata is authoritative from the chunk record. Chunk records store
    // tree-sitter's zero-based rows; this type exposes one-based line numbers,
    // so the boundary conversion happens here and nowhere downstream.
    //
    // A retrieved chunk is only a fragment of its group. Materialize the
    // whole group span so callers receive the complete method/entity and the
    // dedup-by-alignment-key step above collapses sibling fragments onto one
    // result instead of returning partial bodies.
    let (group_start, group_end) = chunk_records
        .values()
        .filter(|candidate| {
            candidate.file_path == chunk.file_path && candidate.segment_id == chunk.segment_id
        })
        .fold((chunk.start_line, chunk.end_line), |acc, candidate| {
            (
                acc.0.min(candidate.start_line),
                acc.1.max(candidate.end_line),
            )
        });
    result.file_path = chunk.file_path.clone();
    result.start_line = to_one_based_line(group_start);
    result.end_line = to_one_based_line(group_end);
    // The chunk-level type describes the group container; refine to the hit's
    // own entity kind the same way the display name is refined below.
    result.kind = choose_entity_kind(chunk, &result.entity_ids, &chunk.chunk_type);
    result.truncated = chunk.truncated != 0;

    let entity_names: Vec<String> = serde_json::from_str(&chunk.entity_names).unwrap_or_default();
    if !entity_names.is_empty() {
        // After hybrid expansion a result carries at most one entity; name
        // it with that entity's own display name when available. Otherwise
        // fall back to the group title (the owning type for container
        // groups), and only then to the first named entry.
        result.name = choose_entity_name(
            &chunk.entity_ids,
            &entity_names,
            &chunk.group_title,
            &result.entity_ids,
        );
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
    //
    // A group span over budget narrows to the hit chunk's own span instead
    // of degrading straight to a bare reference: the chunk is what matched
    // the query, so its body is the most relevant answer available. Only a
    // chunk that is itself over budget still becomes a reference.
    let group_body = read_source_lines_cached(
        cache,
        project_root,
        &chunk.file_path,
        group_start.max(0) as u32,
        group_end.max(0) as u32,
    );
    let Some(group_body) = group_body else {
        result.content_state = ContentState::Reference(DowngradeReason::FileMissing);
        result.content = reference_content(
            &chunk.file_path,
            result.start_line,
            result.end_line,
            0,
            DowngradeReason::FileMissing,
        );
        result.metadata.insert(
            "downgrade".to_string(),
            DowngradeReason::FileMissing.note().to_string(),
        );
        return;
    };
    let group_tokens = estimate_tokens(&group_body);
    if group_tokens <= max_content_tokens {
        result.content_state = ContentState::Full;
        result.content = group_body;
        return;
    }
    if chunk.start_line != group_start || chunk.end_line != group_end {
        let chunk_body = read_source_lines_cached(
            cache,
            project_root,
            &chunk.file_path,
            chunk.start_line.max(0) as u32,
            chunk.end_line.max(0) as u32,
        );
        if let Some(chunk_body) = chunk_body
            && !chunk_body.trim().is_empty()
            && estimate_tokens(&chunk_body) <= max_content_tokens
        {
            result.start_line = to_one_based_line(chunk.start_line);
            result.end_line = to_one_based_line(chunk.end_line);
            result.content_state = ContentState::Full;
            result.content = chunk_body;
            result.metadata.insert(
                "group_narrowed".to_string(),
                format!(
                    "group {}-{} (~{} tokens) over budget; showing hit chunk",
                    to_one_based_line(group_start),
                    to_one_based_line(group_end),
                    group_tokens,
                ),
            );
            return;
        }
    }
    result.content_state = ContentState::Reference(DowngradeReason::OverLimit);
    result.content = reference_content(
        &chunk.file_path,
        result.start_line,
        result.end_line,
        group_tokens,
        DowngradeReason::OverLimit,
    );
    result.metadata.insert(
        "downgrade".to_string(),
        DowngradeReason::OverLimit.note().to_string(),
    );
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

/// Pick the display kind for an enriched hit.
///
/// Mirrors the display-name refinement: when the hit represents exactly one
/// entity, the kind stored positionally alongside that entity wins; missing
/// entries fall back to the chunk-level type.
fn choose_entity_kind(
    chunk: &ChunkRecord,
    hit_entity_ids: &[cce_types::EntityId],
    chunk_type: &str,
) -> String {
    let stored_ids: Vec<i64> = serde_json::from_str(&chunk.entity_ids).unwrap_or_default();
    if let [only] = hit_entity_ids {
        if let Some(index) = stored_ids.iter().position(|&id| id as u64 == only.0) {
            let kinds: Vec<String> = serde_json::from_str(&chunk.entity_kinds).unwrap_or_default();
            if let Some(kind) = kinds.get(index).filter(|kind| !kind.is_empty()) {
                return kind.clone();
            }
        }
    }
    chunk_type.to_string()
}
/// Pick the display name for an enriched hit.
///
/// `stored_ids`/`names` are positionally aligned lists persisted with the
/// chunk. When the hit represents exactly one entity (the post-expansion
/// contract), the matching entry wins. Otherwise a bare group title wins when
/// present (it names the owning container, unlike the first member). A dotted
/// title is ownership qualification synthesized for BM25 retrieval (file stem
/// or parent owner), not a display name, so multi-entity hits prefer the
/// first bare per-entity name in that case and keep the title only as a last
/// resort. Entries are scanned in order only on legacy rows, and empty
/// strings (unknown names) are skipped.
fn choose_entity_name(
    stored_ids: &str,
    names: &[String],
    group_title: &str,
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
    let first_bare = names
        .iter()
        .find(|name| !name.is_empty())
        .cloned()
        .unwrap_or_default();
    if group_title.is_empty() || (group_title.contains('.') && !first_bare.is_empty()) {
        return first_bare;
    }
    group_title.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::vector_store::{MetadataStore, in_memory_metadata_store};
    use cce_storage_common::MetadataStorage;
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
    async fn seed_two_generations() -> MetadataStore {
        let store = in_memory_metadata_store().expect("in-memory store");
        store
            .ensure_project(1, "/tmp/test")
            .await
            .expect("project should be ensured");
        let chunk = |id: &str, path: &str, epoch: i64| {
            ChunkRecord::new(id.to_string(), path.to_string(), "code".to_string(), 1, 2)
                .with_epoch(epoch)
                .with_project_id(1)
        };
        store
            .chunks_upsert(&[
                chunk("chunk_own", "src/new.rs", 5),
                chunk("chunk_parent", "src/old.rs", 4),
                chunk("chunk_excluded", "gone.rs", 4),
            ])
            .await
            .expect("chunks should be inserted");
        store
    }

    #[tokio::test]
    async fn get_chunk_records_resolves_parent_misses_and_drops_overridden_files() {
        let store = seed_two_generations().await;
        let view =
            QueryFilter::inherited(5, Some(4), vec!["gone.rs".to_string()]).expect("valid view");

        let records = get_chunk_records_from_store(
            &store,
            &[
                "chunk_own".to_string(),
                "chunk_parent".to_string(),
                "chunk_excluded".to_string(),
            ],
            1,
            &view,
        )
        .await
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

    #[tokio::test]
    async fn get_chunk_records_full_generation_ignores_other_epochs() {
        let store = seed_two_generations().await;
        let view = QueryFilter::new(5).expect("full view");

        let records = get_chunk_records_from_store(&store, &["chunk_parent".to_string()], 1, &view)
            .await
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
            choose_entity_name("[1,2]", &["a".to_string()], "", &[EntityId(2)]),
            "a"
        );
        assert_eq!(choose_entity_name("[1,2]", &[], "", &[EntityId(1)]), "");
    }

    #[test]
    fn test_choose_entity_name_prefers_group_title_for_multi_entity_hits() {
        // A container-group hit carries several entity IDs; the group title
        // (owning type) wins over the first member name.
        assert_eq!(
            choose_entity_name(
                "[1,2]",
                &["new".to_string(), "open".to_string()],
                "OnceCell",
                &[EntityId(1), EntityId(2)],
            ),
            "OnceCell"
        );
        // Single-entity hits still resolve to their own entry.
        assert_eq!(
            choose_entity_name(
                "[1,2]",
                &["new".to_string(), "open".to_string()],
                "OnceCell",
                &[EntityId(2)],
            ),
            "open"
        );
    }

    #[test]
    fn test_choose_entity_name_prefers_bare_name_over_qualified_title() {
        // A dotted group title is retrieval-time ownership qualification
        // (file stem / parent owner), not a display name: multi-entity hits
        // fall back to the first bare per-entity name.
        assert_eq!(
            choose_entity_name(
                "[0,1]",
                &["add".to_string(), "subtract".to_string()],
                "lib.add",
                &[EntityId(0), EntityId(1)],
            ),
            "add"
        );
        // With no bare name available the qualified title is still better
        // than an empty display name.
        assert_eq!(
            choose_entity_name("[0,1]", &[], "lib.add", &[EntityId(0), EntityId(1)],),
            "lib.add"
        );
    }

    #[test]
    fn test_enrich_kinds_single_entity_hit_by_its_own_kind() {
        let mut record = chunk_record(&[7, 8]);
        record.entity_names =
            serde_json::to_string(&["C".to_string(), "m".to_string()]).expect("serialize names");
        record.entity_kinds = serde_json::to_string(&["class".to_string(), "method".to_string()])
            .expect("serialize kinds");
        let records = HashMap::from([("chunk_x".to_string(), record)]);

        // Expanded hit for entity 8 must carry "method", not the group type.
        let result = SearchResult {
            id: "chunk_x".to_string(),
            entity_ids: vec![EntityId(8)],
            kind: "stale".to_string(),
            ..Default::default()
        };
        let mut results = vec![result];
        enrich_results(&mut results, &records, None, 2000);
        assert_eq!(results[0].kind, "method");
    }

    #[test]
    fn test_enrich_kind_falls_back_to_chunk_type_without_kinds() {
        let records = HashMap::from([("chunk_x".to_string(), chunk_record(&[7]))]);
        let result = SearchResult {
            id: "chunk_x".to_string(),
            entity_ids: vec![EntityId(7)],
            kind: "stale".to_string(),
            ..Default::default()
        };
        let mut results = vec![result];
        enrich_results(&mut results, &records, None, 2000);
        // Legacy rows carry no kinds list; the chunk-level type ("unknown"
        // from the test helper) is kept instead of an empty string.
        assert_eq!(results[0].kind, "unknown");
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
        assert!(
            results[0]
                .content
                .contains("<reference path=\"src/big.rs\" lines=\"1-400\"")
        );
        assert!(results[0].content.contains("reason=\"over_limit\""));
        assert_eq!(results[0].start_line, 1);
        assert_eq!(results[0].end_line, 400);
    }

    #[test]
    fn materialize_narrows_over_budget_group_to_hit_chunk() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_source(
            dir.path(),
            "src/mixed.rs",
            &(0..60)
                .map(|i| format!("source line {i} with filler words"))
                .collect::<Vec<_>>()
                .join("\n"),
        );

        // Two chunks share one segment: the hit covers 3 lines, the group
        // covers the whole file and exceeds the budget.
        let mut hit = ChunkRecord::new(
            "chunk_hit".to_string(),
            "src/mixed.rs".to_string(),
            "code".to_string(),
            10,
            12,
        );
        hit.segment_id = "group_1".to_string();
        let mut sibling = ChunkRecord::new(
            "chunk_sibling".to_string(),
            "src/mixed.rs".to_string(),
            "code".to_string(),
            0,
            59,
        );
        sibling.segment_id = "group_1".to_string();
        let records = HashMap::from([
            ("chunk_hit".to_string(), hit),
            ("chunk_sibling".to_string(), sibling),
        ]);
        let result = SearchResult {
            id: "chunk_hit".to_string(),
            ..Default::default()
        };
        let mut results = vec![result];
        enrich_results(&mut results, &records, Some(dir.path()), 50);

        assert_eq!(results[0].content_state, ContentState::Full);
        assert_eq!(results[0].start_line, 11);
        assert_eq!(results[0].end_line, 13);
        assert!(results[0].content.contains("source line 11"));
        assert!(!results[0].content.contains("source line 0"));
        assert!(
            results[0].metadata.contains_key("group_narrowed"),
            "narrowing must be recorded, got {:?}",
            results[0].metadata
        );
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
        assert!(
            results[0]
                .content
                .contains("<reference path=\"src/gone.rs\" lines=\"4-10\"")
        );
        assert!(results[0].content.contains("reason=\"file_missing\""));
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
    fn materialize_unknown_hit_without_record_downgrades_honestly() {
        let result = SearchResult {
            id: "group_9_bm25_0".to_string(),
            file_path: "src/lib.rs".to_string(),
            ..Default::default()
        };
        let records = HashMap::new();
        let mut results = vec![result];
        enrich_results(&mut results, &records, None, 2000);

        assert_eq!(
            results[0].content_state,
            ContentState::Reference(DowngradeReason::ChunkMissing)
        );
        assert!(
            results[0]
                .content
                .contains("<reference path=\"src/lib.rs\"")
        );
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
        assert!(
            results[0]
                .content
                .contains("<reference path=\"src/lib.rs\"")
        );
        assert!((results[0].score - score).abs() < f32::EPSILON);
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
