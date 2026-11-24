//! Assertions for E2E index results
//!
//! Provides small assertion helpers used by workflow tests.

use cce_orchestrator::IndexResult;

/// Expected high-level index result constraints.
#[derive(Debug, Clone, Default)]
pub struct ExpectedIndexResult {
    /// Minimum number of files expected to be processed.
    pub min_files: Option<usize>,
    /// Minimum number of entities expected to be extracted.
    pub min_entities: Option<usize>,
    /// Minimum number of relations expected to be extracted.
    pub min_relations: Option<usize>,
    /// Minimum number of vectors expected to be stored.
    pub min_vectors: Option<usize>,
    /// Whether the result must contain no errors.
    pub no_errors: bool,
}

/// Assert that an [`IndexResult`] satisfies the expected constraints.
pub fn assert_index_result(result: &IndexResult, expected: ExpectedIndexResult) {
    if let Some(min_files) = expected.min_files {
        assert!(
            result.total_files >= min_files,
            "Expected at least {} files, got {}",
            min_files,
            result.total_files
        );
    }

    if let Some(min_entities) = expected.min_entities {
        assert!(
            result.total_entities >= min_entities,
            "Expected at least {} entities, got {}",
            min_entities,
            result.total_entities
        );
    }

    if let Some(min_relations) = expected.min_relations {
        assert!(
            result.total_relations >= min_relations,
            "Expected at least {} relations, got {}",
            min_relations,
            result.total_relations
        );
    }

    if let Some(min_vectors) = expected.min_vectors {
        assert!(
            result.total_vectors >= min_vectors,
            "Expected at least {} vectors, got {}",
            min_vectors,
            result.total_vectors
        );
    }

    if expected.no_errors {
        assert!(
            result.errors().is_empty(),
            "Expected no errors, got {:?}",
            result.errors()
        );
        assert_eq!(
            result.failed_files, 0,
            "Expected no failed files, got {}",
            result.failed_files
        );
    }
}
