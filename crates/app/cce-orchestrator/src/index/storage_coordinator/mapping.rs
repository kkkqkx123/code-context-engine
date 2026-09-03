//! Pure, side-effect-free mappings between chunked results and storage
//! record shapes (BM25 documents, SQLite chunk records, Qdrant point IDs).
//!
//! Kept separate from [`super::StorageCoordinator`] so benchmark code can
//! exercise the production schema without duplicating field extraction rules.

use std::hash::{Hash, Hasher};

use cce_parser::ast_to_nl::chunker::{ChunkPath, ChunkedResult};
use cce_storage_bm25::Bm25Document;
use cce_storage_sqlite::ChunkRecord;
use cce_types::chunk_refs::ChunkEntityRefs;
use cce_types::{TestInfo, TestStatus};

use crate::error::OrchestratorError;

/// Convert BM25 chunks into the exact document shape used by production indexing.
///
/// The function is intentionally side-effect free so benchmark code can exercise the
/// production schema without duplicating field extraction rules.
pub fn build_bm25_documents(
    chunks: &[&ChunkedResult],
    project_id: i64,
    epoch: i64,
) -> Vec<Bm25Document> {
    let project_id_str = project_id.to_string();
    let epoch_str = epoch.to_string();

    chunks
        .iter()
        .filter(|chunk| chunk.path == ChunkPath::Bm25 && !chunk.text.is_empty())
        .map(|chunk| {
            let title = chunk.bm25_title.as_deref().unwrap_or("");
            let keywords = chunk.bm25_keywords.join(" ");
            // Store ALL covered entity IDs for multi-entity expansion
            let entity_ids = ChunkEntityRefs::new(chunk.metadata.content_entity_ids().to_vec(), "")
                .to_bm25_csv();

            Bm25Document::new(format!("{project_id}::{epoch}::{}", chunk.chunk_id))
                .with_field("chunk_id", &chunk.chunk_id)
                .with_field("content", &chunk.text)
                .with_field("title", title)
                .with_field("keywords", keywords)
                .with_field("file_path", &chunk.metadata.file_path)
                .with_field("project_id", &project_id_str)
                .with_field("epoch", &epoch_str)
                .with_field("entity_id", entity_ids)
                .with_field("segment_id", chunk_segment_id(chunk))
                .with_field(
                    "test",
                    test_storage_value(&chunk.metadata.test_info).to_string(),
                )
                .with_field("category", chunk.metadata.file_category.as_u8().to_string())
        })
        .collect()
}

/// Resolve the segment id to persist for a chunk, guaranteeing a non-empty
/// alignment key.
///
/// Production chunkers always populate `segment_id` (code: `group_id`;
/// document/config: `source_group_id`), so this normally returns it unchanged.
/// Plugin/external chunks may leave it empty; the raw chunk id is used as a
/// per-chunk fallback so the chunk still carries a segment key. The chunk id
/// embeds the retrieval path (`{group}_{emb|bm25}_{index}`), so the fallback
/// does not force cross-path alignment — the read-side `alignment_key` (see
/// `fusion.rs`) no longer strips that suffix. An empty segment id is surfaced
/// as a warning instead of silently degrading.
pub(crate) fn chunk_segment_id(chunk: &ChunkedResult) -> String {
    if chunk.metadata.segment_id.is_empty() {
        tracing::warn!(
            chunk_id = %chunk.chunk_id,
            "Chunk has empty segment_id; falling back to chunk id as segment key"
        );
        chunk.chunk_id.clone()
    } else {
        chunk.metadata.segment_id.clone()
    }
}

/// Render the test marker for storage: 1 (test) / 0 (not test).
pub(crate) fn test_storage_value(test_info: &TestInfo) -> u8 {
    match test_info.status {
        TestStatus::Test => 1,
        TestStatus::Unknown => 0,
    }
}

