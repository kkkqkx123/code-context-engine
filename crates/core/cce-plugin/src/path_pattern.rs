//! The path pattern dialect shared by plugin routing and glossary scoping.
//!
//! Both routing levels match a pattern against the project-relative path of
//! a file, so both speak this one dialect: a pattern that carries a wildcard
//! is a glob, a pattern without one is a literal path suffix.
//!
//! An invalid glob is unusable and selects nothing. It never degrades into a
//! literal suffix: a pattern the host cannot parse states a constraint the
//! host does not understand, and guessing a different meaning for it routes
//! files by a rule nobody wrote.

use globset::{Glob, GlobMatcher};

/// One compiled path pattern.
#[derive(Debug, Clone)]
pub struct PathPattern {
    matcher: Matcher,
}

/// The matcher of a compiled pattern.
#[derive(Debug, Clone)]
enum Matcher {
    /// A glob pattern.
    Glob(GlobMatcher),
    /// A literal path suffix.
    Suffix(String),
}

impl PathPattern {
    /// Compile one pattern.
    ///
    /// Returns `None` when the pattern is unusable: a pattern that carries a
    /// wildcard must be a valid glob.
    pub fn compile(pattern: &str) -> Option<Self> {
        if pattern.contains('*') || pattern.contains('?') || pattern.contains('[') {
            let glob = Glob::new(pattern).ok()?;
            Some(Self {
                matcher: Matcher::Glob(glob.compile_matcher()),
            })
        } else {
            Some(Self {
                matcher: Matcher::Suffix(pattern.to_string()),
            })
        }
    }

    /// Whether the pattern selects `path`.
    pub fn matches(&self, path: &str) -> bool {
        match &self.matcher {
            Matcher::Glob(matcher) => matcher.is_match(path),
            Matcher::Suffix(suffix) => path.ends_with(suffix),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_a_glob_selects_by_pattern() {
        let pattern = PathPattern::compile("packages/*/src/**").expect("glob");
        assert!(pattern.matches("packages/player/src/a.c"));
        assert!(!pattern.matches("packages/player/test/a.c"));
    }

    #[test]
    fn test_a_pattern_without_a_wildcard_is_a_path_suffix() {
        let pattern = PathPattern::compile("player.c").expect("suffix");
        assert!(pattern.matches("src/player.c"));
        assert!(!pattern.matches("src/recorder.c"));
    }

    #[test]
    fn test_an_invalid_glob_is_unusable() {
        assert!(
            PathPattern::compile("[").is_none(),
            "a glob the host cannot parse must not compile"
        );
    }

    #[test]
    fn test_a_glob_crosses_the_path_separator() {
        let pattern = PathPattern::compile("*.c").expect("glob");
        assert!(
            pattern.matches("packages/player/src/a.c"),
            "a single star spans the directories, as the globset default does"
        );
    }
}
