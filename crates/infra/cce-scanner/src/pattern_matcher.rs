//! Pattern matching for file filtering
//!
//! This module provides glob pattern matching functionality for include/exclude
//! patterns in file system scanning. It supports both simple globs and
//! recursive globstar patterns (**).

use std::path::{Path, PathBuf};
use tracing::debug;

use crate::ignore::IgnoreMatcher;
use cce_utils::Glob;

/// Options for loading pattern matcher
#[derive(Debug, Clone, Default)]
pub struct PatternLoadOptions {
    /// Include patterns (glob patterns)
    pub include_patterns: Vec<String>,
    /// Exclude patterns (glob patterns)
    pub exclude_patterns: Vec<String>,
    /// Whether to respect .gitignore files
    pub respect_gitignore: bool,
    /// Additional .gitignore-style patterns
    pub gitignore_patterns: Vec<String>,
    /// Path to .gitignore file (if not in root)
    pub gitignore_path: Option<PathBuf>,
}

/// Pattern matcher for file filtering
///
/// Combines include patterns, exclude patterns, and gitignore matching
/// to determine whether files and directories should be processed.
#[derive(Debug, Clone, Default)]
pub struct PatternMatcher {
    /// Include patterns (compiled globs)
    include_patterns: Vec<Glob>,
    /// Exclude patterns (compiled globs)
    exclude_patterns: Vec<Glob>,
    /// Gitignore matcher (optional)
    gitignore: Option<IgnoreMatcher>,
}

impl PatternMatcher {
    /// Create a new pattern matcher
    pub fn new(include_patterns: Vec<String>, exclude_patterns: Vec<String>) -> Self {
        let include_globs = include_patterns
            .into_iter()
            .filter_map(|p| Glob::new(&p).ok())
            .collect();
        let exclude_globs = exclude_patterns
            .into_iter()
            .filter_map(|p| Glob::new(&p).ok())
            .collect();

        Self {
            include_patterns: include_globs,
            exclude_patterns: exclude_globs,
            gitignore: None,
        }
    }

    /// Create a new pattern matcher with gitignore support
    pub fn with_gitignore(
        include_patterns: Vec<String>,
        exclude_patterns: Vec<String>,
        gitignore: Option<IgnoreMatcher>,
    ) -> Self {
        let include_globs = include_patterns
            .into_iter()
            .filter_map(|p| Glob::new(&p).ok())
            .collect();
        let exclude_globs = exclude_patterns
            .into_iter()
            .filter_map(|p| Glob::new(&p).ok())
            .collect();

        Self {
            include_patterns: include_globs,
            exclude_patterns: exclude_globs,
            gitignore,
        }
    }

    /// Load pattern matcher from options
    ///
    /// This method handles loading .gitignore files and merging custom patterns.
    pub fn from_options(opts: &PatternLoadOptions, root_path: &Path) -> Self {
        let gitignore = Self::load_ignore(opts, root_path);
        Self::with_gitignore(
            opts.include_patterns.clone(),
            opts.exclude_patterns.clone(),
            gitignore,
        )
    }

    /// Load ignore matcher from options
    fn load_ignore(opts: &PatternLoadOptions, root_path: &Path) -> Option<IgnoreMatcher> {
        if !opts.respect_gitignore && opts.gitignore_patterns.is_empty() {
            return None;
        }

        let mut matcher = None;

        if opts.respect_gitignore {
            let ignore_path = if let Some(ref path) = opts.gitignore_path {
                path.clone()
            } else {
                root_path.join(".gitignore")
            };

            let path_display = ignore_path.display().to_string();

            if ignore_path.exists() {
                match IgnoreMatcher::from_file(&ignore_path) {
                    Ok(file_matcher) => {
                        debug!(
                            path = %path_display,
                            patterns_count = file_matcher.pattern_count(),
                            "Loaded ignore file"
                        );
                        matcher = Some(file_matcher);
                    }
                    Err(e) => {
                        debug!(
                            path = %path_display,
                            error = %e,
                            "Failed to load ignore file"
                        );
                    }
                }
            } else {
                debug!(path = %path_display, "ignore file not found");
            }
        }

        // Merge custom gitignore patterns
        if !opts.gitignore_patterns.is_empty() {
            let custom_matcher = IgnoreMatcher::new(opts.gitignore_patterns.clone(), root_path);
            match matcher {
                Some(mut m) => {
                    m.merge(custom_matcher);
                    matcher = Some(m);
                }
                None => matcher = Some(custom_matcher),
            }
        }

        matcher
    }

