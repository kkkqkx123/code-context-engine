//! Standard library entity detector
//!
//! Delegates all stdlib detection to parser/stdlib modules (single source of truth).
//! This file only provides the semantic description generation interface for grouper.

use serde::{Deserialize, Serialize};

use crate::grouper::types::StdlibCategory;
use cce_types::entity::{Entity, EntityId};
use cce_types::language::Language;

/// Standard library entity information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StdlibEntityInfo {
    /// Entity ID
    pub entity_id: EntityId,
    /// Full qualified name (e.g., "std::collections::HashMap")
    pub full_name: String,
    /// Short name (e.g., "HashMap")
    pub short_name: String,
    /// Category of the standard library entity
    pub category: StdlibCategory,
    /// Semantic description
    pub semantic_description: String,
}

impl StdlibEntityInfo {
    /// Create a new StdlibEntityInfo
    pub fn new(
        entity_id: EntityId,
        full_name: String,
        short_name: String,
        category: StdlibCategory,
    ) -> Self {
        let semantic_description = Self::generate_semantic_description(&short_name, category);
        Self {
            entity_id,
            full_name,
            short_name,
            category,
            semantic_description,
        }
    }

    /// Generate semantic description based on name and category
    pub fn generate_semantic_description(name: &str, category: StdlibCategory) -> String {
        format!(
            "Standard library {}: {}",
            category.description_label(),
            name
        )
    }
}

/// Standard library entity detector
///
/// Detects standard library entities and provides semantic information
/// for compression optimization. Detection delegates to the authoritative
/// tables in `cce-stdlib`, mirroring the relation extraction dispatch so
/// both layers agree on the supported language set. Template languages
/// carry no stdlib entities of their own and conservatively return none.
pub struct StdlibEntityDetector;

impl StdlibEntityDetector {
    /// Create a new StdlibEntityDetector
    pub fn new() -> Self {
        Self
    }

    /// Detect if an entity is a standard library entity
    pub fn detect_stdlib_entity(
        &self,
        entity: &Entity,
        language: &Language,
    ) -> Option<StdlibEntityInfo> {
        let category = self.get_stdlib_category(&entity.name, language)?;
        let full_name = match cce_types::language::effective_language(language) {
            Language::Rust => format!("std::{}", entity.name),
            _ => entity.name.clone(),
        };
        Some(StdlibEntityInfo::new(
            entity.id,
            full_name,
            entity.name.clone(),
            category,
        ))
    }

    /// Check if a name is a standard library entity
    pub fn is_stdlib_name(&self, name: &str, language: &Language) -> bool {
        if self.get_stdlib_category(name, language).is_some() {
            return true;
        }
        cce_stdlib::StdlibDetector::is_stdlib_call(name, language)
    }

    /// Get the category for a stdlib name
    ///
    /// Returns the category if the name is a recognized standard library entity,
    /// or None if it's not recognized.
    pub fn get_stdlib_category(&self, name: &str, language: &Language) -> Option<StdlibCategory> {
        // Remapped plugin languages inherit their host grammar's detectors.
        match cce_types::language::effective_language(language) {
            Language::Rust => cce_stdlib::rust::RustStdlibDetector::get_category(name),
            Language::Python => cce_stdlib::python::PythonStdlibDetector::get_category(name),
            Language::JavaScript | Language::TypeScript | Language::Jsx | Language::Tsx => {
                cce_stdlib::javascript::JavaScriptStdlibDetector::get_category(name)
            }
            Language::Go => cce_stdlib::go::GoStdlibDetector::get_category(name),
            Language::Java => cce_stdlib::java::JavaStdlibDetector::get_category(name),
            Language::CSharp => cce_stdlib::csharp::CSharpStdlibDetector::get_category(name),
            Language::C => cce_stdlib::c::CStdlibDetector::get_category(name),
            Language::Cpp => cce_stdlib::cpp::CppStdlibDetector::get_category(name),
            Language::Kotlin => cce_stdlib::kotlin::KotlinStdlibDetector::get_category(name),
            Language::Scala => cce_stdlib::scala::ScalaStdlibDetector::get_category(name),
            Language::Dart => cce_stdlib::dart::DartStdlibDetector::get_category(name),
            Language::Ruby => cce_stdlib::ruby::RubyStdlibDetector::get_category(name),
            Language::Php => cce_stdlib::php::PhpStdlibDetector::get_category(name),
            Language::Lua => cce_stdlib::lua::LuaStdlibDetector::get_category(name),
            Language::Bash => cce_stdlib::bash::BashStdlibDetector::get_category(name),
            _ => None,
        }
    }

