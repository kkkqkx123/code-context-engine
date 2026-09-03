//! File categorization module
//!
//! Provides file category detection, classification, and helper functions for summary generation.
//!
//! The [`FileCategory`] enum moved to `cce_core` (cross-layer chunk contract);
//! this module re-exports it and keeps the parser-side helpers.

pub use cce_types::ast_to_nl::FileCategory;

use cce_types::{Entity, ParsedFile};

/// Test type for test file categorization
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestType {
    Unit,
    Integration,
    E2E,
    Benchmark,
}

impl TestType {
    pub fn detect(parsed_file: &ParsedFile) -> Self {
        let path = &parsed_file.path;
        let source = &parsed_file.source;

        if path.contains("bench") || source.contains("#[bench]") {
            TestType::Benchmark
        } else if path.contains("e2e") || path.contains("end-to-end") {
            TestType::E2E
        } else if path.contains("integration") || path.contains("tests/") {
            TestType::Integration
        } else {
            TestType::Unit
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            TestType::Unit => "unit-test",
            TestType::Integration => "integration-test",
            TestType::E2E => "e2e-test",
            TestType::Benchmark => "benchmark",
        }
    }
}

// Standalone helper functions for external use

/// Check if file is a core module (main, lib, mod, index)
pub fn is_core_module(path: &str) -> bool {
    FileCategory::is_core_module(path)
}

/// Check if file is a test file
pub fn is_test_file(path: &str) -> bool {
    FileCategory::is_test_file(path)
}

/// Display-tag heuristic: does the path look like a config file
/// (presentation only; never decides the stored category).
pub fn is_config_file(path: &str) -> bool {
    FileCategory::looks_like_config(path)
}

/// Display-tag heuristic: does the path look like documentation
/// (presentation only; never decides the stored category).
pub fn is_documentation(path: &str) -> bool {
    FileCategory::looks_like_documentation(path)
}

/// Check if entity is public based on signature
pub fn is_entity_public(entity: &Entity) -> bool {
    let sig_lower = entity.signature.to_lowercase();
    sig_lower.starts_with("pub ")
        || sig_lower.contains(" pub ")
        || sig_lower.starts_with("public ")
        || sig_lower.starts_with("export ")
        || sig_lower.starts_with("export default ")
}

/// Check if file has any documentation
///
/// Simple check if at least one public entity has doc comments
pub fn has_any_documentation(parsed_file: &ParsedFile) -> bool {
    parsed_file
        .entities
        .iter()
        .any(|e| is_entity_public(e) && e.doc_comment.is_some())
}

/// Check if file is a utility file (mostly pure functions, only std imports)
pub fn is_utility_file(parsed_file: &ParsedFile) -> bool {
    // Must have entities
    if parsed_file.entities.is_empty() {
        return false;
    }

    // Parse AST to check imports
    use crate::parser::ast_parser::AstParser;
    let mut parser = AstParser::new();
    let tree = parser
        .parse_with_tree(&parsed_file.source, &parsed_file.language)
        .ok()
        .map(|(t, _)| t);
    let import_count = if let Some(ref tree) = tree {
        crate::relation_helpers::extract_imports(
            tree,
            &parsed_file.source,
            &parsed_file.language,
            None,
        )
        .map(|t| t.import_count())
        .unwrap_or(0)
    } else {
        0
    };

    // Check for minimal or no imports (suggests utility functions)
    let has_minimal_imports = import_count <= 1;

    // Mostly functions
    let all_functions = parsed_file
        .entities
        .iter()
        .all(|e| e.kind.is_function_like());

    has_minimal_imports && all_functions
}

