//! Ignore pattern matching module
//!
//! This module provides functionality to read and parse ignore files
//! and match file paths against ignore-style patterns.
//! Patterns can be loaded from any source(.gitignore and .indexignore) and merged together.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use cce_utils::Glob;

/// A single ignore pattern
#[derive(Debug, Clone)]
struct Pattern {
    /// The original pattern string
    original: String,
    /// Whether this pattern is negated (starts with !)
    negated: bool,
    /// Whether this pattern is directory-only (ends with /)
    dir_only: bool,
    /// Whether this pattern is anchored (starts with / or contains / before **)
    anchored: bool,
    /// Compiled glob matcher
    glob: Option<Glob>,
}

/// Ignore pattern matcher
#[derive(Clone)]
pub struct IgnoreMatcher {
    patterns: Vec<Pattern>,
    base_dir: PathBuf,
}

impl std::fmt::Debug for IgnoreMatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IgnoreMatcher")
            .field("pattern_count", &self.patterns.len())
            .field("base_dir", &self.base_dir)
            .finish()
    }
}

impl Pattern {
    /// Parse a pattern string and compile the glob
    fn parse(pattern: &str) -> Option<Self> {
        let original = pattern.trim().to_string();
        let pattern_str = original.as_str();

        if pattern_str.is_empty() || pattern_str.starts_with('#') {
            return None;
        }

        let negated = pattern_str.starts_with('!');
        let pattern_str = if negated {
            &pattern_str[1..]
        } else {
            pattern_str
        };

        // In gitignore, a pattern is anchored if it contains a slash (other than a trailing one)
        // or if it explicitly starts with a slash.
        let anchored = pattern_str.starts_with('/')
            || (pattern_str.contains('/') && !pattern_str.ends_with('/'));

        let dir_only = pattern_str.ends_with('/');
        let pattern_str = if let Some(stripped) = pattern_str.strip_prefix('/') {
            stripped
        } else {
            pattern_str
        };
        let pattern_str = if dir_only {
            &pattern_str[..pattern_str.len() - 1]
        } else {
            pattern_str
        };

        // We pass the cleaned pattern to Glob.
        // Note: Our Glob implementation handles anchoring via the leading '/' in the pattern string.
        // Since we stripped it for the 'anchored' flag, we need to decide how to handle this.
        // Let's keep the leading '/' for Glob if it was originally anchored by explicit '/'.
        let glob_pattern = if original.trim().starts_with('/') {
            format!("/{}", pattern_str)
        } else {
            pattern_str.to_string()
        };

        let glob = match Glob::new(&glob_pattern) {
            Ok(glob) => Some(glob),
            Err(e) => {
                tracing::warn!(
                    pattern = %original,
                    error = %e,
                    "Ignore pattern failed to compile; it will never match"
                );
                None
            }
        };

        Some(Self {
            original,
            negated,
            dir_only,
            anchored,
            glob,
        })
    }
}

// ============================================================================
// Pattern Matching Functions
// ============================================================================

/// Check if a path matches a pattern
fn matches_pattern(path: &str, pattern: &Pattern) -> bool {
    if let Some(ref glob) = pattern.glob {
        let p = Path::new(path);

        // If the pattern is anchored (starts with /), it must match from the root of the path
        if pattern.anchored {
            return glob.is_match(p);
        }

        // For unanchored patterns, we check the full path and also just the filename
        // This matches gitignore behavior where "*.log" matches "test.log" and "src/test.log"
        if glob.is_match(p) {
            return true;
        }

        // If the pattern doesn't contain a slash, it should also match against any directory name
        // or filename in the path.
        if !pattern.original.contains('/') {
            let file_name = p.file_name().and_then(|f| f.to_str());
            if let Some(name) = file_name {
                if glob.is_match(Path::new(name)) {
                    return true;
                }
            }
            // Also check intermediate directories
            for ancestor in p.ancestors() {
                if let Some(dir_name) = ancestor.file_name().and_then(|f| f.to_str()) {
                    if file_name.as_ref().is_none_or(|fn_| *fn_ != dir_name)
                        && glob.is_match(Path::new(dir_name))
                    {
                        return true;
                    }
                }
            }
        }

        false
    } else {
        false
    }
}