/// Render the test source for BM25 storage: numeric encoding.
pub(crate) fn test_source_storage_value(test_info: &TestInfo) -> u8 {
    test_info.source.as_u8()
}
/// Both retrieval paths (embedding and BM25) are persisted as chunk records so
/// SQLite enrichment can resolve line numbers and entity IDs for hits from
/// either path. The `path` discriminator keeps the two families
/// distinguishable for consumers that enumerate source chunks.
pub(crate) fn build_chunk_record(
    chunk: &ChunkedResult,
    project_id: i64,
    epoch: i64,
    batch_id: i64,
) -> Result<ChunkRecord, OrchestratorError> {
    // Per-entity display names captured at chunk build time; fall back to the
    // group title for legacy/plugin chunks without a name list.
    let entity_names: Vec<String> = match chunk.metadata.as_code() {
        Some(code) if !code.content_entity_names.is_empty() => code.content_entity_names.clone(),
        _ => chunk
            .bm25_title
            .as_ref()
            .map(|n| vec![n.clone()])
            .unwrap_or_default(),
    };
    let refs = ChunkEntityRefs::new(chunk.metadata.content_entity_ids().to_vec(), "");

    Ok(ChunkRecord::new(
        chunk.chunk_id.clone(),
        chunk.metadata.file_path.clone(),
        chunk.text.clone(),
        chunk.metadata.source_span.start_position.row as i64,
        chunk.metadata.source_span.end_position.row as i64,
    )
    .with_entity_ids_json(refs.to_sql_json())
    .with_entity_names(&entity_names)
    // tantivy stores keywords index-only, so SQLite keeps the only copy for
    // epoch cloning to rebuild BM25 documents.
    .with_bm25_keywords(chunk.bm25_keywords.join(" "))
    .with_chunk_type(
        chunk
            .metadata
            .entity_kind()
            .map(|k| k.to_string())
            .unwrap_or_else(|| "unknown".to_string()),
    )
    .with_test_status(test_storage_value(&chunk.metadata.test_info))
    .with_test_source(test_source_storage_value(&chunk.metadata.test_info))
    .with_segment_id(chunk_segment_id(chunk))
    .with_path(chunk.path.as_str())
    .with_epoch(epoch)
    .with_batch_id(batch_id)
    .with_project_id(project_id))
}

/// Build the globally unique Qdrant point ID used for a project chunk.
pub(crate) fn project_chunk_point_id(project_group_id: &str, epoch: i64, chunk_id: &str) -> String {
    format!("{project_group_id}::{epoch}::{chunk_id}")
}

/// Replace the epoch field in a structured ID without substring collisions.
///
/// IDs follow the pattern `{prefix}::{epoch}::{suffix}`. A naive
/// `replace(&format!("::{old_epoch}::"), &format!("::{new_epoch}::"))` can
/// mis-fire when epochs share a digit prefix (e.g. old=1, new=11 turns
/// `::11::` into `::111::`). Splitting on `::` and replacing only the
/// second field avoids this.
pub(crate) fn replace_epoch_in_id(id: &str, old_epoch: i64, new_epoch: i64) -> String {
    let mut parts: Vec<&str> = id.split("::").collect();
    if parts.len() >= 2 && parts[1] == old_epoch.to_string() {
        let new_epoch_str = new_epoch.to_string();
        parts[1] = &new_epoch_str;
        return parts.join("::");
    }
    id.to_string()
}

/// Replace epochs inside a JSON array of ID strings.
pub(crate) fn replace_epoch_in_id_list(list: &str, old_epoch: i64, new_epoch: i64) -> String {
    let Ok(mut ids) = serde_json::from_str::<Vec<String>>(list) else {
        return list.to_string();
    };
    for id in &mut ids {
        *id = replace_epoch_in_id(id, old_epoch, new_epoch);
    }
    serde_json::to_string(&ids).unwrap_or_else(|_| list.to_string())
}