    /// Check if a file should be included based on patterns
    ///
    /// # Logic
    /// 1. If include patterns exist, file must match at least one
    /// 2. File must not match any exclude pattern
    /// 3. File must not be ignored by gitignore
    pub fn should_include_file(&self, path: &Path) -> bool {
        // Check include patterns first
        if !self.include_patterns.is_empty() {
            let matched = self.match_any(path, &self.include_patterns);

            if !matched {
                return false;
            }
        }

        // Check exclude patterns
        if self.match_any(path, &self.exclude_patterns) {
            return false;
        }

        // Check gitignore patterns
        if let Some(matcher) = &self.gitignore {
            if matcher.is_ignored(path) {
                return false;
            }

            // Check parent directories
            let mut current = path.parent();
            while let Some(parent) = current {
                if matcher.is_ignored(parent) {
                    return false;
                }
                current = parent.parent();
            }
        }

        true
    }

    /// Check if a directory should be excluded
    ///
    /// Directories are excluded if they match exclude patterns or gitignore
    pub fn should_exclude_dir(&self, path: &Path) -> bool {
        // Check exclude patterns
        if self.match_any(path, &self.exclude_patterns) {
            return true;
        }

        // Check gitignore patterns
        if let Some(matcher) = &self.gitignore {
            if matcher.is_ignored(path) {
                return true;
            }
        }

        false
    }

    /// Match a path against a compiled glob pattern
    fn match_glob(&self, path: &Path, glob: &Glob) -> bool {
        glob.is_match(path)
    }

    /// Match a path against a list of compiled globs
    fn match_any(&self, path: &Path, globs: &[Glob]) -> bool {
        globs.iter().any(|g| self.match_glob(path, g))
    }

    /// Get include patterns count
    pub fn include_pattern_count(&self) -> usize {
        self.include_patterns.len()
    }

    /// Get exclude patterns count
    pub fn exclude_pattern_count(&self) -> usize {
        self.exclude_patterns.len()
    }

