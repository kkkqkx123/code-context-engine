//! Common helper functions for language extractors
//!
//! Provides shared utilities for import/export extraction across all languages.

/// String processing helpers
pub mod string {
    /// Remove surrounding quotes from a string
    ///
    /// Handles single quotes, double quotes, and backticks
    pub fn unquote(s: &str) -> &str {
        s.trim_matches('"').trim_matches('\'').trim_matches('`')
    }
}

/// Import path detection helpers
pub mod path {
    /// Check if a path is a relative import (JavaScript/TypeScript style)
    pub fn is_relative_js(path: &str) -> bool {
        path.starts_with("./") || path.starts_with("../") || path == "." || path == ".."
    }

    /// Check if a path is a relative import (Python style)
    pub fn is_relative_python(path: &str) -> bool {
        path.starts_with('.') || path.starts_with("..")
    }

    /// Check if a path is a relative import (Rust style)
    pub fn is_relative_rust(path: &str) -> bool {
        path.starts_with("crate::")
            || path.starts_with("super::")
            || path.starts_with("self::")
            || path.starts_with("::")
    }

    /// Extract base module name from a dotted path
    ///
    /// Example: "os.path" -> "os"
    pub fn extract_base_module(path: &str) -> &str {
        path.split('.').next().unwrap_or(path)
    }

    /// Check if a path is nested (contains dots)
    pub fn is_nested_path(path: &str) -> bool {
        path.contains('.')
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unquote() {
        assert_eq!(string::unquote("\"hello\""), "hello");
        assert_eq!(string::unquote("'world'"), "world");
        assert_eq!(string::unquote("`template`"), "template");
        assert_eq!(string::unquote("no_quotes"), "no_quotes");
    }

    #[test]
    fn test_is_relative_js() {
        assert!(path::is_relative_js("./module"));
        assert!(path::is_relative_js("../parent"));
        assert!(path::is_relative_js("."));
        assert!(!path::is_relative_js("lodash"));
    }

    #[test]
    fn test_is_relative_python() {
        assert!(path::is_relative_python(".module"));
        assert!(path::is_relative_python("..parent"));
        assert!(!path::is_relative_python("os"));
    }

    #[test]
    fn test_is_relative_rust() {
        assert!(path::is_relative_rust("crate::module"));
        assert!(path::is_relative_rust("super::parent"));
        assert!(path::is_relative_rust("self::local"));
        assert!(!path::is_relative_rust("std::collections"));
    }

    #[test]
    fn test_extract_base_module() {
        assert_eq!(path::extract_base_module("os.path"), "os");
        assert_eq!(path::extract_base_module("sys"), "sys");
        assert_eq!(path::extract_base_module("numpy.array"), "numpy");
    }

    #[test]
    fn test_is_nested_path() {
        assert!(path::is_nested_path("os.path"));
        assert!(!path::is_nested_path("sys"));
    }
}