impl IgnoreMatcher {
    /// Create a new matcher from patterns
    pub fn new(patterns: Vec<String>, base_dir: impl AsRef<Path>) -> Self {
        let parsed_patterns = patterns
            .into_iter()
            .filter_map(|p| {
                let p = p.trim();
                if p.is_empty() || p.starts_with('#') {
                    return None;
                }
                Pattern::parse(p)
            })
            .collect();

        Self {
            patterns: parsed_patterns,
            base_dir: base_dir.as_ref().to_path_buf(),
        }
    }

    /// Create a new matcher from an ignore file
    pub fn from_file(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let file_path = path.as_ref();
        let base_dir = file_path.parent().unwrap_or(Path::new("."));

        let content = std::fs::read_to_string(file_path)?;
        let patterns: Vec<String> = content
            .lines()
            .map(|line| line.trim().to_string())
            .collect();

        Ok(Self::new(patterns, base_dir))
    }

    /// Merge another matcher into this one
    pub fn merge(&mut self, other: IgnoreMatcher) {
        if self.patterns.is_empty() && !other.patterns.is_empty() {
            self.base_dir = other.base_dir;
        }
        self.patterns.extend(other.patterns);
    }

    /// Evaluate patterns without a default verdict.
    ///
    /// The last matching pattern decides; no match yields no opinion so a
    /// shallower directory level keeps its say when matchers are stacked.
    pub fn check(&self, path: &Path) -> Option<bool> {
        // The scanner often passes paths relative to `base_dir` (root
        // stripped). Directory-ness must be resolved against `base_dir` —
        // resolving the relative path against the process CWD would silently
        // disable `dir_only` patterns.
        let relative_path = path.strip_prefix(&self.base_dir).unwrap_or(path);
        let abs_path = if relative_path.as_os_str() == path.as_os_str() {
            self.base_dir.join(path)
        } else {
            path.to_path_buf()
        };

        let path_str = relative_path.to_string_lossy();
        let is_dir = abs_path.is_dir();

        let mut ignored = None;

        for pattern in &self.patterns {
            if pattern.dir_only && !is_dir {
                continue;
            }

            let matched = matches_pattern(&path_str, pattern);

            if matched {
                ignored = Some(!pattern.negated);
            }
        }

        ignored
    }

    /// Check if a path should be ignored
    pub fn is_ignored(&self, path: &Path) -> bool {
        self.check(path).unwrap_or(false)
    }

    /// Get the number of patterns
    pub fn pattern_count(&self) -> usize {
        self.patterns.len()
    }

    /// Patterns that failed to compile and never match.
    /// Such entries leave files indexed that the ignore file meant to
    /// exclude, so callers surface the count instead of staying silent.
    pub fn invalid_pattern_count(&self) -> usize {
        self.patterns.iter().filter(|p| p.glob.is_none()).count()
    }
}

/// Lazily loaded stack of per-directory ignore matchers under one root.
///
/// Level zero is the caller-supplied root matcher (already composed with any
/// custom patterns or custom ignore file path); deeper levels are discovered
/// on demand by reading the configured file names in each ancestor directory.
/// Folding runs root to leaf so deeper rules override shallower ones, and a
/// level with no opinion leaves the inherited verdict untouched.
pub struct IgnoreStack {
    root: PathBuf,
    filenames: Vec<String>,
    root_matcher: IgnoreMatcher,
    levels: Mutex<HashMap<PathBuf, Option<IgnoreMatcher>>>,
}

impl std::fmt::Debug for IgnoreStack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IgnoreStack")
            .field("root", &self.root)
            .field("filenames", &self.filenames)
            .field(
                "cached_levels",
                &self.levels.lock().map(|levels| levels.len()).unwrap_or(0),
            )
            .finish()
    }
}

