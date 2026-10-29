//! Post-processing: standard library entity classification
//!
//! Detects if an entity is a standard library type/function/trait/constant
//! and sets the is_stdlib flag and stdlib_category accordingly.
//!
//! # Design Note
//!
//! This module performs unified stdlib detection for all entity kinds.
//! The detection result is cached in Entity.stdlib_category to avoid
//! redundant checks in downstream processing (Grouper, relation resolution).

use cce_types::EntityKind;
use cce_types::entity::Entity;
use cce_types::language::Language;

use crate::grouper::types::StdlibCategory;
use crate::parser::stdlib::StdlibDetector;

/// Mark entity as standard library if it matches known stdlib patterns.
///
/// This is the single point where stdlib detection occurs during parsing.
/// Results are cached in Entity.stdlib_category to avoid redundant detection
/// in subsequent processing stages (Grouper, relation resolution).
///
/// # Detection Strategy
///
/// For each entity kind, this uses the most appropriate stdlib detector:
/// - Struct/Class: is_stdlib_type() for collection types
/// - Function/Method/Macro: is_stdlib_call() for functions and macros
/// - Trait: is_stdlib_trait() for trait definitions
/// - Constant: is_stdlib_constant() for constants
/// - Other kinds: uses is_stdlib_call() as fallback (general purpose)
pub fn mark_stdlib(entity: &mut Entity, language: &Language) {
    // 1. Honor an explicit plugin metadata marker first. Custom-language
    //    plugins set `metadata["is_stdlib"] = "true"` (optionally with
    //    `metadata["stdlib_category"]`) so entities from custom languages
    //    can be flagged as stdlib without a built-in detector.
    if let Some("true") = entity.metadata.get("is_stdlib").map(String::as_str) {
        entity.is_stdlib = true;
        entity.stdlib_category = Some(
            entity
                .metadata
                .get("stdlib_category")
                .and_then(|c| parse_category(c))
                .unwrap_or(StdlibCategory::Other),
        );
        return;
    }

    // 2. Fall back to the built-in detection strategy below.
    let name = entity.name.as_str();
    let mut is_stdlib = false;
    let mut category: Option<StdlibCategory> = None;

    // Determine stdlib detection method based on entity kind
    let detection_result = match entity.kind {
        EntityKind::Struct | EntityKind::Class => {
            // Type-specific detection for collection and built-in types
            StdlibDetector::is_stdlib_type(name, language)
        }
        EntityKind::Function | EntityKind::Method | EntityKind::Macro => {
            // General call detection for functions, methods, and macros
            StdlibDetector::is_stdlib_call(name, language)
        }
        EntityKind::Trait => {
            // Trait-specific detection
            StdlibDetector::is_stdlib_trait(name, language)
        }
        EntityKind::Constant => {
            // Constant-specific detection
            StdlibDetector::is_stdlib_constant(name, language)
        }
        _ => {
            // Fallback: use general call detection for other entity kinds
            // This ensures we don't miss stdlib entities with unusual kinds
            StdlibDetector::is_stdlib_call(name, language)
        }
    };

    if detection_result {
        is_stdlib = true;
        let coarse = match entity.kind {
            EntityKind::Struct | EntityKind::Class => StdlibCategory::Collection,
            EntityKind::Function | EntityKind::Method => StdlibCategory::Utility,
            EntityKind::Trait => StdlibCategory::Trait,
            EntityKind::Constant => StdlibCategory::Other,
            EntityKind::Macro => StdlibCategory::Macro,
            _ => StdlibCategory::Other,
        };
        category = Some(fine_stdlib_category(name, language).unwrap_or(coarse));
    }

    entity.is_stdlib = is_stdlib;
    entity.stdlib_category = category;
}

