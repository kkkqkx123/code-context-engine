//! Chunk-level limit checks.
//!
//! The pre-injection check is advisory: it records a violation but keeps the
//! chunk, because the injection is still bounded by the hard limit.
//! The hard-limit check truncates the tail so storage receives a bounded
//! chunk; single-entity BM25 text keeps a bounded whole up to three times
//! the hard limit, beyond which it truncates to that bound. Truncation is
//! warned, therefore not silent, and it keeps the file instead of dropping it.

use cce_types::ParseError;

use super::config::ChunkingConfig;
use super::result::{ChunkPath, ChunkedResult};
use super::splitter::SINGLE_ENTITY_BM25_TOLERANCE_FACTOR;

/// Check the pre-injection limit after decoration has been appended.
///
/// The splitter works on the body limit, so compliant assembly fits the
/// remaining room. An over-limit chunk means the decoration estimate was
/// wrong or a future assembly path bypassed the body budget. The violation
/// is reported as a warning with full budget context; the chunk is kept to
/// avoid dropping the whole file, because injection is still bounded by the
/// absolute hard limit. Chunks that already exceed the hard limit are left
/// to the hard-limit handler so one cause is not warned about twice.
pub(crate) fn enforce_pre_injection_limit(
    chunks: Vec<ChunkedResult>,
    budget: cce_config::modules::ChunkBudget,
    file_path: &str,
    group_id: &str,
) -> Result<Vec<ChunkedResult>, ParseError> {
    if budget.pre_injection_limit == usize::MAX || budget.hard_limit == 0 {
        return Ok(chunks);
    }
    for chunk in &chunks {
        let actual = budget.unit.cost(&chunk.text);
        if actual > budget.pre_injection_limit
            && (budget.hard_limit == 0 || actual <= budget.hard_limit)
        {
            // Tolerated single-entity BM25 wholes stay silent: splitting them
            // would multiply index entries while the reported span stays
            // whole, so the over-pre state is expected, not a violation.
            let tolerated = chunk.path == ChunkPath::Bm25
                && chunk
                    .metadata
                    .code_metadata
                    .as_ref()
                    .is_some_and(|code| code.content_entity_ids.len() == 1)
                && actual
                    <= budget
                        .pre_injection_limit
                        .saturating_mul(SINGLE_ENTITY_BM25_TOLERANCE_FACTOR);
            if !tolerated {
                tracing::warn!(
                    file = file_path,
                    group = group_id,
                    chunk = %chunk.chunk_id,
                    cost = actual,
                    pre = budget.pre_injection_limit,
                    "chunk exceeds the pre-injection limit; keeping it to avoid dropping the file"
                );
            }
        }
    }
    Ok(chunks)
}