/// Compute a deterministic hash from chunk IDs to identify a work unit.
/// The hash is order-independent (IDs are sorted before hashing).
pub(crate) fn compute_work_unit_hash(chunks: &[&ChunkedResult]) -> String {
    let mut ids: Vec<&str> = chunks.iter().map(|c| c.chunk_id.as_str()).collect();
    ids.sort_unstable();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for id in ids {
        id.hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::{
        build_chunk_record, project_chunk_point_id, replace_epoch_in_id, replace_epoch_in_id_list,
    };
    use cce_parser::ast_to_nl::chunker::{
        ChunkMetadata, ChunkPath, ChunkedResult, CodeSpecificMetadata,
    };
    use cce_types::entity::{EntityId, EntityKind};
    use cce_types::{Language, Span};

    #[test]
    fn project_chunk_point_ids_are_scoped_and_stable() {
        let first = project_chunk_point_id("project-1-root", 1, "shared-chunk");
        let second = project_chunk_point_id("project-2-root", 1, "shared-chunk");

        assert_eq!(first, "project-1-root::1::shared-chunk");
        assert_eq!(second, "project-2-root::1::shared-chunk");
        assert_ne!(first, second);
    }

    #[test]
    fn build_chunk_record_tags_bm25_path_and_copies_alignment_data() {
        let mut chunk = ChunkedResult::new(
            "group_9_bm25_1".to_string(),
            "group_9".to_string(),
            ChunkPath::Bm25,
            1,
            2,
        );
        chunk.text = "natural language description".to_string();
        chunk.metadata = ChunkMetadata::for_code(
            "src/lib.rs".to_string(),
            Span::default(),
            Language::Rust,
            CodeSpecificMetadata {
                content_entity_ids: vec![EntityId(10), EntityId(20)],
                entity_kind: EntityKind::Function,
                ..Default::default()
            },
        );
        chunk.metadata.segment_id = "group_9".to_string();

        let record = build_chunk_record(&chunk, 7, 3, 5).expect("build record");

        assert_eq!(record.chunk_id, "group_9_bm25_1");
        assert_eq!(record.path, "bm25");
        assert_eq!(record.content, "natural language description");
        assert_eq!(record.chunk_type, "function");
        assert_eq!(record.epoch, 3);
        assert_eq!(record.batch_id, 5);
        assert_eq!(record.project_id, Some(7));
        let entity_ids = record.get_entity_ids();
        assert_eq!(entity_ids, vec![10, 20]);
    }

    #[test]
    fn replace_epoch_in_id_handles_digit_overlap() {
        assert_eq!(
            replace_epoch_in_id("grp::1::chunk-x", 1, 11),
            "grp::11::chunk-x"
        );
        assert_eq!(
            replace_epoch_in_id("grp::11::chunk-x", 1, 11),
            "grp::11::chunk-x"
        );
        assert_eq!(
            replace_epoch_in_id("grp::1::summary::src/lib.rs", 1, 2),
            "grp::2::summary::src/lib.rs"
        );
    }

    #[test]
    fn replace_epoch_in_id_list_replaces_all_entries() {
        let input = serde_json::to_string(&vec![
            "grp::1::chunk-a".to_string(),
            "grp::1::chunk-b".to_string(),
        ])
        .unwrap();
        let expected = serde_json::to_string(&vec![
            "grp::3::chunk-a".to_string(),
            "grp::3::chunk-b".to_string(),
        ])
        .unwrap();
        assert_eq!(replace_epoch_in_id_list(&input, 1, 3), expected);
    }

    #[test]
    fn build_bm25_documents_stores_entity_id_and_segment_id() {
        let mut chunk = ChunkedResult::new(
            "chunk_1".to_string(),
            "group_1".to_string(),
            ChunkPath::Bm25,
            0,
            1,
        );
        chunk.text = "test content".to_string();
        chunk.metadata = ChunkMetadata::for_code(
            "src/lib.rs".to_string(),
            Span::default(),
            Language::Rust,
            CodeSpecificMetadata {
                content_entity_ids: vec![EntityId(42)],
                entity_kind: EntityKind::Function,
                ..Default::default()
            },
        );
        chunk.metadata.segment_id = "group_1".to_string();

        let docs = super::build_bm25_documents(&[&chunk], 1, 1);

        assert_eq!(docs.len(), 1);
        let doc = &docs[0];

        let entity_id = doc
            .fields
            .iter()
            .find(|(k, _)| *k == "entity_id")
            .map(|(_, v)| v.as_str());
        assert_eq!(entity_id, Some("42"));

        let segment_id = doc
            .fields
            .iter()
            .find(|(k, _)| *k == "segment_id")
            .map(|(_, v)| v.as_str());
        assert_eq!(segment_id, Some("group_1"));
    }

    #[test]
    fn build_bm25_documents_stores_multiple_entity_ids() {
        let mut chunk = ChunkedResult::new(
            "chunk_multi".to_string(),
            "group_1".to_string(),
            ChunkPath::Bm25,
            0,
            1,
        );
        chunk.text = "multi-entity content".to_string();
        chunk.metadata = ChunkMetadata::for_code(
            "src/lib.rs".to_string(),
            Span::default(),
            Language::Rust,
            CodeSpecificMetadata {
                content_entity_ids: vec![EntityId(10), EntityId(20), EntityId(30)],
                entity_kind: EntityKind::Function,
                ..Default::default()
            },
        );
        chunk.metadata.segment_id = "group_1".to_string();

        let docs = super::build_bm25_documents(&[&chunk], 1, 1);

        assert_eq!(docs.len(), 1);
        let doc = &docs[0];

        let entity_id = doc
            .fields
            .iter()
            .find(|(k, _)| *k == "entity_id")
            .map(|(_, v)| v.as_str());
        assert_eq!(entity_id, Some("10,20,30"));
    }

    #[test]
    fn build_bm25_documents_document_chunk_no_entity_id() {
        let mut chunk = ChunkedResult::new(
            "doc_chunk_1".to_string(),
            "doc_group_1".to_string(),
            ChunkPath::Bm25,
            0,
            2,
        );
        chunk.text = "document content".to_string();
        chunk.metadata.segment_id = "doc_group_1".to_string();

        let docs = super::build_bm25_documents(&[&chunk], 1, 1);

        assert_eq!(docs.len(), 1);
        let doc = &docs[0];

        let entity_id = doc
            .fields
            .iter()
            .find(|(k, _)| *k == "entity_id")
            .map(|(_, v)| v.as_str());
        assert_eq!(entity_id, Some(""));

        let segment_id = doc
            .fields
            .iter()
            .find(|(k, _)| *k == "segment_id")
            .map(|(_, v)| v.as_str());
        assert_eq!(segment_id, Some("doc_group_1"));
    }

    #[test]
    fn build_bm25_documents_fills_empty_segment_id_with_chunk_id() {
        let mut chunk = ChunkedResult::new(
            "external_bm25_0".to_string(),
            "external".to_string(),
            ChunkPath::Bm25,
            0,
            1,
        );
        chunk.text = "plugin content".to_string();
        chunk.metadata.segment_id = String::new();

        let docs = super::build_bm25_documents(&[&chunk], 1, 1);

        assert_eq!(docs.len(), 1);
        let doc = &docs[0];

        let segment_id = doc
            .fields
            .iter()
            .find(|(k, _)| *k == "segment_id")
            .map(|(_, v)| v.as_str());
        assert_eq!(
            segment_id,
            Some("external_bm25_0"),
            "empty segment_id must fall back to the raw chunk id"
        );
    }

    #[test]
    fn build_bm25_documents_filters_empty_text() {
        let mut chunk = ChunkedResult::new(
            "empty_chunk".to_string(),
            "group_1".to_string(),
            ChunkPath::Bm25,
            0,
            1,
        );
        chunk.text = String::new();
        chunk.metadata.segment_id = "group_1".to_string();

        let docs = super::build_bm25_documents(&[&chunk], 1, 1);

        assert!(docs.is_empty());
    }
}
