//! Semantic unit extractor
//!
//! Slices complete semantic units (functions, classes, etc.) from caller
//! supplied content strings without touching the filesystem, so annotation
//! always observes the index snapshot view.

use super::error::{AnnotationError, Result};
use super::types::{ExpandedUnit, SemanticUnitType};

/// Semantic unit extractor
///
/// Slices complete code units from caller supplied content based on:
/// - File path
/// - Line range
/// - Entity type
#[derive(Clone)]
pub struct SemanticUnitExtractor;

impl SemanticUnitExtractor {
    /// Create a new extractor
    pub fn new() -> Self {
        Self
    }

    /// Extract unit from content string (no file I/O)
    ///
    /// The content is already the exact unit code; the line range is the
    /// unit's file-absolute span and is recorded as-is, never used to slice
    /// the content. Callers must pass the true file span with the matching
    /// body so downstream position math stays in one coordinate system.
    pub fn extract_unit_from_content(
        &self,
        content: &str,
        file_path: &str,
        start_line: u32,
        end_line: u32,
        name: &str,
        kind: &str,
    ) -> Result<ExpandedUnit> {
        // Validate line range
        if start_line == 0 || end_line == 0 {
            return Err(AnnotationError::invalid_line_range(
                file_path,
                start_line,
                end_line,
                content.lines().count() as u32,
            ));
        }

        if start_line > end_line {
            return Err(AnnotationError::invalid_line_range(
                file_path,
                start_line,
                end_line,
                content.lines().count() as u32,
            ));
        }

        if content.trim().is_empty() {
            return Err(AnnotationError::extraction_failed(
                file_path,
                "empty unit body".to_string(),
            ));
        }

        let unit_type = Self::parse_unit_type(kind);

        Ok(ExpandedUnit {
            entity_id: None,
            code: content.to_string(),
            file_path: file_path.to_string(),
            start_line,
            end_line,
            name: name.to_string(),
            unit_type,
            origin: super::types::ExpansionOrigin::Primary,
            edge_label: String::new(),
            relation_type: None,
            score: 0.0,
            is_stdlib: false,
            is_external: false,
            is_excerpt: false,
        })
    }

    /// Parse entity kind to semantic unit type
    fn parse_unit_type(kind: &str) -> SemanticUnitType {
        match kind.to_lowercase().as_str() {
            "function" | "func" => SemanticUnitType::Function,
            "method" => SemanticUnitType::Method,
            "class" => SemanticUnitType::Class,
            "struct" => SemanticUnitType::Struct,
            "interface" | "trait" => SemanticUnitType::Interface,
            "enum" => SemanticUnitType::Enum,
            "module" | "mod" => SemanticUnitType::Module,
            _ => SemanticUnitType::Unknown,
        }
    }
}

impl Default for SemanticUnitExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_unit_from_content() {
        let extractor = SemanticUnitExtractor::new();
        let content = r#"fn add(a: i32, b: i32) -> i32 {
    a + b
}"#;

        let unit = extractor
            .extract_unit_from_content(content, "src/math.rs", 10, 12, "add", "function")
            .expect("Failed to extract unit");

        assert_eq!(unit.name, "add");
        assert_eq!(unit.start_line, 10);
        assert_eq!(unit.end_line, 12);
        assert_eq!(unit.code, content);
        assert_eq!(unit.unit_type, SemanticUnitType::Function);
    }

    #[test]
    fn test_extract_unit_invalid_range() {
        let extractor = SemanticUnitExtractor::new();
        let content = "fn foo() {}";

        // A file-absolute span beyond the body length is legal; the body is
        // already the exact unit and is never sliced.
        let unit = extractor
            .extract_unit_from_content(content, "test.rs", 9, 9, "foo", "function")
            .expect("absolute span accepted");
        assert_eq!(unit.code, content);

        assert!(
            extractor
                .extract_unit_from_content(content, "test.rs", 0, 1, "foo", "function")
                .is_err()
        );
        assert!(
            extractor
                .extract_unit_from_content(content, "test.rs", 5, 4, "foo", "function")
                .is_err()
        );
        assert!(
            extractor
                .extract_unit_from_content("   \n ", "test.rs", 1, 2, "foo", "function")
                .is_err()
        );
    }

    #[test]
    fn test_parse_unit_type() {
        assert_eq!(
            SemanticUnitExtractor::parse_unit_type("function"),
            SemanticUnitType::Function
        );
        assert_eq!(
            SemanticUnitExtractor::parse_unit_type("class"),
            SemanticUnitType::Class
        );
        assert_eq!(
            SemanticUnitExtractor::parse_unit_type("struct"),
            SemanticUnitType::Struct
        );
        assert_eq!(
            SemanticUnitExtractor::parse_unit_type("unknown"),
            SemanticUnitType::Unknown
        );
    }
}
