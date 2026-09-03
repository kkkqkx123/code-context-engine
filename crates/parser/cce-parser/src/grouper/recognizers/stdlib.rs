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
/// for compression optimization.
pub struct StdlibEntityDetector {
    /// Rust standard library detector
    rust_detector: RustStdlibDetector,
    /// Python standard library detector
    python_detector: PythonStdlibDetector,
    /// JavaScript standard library detector
    javascript_detector: JavaScriptStdlibDetector,
}

impl StdlibEntityDetector {
    /// Create a new StdlibEntityDetector
    pub fn new() -> Self {
        Self {
            rust_detector: RustStdlibDetector::new(),
            python_detector: PythonStdlibDetector::new(),
            javascript_detector: JavaScriptStdlibDetector::new(),
        }
    }

    /// Detect if an entity is a standard library entity
    pub fn detect_stdlib_entity(
        &self,
        entity: &Entity,
        language: &Language,
    ) -> Option<StdlibEntityInfo> {
        match language {
            Language::Rust => self.rust_detector.detect(entity),
            Language::Python => self.python_detector.detect(entity),
            Language::JavaScript | Language::TypeScript => self.javascript_detector.detect(entity),
            _ => None,
        }
    }

    /// Check if a name is a standard library entity
    pub fn is_stdlib_name(&self, name: &str, language: &Language) -> bool {
        match language {
            Language::Rust => self.rust_detector.is_stdlib_name(name),
            Language::Python => self.python_detector.is_stdlib_name(name),
            Language::JavaScript | Language::TypeScript => {
                self.javascript_detector.is_stdlib_name(name)
            }
            _ => false,
        }
    }

    /// Get the category for a stdlib name
    ///
    /// Returns the category if the name is a recognized standard library entity,
    /// or None if it's not recognized.
    pub fn get_stdlib_category(&self, name: &str, language: &Language) -> Option<StdlibCategory> {
        match language {
            Language::Rust => self.rust_detector.get_category(name),
            Language::Python => self.python_detector.get_category(name),
            Language::JavaScript | Language::TypeScript => {
                self.javascript_detector.get_category(name)
            }
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

/// Rust standard library detector
///
/// Delegates all detection to the authoritative source in parser/stdlib/rust.rs
/// This ensures single source of truth for stdlib definitions.
struct RustStdlibDetector;

impl RustStdlibDetector {
    /// Create a new RustStdlibDetector
    fn new() -> Self {
        Self
    }

    /// Detect if an entity is a Rust standard library entity
    fn detect(&self, entity: &Entity) -> Option<StdlibEntityInfo> {
        let name = &entity.name;

        // Delegate to the standalone stdlib detector crate
        let category = cce_stdlib::rust::RustStdlibDetector::get_category(name)?;

        Some(StdlibEntityInfo::new(
            entity.id,
            format!("std::{}", name),
            name.clone(),
            category,
        ))
    }

    /// Check if a name is a Rust standard library entity
    fn is_stdlib_name(&self, name: &str) -> bool {
        cce_stdlib::rust::RustStdlibDetector::is_stdlib_name(name)
    }

    /// Get the category for a Rust stdlib name
    fn get_category(&self, name: &str) -> Option<StdlibCategory> {
        cce_stdlib::rust::RustStdlibDetector::get_category(name)
    }
}

/// Python standard library detector
struct PythonStdlibDetector;

impl PythonStdlibDetector {
    /// Create a new PythonStdlibDetector
    fn new() -> Self {
        Self
    }

    /// Detect if an entity is a Python standard library entity
    fn detect(&self, entity: &Entity) -> Option<StdlibEntityInfo> {
        let name = &entity.name;

        // Delegate to the standalone stdlib detector crate
        let category = cce_stdlib::python::PythonStdlibDetector::get_category(name)?;

        Some(StdlibEntityInfo::new(
            entity.id,
            name.clone(),
            name.clone(),
            category,
        ))
    }

    /// Check if a name is a Python standard library entity
    fn is_stdlib_name(&self, name: &str) -> bool {
        cce_stdlib::python::PythonStdlibDetector::is_stdlib_name(name)
    }

    /// Get the category for a Python stdlib name
    fn get_category(&self, name: &str) -> Option<StdlibCategory> {
        cce_stdlib::python::PythonStdlibDetector::get_category(name)
    }
}

/// JavaScript standard library detector
///
/// Delegates all detection to the authoritative source in parser/stdlib/javascript.rs
/// This ensures single source of truth for stdlib definitions.
struct JavaScriptStdlibDetector;

impl JavaScriptStdlibDetector {
    /// Create a new JavaScriptStdlibDetector
    fn new() -> Self {
        Self
    }

    /// Detect if an entity is a JavaScript standard library entity
    fn detect(&self, entity: &Entity) -> Option<StdlibEntityInfo> {
        let name = &entity.name;

        // Delegate to the standalone stdlib detector crate
        let category = cce_stdlib::javascript::JavaScriptStdlibDetector::get_category(name)?;

        Some(StdlibEntityInfo::new(
            entity.id,
            name.clone(),
            name.clone(),
            category,
        ))
    }

    /// Check if a name is a JavaScript standard library entity
    fn is_stdlib_name(&self, name: &str) -> bool {
        cce_stdlib::javascript::JavaScriptStdlibDetector::get_category(name).is_some()
    }

    /// Get the category for a JavaScript stdlib name
    fn get_category(&self, name: &str) -> Option<StdlibCategory> {
        cce_stdlib::javascript::JavaScriptStdlibDetector::get_category(name)
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
}
