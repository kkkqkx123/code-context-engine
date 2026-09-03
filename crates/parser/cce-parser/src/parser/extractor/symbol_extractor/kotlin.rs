//! Kotlin language import/export extractor
//!
//! Extracts import declarations from Kotlin source files.
//!
//! # Supported Import Syntax
//!
//! - `import package.Class` - Single class import
//! - `import package.*` - Wildcard import
//! - `import package.Class as Alias` - Aliased import
//!
//! # Export Semantics
//!
//! Kotlin doesn't have explicit export statements. All public classes/interfaces/objects
//! are automatically accessible. This extractor analyzes class/interface/object
//! definitions to identify exported symbols.

use super::traits::SymbolExtractor;
use crate::tree_sitter_query::{node_text, node_to_span};
use cce_types::import::{
    ExportKind, ExportTarget, ImportKind, ImportTarget, StandardizedExport, StandardizedImport,
    TargetKind,
};
use cce_types::language::Language;
use tree_sitter::Tree;

/// Kotlin language import/export extractor
pub struct KotlinExtractor;

impl KotlinExtractor {
    pub fn new() -> Self {
        Self
    }

    /// Extract package declaration from Kotlin source
    ///
    /// Example: `package com.example.myapp` -> `com.example.myapp`
    fn extract_package_declaration(tree: &Tree, source: &str) -> Option<String> {
        let root_node = tree.root_node();

        // Find package directive
        for child in root_node.children(&mut root_node.walk()) {
            if child.kind() == "package_directive" {
                // Find the qualified_identifier child
                for node in child.children(&mut child.walk()) {
                    if node.kind() == "qualified_identifier" {
                        // Build the full package path from identifiers
                        let mut parts = Vec::new();
                        for grandchild in node.children(&mut node.walk()) {
                            if grandchild.kind() == "identifier"
                                || grandchild.kind() == "simple_identifier"
                            {
                                let part = node_text(&grandchild, source);
                                if !part.is_empty() {
                                    parts.push(part);
                                }
                            }
                        }
                        if !parts.is_empty() {
                            return Some(parts.join("."));
                        }
                    }
                }
            }
        }

        None
    }

    /// Check if an import is a Kotlin standard library import
    fn is_kotlin_stdlib(path: &str) -> bool {
        path.starts_with("kotlin.")
            || path.starts_with("kotlinx.")
            || path.starts_with("java.")
            || path.starts_with("javax.")
    }

    /// Create a standardized import from Kotlin import
    fn create_import(
        kind: ImportKind,
        source_path: &str,
        target_name: Option<&str>,
        alias: Option<&str>,
        is_wildcard: bool,
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
            is_wildcard,
            is_default: false,
            is_system_header: Self::is_kotlin_stdlib(source_path),
            is_relative: false,
            span: Some(node_to_span(node)),
        }
    }

    /// Extract import path from import declaration
    fn extract_import_path(node: &tree_sitter::Node, source: &str) -> Option<String> {
        // Look for qualified_identifier child node which contains the full path
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "qualified_identifier" {
                let mut parts = Vec::new();
                let mut child_cursor = child.walk();
                for grandchild in child.children(&mut child_cursor) {
                    let kind = grandchild.kind();
                    if kind == "identifier" || kind == "simple_identifier" {
                        parts.push(node_text(&grandchild, source));
                    }
                }
                if !parts.is_empty() {
                    return Some(parts.join("."));
                }
            }
        }
        None
    }
}

impl SymbolExtractor for KotlinExtractor {
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

        // Recursively find all export nodes (class/interface/object definitions)
        Self::find_exports_recursive(&root_node, source, &mut exports, 0);

        exports
    }

    fn language(&self) -> Language {
        Language::Kotlin
    }

    fn extract_package_declaration(&self, tree: &Tree, source: &str) -> Option<String> {
        Self::extract_package_declaration(tree, source)
    }
}