/// Check if file contains only type definitions (no functions)
pub fn is_definition_only_file(parsed_file: &ParsedFile) -> bool {
    !parsed_file.entities.is_empty()
        && parsed_file
            .entities
            .iter()
            .all(|e| e.kind.is_type_definition())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::{Entity, EntityId, EntityKind, Language};

    #[test]
    fn test_file_category_determine() {
        // Config file detection
        let config_file = ParsedFile::new(Language::Unknown, "config.yaml".to_string(), "");
        assert_eq!(FileCategory::determine(&config_file), FileCategory::Config);

        // Documentation detection
        let doc_file = ParsedFile::new(Language::Unknown, "README.md".to_string(), "");
        assert_eq!(
            FileCategory::determine(&doc_file),
            FileCategory::Documentation
        );

        // Code file
        let code_file = ParsedFile::new(Language::Rust, "src/main.rs".to_string(), "fn main() {}");
        assert_eq!(FileCategory::determine(&code_file), FileCategory::Code);

        // Small code files are still code (no minimal category)
        let empty_file = ParsedFile::new(Language::Rust, "src/empty.rs".to_string(), "");
        assert_eq!(FileCategory::determine(&empty_file), FileCategory::Code);

        // Test files carry the content type, not a test category
        let test_file = ParsedFile::new(Language::Rust, "tests/foo.rs".to_string(), "");
        assert_eq!(FileCategory::determine(&test_file), FileCategory::Code);
        assert!(FileCategory::is_test_file("tests/foo.rs"));
    }

    #[test]
    fn test_file_category_as_str() {
        assert_eq!(FileCategory::Code.as_str(), "code");
        assert_eq!(FileCategory::Config.as_str(), "config");
        assert_eq!(FileCategory::Documentation.as_str(), "documentation");
        assert_eq!(FileCategory::Schema.as_str(), "schema");
    }

    #[test]
    fn test_specialized_file_routing() {
        // Test files use specialized generators
        let test_file = ParsedFile::new(Language::Rust, "tests/foo.rs".to_string(), "");
        assert!(FileCategory::is_specialized_file(&test_file));

        // Generated files use specialized generators
        let generated = ParsedFile::new(
            Language::Rust,
            "src/generated.rs".to_string(),
            "// Code generated by protoc\nfn main() {}".to_string(),
        );
        assert!(FileCategory::is_specialized_file(&generated));
        assert!(FileCategory::should_skip_model_enhancement(&generated));

        // Config/docs use specialized generators and skip model enhancement
        let config = ParsedFile::new(Language::Unknown, "config.yaml".to_string(), "");
        assert!(FileCategory::is_specialized_file(&config));
        assert!(FileCategory::should_skip_model_enhancement(&config));

        // Schema files use specialized generators but stay model-eligible
        let schema = ParsedFile::new(Language::Unknown, "api.proto".to_string(), "");
        assert!(FileCategory::is_specialized_file(&schema));
        assert!(!FileCategory::should_skip_model_enhancement(&schema));

        // Plain code uses group-based generation
        let code = ParsedFile::new(Language::Rust, "src/lib.rs".to_string(), "fn f() {}");
        assert!(!FileCategory::is_specialized_file(&code));
        assert!(!FileCategory::should_skip_model_enhancement(&code));
    }

    #[test]
    fn test_test_type_detect() {
        let benchmark_file = ParsedFile::new(Language::Rust, "benches/bench.rs".to_string(), "");
        assert_eq!(TestType::detect(&benchmark_file), TestType::Benchmark);

        let integration_file =
            ParsedFile::new(Language::Rust, "tests/integration.rs".to_string(), "");
        assert_eq!(TestType::detect(&integration_file), TestType::Integration);

        let unit_file = ParsedFile::new(Language::Rust, "src/lib_test.rs".to_string(), "");
        assert_eq!(TestType::detect(&unit_file), TestType::Unit);
    }

    #[test]
    fn test_generated_file_detection() {
        let generated = ParsedFile::new(
            Language::Rust,
            "src/generated.rs".to_string(),
            "// Code generated by protoc\nfn main() {}".to_string(),
        );
        assert!(FileCategory::is_generated_file(
            &generated.path,
            &generated.source
        ));

        let handwritten = ParsedFile::new(Language::Rust, "src/manual.rs".to_string(), "");
        assert!(!FileCategory::is_generated_file(
            &handwritten.path,
            &handwritten.source
        ));

        // `#`/`<!--`-style markers (Python, Shell, XML, HTML)
        let python_generated = ParsedFile::new(
            Language::Python,
            "src/gen.py".to_string(),
            "# generated by script\nx = 1".to_string(),
        );
        assert!(FileCategory::is_generated_file(
            &python_generated.path,
            &python_generated.source
        ));
        let xml_generated = ParsedFile::new(
            Language::Html,
            "index.html".to_string(),
            "<!-- generated by doxygen -->\n<div/>".to_string(),
        );
        assert!(FileCategory::is_generated_file(
            &xml_generated.path,
            &xml_generated.source
        ));
    }

    #[test]
    fn test_schema_file_detection() {
        let proto = ParsedFile::new(Language::Unknown, "api.proto".to_string(), "");
        assert_eq!(FileCategory::determine(&proto), FileCategory::Schema);

        let graphql = ParsedFile::new(Language::Unknown, "schema.graphql".to_string(), "");
        assert_eq!(FileCategory::determine(&graphql), FileCategory::Schema);
    }

    #[test]
    fn test_is_core_module() {
        assert!(is_core_module("src/main.rs"));
        assert!(is_core_module("src/lib.rs"));
        assert!(is_core_module("src/mod.rs"));
        assert!(is_core_module("src/index.ts"));
        assert!(!is_core_module("src/utils.rs"));
        assert!(!is_core_module("src/helper.rs"));
        assert!(!is_core_module("src/library.rs"));
        assert!(!is_core_module("src/cmd_indexer.rs"));
        assert!(!is_core_module("src/commodity.rs"));
        assert!(!is_core_module("src/imodal.rs"));
    }

    #[test]
    fn test_is_test_file() {
        // Rust conventions: `tests/` segment or `*_test.rs` suffix
        assert!(is_test_file("tests/integration.rs"));
        assert!(is_test_file("src/main_test.rs"));
        // JS/TS conventions: `.spec.*`/`.test.*` suffix or `__tests__/` segment
        assert!(is_test_file("lib.spec.ts"));
        assert!(is_test_file("src/main.test.ts"));
        assert!(is_test_file("user.spec.tsx"));
        assert!(is_test_file("user.test.mts"));
        // Generic `tests/` segment for unknown-extension files
        assert!(is_test_file("tests/readme.md"));
        // Negative cases: exact rules never match substrings
        assert!(!is_test_file("src/test_main.rs"));
        assert!(!is_test_file("src/contest.rs"));
        assert!(!is_test_file("src/testutils.rs"));
        assert!(!is_test_file("src/main.rs"));
    }

    #[test]
    fn test_is_config_file() {
        assert!(is_config_file("config.yaml"));
        assert!(is_config_file("Cargo.toml"));
        assert!(is_config_file(".env"));
        assert!(is_config_file("tsconfig.json"));
        assert!(is_config_file("config/loader.rs"));
        assert!(is_config_file("package-lock.json"));
        // Substring false positives no longer match
        assert!(!is_config_file("src/settings_page.rs"));
        assert!(!is_config_file("src/config_backup.rs"));
        assert!(!is_config_file("main.rs"));
    }

    #[test]
    fn test_is_documentation() {
        assert!(is_documentation("README.md"));
        assert!(is_documentation("CHANGELOG.rst"));
        assert!(is_documentation("docs/guide.adoc"));
        assert!(is_documentation("LICENSE"));
        // Substring false positives no longer match
        assert!(!is_documentation("src/license_generator.rs"));
        assert!(!is_documentation("main.rs"));
    }

    #[test]
    fn test_is_entity_public() {
        let entity = Entity {
            id: EntityId(0),
            kind: EntityKind::Function,
            name: "public_fn".to_string(),
            signature: "pub fn public_fn()".to_string(),
            parameters: Vec::new(),
            return_type: None,
            span: cce_types::Span::default(),
            depth: 0,
            parent: None,
            children: Vec::new(),
            doc_comment: None,
            modifiers: Vec::new(),
            attributes: std::collections::HashMap::new(),
            metadata: std::collections::HashMap::new(),
            is_stdlib: false,
            subtype: None,
            stdlib_category: None,
        };
        assert!(is_entity_public(&entity));

        let entity = Entity {
            id: EntityId(1),
            kind: EntityKind::Function,
            name: "private_fn".to_string(),
            signature: "fn private_fn()".to_string(),
            parameters: Vec::new(),
            return_type: None,
            span: cce_types::Span::default(),
            depth: 0,
            parent: None,
            children: Vec::new(),
            doc_comment: None,
            modifiers: Vec::new(),
            attributes: std::collections::HashMap::new(),
            metadata: std::collections::HashMap::new(),
            is_stdlib: false,
            subtype: None,
            stdlib_category: None,
        };
        assert!(!is_entity_public(&entity));
    }

    #[test]
    fn test_is_utility_file() {
        let mut file = ParsedFile::new(Language::Rust, "src/utils.rs".to_string(), "");

        // Add only functions
        for i in 0..5 {
            let entity = Entity {
                id: EntityId(i),
                kind: EntityKind::Function,
                name: format!("helper{}", i),
                signature: format!("fn helper{ }()", i),
                parameters: Vec::new(),
                return_type: None,
                span: cce_types::Span::default(),
                depth: 0,
                parent: None,
                children: Vec::new(),
                doc_comment: None,
                modifiers: Vec::new(),
                attributes: std::collections::HashMap::new(),
                metadata: std::collections::HashMap::new(),
                is_stdlib: false,
                subtype: None,
                stdlib_category: None,
            };
            file.add_entity(entity);
        }

        assert!(is_utility_file(&file));
    }

    #[test]
    fn test_is_definition_only_file() {
        let mut file = ParsedFile::new(Language::Rust, "src/types.rs".to_string(), "");

        // Add only type definitions
        for i in 0..3 {
            let entity = Entity {
                id: EntityId(i),
                kind: EntityKind::Enum,
                name: format!("Enum{}", i),
                signature: format!("enum Enum{}", i),
                parameters: Vec::new(),
                return_type: None,
                span: cce_types::Span::default(),
                depth: 0,
                parent: None,
                children: Vec::new(),
                doc_comment: None,
                modifiers: Vec::new(),
                attributes: std::collections::HashMap::new(),
                metadata: std::collections::HashMap::new(),
                is_stdlib: false,
                subtype: None,
                stdlib_category: None,
            };
            file.add_entity(entity);
        }

        assert!(is_definition_only_file(&file));
    }
}
