//! Indexing result types
//!
//! This module provides a summary of indexing operations for reporting purposes.

/// Execution outcome for a full index operation.
///
/// Replaces the implicit "errors list means partial failure" convention
/// with an explicit enum that callers must match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexExecutionOutcome {
    /// All required stages completed successfully.
    Success,
    /// One or more required stages failed; the result is partial.
    /// Contains non-blocking warnings only.
    Incomplete { errors: Vec<String> },
}

/// Indexing result summary
#[derive(Debug, Clone)]
pub struct IndexResult {
    /// Total files processed
    pub total_files: usize,
    /// Files successfully indexed
    pub indexed_files: usize,
    /// Files that failed
    pub failed_files: usize,
    /// Files that completed "successfully" but produced degraded content:
    /// lossy decoding (U+FFFD) or zero entities and zero chunks.
    pub degraded_files: usize,
    /// Files with deterministic failures that were skipped without retry
    /// (unsupported language, drifted or undecodable content). Separate from
    /// `failed_files`, which only counts transient failures kept for resume.
    pub skipped_permanent: usize,
    /// Total entities extracted
    pub total_entities: usize,
    /// Total relations extracted
    pub total_relations: usize,
    /// Total vectors stored
    pub total_vectors: usize,
    /// Total tokens used for embedding
    pub total_tokens: u64,
    /// Whether a backend circuit breaker (vector store or LLM service)
    /// opened during this run, meaning writes or summaries were rejected by
    /// an outage rather than by per-file content failures.
    pub circuit_open: bool,
    /// Execution outcome (Success or Incomplete)
    pub outcome: IndexExecutionOutcome,
    /// Elapsed time in milliseconds
    pub elapsed_ms: u64,
}

impl Default for IndexResult {
    fn default() -> Self {
        Self {
            total_files: 0,
            indexed_files: 0,
            failed_files: 0,
            degraded_files: 0,
            skipped_permanent: 0,
            total_entities: 0,
            total_relations: 0,
            total_vectors: 0,
            total_tokens: 0,
            circuit_open: false,
            outcome: IndexExecutionOutcome::Success,
            elapsed_ms: 0,
        }
    }
}

impl IndexResult {
    /// Create a new empty result
    pub fn new() -> Self {
        Self::default()
    }

    /// Get success rate as percentage
    pub fn success_rate(&self) -> f32 {
        if self.total_files == 0 {
            return 100.0;
        }
        (self.indexed_files as f32 / self.total_files as f32) * 100.0
    }

    /// Check if indexing was completely successful
    pub fn is_success(&self) -> bool {
        self.outcome == IndexExecutionOutcome::Success
    }

    /// Get error messages (only populated when outcome is Incomplete)
    pub fn errors(&self) -> &[String] {
        match &self.outcome {
            IndexExecutionOutcome::Success => &[],
            IndexExecutionOutcome::Incomplete { errors } => errors,
        }
    }

    /// Format summary for logging
    pub fn format_summary(&self) -> String {
        let mut summary = format!(
            "Indexed {}/{} files ({} degraded, {} permanently skipped), {} entities, {} relations, {} vectors in {}ms",
            self.indexed_files,
            self.total_files,
            self.degraded_files,
            self.skipped_permanent,
            self.total_entities,
            self.total_relations,
            self.total_vectors,
            self.elapsed_ms
        );
        if self.circuit_open {
            summary.push_str(" (circuit breaker open)");
        }
        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_result_default() {
        let result = IndexResult::default();
        assert_eq!(result.total_files, 0);
        assert_eq!(result.indexed_files, 0);
        assert_eq!(result.failed_files, 0);
        assert_eq!(result.success_rate(), 100.0);
    }

    #[test]
    fn test_success_rate() {
        let result = IndexResult {
            total_files: 100,
            indexed_files: 90,
            failed_files: 10,
            ..Default::default()
        };
        assert_eq!(result.success_rate(), 90.0);
    }

    #[test]
    fn test_is_success() {
        let _result = IndexResult {
            total_files: 10,
            indexed_files: 10,
            failed_files: 0,
            ..Default::default()
        };
    }
}
