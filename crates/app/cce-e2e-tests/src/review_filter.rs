//! Output-side filter options for review-fixture exports.
//!
//! Mirrors the semantics of the main query path's `RelationQueryOptions`
//! file filters: disabled by default, enabled per export job. Indexing always
//! covers every file; filtering only applies when results are written out, so
//! exports keep full graph-build fidelity while reports stay reviewable.
//!
//! Test-file detection reuses the single authoritative rule set
//! (`cce_types::TestInfo`), shared with the main query path.

use std::collections::HashSet;

use cce_types::language::LanguageInfo;

#[derive(Debug, Clone, Default)]
pub struct ReviewFilterOptions {
    /// Exclude entities and edges located in test files.
    pub exclude_tests: bool,
    /// Keep only files under this directory prefix.
    pub directory_prefix: Option<String>,
    /// Exact file paths to exclude.
    pub excluded_files: Vec<String>,
}

impl ReviewFilterOptions {
    pub fn with_exclude_tests(mut self, exclude: bool) -> Self {
        self.exclude_tests = exclude;
        self
    }

    pub fn with_directory_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.directory_prefix = Some(prefix.into());
        self
    }

    pub fn with_excluded_files(mut self, files: Vec<String>) -> Self {
        self.excluded_files = files;
        self
    }

    /// Normalized view used by path checks; empty options never exclude.
    fn normalized(&self) -> NormalizedFilter {
        NormalizedFilter {
            directory_prefix: self
                .directory_prefix
                .as_deref()
                .map(|p| cce_types::normalize_project_path(p.trim_matches('/'))),
            exclude_tests: self.exclude_tests,
            excluded_files: self
                .excluded_files
                .iter()
                .map(|f| cce_types::normalize_project_path(f))
                .collect(),
        }
    }

    /// Whether the given project-relative file path must be filtered out.
    pub fn is_excluded_path(&self, path: &str) -> bool {
        self.normalized().is_excluded(path)
    }
}

struct NormalizedFilter {
    directory_prefix: Option<String>,
    exclude_tests: bool,
    excluded_files: HashSet<String>,
}

impl NormalizedFilter {
    fn is_excluded(&self, path: &str) -> bool {
        let normalized = cce_types::normalize_project_path(path);
        if let Some(prefix) = &self.directory_prefix {
            let inside = normalized == *prefix || normalized.starts_with(&format!("{prefix}/"));
            if !inside {
                return true;
            }
        }
        if self.exclude_tests {
            let info = LanguageInfo::detect_from_path(&normalized);
            if cce_types::TestInfo::from_path(Some(&info.language), &normalized).is_test() {
                return true;
            }
        }
        self.excluded_files.contains(&normalized)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_excludes_nothing() {
        let f = ReviewFilterOptions::default();
        assert!(!f.is_excluded_path("src/flask/ctx.py"));
        assert!(!f.is_excluded_path("tests/test_reqctx.py"));
    }

    #[test]
    fn exclude_tests_drops_test_paths() {
        let f = ReviewFilterOptions::default().with_exclude_tests(true);
        assert!(!f.is_excluded_path("src/flask/ctx.py"));
        assert!(f.is_excluded_path("tests/test_reqctx.py"));
    }

    #[test]
    fn directory_prefix_keeps_only_inside() {
        let f = ReviewFilterOptions::default().with_directory_prefix("src/flask");
        assert!(!f.is_excluded_path("src/flask/ctx.py"));
        assert!(f.is_excluded_path("tests/test_reqctx.py"));
        // A path equal to the prefix is inside it and is kept.
        assert!(!f.is_excluded_path("src/flask"));
    }

    #[test]
    fn excluded_files_match_exactly() {
        let f = ReviewFilterOptions::default()
            .with_excluded_files(vec!["src/flask/app.py".to_string()]);
        assert!(f.is_excluded_path("src/flask/app.py"));
        assert!(!f.is_excluded_path("src/flask/ctx.py"));
    }
}
