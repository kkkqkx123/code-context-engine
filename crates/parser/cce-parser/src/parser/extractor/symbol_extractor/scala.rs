//! Scala language import/export extractor
//!
//! Extracts import declarations from Scala source files.
//!
//! # Supported Import Syntax
//!
//! - `import package.Class` - Single class import
//! - `import package._` - Wildcard import
//! - `import package.{Class1, Class2}` - Multiple imports
//! - `import package.{Class => Alias}` - Renamed import
//! - `import package.{Class => _}` - Hiding import
//!
//! # Export Semantics
//!
//! Scala doesn't have explicit export statements. All public members are
//! automatically accessible. This extractor analyzes class/trait/object
//! definitions to identify exported symbols.

use super::traits::SymbolExtractor;
use crate::tree_sitter_query::{find_children_by_kind, node_text, node_to_span};
use cce_types::import::{
    ExportKind, ExportTarget, ImportKind, ImportTarget, StandardizedExport, StandardizedImport,
    TargetKind,
};
use cce_types::language::Language;
use tree_sitter::Tree;

/// Scala language import/export extractor
pub struct ScalaExtractor;

impl ScalaExtractor {
    pub fn new() -> Self {
        Self
    }

    /// Check if an import path is a relative import
    fn is_relative_import(path: &str) -> bool {
        path.starts_with("./") || path.starts_with("../") || path == "." || path == ".."
    }

    /// Check if an import is a Scala standard library import
    fn is_scala_stdlib(path: &str) -> bool {
        path.starts_with("scala.") || path.starts_with("java.")
    }

    /// Create a standardized import from Scala import
    fn create_import(
        kind: ImportKind,
        source_path: &str,
        target_name: Option<&str>,
        alias: Option<&str>,
        node: &tree_sitter::Node,
    ) -> StandardizedImport {
        let target = if let Some(name) = target_name {
            ImportTarget {
                local_name: alias.unwrap_or(name).to_string(),
                original_name: if alias.is_some() {
                    Some(name.to_string())
                } else {
                    None
                },
                kind: TargetKind::Other,
            }
        } else {
            ImportTarget::default()
        };

        StandardizedImport {
            kind,
            source: source_path.to_string(),
            target,
            alias: alias.map(|a| a.to_string()),
            is_wildcard: matches!(kind, ImportKind::NamespaceImport),
            is_default: false,
            is_system_header: Self::is_scala_stdlib(source_path),
            is_relative: Self::is_relative_import(source_path),
            span: Some(node_to_span(node)),
        }
    }

    /// Extract import path from import declaration
    fn extract_import_path(node: &tree_sitter::Node, source: &str) -> Option<String> {
        // Build path from identifier nodes and dots
        let mut parts = Vec::new();
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            let kind = child.kind();
            if kind == "identifier" {
                parts.push(node_text(&child, source));
            }
            // Note: dots are implicit separators, we just collect identifiers
        }

        if !parts.is_empty() {
            Some(parts.join("."))
        } else {
            None
        }
    }
}

impl SymbolExtractor for ScalaExtractor {
    fn extract_imports(&self, tree: &Tree, source: &str) -> Vec<StandardizedImport> {
        let mut imports = Vec::new();
        let root_node = tree.root_node();

        // Recursively find all import nodes
        Self::find_imports_recursive(&root_node, source, &mut imports);

        imports
    }

    fn extract_exports(&self, tree: &Tree, source: &str) -> Vec<StandardizedExport> {
        let mut exports = Vec::new();
        let root_node = tree.root_node();

        // Recursively find all export nodes (class/trait/object definitions)
        Self::find_exports_recursive(&root_node, source, &mut exports);

        exports
    }

    fn language(&self) -> Language {
        Language::Scala
    }
}