impl IgnoreStack {
    /// Stack per-directory ignore files under `root` on a composed root matcher.
    pub fn new(root: PathBuf, root_matcher: IgnoreMatcher, filenames: Vec<String>) -> Self {
        Self {
            root,
            filenames,
            root_matcher,
            levels: Mutex::new(HashMap::new()),
        }
    }

    /// Whether the absolute or root-relative `path` is ignored.
    ///
    /// Ancestor directories strictly below the root contribute their own
    /// ignore files with the deepest opinion winning; a directory's own
    /// ignore files never decide the directory itself, only its contents.
    pub fn is_ignored(&self, path: &Path) -> bool {
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.root.join(path)
        };
        let mut verdict = self.root_matcher.check(&absolute);
        // Levels are visited deepest first, so the first opinion found is
        // the deepest one and wins; levels with no opinion defer upward.
        let mut current = absolute.parent();
        while let Some(dir) = current {
            if dir == self.root || !dir.starts_with(&self.root) {
                break;
            }
            if let Some(matcher) = self.level_matcher(dir)
                && let Some(opinion) = matcher.check(&absolute)
            {
                verdict = Some(opinion);
                break;
            }
            current = dir.parent();
        }
        verdict.unwrap_or(false)
    }

    /// Matcher composed from the ignore files directly inside `dir`.
    ///
    /// Missing or unreadable files contribute nothing; the outcome (including
    /// the absence) is cached so every directory pays discovery once.
    fn level_matcher(&self, dir: &Path) -> Option<IgnoreMatcher> {
        let mut cached = self.levels.lock().expect("ignore stack cache poisoned");
        if let Some(entry) = cached.get(dir) {
            return entry.clone();
        }
        let mut merged: Option<IgnoreMatcher> = None;
        for name in &self.filenames {
            let candidate = dir.join(name);
            if !candidate.is_file() {
                continue;
            }
            if let Ok(matcher) = IgnoreMatcher::from_file(&candidate) {
                if let Some(existing) = merged.as_mut() {
                    existing.merge(matcher);
                } else {
                    merged = Some(matcher);
                }
            }
        }
        cached.insert(dir.to_path_buf(), merged.clone());
        merged
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_pattern() {
        let pattern = Pattern::parse("node_modules/").expect("Failed to parse pattern");
        assert!(pattern.dir_only);
        assert!(!pattern.negated);
        assert!(!pattern.anchored);

        let pattern = Pattern::parse("/target").expect("Failed to parse pattern");
        assert!(pattern.anchored);
        assert!(!pattern.dir_only);

        let pattern = Pattern::parse("!important.txt").expect("Failed to parse pattern");
        assert!(pattern.negated);
    }

    #[test]
    fn test_match_simple() {
        let matcher = IgnoreMatcher::new(
            vec!["*.log".to_string(), "node_modules".to_string()],
            Path::new("."),
        );

        assert!(matcher.is_ignored(Path::new("test.log")));
        assert!(matcher.is_ignored(Path::new("node_modules")));
        assert!(!matcher.is_ignored(Path::new("test.txt")));
    }

    #[test]
    fn test_match_anchored() {
        let matcher = IgnoreMatcher::new(vec!["/target".to_string()], Path::new("."));

        assert!(matcher.is_ignored(Path::new("target")));
        assert!(!matcher.is_ignored(Path::new("src/target")));
    }

    #[test]
    fn test_match_globstar() {
        let matcher = IgnoreMatcher::new(vec!["**/*.log".to_string()], Path::new("."));

        assert!(matcher.is_ignored(Path::new("test.log")));
        assert!(matcher.is_ignored(Path::new("src/test.log")));
        assert!(matcher.is_ignored(Path::new("src/nested/test.log")));
    }

    #[test]
    fn test_negation() {
        let matcher = IgnoreMatcher::new(
            vec!["*.log".to_string(), "!important.log".to_string()],
            Path::new("."),
        );

        assert!(!matcher.is_ignored(Path::new("important.log")));
        assert!(matcher.is_ignored(Path::new("test.log")));
    }

    #[test]
    fn test_merge() {
        let mut matcher1 = IgnoreMatcher::new(vec!["*.log".to_string()], Path::new("."));
        let matcher2 = IgnoreMatcher::new(vec!["*.tmp".to_string()], Path::new("."));

        matcher1.merge(matcher2);

        assert!(matcher1.is_ignored(Path::new("test.log")));
        assert!(matcher1.is_ignored(Path::new("test.tmp")));
        assert_eq!(matcher1.pattern_count(), 2);
    }

    #[test]
    fn test_dir_only_pattern_matches_relative_path() {
        // The scanner strips the root prefix before querying the matcher, so
        // `is_ignored` receives a *relative* path. Directory-only patterns
        // must still match: the directory-ness check must resolve against
        // the matcher's base dir, not the process CWD.
        let temp_dir = tempfile::TempDir::new().expect("temp dir");
        let base = temp_dir.path().canonicalize().expect("canonical");
        std::fs::create_dir(base.join("node_modules")).expect("create dir");
        let matcher = IgnoreMatcher::new(vec!["node_modules/".to_string()], &base);
        assert!(matcher.is_ignored(Path::new("node_modules")));
        assert!(!matcher.is_ignored(Path::new("node_modules.js")));
    }

    fn stack_fixture(root: &Path, root_patterns: &[&str]) -> IgnoreStack {
        let patterns = root_patterns.iter().map(|s| s.to_string()).collect();
        IgnoreStack::new(
            root.to_path_buf(),
            IgnoreMatcher::new(patterns, root),
            vec![".gitignore".to_string()],
        )
    }

    #[test]
    fn test_stack_child_reinclude_overrides_parent() {
        let temp_dir = tempfile::TempDir::new().expect("temp dir");
        let base = temp_dir.path().canonicalize().expect("canonical");
        std::fs::create_dir_all(base.join("sub")).expect("sub dir");
        std::fs::write(base.join("sub/.gitignore"), "!keep.log\n").expect("ignore file");
        let stack = stack_fixture(&base, &["*.log"]);
        assert!(!stack.is_ignored(&base.join("sub/keep.log")));
        assert!(stack.is_ignored(&base.join("sub/drop.log")));
        assert!(stack.is_ignored(&base.join("root.log")));
    }

    #[test]
    fn test_stack_parent_ignore_prunes_subtree() {
        // Directory-only patterns match the directory itself; descendants
        // never reach the stack because callers prune top-down.
        let temp_dir = tempfile::TempDir::new().expect("temp dir");
        let base = temp_dir.path().canonicalize().expect("canonical");
        std::fs::create_dir_all(base.join("build")).expect("build dir");
        let stack = stack_fixture(&base, &["build/"]);
        assert!(stack.is_ignored(&base.join("build")));
        assert!(!stack.is_ignored(&base.join("src/main.rs")));
    }

    #[test]
    fn test_stack_directory_own_file_never_decides_itself() {
        // A directory's own ignore file constrains its contents, not the
        // directory itself: the parent verdict still applies.
        let temp_dir = tempfile::TempDir::new().expect("temp dir");
        let base = temp_dir.path().canonicalize().expect("canonical");
        std::fs::create_dir_all(base.join("sub")).expect("sub dir");
        std::fs::write(base.join("sub/.gitignore"), "sub/\n").expect("ignore file");
        let stack = stack_fixture(&base, &[]);
        assert!(!stack.is_ignored(&base.join("sub")));
        assert!(!stack.is_ignored(&base.join("sub/inner.rs")));
    }

    #[test]
    fn test_stack_empty_without_files() {
        let temp_dir = tempfile::TempDir::new().expect("temp dir");
        let base = temp_dir.path().canonicalize().expect("canonical");
        let stack = stack_fixture(&base, &[]);
        assert!(!stack.is_ignored(&base.join("src/main.rs")));
        assert!(!stack.is_ignored(Path::new("src/main.rs")));
    }
}
