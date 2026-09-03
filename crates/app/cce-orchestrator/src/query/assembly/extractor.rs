//! Semantic unit extractor
//!
//! Extracts complete semantic units (functions, classes, etc.) from source files.

use std::path::Path;

use cce_utils::file::read_file_to_utf8_async;

use super::error::{AssemblyError, Result};
use super::types::{ExpandedUnit, SemanticUnitType};

/// Semantic unit extractor
///
/// Extracts complete code units from source files based on:
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

    /// Extract a complete semantic unit from a file
    ///
    /// # Arguments
    ///
    /// * `file_path` - Path to the source file
    /// * `start_line` - Start line (1-based)
    /// * `end_line` - End line (1-based, inclusive)
    /// * `name` - Entity name
    /// * `kind` - Entity kind (function, class, etc.)
    ///
    /// # Returns
    ///
    /// The extracted unit with complete code content.
    pub async fn extract_unit(
        &self,
        file_path: &str,
        start_line: u32,
        end_line: u32,
        name: &str,
        kind: &str,
    ) -> Result<ExpandedUnit> {
        // Read the file content
        let content = read_file_to_utf8_async(Path::new(file_path))
            .await
            .map_err(|e| AssemblyError::extraction_failed(file_path, e))?;

        // Extract lines
        let lines: Vec<&str> = content.lines().collect();
        let line_count = lines.len() as u32;

        // Validate line range
        if start_line == 0 || end_line == 0 {
            return Err(AssemblyError::invalid_line_range(
                file_path, start_line, end_line, line_count,
            ));
        }

        if start_line > line_count || end_line > line_count {
            return Err(AssemblyError::invalid_line_range(
                file_path, start_line, end_line, line_count,
            ));
        }

        if start_line > end_line {
            return Err(AssemblyError::invalid_line_range(
                file_path, start_line, end_line, line_count,
            ));
        }

        // Extract the code (convert to 0-based index)
        let start_idx = (start_line - 1) as usize;
        let end_idx = end_line as usize; // end_line is inclusive, so we take up to end_line

        let code = lines[start_idx..end_idx.min(lines.len())].join("\n");

        // Determine semantic unit type
        let unit_type = Self::parse_unit_type(kind);

        Ok(ExpandedUnit {
            entity_id: None,
            code,
            file_path: file_path.to_string(),
            start_line,
            end_line,
            name: name.to_string(),
            unit_type,
            relation: super::types::RelationType::Primary,
            depth: 0,
        })
    }

    /// Extract unit from content string (no file I/O)
    ///
    /// Useful when content is already available.
    pub fn extract_unit_from_content(
        &self,
        content: &str,
        file_path: &str,
        start_line: u32,
        end_line: u32,
        name: &str,
        kind: &str,
    ) -> Result<ExpandedUnit> {
        let lines: Vec<&str> = content.lines().collect();
        let line_count = lines.len() as u32;

        // Validate line range
        if start_line == 0 || end_line == 0 {
            return Err(AssemblyError::invalid_line_range(
                file_path, start_line, end_line, line_count,
            ));
        }

        if start_line > line_count || end_line > line_count {
            return Err(AssemblyError::invalid_line_range(
                file_path, start_line, end_line, line_count,
            ));
        }

        if start_line > end_line {
            return Err(AssemblyError::invalid_line_range(
                file_path, start_line, end_line, line_count,
            ));
        }

        // Extract the code
        let start_idx = (start_line - 1) as usize;
        let end_idx = end_line as usize;

        let code = lines[start_idx..end_idx.min(lines.len())].join("\n");

        let unit_type = Self::parse_unit_type(kind);

        Ok(ExpandedUnit {
            entity_id: None,
            code,
            file_path: file_path.to_string(),
            start_line,
            end_line,
            name: name.to_string(),
            unit_type,
            relation: super::types::RelationType::Primary,
            depth: 0,
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
}

fn multiply(a: i32, b: i32) -> i32 {
    a * b
}"#;

        let unit = extractor
            .extract_unit_from_content(content, "src/math.rs", 1, 3, "add", "function")
            .expect("Failed to extract unit");

        assert_eq!(unit.name, "add");
        assert_eq!(unit.start_line, 1);
        assert_eq!(unit.end_line, 3);
        assert_eq!(unit.unit_type, SemanticUnitType::Function);
        assert!(unit.code.contains("fn add"));
    }

    #[test]
    fn test_extract_unit_invalid_range() {
        let extractor = SemanticUnitExtractor::new();
        let content = "fn foo() {}";

        let result =
            extractor.extract_unit_from_content(content, "test.rs", 1, 10, "foo", "function");

        assert!(result.is_err());
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
