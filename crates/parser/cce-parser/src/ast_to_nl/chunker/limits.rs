//! Chunk-level limit checks.
//!
//! The pre-injection check is advisory: it records a violation but keeps the
//! chunk, because the injection is still bounded by the absolute hard limit.
//! The hard-limit check truncates the tail so storage never receives an
//! oversized chunk; the truncation is warned, therefore not silent, and it
//! keeps the file instead of dropping it.

use cce_types::ParseError;

use super::config::ChunkingConfig;
use super::result::{ChunkPath, ChunkedResult};

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
    Ok(chunks)
}

/// Truncate any chunk that still exceeds its configured hard limit.
///
/// The splitter and the decoration cap keep compliant assembly within the
/// hard limit. This is the last resort for a plugin-produced chunk or a
/// future assembly path that bypassed the body budget: the tail is cut to
/// the hard limit with a warning, so the file is kept and storage never
/// receives an oversized chunk. Truncated chunks carry the truncated flag
/// so callers know the tail was lost.
pub(crate) fn enforce_hard_limit(
    chunks: &mut [ChunkedResult],
    config: &ChunkingConfig,
    file_path: &str,
) -> Result<(), ParseError> {
    for chunk in chunks {
        let limit = config.limit(chunk.path);
        let actual = config.cost(&chunk.text, chunk.path);
        if limit > 0 && actual > limit {
            tracing::warn!(
                file = file_path,
                group = chunk.source_group_id.as_str(),
                chunk = %chunk.chunk_id,
                cost = actual,
                hard = limit,
                "chunk exceeds the hard limit; truncating tail to keep the file"
            );
            let truncated = cce_text::truncate_to_cost(&chunk.text, config.unit(chunk.path), limit);
            chunk.text = truncated.to_string();
            chunk.truncated = true;
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
