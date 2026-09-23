//! Shared candidate-arbitration helpers.
//!
//! Provides the deterministic file-level predicates used when a name has
//! several cross-file candidates: test-file classification (reusing the
//! single authoritative rule set in `cce_types::TestInfo`) and module
//! affinity (longest common directory prefix with the caller file).

use cce_types::{TestInfo, language::LanguageInfo};

/// Whether `path` is a test file under the shared path-pattern rules
/// (directory segments like `tests/`, plus per-language file-name rules).
pub(crate) fn is_test_path(path: &str) -> bool {
    let info = LanguageInfo::detect_from_path(path);
    TestInfo::from_path(Some(&info.language), path).is_test()
}

/// Directory segments of a normalized project path, file name excluded.
pub(crate) fn directory_segments(path: &str) -> Vec<String> {
    let normalized = cce_types::normalize_project_path(path);
    let mut segments: Vec<String> = normalized.split('/').map(|s| s.to_string()).collect();
    segments.pop();
    segments
}

/// Number of leading directory segments shared by two paths.
pub(crate) fn common_prefix_len(left: &[String], right: &[String]) -> usize {
    left.iter()
        .zip(right.iter())
        .take_while(|(a, b)| a == b)
        .count()
}

/// Rank candidate file paths against a caller file: order by longest common
/// directory prefix, then prefer non-test files. Returns the index of a
/// unique winner, or `None` when the candidates stay ambiguous
/// (abstain rather than pick arbitrarily).
///
/// `caller_file` may be `None`; then only the test-file preference applies.
pub(crate) fn unique_candidate_by_affinity(
    caller_file: Option<&str>,
    candidate_paths: &[Option<String>],
) -> Option<usize> {
    let total = candidate_paths.len();
    if total == 0 {
        return None;
    }
    if total == 1 {
        return Some(0);
    }

    let mut pool: Vec<usize> = (0..total).collect();

    if let Some(caller) = caller_file {
        let caller_dir = directory_segments(caller);
        let score = |idx: usize| -> usize {
            candidate_paths[idx]
                .as_deref()
                .map(|p| common_prefix_len(&caller_dir, &directory_segments(p)))
                .unwrap_or(0)
        };
        let best = pool.iter().map(|i| score(*i)).max().unwrap_or(0);
        if best > 0 {
            let narrowed: Vec<usize> = pool.iter().copied().filter(|i| score(*i) == best).collect();
            if narrowed.len() == 1 {
                return Some(narrowed[0]);
            }
            pool = narrowed;
        }
    }

    let non_test: Vec<usize> = pool
        .iter()
        .copied()
        .filter(|i| !candidate_paths[*i].as_deref().is_some_and(is_test_path))
        .collect();
    if !non_test.is_empty() && non_test.len() < pool.len() {
        if non_test.len() == 1 {
            return Some(non_test[0]);
        }
        pool = non_test;
    }

    if pool.len() == 1 { Some(pool[0]) } else { None }
}
