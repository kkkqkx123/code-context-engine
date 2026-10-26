//! Chunk-level limit checks.
//!
//! The pre-injection check is advisory: it records a violation but keeps the
//! chunk, because the injection is still bounded by the absolute hard limit.
//! The hard-limit check is enforcing: a chunk that crosses its configured
//! hard limit is a deterministic splitting or plugin bug and must not reach
//! storage.

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
/// absolute hard limit.
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
        if actual > budget.pre_injection_limit {
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

/// Reject any chunk that still exceeds its configured hard limit.
///
/// Injection is bounded by the absolute final text, so this is a guard
/// against a plugin-produced chunk, an incorrect budget, or a future
/// assembly path that bypasses the pre-injection check. It runs in release
/// builds as well; a silent overflow is not an acceptable degradation.
pub(crate) fn enforce_hard_limit(
    chunks: &mut [ChunkedResult],
    config: &ChunkingConfig,
    file_path: &str,
    _has_vocabulary: bool,
    _emb_decoration: usize,
    _bm25_decoration: usize,
) -> Result<(), ParseError> {
    for chunk in chunks {
        let limit = config.limit(chunk.path);
        let actual = config.cost(&chunk.text, chunk.path);
        if limit > 0 && actual > limit {
            return Err(ParseError::code_splitting(format!(
                "chunk exceeds the hard limit (file={}, group={}, chunk={}, path={}, cost={}, hard={})",
                file_path, chunk.source_group_id, chunk.chunk_id, chunk.path, actual, limit
            )));
        }
        chunk.token_count = actual;
        if chunk.metadata.bm25_word_count.is_some() {
            chunk.metadata.bm25_word_count = Some(config.cost(&chunk.text, ChunkPath::Bm25));
        }
    }
    Ok(())
}
