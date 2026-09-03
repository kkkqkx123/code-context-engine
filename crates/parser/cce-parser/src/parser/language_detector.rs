//! Language detector for identifying programming language from file path
//!
//! This module provides a lightweight wrapper around `LanguageInfo::detect_from_path()`
//! for convenience and backward compatibility.
//!
//! Note: The core language detection logic is implemented in `types::language::LanguageInfo`.
//! This struct exists primarily for API compatibility and convenience in contexts where
//! an instance-based detector is preferred (e.g., dependency injection, testing).
//!
//! # Examples
//!
//! Using the detector instance:
//! ```
//! use cce_parser::parser::LanguageDetector;
//!
//! let detector = LanguageDetector::new();
//! let info = detector.detect("src/main.rs").expect("Failed to detect language");
//! ```
//!
//! Using the static method (recommended):
//! ```
//! use cce_types::language::LanguageInfo;
//!
//! let info = LanguageInfo::detect_from_path("src/main.rs");
//! ```

use cce_types::ParseError;
use cce_types::language::LanguageInfo;

/// Language detector using compile-time static matching
///
/// This is a lightweight wrapper around `LanguageInfo::detect_from_path()`.
/// For most use cases, prefer using `LanguageInfo::detect_from_path()` directly.
pub struct LanguageDetector;

impl LanguageDetector {
    /// Create a new language detector
    pub fn new() -> Self {
        Self
    }

    /// Detect language from file path
    ///
    /// This method delegates to `LanguageInfo::detect_from_path()`.
    ///
    /// # Arguments
    /// * `file_path` - Path to the file (absolute or relative)
    ///
    /// # Returns
    /// * `Ok(LanguageInfo)` - Detected language information
    /// * `Err(ParseError)` - If path is invalid
    ///
    /// # Examples
    /// ```
    /// use cce_parser::parser::LanguageDetector;
    ///
    /// let detector = LanguageDetector::new();
    /// let info = detector.detect("src/main.rs").expect("Failed to detect language");
    /// assert_eq!(info.language, cce_types::language::Language::Rust);
    /// ```
    pub fn detect(&self, file_path: &str) -> Result<LanguageInfo, ParseError> {
        // Delegate to the static method in LanguageInfo
        let language_info = LanguageInfo::detect_from_path(file_path);

        // Convert empty extensions to an error for backward compatibility
        if language_info.extensions.is_empty() {
            return Err(ParseError::language_detection(
                "Failed to detect language: invalid file path".to_string(),
            ));
        }

        Ok(language_info)
    }
}

