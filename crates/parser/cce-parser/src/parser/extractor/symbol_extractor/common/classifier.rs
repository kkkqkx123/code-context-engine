//! Unified import classifier
//!
//! The single place that answers "what kind of import is this": standard
//! library, third-party package, or project module. Consumers (symbol table,
//! import statistics, external-call attribution) read this answer instead of
//! each carrying their own prefix list or split heuristic.
//!
//! The classifier deliberately knows nothing about manifests: whether a
//! package is one the project declared is index-layer knowledge, not a
//! property of the import's shape. Callers that hold that knowledge refine
//! [`ImportClass::ExternalPackage`] themselves.

use crate::parser::stdlib::StdlibDetector;
use cce_types::ImportTable;
use cce_types::import::ImportSourceStats;
use cce_types::language::Language;

use super::{
    ClassificationMetadata, ExtractionContext, ImportClass, ImportClassification,
    StandardizedImport,
};

/// Unified import classifier
pub struct ImportClassifier;

impl ImportClassifier {
    /// Classify multiple imports in batch
    ///
    /// Thin loop over [`Self::classify`]: the per-import work is a table
    /// lookup, so batching used to buy nothing and cost a second code path.
    pub fn classify_batch(
        imports: &[StandardizedImport],
        context: &ExtractionContext,
        language: Language,
    ) -> Vec<ImportClassification> {
        imports
            .iter()
            .map(|import| Self::classify(import, context, language))
            .collect()
    }

    /// Classify a whole table once and record the counts on it.
    ///
    /// `ImportTable::source_stats` has existed since the table type was added
    /// and has always been zero: four buckets were declared, nothing ever
    /// filled them, so "how much of this file is standard library" had no
    /// answer. Both extraction paths call this, so the counts describe the
    /// same classification the rest of the pipeline reads.
    pub fn fill_source_stats(
        table: &mut ImportTable,
        context: &ExtractionContext,
        language: Language,
    ) {
        // `system_imports` is a C/C++ concept: a header the compiler finds
        // outside the project. Other extractors reuse `is_system_header` to
        // carry the stdlib marker (`is_system_header: is_stdlib` in Python,
        // Go, PHP, Ruby, Lua), so counting it there would report the same
        // import twice — once as stdlib, once as system.
        let system_imports = if matches!(language, Language::C | Language::Cpp) {
            table
                .standardized_imports
                .iter()
                .filter(|import| import.is_system_header)
                .count()
        } else {
            0
        };
        let mut stats = ImportSourceStats {
            total_imports: table.standardized_imports.len(),
            system_imports,
            ..Default::default()
        };
        for import in &table.standardized_imports {
            match Self::classify(import, context, language).class {
                ImportClass::StandardLibrary => stats.stdlib_imports += 1,
                ImportClass::ExternalPackage => stats.external_imports += 1,
                ImportClass::InternalModule => stats.internal_imports += 1,
                ImportClass::Unknown => {}
            }
        }
        table.source_stats = stats;
    }

    /// Classify one import.
    ///
    /// Order matters: stdlib first (a `java.util.List` import is never
    /// internal, whatever the project's package prefixes say), then C/C++
    /// system headers, then project-local resolution, then the fallback.
    pub fn classify(
        import: &StandardizedImport,
        context: &ExtractionContext,
        language: Language,
    ) -> ImportClassification {
        let path = &import.source;

        // 1. Standard library. The table lookup normalizes quoting, so the
        //    JS family's `'fs'` and Dart's `'dart:async'` arrive as paths.
        if StdlibDetector::is_stdlib_import_path(path, &language) {
            return ImportClassification::new(import.clone(), ImportClass::StandardLibrary)
                .with_confidence(1.0);
        }

        // 2. C/C++ system headers (including standard library headers) are
        //    never project-local, even when their cleaned path looks like a
        //    relative specifier; the `is_system_header` flag takes precedence.
        if matches!(language, Language::C | Language::Cpp) && import.is_system_header {
            return ImportClassification::new(import.clone(), ImportClass::ExternalPackage)
                .with_confidence(0.9)
                .with_metadata(ClassificationMetadata {
                    package_name: Self::extract_package_name(path, language),
                    ..Default::default()
                });
        }

        // 3. Project-local module.
        if context.is_internal_import(path, language) {
            return ImportClassification::new(import.clone(), ImportClass::InternalModule)
                .with_confidence(1.0)
                .with_metadata(ClassificationMetadata {
                    resolved_path: Some(path.to_string()),
                    ..Default::default()
                });
        }

        // 4. Anything else is presumed third-party — the only other place an
        //    import can resolve to in a language with a package ecosystem.
        //    Without a package name there is no such ecosystem to speak of
        //    (`source lib.sh`, unknown plugin languages), so the answer is
        //    honestly `Unknown` rather than an invented package.
        match Self::extract_package_name(path, language) {
            Some(package_name) => {
                ImportClassification::new(import.clone(), ImportClass::ExternalPackage)
                    .with_confidence(0.9)
                    .with_metadata(ClassificationMetadata {
                        package_name: Some(package_name),
                        ..Default::default()
                    })
            }
            None => {
                ImportClassification::new(import.clone(), ImportClass::Unknown).with_confidence(0.5)
            }
        }
    }