/// Reuse the per-language category tables so entity and relation layers agree.
///
/// Template languages conservatively return none. Remapped plugin languages
/// inherit their host language tables through effective language resolution.
fn fine_stdlib_category(name: &str, language: &Language) -> Option<StdlibCategory> {
    match cce_types::language::effective_language(language) {
        Language::Rust => crate::parser::stdlib::rust::RustStdlibDetector::get_category(name),
        Language::Python => crate::parser::stdlib::python::PythonStdlibDetector::get_category(name),
        Language::JavaScript | Language::TypeScript | Language::Jsx | Language::Tsx => {
            crate::parser::stdlib::javascript::JavaScriptStdlibDetector::get_category(name)
        }
        Language::Go => crate::parser::stdlib::go::GoStdlibDetector::get_category(name),
        Language::Java => crate::parser::stdlib::java::JavaStdlibDetector::get_category(name),
        Language::CSharp => crate::parser::stdlib::csharp::CSharpStdlibDetector::get_category(name),
        Language::C => crate::parser::stdlib::c::CStdlibDetector::get_category(name),
        Language::Cpp => crate::parser::stdlib::cpp::CppStdlibDetector::get_category(name),
        Language::Kotlin => crate::parser::stdlib::kotlin::KotlinStdlibDetector::get_category(name),
        Language::Scala => crate::parser::stdlib::scala::ScalaStdlibDetector::get_category(name),
        Language::Dart => crate::parser::stdlib::dart::DartStdlibDetector::get_category(name),
        Language::Ruby => crate::parser::stdlib::ruby::RubyStdlibDetector::get_category(name),
        Language::Php => crate::parser::stdlib::php::PhpStdlibDetector::get_category(name),
        Language::Lua => crate::parser::stdlib::lua::LuaStdlibDetector::get_category(name),
        Language::Bash => crate::parser::stdlib::bash::BashStdlibDetector::get_category(name),
        _ => None,
    }
}

/// Parse a stdlib category string from plugin metadata.
///
/// Accepts the serde variant names (`"Collection"`, `"Io"`, …) and the
/// lowercase display labels (`"collections"`, `"io"`, `"utilities"`, …).
fn parse_category(value: &str) -> Option<StdlibCategory> {
    if let Ok(c) = serde_json::from_str::<StdlibCategory>(value) {
        return Some(c);
    }
    let lower = value.to_lowercase();
    match lower.as_str() {
        "collections" | "collection" | "data_structure" => Some(StdlibCategory::Collection),
        "io" => Some(StdlibCategory::Io),
        "concurrency" | "threading" => Some(StdlibCategory::Concurrency),
        "utilities" | "utility" => Some(StdlibCategory::Utility),
        "strings" | "string" => Some(StdlibCategory::String),
        "numerics" | "numeric" | "numbers" | "math" => Some(StdlibCategory::Numeric),
        "errors" | "error" => Some(StdlibCategory::Error),
        "macros" | "macro" => Some(StdlibCategory::Macro),
        "traits" | "trait" | "interfaces" => Some(StdlibCategory::Trait),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_category_serde_variant() {
        assert_eq!(
            parse_category("Collection"),
            Some(StdlibCategory::Collection)
        );
        assert_eq!(parse_category("Io"), Some(StdlibCategory::Io));
    }

    #[test]
    fn test_parse_category_display_labels() {
        assert_eq!(
            parse_category("collections"),
            Some(StdlibCategory::Collection)
        );
        assert_eq!(parse_category("utilities"), Some(StdlibCategory::Utility));
        assert_eq!(parse_category("other_stdlib"), None);
    }

    #[test]
    fn test_mark_stdlib_from_metadata() {
        let mut entity = Entity {
            name: "my_stdlib_fn".to_string(),
            ..Default::default()
        };
        entity
            .metadata
            .insert("is_stdlib".to_string(), "true".to_string());
        entity
            .metadata
            .insert("stdlib_category".to_string(), "utility".to_string());
        mark_stdlib(&mut entity, &Language::Custom(0));
        assert!(entity.is_stdlib);
        assert_eq!(entity.stdlib_category, Some(StdlibCategory::Utility));
    }

    #[test]
    fn test_mark_stdlib_metadata_defaults_to_other() {
        let mut entity = Entity {
            name: "fn".to_string(),
            ..Default::default()
        };
        entity
            .metadata
            .insert("is_stdlib".to_string(), "true".to_string());
        mark_stdlib(&mut entity, &Language::Custom(0));
        assert!(entity.is_stdlib);
        assert_eq!(entity.stdlib_category, Some(StdlibCategory::Other));
    }

    #[test]
    fn test_mark_stdlib_unmarked_falls_back_to_builtin() {
        let mut entity = Entity {
            name: "println".to_string(),
            ..Default::default()
        };
        mark_stdlib(&mut entity, &Language::Rust);
        assert!(entity.is_stdlib);
    }

    #[test]
    fn test_mark_stdlib_prefers_fine_category() {
        use cce_types::entity::EntityKind;
        let mut entity = Entity {
            name: "print".to_string(),
            kind: EntityKind::Function,
            ..Default::default()
        };
        mark_stdlib(&mut entity, &Language::Python);
        assert!(entity.is_stdlib);
        assert_eq!(
            entity.stdlib_category,
            Some(crate::grouper::types::StdlibCategory::Io)
        );
    }
}