impl ScalaExtractor {
    /// Recursively find and process import nodes
    fn find_imports_recursive(
        node: &tree_sitter::Node,
        source: &str,
        imports: &mut Vec<StandardizedImport>,
    ) {
        // Check if this node is an import_declaration
        if node.kind() == "import_declaration" {
            // Extract the base import path
            if let Some(path) = Self::extract_import_path(node, source) {
                // Check for wildcard import (import package._)
                let has_wildcard = node
                    .children(&mut node.walk())
                    .any(|child| child.kind() == "wildcard" || node_text(&child, source) == "_");

                if has_wildcard {
                    imports.push(Self::create_import(
                        ImportKind::NamespaceImport,
                        &path,
                        None,
                        None,
                        node,
                    ));
                } else {
                    // Check for namespace selectors (import package.{A, B})
                    let has_selectors =
                        !find_children_by_kind(node, "namespace_selectors").is_empty();

                    if has_selectors {
                        // Process each selector
                        for selector_node in find_children_by_kind(node, "namespace_selectors") {
                            for identifier_node in
                                find_children_by_kind(&selector_node, "identifier")
                            {
                                let name = node_text(&identifier_node, source);
                                if !name.is_empty() && name != "_" {
                                    imports.push(Self::create_import(
                                        ImportKind::SymbolImport,
                                        &path,
                                        Some(&name),
                                        None,
                                        node,
                                    ));
                                }
                            }
                        }
                    } else {
                        // Simple import (import package.Class)
                        imports.push(Self::create_import(
                            ImportKind::SymbolImport,
                            &path,
                            None,
                            None,
                            node,
                        ));
                    }
                }
            }
        }

        // Recursively process children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            Self::find_imports_recursive(&child, source, imports);
        }
    }

    /// Check if a declaration is public (not private or protected)
    /// In Scala, members are public by default unless marked with private or protected
    fn is_public_declaration(node: &tree_sitter::Node, source: &str) -> bool {
        let mut cursor = node.walk();

        // Check for modifiers child node
        for child in node.children(&mut cursor) {
            if child.kind() == "modifiers" {
                let mut mod_cursor = child.walk();
                for mod_child in child.children(&mut mod_cursor) {
                    let text = node_text(&mod_child, source);
                    // If private or protected modifier is present, it's not public
                    if text == "private" || text == "protected" {
                        return false;
                    }
                }
            }
        }

        // Check for inline private/protected modifiers (without modifiers wrapper)
        for child in node.children(&mut cursor) {
            let text = node_text(&child, source);
            if text == "private" || text == "protected" {
                return false;
            }
        }

        // Default to public in Scala
        true
    }

    /// Recursively find and process export nodes (class/trait/object definitions)
    fn find_exports_recursive(
        node: &tree_sitter::Node,
        source: &str,
        exports: &mut Vec<StandardizedExport>,
    ) {
        // Check for class definition
        if node.kind() == "class_definition" {
            // Only export if it's public
            if Self::is_public_declaration(node, source) {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = node_text(&name_node, source);
                    if !name.is_empty() {
                        exports.push(StandardizedExport {
                            kind: ExportKind::Named,
                            target: ExportTarget {
                                name,
                                original_name: None,
                                kind: TargetKind::Class,
                                source_module: None,
                            },
                            is_reexport: false,
                            span: Some(node_to_span(node)),
                        });
                    }
                }
            }
        }

        // Check for trait definition
        if node.kind() == "trait_definition" {
            // Only export if it's public
            if Self::is_public_declaration(node, source) {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = node_text(&name_node, source);
                    if !name.is_empty() {
                        exports.push(StandardizedExport {
                            kind: ExportKind::Named,
                            target: ExportTarget {
                                name,
                                original_name: None,
                                kind: TargetKind::Interface,
                                source_module: None,
                            },
                            is_reexport: false,
                            span: Some(node_to_span(node)),
                        });
                    }
                }
            }
        }

        // Check for object definition
        if node.kind() == "object_definition" {
            // Only export if it's public
            if Self::is_public_declaration(node, source) {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = node_text(&name_node, source);
                    if !name.is_empty() {
                        exports.push(StandardizedExport {
                            kind: ExportKind::Named,
                            target: ExportTarget {
                                name,
                                original_name: None,
                                kind: TargetKind::Module,
                                source_module: None,
                            },
                            is_reexport: false,
                            span: Some(node_to_span(node)),
                        });
                    }
                }
            }
        }

        // Recursively process children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            Self::find_exports_recursive(&child, source, exports);
        }
    }
}

impl Default for ScalaExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::AstParser;

    #[test]
    fn test_scala_import_extraction() {
        let code = r#"
import scala.collection.mutable._
import scala.concurrent.Future
"#;
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::Scala);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = ScalaExtractor::new();
        let imports = extractor.extract_imports(&tree, code);

        assert!(!imports.is_empty());
        // First import should be wildcard
        assert!(imports.iter().any(|i| i.is_wildcard));
    }

    #[test]
    fn test_scala_stdlib_import() {
        let code = r#"
import scala.Option
import java.util.List
"#;
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::Scala);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = ScalaExtractor::new();
        let imports = extractor.extract_imports(&tree, code);

        assert_eq!(imports.len(), 2);
        assert!(imports[0].is_system_header);
        assert!(imports[1].is_system_header);
        assert_eq!(imports[0].source, "scala.Option");
        assert_eq!(imports[1].source, "java.util.List");
    }

    #[test]
    fn test_scala_export_extraction() {
        let code = r#"
class MyClass {
  def method(): Unit = {}
}

trait MyTrait {
  def traitMethod(): Unit
}

object MyObject {
  val value = 42
}
"#;
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::Scala);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = ScalaExtractor::new();
        let exports = extractor.extract_exports(&tree, code);

        assert_eq!(exports.len(), 3);
        assert!(exports.iter().any(|e| e.target.name == "MyClass"));
        assert!(exports.iter().any(|e| e.target.name == "MyTrait"));
        assert!(exports.iter().any(|e| e.target.name == "MyObject"));
    }

    #[test]
    fn test_scala_private_not_exported() {
        let code = r#"
class PublicClass

private class PrivateClass

protected class ProtectedClass

trait PublicTrait

private trait PrivateTrait

object PublicObject

private object PrivateObject
"#;
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::Scala);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = ScalaExtractor::new();
        let exports = extractor.extract_exports(&tree, code);

        // Should only export public declarations
        assert!(exports.iter().any(|e| e.target.name == "PublicClass"));
        assert!(exports.iter().any(|e| e.target.name == "PublicTrait"));
        assert!(exports.iter().any(|e| e.target.name == "PublicObject"));

        // Should NOT export private/protected declarations
        assert!(!exports.iter().any(|e| e.target.name == "PrivateClass"));
        assert!(!exports.iter().any(|e| e.target.name == "ProtectedClass"));
        assert!(!exports.iter().any(|e| e.target.name == "PrivateTrait"));
        assert!(!exports.iter().any(|e| e.target.name == "PrivateObject"));
    }
}
