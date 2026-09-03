//! Unified import classifier
//!
//! Provides a single point for classifying imports across all languages.

use crate::parser::stdlib::{
    GoStdlibDetector, JavaScriptStdlibDetector, PythonStdlibDetector, RustStdlibDetector,
    StdlibDetector,
};
use cce_types::language::Language;

use super::{
    ClassificationMetadata, ExtractionContext, ImportClass, ImportClassification,
    StandardizedImport,
};

/// Unified import classifier
///
/// Provides a single point for classifying imports across all languages.
pub struct ImportClassifier;

impl ImportClassifier {
    /// Classify multiple imports in batch
    ///
    /// More efficient than individual classification due to:
    /// - Batch stdlib lookups
    /// - Reduced allocations
    /// - Better cache utilization
    pub fn classify_batch(
        imports: &[StandardizedImport],
        context: &ExtractionContext,
        language: Language,
    ) -> Vec<ImportClassification> {
        // Group by source for batch processing
        let mut results = Vec::with_capacity(imports.len());

        // Batch stdlib detection
        let sources: Vec<&str> = imports.iter().map(|i| i.source.as_str()).collect();
        let stdlib_flags = Self::batch_is_stdlib(&sources, language);

        // Classify each import
        for (import, is_stdlib) in imports.iter().zip(stdlib_flags.iter()) {
            results.push(Self::classify_single(import, context, language, *is_stdlib));
        }

        results
    }

    /// Classify a single import with pre-computed stdlib flag
    fn classify_single(
        import: &StandardizedImport,
        context: &ExtractionContext,
        language: Language,
        is_stdlib: bool,
    ) -> ImportClassification {
        let path = &import.source;

        // 1. Check if it's a standard library import
        if is_stdlib {
            return ImportClassification::new(import.clone(), ImportClass::StandardLibrary)
                .with_confidence(1.0);
        }

        // 2. Check if it's an internal import
        if context.is_internal_import(path, language) {
            return ImportClassification::new(import.clone(), ImportClass::InternalModule)
                .with_confidence(1.0)
                .with_metadata(ClassificationMetadata {
                    resolved_path: Some(path.to_string()),
                    ..Default::default()
                });
        }

        // 3. Default to external package (including system headers)
        let package_name = Self::extract_package_name(path, language);
        ImportClassification::new(import.clone(), ImportClass::ExternalPackage)
            .with_confidence(0.9)
            .with_metadata(ClassificationMetadata {
                package_name,
                ..Default::default()
            })
    }

    /// Check if an import is from the standard library
    fn is_stdlib_import(path: &str, language: Language) -> bool {
        match language {
            Language::Python => {
                let module = path.split('.').next().unwrap_or(path);
                PythonStdlibDetector::is_stdlib_module(module)
            }
            Language::Rust => RustStdlibDetector::STDLIB_CRATES
                .iter()
                .any(|&prefix| path == prefix || path.starts_with(&format!("{}::", prefix))),
            Language::Java => {
                path.starts_with("java.") || path.starts_with("javax.") || path.starts_with("jdk.")
            }
            Language::JavaScript | Language::TypeScript => {
                JavaScriptStdlibDetector::is_node_stdlib(path)
            }
            Language::Go => {
                let package = path.split('/').next().unwrap_or(path);
                GoStdlibDetector::is_stdlib_package(package)
            }
            Language::C | Language::Cpp => {
                // C/C++ stdlib is handled by is_system_header flag
                false
            }
            Language::Kotlin | Language::Scala => {
                // JVM languages use Java stdlib
                path.starts_with("java.")
                    || path.starts_with("javax.")
                    || path.starts_with("kotlin.")
                    || path.starts_with("scala.")
            }
            Language::CSharp => {
                // C# stdlib detection
                path.starts_with("System.") || path.starts_with("Microsoft.")
            }
            Language::Php => {
                // PHP stdlib detection
                StdlibDetector::is_stdlib_call(path, &language)
            }
            Language::Ruby => {
                // Ruby stdlib detection
                StdlibDetector::is_stdlib_call(path, &language)
            }
            Language::Dart => {
                // Dart stdlib detection
                path.starts_with("dart.")
            }
            _ => false,
        }
    }