    /// Extract the distribution/package name an import belongs to.
    ///
    /// Language-specific roots: the first dot segment for dotted namespaces
    /// (Python, JVM, C#, Lua), the first slash segment for slash-separated
    /// ones (Go, C/C++, PHP, Ruby), `@scope/name` kept whole for npm, and
    /// Dart's `package:` URI reduced to the package it names.
    ///
    /// `None` means the language has no package concept to speak of; the
    /// caller turns that into [`ImportClass::Unknown`].
    fn extract_package_name(path: &str, language: Language) -> Option<String> {
        let first = |sep: char| -> Option<String> {
            path.split(sep)
                .next()
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        };
        match language {
            Language::Python
            | Language::Java
            | Language::Kotlin
            | Language::Scala
            | Language::CSharp
            | Language::Lua => first('.'),
            Language::JavaScript | Language::TypeScript | Language::Jsx | Language::Tsx => {
                // Scoped packages (`@org/pkg`) span two segments.
                if path.starts_with('@') {
                    Some(path.split('/').take(2).collect::<Vec<_>>().join("/"))
                } else {
                    first('/')
                }
            }
            Language::Go => {
                // `github.com/org/repo/pkg` — three segments name the module.
                Some(path.split('/').take(3).collect::<Vec<_>>().join("/"))
                    .filter(|s| !s.is_empty())
            }
            Language::Rust => path.split("::").next().map(str::to_string),
            Language::C | Language::Cpp => first('/'),
            Language::Php | Language::Ruby => first('/').or_else(|| first('\\')),
            Language::Dart => {
                // `package:my_app/main.dart` -> `my_app`; anything else
                // (`dart:async`, `file:///…`) has no package behind it.
                let rest = path.strip_prefix("package:")?;
                rest.split('/').next().map(str::to_string)
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
    fn test_js_family_import_parity() {
        let context = ExtractionContext::default();
        for language in [
            Language::JavaScript,
            Language::TypeScript,
            Language::Jsx,
            Language::Tsx,
        ] {
            let import = StandardizedImport::new(ImportKind::SymbolImport, "fs");
            let results =
                ImportClassifier::classify_batch(std::slice::from_ref(&import), &context, language);
            assert_eq!(
                results[0].class,
                ImportClass::StandardLibrary,
                "fs must be stdlib for {language:?}"
            );
            let scoped = StandardizedImport::new(ImportKind::SymbolImport, "@types/node");
            let results =
                ImportClassifier::classify_batch(std::slice::from_ref(&scoped), &context, language);
            assert_eq!(
                results[0].metadata.package_name,
                Some("@types/node".to_string()),
                "scoped package must parse for {language:?}"
            );
        }
    }

    #[test]
    fn test_import_tables_catch_what_prefix_lists_missed() {
        // Three regressions against the removed local prefix list: a bare
        // namespace (`using System;`), a `dart:` URI and a Ruby require all
        // used to fall through to `ExternalPackage`.
        let cases: &[(Language, ImportKind, &str)] = &[
            (Language::CSharp, ImportKind::NamespaceImport, "System"),
            (
                Language::CSharp,
                ImportKind::NamespaceImport,
                "System.Collections.Generic",
            ),
            (Language::Dart, ImportKind::SymbolImport, "dart:async"),
            (Language::Ruby, ImportKind::CommonJSRequire, "json"),
        ];
        for (language, kind, path) in cases {
            let context = ExtractionContext::default();
            let import = StandardizedImport::new(*kind, *path);
            let results = ImportClassifier::classify_batch(
                std::slice::from_ref(&import),
                &context,
                *language,
            );
            assert_eq!(
                results[0].class,
                ImportClass::StandardLibrary,
                "{path} must classify as stdlib for {language:?}"
            );
        }
    }

    #[test]
    fn test_quoted_specifiers_reach_the_tables() {
        // Extractors keep the quotes they saw; the stdlib lookup strips them.
        // Before that, the JS family and Dart had no stdlib detection at all.
        let context = ExtractionContext::default();
        for (language, kind, path) in [
            (Language::JavaScript, ImportKind::SymbolImport, "'node:fs'"),
            (Language::TypeScript, ImportKind::SymbolImport, "\"fs\""),
            (Language::Dart, ImportKind::SymbolImport, "'dart:async'"),
            (Language::Ruby, ImportKind::CommonJSRequire, "'json'"),
        ] {
            let import = StandardizedImport::new(kind, path);
            let classification = ImportClassifier::classify(&import, &context, language);
            assert_eq!(
                classification.class,
                ImportClass::StandardLibrary,
                "{path} in {language:?}"
            );
        }
    }

    #[test]
    fn test_package_name_follows_the_language_layout() {
        let context = ExtractionContext::default();
        let cases: &[(Language, &str, &str)] = &[
            (
                Language::Go,
                "github.com/gin-gonic/gin/binding",
                "github.com/gin-gonic/gin",
            ),
            (Language::TypeScript, "@types/node", "@types/node"),
            (Language::TypeScript, "lodash/fp", "lodash"),
            (Language::Dart, "package:my_app/main.dart", "my_app"),
            (Language::Ruby, "sinatra/base", "sinatra"),
            (
                Language::Php,
                "Symfony\\Component\\HttpFoundation",
                "Symfony\\Component\\HttpFoundation",
            ),
            (Language::Python, "requests.api", "requests"),
            (Language::Rust, "serde::Serialize", "serde"),
        ];
        for (language, path, expected) in cases {
            let import = StandardizedImport::new(ImportKind::SymbolImport, *path);
            let classification = ImportClassifier::classify(&import, &context, *language);
            assert_eq!(classification.class, ImportClass::ExternalPackage, "{path}");
            assert_eq!(
                classification.metadata.package_name.as_deref(),
                Some(*expected),
                "{path} in {language:?}"
            );
        }
    }

    #[test]
    fn test_no_package_concept_is_unknown_not_external() {
        // `source lib.sh` is neither stdlib nor project-local by shape, and
        // shell has no package to attribute it to. Reporting `ExternalPackage`
        // would invent a dependency.
        let context = ExtractionContext::default();
        let import = StandardizedImport::new(ImportKind::SideEffectImport, "lib.sh");
        let classification = ImportClassifier::classify(&import, &context, Language::Bash);
        assert_eq!(classification.class, ImportClass::Unknown);
    }

    #[test]
    fn test_relative_import_is_internal_before_package_split() {
        let context = ExtractionContext::default();
        for (language, path) in [
            (Language::TypeScript, "./helpers"),
            (Language::Python, ".sibling"),
            (Language::Rust, "crate::core::thing"),
        ] {
            let import = StandardizedImport::new(ImportKind::SymbolImport, path);
            let classification = ImportClassifier::classify(&import, &context, language);
            assert_eq!(
                classification.class,
                ImportClass::InternalModule,
                "{path} in {language:?}"
            );
            assert_eq!(classification.metadata.resolved_path.as_deref(), Some(path));
        }
    }

    #[test]
    fn test_fill_source_stats() {
        let context = ExtractionContext::default();

        let mut python = ImportTable {
            standardized_imports: vec![
                StandardizedImport::new(ImportKind::ModuleImport, "os"),
                StandardizedImport::new(ImportKind::ModuleImport, "requests"),
                StandardizedImport::new(ImportKind::SymbolImport, "."),
            ],
            ..Default::default()
        };
        ImportClassifier::fill_source_stats(&mut python, &context, Language::Python);
        let stats = &python.source_stats;
        assert_eq!(stats.total_imports, 3);
        assert_eq!(stats.stdlib_imports, 1, "os");
        assert_eq!(stats.external_imports, 1, "requests");
        assert_eq!(stats.internal_imports, 1, "relative");
        assert_eq!(stats.system_imports, 0);

        // A C system header is counted as system, not stdlib: the C tables
        // answer `false` for import paths by design and the parser flag is the
        // authority there.
        let mut header = StandardizedImport::new(ImportKind::Include, "stdio.h");
        header.is_system_header = true;
        let mut c_table = ImportTable {
            standardized_imports: vec![header],
            ..Default::default()
        };
        ImportClassifier::fill_source_stats(&mut c_table, &context, Language::C);
        assert_eq!(c_table.source_stats.total_imports, 1);
        assert_eq!(c_table.source_stats.system_imports, 1);
        assert_eq!(c_table.source_stats.stdlib_imports, 0);
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
