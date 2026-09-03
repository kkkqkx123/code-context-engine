use cce_types::Span;
use cce_types::entity::{Entity, EntityId, EntityKind};
use cce_types::language::Language;

use super::StdlibEntityDetector;
use crate::grouper::types::StdlibCategory;

fn create_struct(id: EntityId, name: &str) -> Entity {
    Entity::new(id, EntityKind::Struct, name.to_string(), Span::default())
}

fn create_func(id: EntityId, name: &str) -> Entity {
    Entity::new(id, EntityKind::Function, name.to_string(), Span::default())
}

// ============================================================
// Stdlib entity detector tests
// ============================================================

#[test]
fn test_stdlib_detection_workflow() {
    let detector = StdlibEntityDetector::new();

    let vec_entity = create_struct(EntityId(0), "Vec");
    let info = detector.detect_stdlib_entity(&vec_entity, &Language::Rust);
    assert!(info.is_some(), "Vec should be detected as stdlib entity");
    assert_eq!(info.unwrap().category, StdlibCategory::Collection);

    let hashmap_entity = create_struct(EntityId(0), "HashMap");
    let info = detector.detect_stdlib_entity(&hashmap_entity, &Language::Rust);
    assert!(
        info.is_some(),
        "HashMap should be detected as stdlib entity"
    );
    assert_eq!(info.unwrap().category, StdlibCategory::Collection);

    let string_entity = create_struct(EntityId(0), "String");
    let info = detector.detect_stdlib_entity(&string_entity, &Language::Rust);
    assert!(info.is_some(), "String should be detected as stdlib entity");
    assert_eq!(info.unwrap().category, StdlibCategory::String);
}

#[test]
fn test_stdlib_name_detection() {
    let detector = StdlibEntityDetector::new();

    assert!(detector.is_stdlib_name("Vec", &Language::Rust));
    assert!(detector.is_stdlib_name("HashMap", &Language::Rust));
    assert!(detector.is_stdlib_name("String", &Language::Rust));
    assert!(!detector.is_stdlib_name("CustomType", &Language::Rust));
}

#[test]
fn test_stdlib_category_detection() {
    let detector = StdlibEntityDetector::new();

    assert_eq!(
        detector.get_stdlib_category("Vec", &Language::Rust),
        Some(StdlibCategory::Collection)
    );
    assert_eq!(
        detector.get_stdlib_category("File", &Language::Rust),
        Some(StdlibCategory::Io)
    );
    assert_eq!(
        detector.get_stdlib_category("Mutex", &Language::Rust),
        Some(StdlibCategory::Concurrency)
    );
    assert_eq!(
        detector.get_stdlib_category("Option", &Language::Rust),
        Some(StdlibCategory::Utility)
    );
    assert_eq!(
        detector.get_stdlib_category("i32", &Language::Rust),
        Some(StdlibCategory::Numeric)
    );
    assert_eq!(
        detector.get_stdlib_category("Iterator", &Language::Rust),
        Some(StdlibCategory::Trait)
    );

    assert_eq!(
        detector.get_stdlib_category("CustomType", &Language::Rust),
        None
    );
}

#[test]
fn test_stdlib_python_detection() {
    let detector = StdlibEntityDetector::new();

    let list_entity = create_func(EntityId(0), "list");
    let info = detector.detect_stdlib_entity(&list_entity, &Language::Python);
    assert!(info.is_some(), "list should be detected in Python");

    let dict_entity = create_func(EntityId(0), "dict");
    let info = detector.detect_stdlib_entity(&dict_entity, &Language::Python);
    assert!(info.is_some(), "dict should be detected in Python");
}

#[test]
fn test_stdlib_javascript_detection() {
    let detector = StdlibEntityDetector::new();

    let array_entity = create_func(EntityId(0), "Array");
    let info = detector.detect_stdlib_entity(&array_entity, &Language::JavaScript);
    assert!(info.is_some(), "Array should be detected in JavaScript");

    let promise_entity = create_func(EntityId(0), "Promise");
    let info = detector.detect_stdlib_entity(&promise_entity, &Language::JavaScript);
    assert!(info.is_some(), "Promise should be detected in JavaScript");
}