    /// Generate semantic description for a stdlib name
    pub fn generate_semantic_description(name: &str, category: StdlibCategory) -> String {
        StdlibEntityInfo::generate_semantic_description(name, category)
    }
}

impl Default for StdlibEntityDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::entity::{Entity, EntityId, EntityKind};
    use cce_types::{Span, language::Language};

    fn create_test_entity(name: &str) -> Entity {
        Entity {
            id: EntityId(1),
            name: name.to_string(),
            kind: EntityKind::Function,
            span: Span::default(),
            ..Default::default()
        }
    }

    #[test]
    fn test_rust_stdlib_detection() {
        let detector = StdlibEntityDetector::new();

        // Test Vec detection
        let entity = create_test_entity("Vec");
        let info = detector.detect_stdlib_entity(&entity, &Language::Rust);
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.category, StdlibCategory::Collection);
        assert_eq!(info.short_name, "Vec");

        // Test println! macro detection
        let entity = create_test_entity("println!");
        let info = detector.detect_stdlib_entity(&entity, &Language::Rust);
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.category, StdlibCategory::Macro);

        // Test non-stdlib entity
        let entity = create_test_entity("my_custom_function");
        let info = detector.detect_stdlib_entity(&entity, &Language::Rust);
        assert!(info.is_none());
    }

    #[test]
    fn test_python_stdlib_detection() {
        let detector = StdlibEntityDetector::new();

        // Test list detection
        let entity = create_test_entity("list");
        let info = detector.detect_stdlib_entity(&entity, &Language::Python);
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.category, StdlibCategory::Collection);

        // Test dict detection
        let entity = create_test_entity("dict");
        let info = detector.detect_stdlib_entity(&entity, &Language::Python);
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.category, StdlibCategory::Collection);
    }

    #[test]
    fn test_javascript_stdlib_detection() {
        let detector = StdlibEntityDetector::new();

        // Test Array detection
        let entity = create_test_entity("Array");
        let info = detector.detect_stdlib_entity(&entity, &Language::JavaScript);
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.category, StdlibCategory::Collection);

        // Test console detection
        let entity = create_test_entity("console");
        let info = detector.detect_stdlib_entity(&entity, &Language::JavaScript);
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.category, StdlibCategory::Io);

        // Test newly added TypedArray detection (Collection)
        let entity = create_test_entity("Int8Array");
        let info = detector.detect_stdlib_entity(&entity, &Language::JavaScript);
        assert!(info.is_some());
        assert_eq!(info.unwrap().category, StdlibCategory::Collection);

        let entity = create_test_entity("ArrayBuffer");
        let info = detector.detect_stdlib_entity(&entity, &Language::JavaScript);
        assert!(info.is_some());
        assert_eq!(info.unwrap().category, StdlibCategory::Collection);

        // Test newly added Concurrency types detection
        let entity = create_test_entity("Worker");
        let info = detector.detect_stdlib_entity(&entity, &Language::JavaScript);
        assert!(info.is_some());
        assert_eq!(info.unwrap().category, StdlibCategory::Concurrency);

        let entity = create_test_entity("AbortController");
        let info = detector.detect_stdlib_entity(&entity, &Language::JavaScript);
        assert!(info.is_some());
        assert_eq!(info.unwrap().category, StdlibCategory::Concurrency);

        // Test newly added Web API detection (Io)
        let entity = create_test_entity("Request");
        let info = detector.detect_stdlib_entity(&entity, &Language::JavaScript);
        assert!(info.is_some());
        assert_eq!(info.unwrap().category, StdlibCategory::Io);

        let entity = create_test_entity("WebSocket");
        let info = detector.detect_stdlib_entity(&entity, &Language::JavaScript);
        assert!(info.is_some());
        assert_eq!(info.unwrap().category, StdlibCategory::Io);

        // Test newly added IDB types detection (Utility)
        let entity = create_test_entity("IDBDatabase");
        let info = detector.detect_stdlib_entity(&entity, &Language::JavaScript);
        assert!(info.is_some());
        assert_eq!(info.unwrap().category, StdlibCategory::Utility);

        // Test newly added String encoding (String)
        let entity = create_test_entity("TextEncoder");
        let info = detector.detect_stdlib_entity(&entity, &Language::JavaScript);
        assert!(info.is_some());
        assert_eq!(info.unwrap().category, StdlibCategory::Io);

        // Test newly added Error types (Error)
        let entity = create_test_entity("EvalError");
        let info = detector.detect_stdlib_entity(&entity, &Language::JavaScript);
        assert!(info.is_some());
        assert_eq!(info.unwrap().category, StdlibCategory::Error);

        // Test non-stdlib entity (should still return None)
        let entity = create_test_entity("MyCustomClass");
        let info = detector.detect_stdlib_entity(&entity, &Language::JavaScript);
        assert!(info.is_none());
    }

    #[test]
    fn test_stdlib_entity_info() {
        let entity = create_test_entity("HashMap");
        let detector = StdlibEntityDetector::new();
        let info = detector
            .detect_stdlib_entity(&entity, &Language::Rust)
            .unwrap();

        assert_eq!(info.short_name, "HashMap");
        assert_eq!(info.category, StdlibCategory::Collection);
        assert!(info.semantic_description.contains("HashMap"));
    }

    #[test]
    fn test_typescript_stdlib_detection() {
        // TypeScript should benefit from JavaScript stdlib improvements
        let detector = StdlibEntityDetector::new();

        // Test that TypeScript can detect JavaScript stdlib entities
        let entity = create_test_entity("Array");
        let info = detector.detect_stdlib_entity(&entity, &Language::TypeScript);
        assert!(info.is_some());
        assert_eq!(info.unwrap().category, StdlibCategory::Collection);

        // Test new Web API types in TypeScript
        let entity = create_test_entity("Worker");
        let info = detector.detect_stdlib_entity(&entity, &Language::TypeScript);
        assert!(info.is_some());
        assert_eq!(info.unwrap().category, StdlibCategory::Concurrency);

        let entity = create_test_entity("ArrayBuffer");
        let info = detector.detect_stdlib_entity(&entity, &Language::TypeScript);
        assert!(info.is_some());
        assert_eq!(info.unwrap().category, StdlibCategory::Collection);

        let entity = create_test_entity("Promise");
        let info = detector.detect_stdlib_entity(&entity, &Language::TypeScript);
        assert!(info.is_some());
        assert_eq!(info.unwrap().category, StdlibCategory::Utility);
    }

    #[test]
    fn test_grouper_stdlib_covers_all_supported_languages() {
        let detector = StdlibEntityDetector::new();
        let template = [
            Language::Html,
            Language::Css,
            Language::Vue,
            Language::Svelte,
            Language::Ejs,
        ];
        let sentinels: &[(Language, &str)] = &[
            (Language::Bash, "echo"),
            (Language::C, "printf"),
            (Language::Cpp, "cout"),
            (Language::CSharp, "Console.WriteLine"),
            (Language::Dart, "print"),
            (Language::Go, "fmt.Println"),
            (Language::Java, "System.out.println"),
            (Language::JavaScript, "console.log"),
            (Language::TypeScript, "console.log"),
            (Language::Jsx, "console.log"),
            (Language::Tsx, "console.log"),
            (Language::Kotlin, "listOf"),
            (Language::Lua, "string.format"),
            (Language::Php, "strlen"),
            (Language::Python, "print"),
            (Language::Ruby, "puts"),
            (Language::Rust, "Vec::new"),
            (Language::Scala, "println"),
        ];
        for language in Language::ast_parsable_languages() {
            if template.contains(language) {
                let entity = create_test_entity("something");
                assert!(
                    detector.detect_stdlib_entity(&entity, language).is_none(),
                    "template language {language:?} must stay conservative"
                );
                continue;
            }
            let (_, sentinel) = sentinels
                .iter()
                .find(|(lang, _)| lang == language)
                .unwrap_or_else(|| panic!("{language:?} has no grouper sentinel"));
            let entity = create_test_entity(sentinel);
            assert!(
                detector.detect_stdlib_entity(&entity, language).is_some(),
                "sentinel {sentinel} not recognized for {language:?}"
            );
            assert!(
                detector.get_stdlib_category(sentinel, language).is_some(),
                "category missing for {sentinel} in {language:?}"
            );
        }
    }
}
