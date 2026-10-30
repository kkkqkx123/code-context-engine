//! Post-processing: standard library entity classification
//!
//! Sets the `is_stdlib` flag and `stdlib_category` of entities that *reference*
//! a standard library symbol, so downstream stages do not re-run the lookup.
//!
//! # Design Note
//!
//! A name lookup only answers "does this name exist in the stdlib tables", not
//! "is this entity a stdlib entity". The two differ exactly where it matters:
//! `struct Command` written by the project is not `std::process::Command`, yet
//! a name match used to mark it stdlib and downstream stages then compressed it
//! away (dropped fields, methods, parameters and return types from the natural
//! language text). Definitions therefore never match by name; only entities
//! that pull in someone else's symbol (`import` / `require` / `include`) do.

use cce_types::EntityKind;
use cce_types::entity::Entity;
use cce_types::language::Language;

use crate::grouper::types::StdlibCategory;
use crate::parser::stdlib::StdlibDetector;

/// Entity kinds that pull in a symbol defined elsewhere.
///
/// These are the only kinds whose name is a foreign reference, so the only
/// kinds a stdlib name lookup can answer for. Definitions (`Struct`, `Class`,
/// `Function`, `Method`, `Constant`, …) are the project's own code and are
/// never matched by name — otherwise a project type that happens to share a
/// name with a stdlib one is silently treated as external.
///
/// `Export` stays out on purpose: `export function foo()` is a definition even
/// though a re-export names a foreign symbol.
fn is_reference_kind(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::Import | EntityKind::Require | EntityKind::Include
    )
}

/// Mark entity as standard library if it references a stdlib symbol.
///
/// # Detection Strategy
///
/// 1. An explicit plugin marker (`metadata["is_stdlib"]`) always wins, for any
///    kind: custom-language plugins supply the knowledge the built-in tables
///    lack.
/// 2. Otherwise only reference kinds are looked up, through the shared import
///    entry in `cce-stdlib` (per-language tables, not local prefixes).
/// 3. The category is the table's answer for the whole path, else for its last
///    segment (`std::collections::HashMap` → `HashMap` → `Collection`), else
///    `Other`.
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

    // 2. A definition is never a stdlib entity, whatever its name says.
    if !is_reference_kind(entity.kind) {
        return;
    }

    // 3. Reference: the shared lookup both decides and categorizes.
    if let Some(category) = StdlibDetector::ref_category(&entity.name, language) {
        entity.is_stdlib = true;
        entity.stdlib_category = Some(category);
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
        // Matches the display label of `StdlibCategory::Other`, so a category
        // read back from metadata round-trips instead of being dropped.
        "other_stdlib" | "other" => Some(StdlibCategory::Other),
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
        assert_eq!(parse_category("other_stdlib"), Some(StdlibCategory::Other));
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
    fn test_mark_stdlib_ignores_project_definitions() {
        // Regression: `struct Command` shares a name with `std::process::Command`
        // and used to be marked stdlib, which made every downstream stage drop
        // its fields, methods and signature from the natural language text.
        for (name, kind) in [
            ("Command", EntityKind::Struct),
            ("Error", EntityKind::Enum),
            ("String", EntityKind::Class),
            ("println", EntityKind::Function),
            ("Vec", EntityKind::TypeAlias),
            ("MAX", EntityKind::Constant),
        ] {
            let mut entity = Entity {
                name: name.to_string(),
                kind,
                ..Default::default()
            };
            mark_stdlib(&mut entity, &Language::Rust);
            assert!(
                !entity.is_stdlib,
                "{kind} {name} is a project definition, not a stdlib entity"
            );
            assert_eq!(entity.stdlib_category, None);
        }
    }

    #[test]
    fn test_mark_stdlib_marks_reference_entities() {
        let mut entity = Entity {
            name: "std::collections::HashMap".to_string(),
            kind: EntityKind::Import,
            ..Default::default()
        };
        mark_stdlib(&mut entity, &Language::Rust);
        assert!(entity.is_stdlib);
        assert_eq!(entity.stdlib_category, Some(StdlibCategory::Collection));

        let mut python = Entity {
            name: "os".to_string(),
            kind: EntityKind::Import,
            ..Default::default()
        };
        mark_stdlib(&mut python, &Language::Python);
        assert!(python.is_stdlib);
        assert!(python.stdlib_category.is_some());

        let mut ruby = Entity {
            name: "json".to_string(),
            kind: EntityKind::Require,
            ..Default::default()
        };
        mark_stdlib(&mut ruby, &Language::Ruby);
        assert!(ruby.is_stdlib, "require 'json' is a stdlib reference");

        let mut local = Entity {
            name: "our_internal_module".to_string(),
            kind: EntityKind::Import,
            ..Default::default()
        };
        mark_stdlib(&mut local, &Language::Python);
        assert!(!local.is_stdlib);
    }
}