    /// Get gitignore pattern count
    pub fn gitignore_pattern_count(&self) -> usize {
        self.gitignore
            .as_ref()
            .map(|m| m.pattern_count())
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_matcher(include: Vec<String>, exclude: Vec<String>, _root: &Path) -> PatternMatcher {
        PatternMatcher::new(include, exclude)
    }

    #[test]
    fn test_should_include_file_no_patterns() {
        let temp_dir = TempDir::new().unwrap();
        let matcher = create_matcher(vec![], vec![], temp_dir.path());

        assert!(matcher.should_include_file(Path::new("test.txt")));
        assert!(matcher.should_include_file(Path::new("any/file.rs")));
    }

    #[test]
    fn test_should_include_file_with_include_pattern() {
        let temp_dir = TempDir::new().unwrap();
        let matcher = create_matcher(vec!["*.rs".to_string()], vec![], temp_dir.path());

        assert!(matcher.should_include_file(Path::new("test.rs")));
        assert!(!matcher.should_include_file(Path::new("test.txt")));
    }

    #[test]
    fn test_should_include_file_with_exclude_pattern() {
        let temp_dir = TempDir::new().unwrap();
        let matcher = create_matcher(vec![], vec!["*.log".to_string()], temp_dir.path());

        assert!(!matcher.should_include_file(Path::new("test.log")));
        assert!(matcher.should_include_file(Path::new("test.rs")));
    }

    #[test]
    fn test_should_include_file_include_and_exclude() {
        let temp_dir = TempDir::new().unwrap();
        let matcher = create_matcher(
            vec!["*.rs".to_string()],
            vec!["test.rs".to_string()],
            temp_dir.path(),
        );

        assert!(!matcher.should_include_file(Path::new("test.rs")));
        assert!(matcher.should_include_file(Path::new("other.rs")));
        assert!(!matcher.should_include_file(Path::new("test.txt")));
    }

    #[test]
    fn test_should_include_file_recursive_globstar() {
        let temp_dir = TempDir::new().unwrap();
        let matcher = create_matcher(vec!["**/*.rs".to_string()], vec![], temp_dir.path());

        assert!(matcher.should_include_file(Path::new("test.rs")));
        assert!(matcher.should_include_file(Path::new("src/test.rs")));
        assert!(matcher.should_include_file(Path::new("src/nested/deep/file.rs")));
        assert!(!matcher.should_include_file(Path::new("test.txt")));
    }

    #[test]
    fn test_should_exclude_dir() {
        let temp_dir = TempDir::new().unwrap();
        let matcher = create_matcher(vec![], vec!["node_modules".to_string()], temp_dir.path());

        assert!(matcher.should_exclude_dir(Path::new("node_modules")));
        assert!(!matcher.should_exclude_dir(Path::new("src")));
    }

    #[test]
    fn test_should_include_file_with_gitignore() {
        let temp_dir = TempDir::new().unwrap();

        // Create gitignore file
        let gitignore_path = temp_dir.path().join(".gitignore");
        std::fs::write(&gitignore_path, "*.ignored\n").unwrap();

        let gitignore = IgnoreMatcher::from_file(&gitignore_path).unwrap();
        let matcher = PatternMatcher::with_gitignore(vec![], vec![], Some(gitignore));

        assert!(!matcher.should_include_file(&temp_dir.path().join("test.ignored")));
        assert!(matcher.should_include_file(&temp_dir.path().join("test.txt")));
    }

    #[test]
    fn test_should_exclude_dir_with_gitignore() {
        let temp_dir = TempDir::new().unwrap();

        let target_dir = temp_dir.path().join("target");
        std::fs::create_dir(&target_dir).unwrap();

        let gitignore = IgnoreMatcher::new(vec!["target/".to_string()], temp_dir.path());
        let matcher = PatternMatcher::with_gitignore(vec![], vec![], Some(gitignore));

        assert!(matcher.should_exclude_dir(&target_dir));
        assert!(!matcher.should_exclude_dir(&temp_dir.path().join("src")));
    }

    #[test]
    fn test_pattern_counts() {
        let temp_dir = TempDir::new().unwrap();
        let matcher = create_matcher(
            vec!["*.rs".to_string(), "*.toml".to_string()],
            vec!["target".to_string()],
            temp_dir.path(),
        );

        assert_eq!(matcher.include_pattern_count(), 2);
        assert_eq!(matcher.exclude_pattern_count(), 1);
        assert_eq!(matcher.gitignore_pattern_count(), 0);
    }

    #[test]
    fn test_from_options_no_ignore() {
        let temp_dir = TempDir::new().unwrap();
        let opts = PatternLoadOptions {
            include_patterns: vec!["*.rs".to_string()],
            exclude_patterns: vec!["*.log".to_string()],
            respect_gitignore: false,
            gitignore_patterns: vec![],
            gitignore_path: None,
        };

        let matcher = PatternMatcher::from_options(&opts, temp_dir.path());
        assert_eq!(matcher.include_pattern_count(), 1);
        assert_eq!(matcher.exclude_pattern_count(), 1);
        assert_eq!(matcher.gitignore_pattern_count(), 0);
    }

    #[test]
    fn test_from_options_with_gitignore_file() {
        let temp_dir = TempDir::new().unwrap();

        // Create gitignore file
        let gitignore_path = temp_dir.path().join(".gitignore");
        std::fs::write(&gitignore_path, "*.ignored\n").unwrap();

        let opts = PatternLoadOptions {
            include_patterns: vec![],
            exclude_patterns: vec![],
            respect_gitignore: true,
            gitignore_patterns: vec![],
            gitignore_path: None,
        };

        let matcher = PatternMatcher::from_options(&opts, temp_dir.path());
        assert!(matcher.gitignore_pattern_count() > 0);
        assert!(!matcher.should_include_file(&temp_dir.path().join("test.ignored")));
    }

    #[test]
    fn test_from_options_with_custom_patterns() {
        let temp_dir = TempDir::new().unwrap();

        let opts = PatternLoadOptions {
            include_patterns: vec![],
            exclude_patterns: vec![],
            respect_gitignore: false,
            gitignore_patterns: vec!["*.tmp".to_string(), "build/".to_string()],
            gitignore_path: None,
        };

        let matcher = PatternMatcher::from_options(&opts, temp_dir.path());
        assert_eq!(matcher.gitignore_pattern_count(), 2);
        assert!(!matcher.should_include_file(&temp_dir.path().join("test.tmp")));

        // Create the build directory so is_dir() returns true
        let build_dir = temp_dir.path().join("build");
        std::fs::create_dir(&build_dir).unwrap();
        assert!(matcher.should_exclude_dir(&build_dir));
    }

    #[test]
    fn test_from_options_merge_gitignore_and_custom() {
        let temp_dir = TempDir::new().unwrap();

        // Create gitignore file
        let gitignore_path = temp_dir.path().join(".gitignore");
        std::fs::write(&gitignore_path, "*.log\n").unwrap();

        let opts = PatternLoadOptions {
            include_patterns: vec![],
            exclude_patterns: vec![],
            respect_gitignore: true,
            gitignore_patterns: vec!["*.tmp".to_string()],
            gitignore_path: None,
        };

        let matcher = PatternMatcher::from_options(&opts, temp_dir.path());
        // Should have patterns from both gitignore file and custom patterns
        assert!(matcher.gitignore_pattern_count() >= 2);
        assert!(!matcher.should_include_file(&temp_dir.path().join("test.log")));
        assert!(!matcher.should_include_file(&temp_dir.path().join("test.tmp")));
    }
}