impl KotlinExtractor {
    /// Recursively find and process import nodes
    fn find_imports_recursive(
        node: &tree_sitter::Node,
        source: &str,
        imports: &mut Vec<StandardizedImport>,
    ) {
        // Check if this node is an import (Kotlin's import declaration)
        if node.kind() == "import" {
            // Extract the import path
            if let Some(path) = Self::extract_import_path(node, source) {
                // Check for wildcard import (import package.*)
                let has_wildcard = node
                    .children(&mut node.walk())
                    .any(|child| child.kind() == "*" || node_text(&child, source) == "*");

                // Check for alias (import package.Class as Alias)
                let alias = Self::extract_import_alias(node, source);

                if has_wildcard {
                    imports.push(Self::create_import(
                        ImportKind::NamespaceImport,
                        &path,
                        None,
                        None,
                        true,
                        node,
                    ));
                } else {
                    // Extract the class name from the path
                    let target_name = path.split('.').next_back();
                    imports.push(Self::create_import(
                        ImportKind::SymbolImport,
                        &path,
                        target_name,
                        alias.as_deref(),
                        false,
                        node,
                    ));
                }
            }
        }

        // Recursively process children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            Self::find_imports_recursive(&child, source, imports);
        }
    }

    /// Extract alias from import declaration
    fn extract_import_alias(node: &tree_sitter::Node, source: &str) -> Option<String> {
        // Look for "as" keyword followed by identifier
        let mut cursor = node.walk();
        let mut found_as = false;

        for child in node.children(&mut cursor) {
            if node_text(&child, source) == "as" {
                found_as = true;
            } else if found_as
                && (child.kind() == "identifier" || child.kind() == "simple_identifier")
            {
                return Some(node_text(&child, source));
            }
        }

        None
    }

    /// Recursively find and process export nodes (class/interface/object definitions)
    fn find_exports_recursive(
        node: &tree_sitter::Node,
        source: &str,
        exports: &mut Vec<StandardizedExport>,
        depth: usize,
    ) {
        // Check for class/interface/object/function/property definitions
        let (declaration_kind, target_kind) = match node.kind() {
            "class_declaration" => {
                // Check if this is actually an interface by looking for 'interface' keyword
                let is_interface = Self::check_is_interface(node, source);
                if is_interface {
                    ("class_declaration", TargetKind::Interface)
                } else {
                    ("class_declaration", TargetKind::Class)
                }
            }
            "interface_declaration" => ("interface_declaration", TargetKind::Interface),
            "object_declaration" => ("object_declaration", TargetKind::Module),
            "function_declaration" if depth <= 1 => ("function_declaration", TargetKind::Function), // Top-level functions (depth 0 or 1)
            "property_declaration" if depth <= 1 => ("property_declaration", TargetKind::Variable), // Top-level properties
            // Kotlin tree-sitter might use different node types
            "function_definition" if depth <= 1 => ("function_definition", TargetKind::Function),
            "getter" | "setter" if depth <= 1 => ("property_accessor", TargetKind::Function),
            _ => ("", TargetKind::Other),
        };

        if !declaration_kind.is_empty() {
            let is_public = Self::is_public_declaration(node, source);

            if is_public {
                // For property_declaration, the name is in a variable_declaration child
                let name = if node.kind() == "property_declaration" {
                    let mut cursor = node.walk();
                    let var_decl = node
                        .children(&mut cursor)
                        .find(|child| child.kind() == "variable_declaration");
                    if let Some(var_decl) = var_decl {
                        let mut var_cursor = var_decl.walk();
                        var_decl
                            .children(&mut var_cursor)
                            .find(|grandchild| grandchild.kind() == "identifier")
                            .map(|id| node_text(&id, source))
                    } else {
                        None
                    }
                } else if node.kind() == "function_declaration"
                    || node.kind() == "function_definition"
                {
                    // For function_declaration, try to get name using helper
                    Self::get_function_name(node, source)
                } else {
                    node.child_by_field_name("name")
                        .map(|n| node_text(&n, source))
                };

                if let Some(name) = name {
                    if !name.is_empty() {
                        exports.push(StandardizedExport {
                            kind: ExportKind::Named,
                            target: ExportTarget {
                                name,
                                original_name: None,
                                kind: target_kind,
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
            Self::find_exports_recursive(&child, source, exports, depth + 1);
        }
    }

    /// Check if a class_declaration node is actually an interface
    fn check_is_interface(node: &tree_sitter::Node, source: &str) -> bool {
        // Look for 'interface' keyword in modifiers or children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            let text = node_text(&child, source);
            if text == "interface" {
                return true;
            }
            // Check in modifiers
            if child.kind() == "modifiers" {
                let mut mod_cursor = child.walk();
                for mod_child in child.children(&mut mod_cursor) {
                    if node_text(&mod_child, source) == "interface" {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Get the name from a function declaration node
    fn get_function_name(node: &tree_sitter::Node, source: &str) -> Option<String> {
        // Try to get name from field first
        if let Some(name_node) = node.child_by_field_name("name") {
            let name = node_text(&name_node, source);
            if !name.is_empty() {
                return Some(name);
            }
        }

        // Fallback: look for identifier child (Kotlin tree-sitter uses identifier child)
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "identifier" {
                let name = node_text(&child, source);
                if !name.is_empty() {
                    return Some(name);
                }
            }
        }

        None
    }

    /// Check if a declaration is public
    fn is_public_declaration(node: &tree_sitter::Node, source: &str) -> bool {
        // Kotlin declarations are public by default
        // Check for explicit visibility modifiers
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            if child.kind() == "modifiers" {
                let mut mod_cursor = child.walk();
                for mod_child in child.children(&mut mod_cursor) {
                    let text = node_text(&mod_child, source);
                    // If we find private or protected, it's not public
                    if text == "private" || text == "protected" || text == "internal" {
                        return false;
                    }
                }
            }
        }

        // Default is public
        true
    }
}

impl Default for KotlinExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::AstParser;

    #[test]
    fn test_kotlin_import_extraction() {
        let code = r#"
import kotlin.collections.List
import kotlin.collections.*
"#;
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::Kotlin);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = KotlinExtractor::new();
        let imports = extractor.extract_imports(&tree, code);

        assert!(!imports.is_empty());
        // Should have wildcard import
        assert!(imports.iter().any(|i| i.is_wildcard));
    }

    #[test]
    fn test_kotlin_stdlib_import() {
        let code = r#"
import kotlin.collections.List
import kotlinx.coroutines.CoroutineScope
import java.util.ArrayList
"#;
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::Kotlin);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = KotlinExtractor::new();
        let imports = extractor.extract_imports(&tree, code);

        assert_eq!(imports.len(), 3);
        assert!(imports[0].is_system_header);
        assert!(imports[1].is_system_header);
        assert!(imports[2].is_system_header);
    }

    #[test]
    fn test_kotlin_aliased_import() {
        let code = r#"
import kotlin.collections.List as KList
"#;
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::Kotlin);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = KotlinExtractor::new();
        let imports = extractor.extract_imports(&tree, code);

        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].alias.as_deref(), Some("KList"));
    }

    #[test]
    fn test_kotlin_export_extraction() {
        let code = r#"
class MyClass {
    fun method() {}
}

private class PrivateClass {}

interface MyInterface {
    fun interfaceMethod()
}

object MyObject {
    val value = 42
}
"#;
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::Kotlin);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = KotlinExtractor::new();
        let exports = extractor.extract_exports(&tree, code);

        // Should have public exports: MyClass, MyInterface, MyObject
        assert!(exports.len() >= 3);
        assert!(exports.iter().any(|e| e.target.name == "MyClass"));
        assert!(exports.iter().any(|e| e.target.name == "MyInterface"));
        assert!(exports.iter().any(|e| e.target.name == "MyObject"));
    }

    #[test]
    fn test_kotlin_function_and_property_export() {
        let code = r#"
fun topLevelFunction() {}

val topLevelProperty = 42

private fun privateFunction() {}

internal fun internalFunction() {}
"#;
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::Kotlin);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = KotlinExtractor::new();
        let exports = extractor.extract_exports(&tree, code);

        // Should have public function and property
        assert!(
            exports
                .iter()
                .any(|e| e.target.name == "topLevelFunction"
                    && e.target.kind == TargetKind::Function)
        );
        assert!(
            exports
                .iter()
                .any(|e| e.target.name == "topLevelProperty"
                    && e.target.kind == TargetKind::Variable)
        );
        // Should not have private or internal functions
        assert!(!exports.iter().any(|e| e.target.name == "privateFunction"));
        assert!(!exports.iter().any(|e| e.target.name == "internalFunction"));
    }

    #[test]
    fn test_kotlin_visibility_modifiers() {
        let code = r#"
public class PublicClass {}
private class PrivateClass {}
protected class ProtectedClass {}
internal class InternalClass {}
class DefaultClass {}
"#;
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::Kotlin);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = KotlinExtractor::new();
        let exports = extractor.extract_exports(&tree, code);

        // Should have public and default (public) classes
        assert!(exports.iter().any(|e| e.target.name == "PublicClass"));
        assert!(exports.iter().any(|e| e.target.name == "DefaultClass"));
        // Should not have private, protected, or internal classes
        assert!(!exports.iter().any(|e| e.target.name == "PrivateClass"));
        assert!(!exports.iter().any(|e| e.target.name == "ProtectedClass"));
        assert!(!exports.iter().any(|e| e.target.name == "InternalClass"));
    }
}