/// Truncate any chunk that still exceeds its bounded hard limit.
///
/// The splitter keeps compliant assembly within the hard limit. This is the
/// last resort for a plugin-produced chunk or a future assembly path that
/// bypassed the body budget: the tail is cut with a warning, so the file is
/// kept and storage receives a bounded chunk. Single-entity BM25 text keeps
/// a whole up to three times the hard limit; beyond that it truncates to
/// that bound. Truncated chunks carry the truncated flag so callers know the
/// tail was lost.
pub(crate) fn enforce_hard_limit(
    chunks: &mut [ChunkedResult],
    config: &ChunkingConfig,
    file_path: &str,
) -> Result<(), ParseError> {
    for chunk in chunks {
        let limit = config.limit(chunk.path);
        let actual = config.cost(&chunk.text, chunk.path);
        if limit > 0 && actual > limit {
            let single_bm25 = chunk.path == ChunkPath::Bm25
                && chunk
                    .metadata
                    .code_metadata
                    .as_ref()
                    .is_some_and(|code| code.content_entity_ids.len() == 1);
            let extended = limit.saturating_mul(SINGLE_ENTITY_BM25_TOLERANCE_FACTOR);
            if single_bm25 && actual <= extended {
                // Bounded whole: splitting would multiply index entries while
                // the reported span stays whole, so keep it silently.
            } else {
                let cap = if single_bm25 { extended } else { limit };
                tracing::warn!(
                    file = file_path,
                    group = chunk.source_group_id.as_str(),
                    chunk = %chunk.chunk_id,
                    cost = actual,
                    hard = limit,
                    "chunk exceeds the hard limit; truncating tail to keep the file"
                );
                let truncated =
                    cce_text::truncate_to_cost(&chunk.text, config.unit(chunk.path), cap);
                chunk.text = truncated.to_string();
                chunk.truncated = true;
            }
        }
        let fitted = config.cost(&chunk.text, chunk.path);
        chunk.token_count = fitted;
        if chunk.metadata.bm25_word_count.is_some() {
            chunk.metadata.bm25_word_count = Some(config.cost(&chunk.text, ChunkPath::Bm25));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_config::modules::ChunkingConfig;

    fn words(n: usize) -> String {
        (0..n)
            .map(|i| format!("w{i}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn test_hard_limit_truncates_single_entity_bm25_beyond_tolerance() {
        // Single-entity BM25 keeps a whole up to three times the hard limit;
        // beyond that it truncates to the extended bound, not to the base.
        use cce_types::entity::{EntityId, EntityKind};

        let config = ChunkingConfig {
            max_bm25_terms: 10,
            ..Default::default()
        };
        let text = words(40);
        assert!(
            config.cost(&text, ChunkPath::Bm25) > 10,
            "fixture must exceed the hard limit"
        );
        let mut chunk = ChunkedResult::new(
            "group_1_bm25_0".to_string(),
            "group_1".to_string(),
            ChunkPath::Bm25,
            0,
            1,
        );
        chunk.text = text;
        chunk.metadata.code_metadata = Some(cce_types::ast_to_nl::CodeSpecificMetadata {
            content_entity_ids: vec![EntityId(1)],
            content_entity_kinds: vec![EntityKind::Function.to_string()],
            ..Default::default()
        });
        let mut chunks = vec![chunk];
        super::enforce_hard_limit(&mut chunks, &config, "atomic.rs")
            .expect("hard limit must not drop the file");
        assert_eq!(chunks.len(), 1);
        assert!(
            config.cost(&chunks[0].text, ChunkPath::Bm25) <= 30,
            "beyond-tolerance chunk truncates to the extended bound"
        );
        assert!(
            chunks[0].truncated,
            "truncation must set the truncated flag"
        );
    }

    #[test]
    fn test_hard_limit_keeps_single_entity_bm25_within_tolerance() {
        // Within three times the hard limit the whole stays, silently, so one
        // function keeps one index entry and the reported span stays whole.
        use cce_types::entity::{EntityId, EntityKind};

        let config = ChunkingConfig {
            max_bm25_terms: 10,
            ..Default::default()
        };
        let text = words(30);
        let mut chunk = ChunkedResult::new(
            "group_1_bm25_0".to_string(),
            "group_1".to_string(),
            ChunkPath::Bm25,
            0,
            1,
        );
        chunk.text = text;
        chunk.metadata.code_metadata = Some(cce_types::ast_to_nl::CodeSpecificMetadata {
            content_entity_ids: vec![EntityId(1)],
            content_entity_kinds: vec![EntityKind::Function.to_string()],
            ..Default::default()
        });
        let mut chunks = vec![chunk];
        super::enforce_hard_limit(&mut chunks, &config, "atomic.rs")
            .expect("tolerated whole must not drop the file");
        assert!(
            config.cost(&chunks[0].text, ChunkPath::Bm25) > 10,
            "tolerated whole exceeds the base hard limit"
        );
        assert!(
            !chunks[0].truncated,
            "tolerated whole must not set the truncated flag"
        );
    }

    #[test]
    fn test_hard_limit_still_truncates_multi_entity_chunks() {
        // Over-hard chunks truncate regardless of entity count.
        use cce_types::entity::EntityId;

        let config = ChunkingConfig {
            max_bm25_terms: 10,
            ..Default::default()
        };
        let text = words(30);
        let mut chunk = ChunkedResult::new(
            "group_2_bm25_0".to_string(),
            "group_2".to_string(),
            ChunkPath::Bm25,
            0,
            1,
        );
        chunk.text = text;
        chunk.metadata.code_metadata = Some(cce_types::ast_to_nl::CodeSpecificMetadata {
            content_entity_ids: vec![EntityId(1), EntityId(2)],
            ..Default::default()
        });
        let mut chunks = vec![chunk];
        super::enforce_hard_limit(&mut chunks, &config, "multi.rs")
            .expect("hard limit must not drop the file");
        assert!(
            config.cost(&chunks[0].text, ChunkPath::Bm25) <= 10,
            "multi-entity chunk must fit the hard limit"
        );
        assert!(
            chunks[0].truncated,
            "truncation must set the truncated flag"
        );
    }

    #[test]
    fn test_hard_limit_truncates_tail_and_keeps_file() {
        let config = ChunkingConfig {
            max_tokens: 512,
            ..Default::default()
        };
        let text = words(600);
        assert!(
            config.cost(&text, ChunkPath::Embedding) > 512,
            "fixture must exceed the hard limit"
        );
        let mut chunk = ChunkedResult::new(
            "group_1474_emb_0".to_string(),
            "group_1474".to_string(),
            ChunkPath::Embedding,
            0,
            1,
        );
        chunk.text = text;
        let mut chunks = vec![chunk];
        super::enforce_hard_limit(&mut chunks, &config, "PipelineTests.cs")
            .expect("hard limit must not drop the file");
        assert_eq!(chunks.len(), 1);
        assert!(
            config.cost(&chunks[0].text, ChunkPath::Embedding) <= 512,
            "truncated chunk must fit the hard limit"
        );
        assert!(!chunks[0].text.is_empty(), "truncation must keep the head");
        assert!(
            chunks[0].truncated,
            "truncation must set the truncated flag"
        );
    }

    #[test]
    fn test_hard_limit_compliant_chunk_passes_through() {
        let config = ChunkingConfig::default();
        let mut chunk = ChunkedResult::new(
            "g_emb_0".to_string(),
            "g".to_string(),
            ChunkPath::Embedding,
            0,
            1,
        );
        chunk.text = words(10);
        let mut chunks = vec![chunk];
        super::enforce_hard_limit(&mut chunks, &config, "a.cs").expect("compliant chunk must pass");
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].text.contains("w9"));
        assert!(
            !chunks[0].truncated,
            "compliant chunk must not set the truncated flag"
        );
    }
}