    /// Batch check if imports are from the standard library
    fn batch_is_stdlib(paths: &[&str], language: Language) -> Vec<bool> {
        paths
            .iter()
            .map(|&path| Self::is_stdlib_import(path, language))
            .collect()
    }

    /// Extract package name from import path
    fn extract_package_name(path: &str, language: Language) -> Option<String> {
        match language {
            Language::Python => path.split('.').next().map(|s| s.to_string()),
            Language::Java | Language::Kotlin | Language::Scala => {
                path.split('.').next().map(|s| s.to_string())
            }
            Language::JavaScript | Language::TypeScript => {
                // Handle scoped packages: @org/package
                if path.starts_with('@') {
                    Some(path.split('/').take(2).collect::<Vec<_>>().join("/"))
                } else {
                    path.split('/').next().map(|s| s.to_string())
                }
            }
            Language::Go => {
                // Handle: github.com/org/repo/pkg
                Some(path.split('/').take(3).collect::<Vec<_>>().join("/"))
            }
            Language::Rust => path.split("::").next().map(|s| s.to_string()),
            Language::C | Language::Cpp => {
                // Extract package from include path
                path.split('/').next().map(|s| s.to_string())
            }
            Language::CSharp => {
                // Extract namespace
                path.split('.').next().map(|s| s.to_string())
            }
            Language::Php | Language::Ruby | Language::Dart => {
                path.split('\\').next().map(|s| s.to_string())
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ImportKind;
    use std::path::PathBuf;

    #[test]
    fn test_python_stdlib_import() {
        let context = ExtractionContext::default();
        let import = StandardizedImport::new(ImportKind::SymbolImport, "os.path");

        let results = ImportClassifier::classify_batch(
            std::slice::from_ref(&import),
            &context,
            Language::Python,
        );
        let classification = &results[0];
        assert_eq!(classification.class, ImportClass::StandardLibrary);
        assert_eq!(classification.confidence, 1.0);
    }

    #[test]
    fn test_python_external_import() {
        let context = ExtractionContext::default();
        let import = StandardizedImport::new(ImportKind::SymbolImport, "numpy.array");

        let results = ImportClassifier::classify_batch(
            std::slice::from_ref(&import),
            &context,
            Language::Python,
        );
        let classification = &results[0];
        assert_eq!(classification.class, ImportClass::ExternalPackage);
        assert_eq!(classification.confidence, 0.9);
        assert_eq!(
            classification.metadata.package_name,
            Some("numpy".to_string())
        );
    }

    #[test]
    fn test_rust_stdlib_import() {
        let context = ExtractionContext::default();
        let import = StandardizedImport::new(ImportKind::SymbolImport, "std::collections::HashMap");

        let results = ImportClassifier::classify_batch(
            std::slice::from_ref(&import),
            &context,
            Language::Rust,
        );
        let classification = &results[0];
        assert_eq!(classification.class, ImportClass::StandardLibrary);
    }

    #[test]
    fn test_rust_external_import() {
        let context = ExtractionContext::default();
        let import = StandardizedImport::new(ImportKind::SymbolImport, "serde::Serialize");

        let results = ImportClassifier::classify_batch(
            std::slice::from_ref(&import),
            &context,
            Language::Rust,
        );
        let classification = &results[0];
        assert_eq!(classification.class, ImportClass::ExternalPackage);
        assert_eq!(
            classification.metadata.package_name,
            Some("serde".to_string())
        );
    }

    #[test]
    fn test_java_stdlib_import() {
        let context = ExtractionContext::default();
        let import = StandardizedImport::new(ImportKind::SymbolImport, "java.util.ArrayList");

        let results = ImportClassifier::classify_batch(
            std::slice::from_ref(&import),
            &context,
            Language::Java,
        );
        let classification = &results[0];
        assert_eq!(classification.class, ImportClass::StandardLibrary);
    }

    #[test]
    fn test_java_internal_import() {
        let context = ExtractionContext {
            file_path: PathBuf::from("/project/src/com/example/myapp/Main.java"),
            project_root: PathBuf::from("/project"),
            current_module: Some("com.example.myapp.Main".to_string()),
            config: cce_parser_core::extraction::ExtractionConfig {
                project_packages: vec!["com.example.myapp".to_string()],
                ..Default::default()
            },
            ..Default::default()
        };

        let import =
            StandardizedImport::new(ImportKind::SymbolImport, "com.example.myapp.utils.Helper");

        let results = ImportClassifier::classify_batch(
            std::slice::from_ref(&import),
            &context,
            Language::Java,
        );
        let classification = &results[0];
        assert_eq!(classification.class, ImportClass::InternalModule);
    }

    #[test]
    fn test_javascript_stdlib_import() {
        let context = ExtractionContext::default();
        let import = StandardizedImport::new(ImportKind::SymbolImport, "fs");

        let results = ImportClassifier::classify_batch(
            std::slice::from_ref(&import),
            &context,
            Language::JavaScript,
        );
        let classification = &results[0];
        assert_eq!(classification.class, ImportClass::StandardLibrary);
    }

    #[test]
    fn test_javascript_scoped_package() {
        let context = ExtractionContext::default();
        let import = StandardizedImport::new(ImportKind::SymbolImport, "@types/node");

        let results = ImportClassifier::classify_batch(
            std::slice::from_ref(&import),
            &context,
            Language::TypeScript,
        );
        let classification = &results[0];
        assert_eq!(classification.class, ImportClass::ExternalPackage);
        assert_eq!(
            classification.metadata.package_name,
            Some("@types/node".to_string())
        );
    }

    #[test]
    fn test_go_stdlib_import() {
        let context = ExtractionContext::default();
        let import = StandardizedImport::new(ImportKind::SymbolImport, "fmt");

        let results =
            ImportClassifier::classify_batch(std::slice::from_ref(&import), &context, Language::Go);
        let classification = &results[0];
        assert_eq!(classification.class, ImportClass::StandardLibrary);
    }

    #[test]
    fn test_go_external_import() {
        let context = ExtractionContext::default();
        let import = StandardizedImport::new(ImportKind::SymbolImport, "github.com/gin-gonic/gin");

        let results =
            ImportClassifier::classify_batch(std::slice::from_ref(&import), &context, Language::Go);
        let classification = &results[0];
        assert_eq!(classification.class, ImportClass::ExternalPackage);
        assert_eq!(
            classification.metadata.package_name,
            Some("github.com/gin-gonic/gin".to_string())
        );
    }

    #[test]
    fn test_c_system_header() {
        let context = ExtractionContext::default();
        let mut import = StandardizedImport::new(ImportKind::Include, "stdio.h");
        import.is_system_header = true;

        let results =
            ImportClassifier::classify_batch(std::slice::from_ref(&import), &context, Language::C);
        let classification = &results[0];
        assert_eq!(classification.class, ImportClass::ExternalPackage);
    }

    #[test]
    fn test_batch_classification() {
        let context = ExtractionContext::default();
        let imports = vec![
            StandardizedImport::new(ImportKind::SymbolImport, "os"),
            StandardizedImport::new(ImportKind::SymbolImport, "numpy"),
            StandardizedImport::new(ImportKind::SymbolImport, "sys"),
        ];

        let classifications =
            ImportClassifier::classify_batch(&imports, &context, Language::Python);

        assert_eq!(classifications.len(), 3);
        assert_eq!(classifications[0].class, ImportClass::StandardLibrary);
        assert_eq!(classifications[1].class, ImportClass::ExternalPackage);
        assert_eq!(classifications[2].class, ImportClass::StandardLibrary);
    }
}