impl Default for LanguageDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::language::{FileType, Language};

    #[test]
    fn test_detect_c_files() {
        let detector = LanguageDetector::new();

        let info = detector.detect("src/main.c").expect("Failed to detect");
        assert_eq!(info.language, Language::C);
        assert_eq!(info.file_type, FileType::Source);

        let info = detector
            .detect("include/header.h")
            .expect("Failed to detect");
        assert_eq!(info.language, Language::C);
        assert_eq!(info.file_type, FileType::Header);
    }

    #[test]
    fn test_detect_cpp_files() {
        let detector = LanguageDetector::new();

        let info = detector.detect("src/main.cpp").expect("Failed to detect");
        assert_eq!(info.language, Language::Cpp);
        assert_eq!(info.file_type, FileType::Source);

        let info = detector.detect("src/main.cc").expect("Failed to detect");
        assert_eq!(info.language, Language::Cpp);

        let info = detector
            .detect("include/header.hpp")
            .expect("Failed to detect");
        assert_eq!(info.language, Language::Cpp);
        assert_eq!(info.file_type, FileType::Header);
    }

    #[test]
    fn test_detect_javascript_files() {
        let detector = LanguageDetector::new();

        let info = detector.detect("src/main.js").expect("Failed to detect");
        assert_eq!(info.language, Language::JavaScript);

        let info = detector.detect("src/main.mjs").expect("Failed to detect");
        assert_eq!(info.language, Language::JavaScript);

        let info = detector
            .detect("src/component.jsx")
            .expect("Failed to detect");
        assert_eq!(info.language, Language::Jsx);
    }

    #[test]
    fn test_detect_typescript_files() {
        let detector = LanguageDetector::new();

        let info = detector.detect("src/main.ts").expect("Failed to detect");
        assert_eq!(info.language, Language::TypeScript);

        let info = detector
            .detect("src/component.tsx")
            .expect("Failed to detect");
        assert_eq!(info.language, Language::Tsx);
    }

    #[test]
    fn test_detect_rust_files() {
        let detector = LanguageDetector::new();

        let info = detector.detect("src/main.rs").expect("Failed to detect");
        assert_eq!(info.language, Language::Rust);
        assert_eq!(info.file_type, FileType::Source);
    }

    #[test]
    fn test_detect_python_files() {
        let detector = LanguageDetector::new();

        let info = detector.detect("main.py").expect("Failed to detect");
        assert_eq!(info.language, Language::Python);
        assert_eq!(info.file_type, FileType::Source);

        let info = detector.detect("module.pyi").expect("Failed to detect");
        assert_eq!(info.language, Language::Python);
        assert_eq!(info.file_type, FileType::Header);
    }

    #[test]
    fn test_detect_go_files() {
        let detector = LanguageDetector::new();

        let info = detector.detect("main.go").expect("Failed to detect");
        assert_eq!(info.language, Language::Go);
    }

    #[test]
    fn test_detect_java_files() {
        let detector = LanguageDetector::new();

        let info = detector.detect("Main.java").expect("Failed to detect");
        assert_eq!(info.language, Language::Java);
    }

    #[test]
    fn test_detect_kotlin_files() {
        let detector = LanguageDetector::new();

        let info = detector.detect("Main.kt").expect("Failed to detect");
        assert_eq!(info.language, Language::Kotlin);

        let info = detector.detect("build.kts").expect("Failed to detect");
        assert_eq!(info.language, Language::Kotlin);
    }

    #[test]
    fn test_detect_unknown_files() {
        let detector = LanguageDetector::new();

        let info = detector.detect("README.txt").expect("Failed to detect");
        assert_eq!(info.language, Language::Unknown);
        assert_eq!(info.file_type, FileType::Text);

        let info = detector.detect("config.toml").expect("Failed to detect");
        // Structured data formats keep their language for config
        // sub-pipeline selection.
        assert_eq!(info.language, Language::Toml);

        let info = detector.detect("no_extension").expect("Failed to detect");
        assert_eq!(info.language, Language::Unknown);
    }

    #[test]
    fn test_is_supported_for_ast() {
        assert!(Language::C.is_supported_for_ast());
        assert!(Language::Cpp.is_supported_for_ast());
        assert!(Language::Rust.is_supported_for_ast());
        assert!(Language::Python.is_supported_for_ast());
        assert!(Language::JavaScript.is_supported_for_ast());
        assert!(Language::TypeScript.is_supported_for_ast());
        assert!(Language::Go.is_supported_for_ast());
        assert!(Language::Java.is_supported_for_ast());
        assert!(Language::Ruby.is_supported_for_ast());
        assert!(Language::Php.is_supported_for_ast());

        assert!(!Language::Unknown.is_supported_for_ast());
    }

    #[test]
    fn test_case_sensitive() {
        let detector = LanguageDetector::new();

        // Extensions are case-sensitive
        let info = detector.detect("main.RS").expect("Failed to detect");
        assert_eq!(info.language, Language::Unknown);

        let info = detector.detect("main.PY").expect("Failed to detect");
        assert_eq!(info.language, Language::Unknown);
    }

    #[test]
    fn test_detect_documentation_files() {
        let detector = LanguageDetector::new();

        let info = detector.detect("README.md").expect("Failed to detect");
        assert_eq!(info.language, Language::Unknown);
        assert_eq!(info.file_type, FileType::Documentation);

        let info = detector
            .detect("docs/index.markdown")
            .expect("Failed to detect");
        assert_eq!(info.language, Language::Unknown);
        assert_eq!(info.file_type, FileType::Documentation);

        let info = detector.detect("docs/index.rst").expect("Failed to detect");
        assert_eq!(info.language, Language::Unknown);
        assert_eq!(info.file_type, FileType::Documentation);

        let info = detector
            .detect("docs/index.adoc")
            .expect("Failed to detect");
        assert_eq!(info.language, Language::Unknown);
        assert_eq!(info.file_type, FileType::Documentation);
    }

    #[test]
    fn test_detect_text_files() {
        let detector = LanguageDetector::new();

        let info = detector.detect("notes.txt").expect("Failed to detect");
        assert_eq!(info.language, Language::Unknown);
        assert_eq!(info.file_type, FileType::Text);

        let info = detector.detect("app.log").expect("Failed to detect");
        assert_eq!(info.language, Language::Unknown);
        assert_eq!(info.file_type, FileType::Text);
    }

    #[test]
    fn test_detect_config_files() {
        let detector = LanguageDetector::new();

        let info = detector.detect("config.toml").expect("Failed to detect");
        assert_eq!(info.language, Language::Toml);
        assert_eq!(info.file_type, FileType::Config);

        let info = detector.detect("config.yaml").expect("Failed to detect");
        assert_eq!(info.language, Language::Yaml);
        assert_eq!(info.file_type, FileType::Config);

        let info = detector.detect("config.yml").expect("Failed to detect");
        assert_eq!(info.language, Language::Yaml);
        assert_eq!(info.file_type, FileType::Config);

        let info = detector.detect("config.json").expect("Failed to detect");
        assert_eq!(info.language, Language::Json);
        assert_eq!(info.file_type, FileType::Config);

        let info = detector.detect("config.ini").expect("Failed to detect");
        assert_eq!(info.language, Language::Unknown);
        assert_eq!(info.file_type, FileType::Config);
    }
}
