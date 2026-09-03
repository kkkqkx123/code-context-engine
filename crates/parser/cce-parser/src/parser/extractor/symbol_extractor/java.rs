//! Java language import/export extractor
//!
//! Extracts import declarations from Java source files.
//!
//! # Supported Import Syntax
//!
//! - `import package.Class;` - Single class import
//! - `import package.*;` - Wildcard import (all classes in package)
//! - `import static package.Class.method;` - Static import
//! - `import static package.Class.*;` - Static wildcard import
//!
//! # Export Semantics
//!
//! Java doesn't have explicit export statements. All public classes/interfaces/enums
//! are automatically accessible. This extractor analyzes class/interface/enum
//! definitions to identify exported symbols.

use super::traits::SymbolExtractor;
use crate::tree_sitter_query::{node_text, node_to_span};
use cce_types::import::{
    ExportKind, ExportTarget, ImportKind, ImportTarget, StandardizedExport, StandardizedImport,
    TargetKind,
};
use cce_types::language::Language;
use tree_sitter::Tree;

/// Java language import/export extractor
pub struct JavaExtractor;

impl JavaExtractor {
    pub fn new() -> Self {
        Self
    }

    /// Extract package declaration from Java source
    ///
    /// Example: `package com.example.myapp;` -> `com.example.myapp`
    fn extract_package_declaration(tree: &Tree, source: &str) -> Option<String> {
        let root_node = tree.root_node();

        // Find package declaration
        for child in root_node.children(&mut root_node.walk()) {
            if child.kind() == "package_declaration" {
                // Find the scoped_identifier or identifier child
                for node in child.children(&mut child.walk()) {
                    if node.kind() == "scoped_identifier" || node.kind() == "identifier" {
                        // For scoped_identifier, it already contains the full package path
                        // For identifier, it's a simple package name
                        let package = node_text(&node, source);
                        if !package.is_empty() {
                            return Some(package);
                        }
                    }
                }
            }
        }

        None
    }

    /// Check if an import is a Java standard library import
    fn is_java_stdlib(path: &str) -> bool {
        path.starts_with("java.")
            || path.starts_with("javax.")
            || path.starts_with("jdk.")
            || path.starts_with("sun.")
            || path.starts_with("com.sun.")
    }

    /// Create a standardized import from Java import
    ///
    /// Note: For static imports, the `is_static` parameter is used to determine
    /// the import kind. Static imports are represented as `ImportKind::SymbolImport`
    /// with the source path containing the full qualified name.
    fn create_import(
        kind: ImportKind,
        source_path: &str,
        target_name: Option<&str>,
        is_wildcard: bool,
        is_static: bool,
        node: &tree_sitter::Node,
    ) -> StandardizedImport {
        let target = if let Some(name) = target_name {
            ImportTarget {
                local_name: name.to_string(),
                original_name: None,
                kind: if is_static {
                    TargetKind::Function // Static imports typically import methods
                } else {
                    TargetKind::Other
                },
            }
        } else {
            ImportTarget::default()
        };

        StandardizedImport {
            kind,
            source: source_path.to_string(),
            target,
            alias: if is_static {
                // Mark static imports with a special alias prefix
                Some(format!("static:{}", source_path))
            } else {
                None
            },
            is_wildcard,
            is_default: false,
            is_system_header: Self::is_java_stdlib(source_path),
            is_relative: false,
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
            if kind == "identifier" || kind == "scoped_identifier" {
                parts.push(node_text(&child, source));
            }
        }

        if !parts.is_empty() {
            Some(parts.join("."))
        } else {
            None
        }
    }
}

impl SymbolExtractor for JavaExtractor {
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

        // Recursively find all export nodes (class/interface/enum definitions)
        Self::find_exports_recursive(&root_node, source, &mut exports);

        exports
    }

    fn language(&self) -> Language {
        Language::Java
    }

    fn extract_package_declaration(&self, tree: &Tree, source: &str) -> Option<String> {
        Self::extract_package_declaration(tree, source)
    }
}

impl JavaExtractor {
    /// Recursively find and process import nodes
    fn find_imports_recursive(
        node: &tree_sitter::Node,
        source: &str,
        imports: &mut Vec<StandardizedImport>,
    ) {
        // Check if this node is an import_declaration
        if node.kind() == "import_declaration" {
            // Check for static import
            let is_static = node
                .children(&mut node.walk())
                .any(|child| child.kind() == "static");

            // Extract the import path
            if let Some(path) = Self::extract_import_path(node, source) {
                // Check for wildcard import (import package.*)
                let has_wildcard = node
                    .children(&mut node.walk())
                    .any(|child| child.kind() == "asterisk" || node_text(&child, source) == "*");

                if has_wildcard {
                    imports.push(Self::create_import(
                        ImportKind::NamespaceImport,
                        &path,
                        None,
                        true,
                        is_static,
                        node,
                    ));
                } else {
                    // Extract the class/method name from the path
                    let target_name = path.split('.').next_back();
                    imports.push(Self::create_import(
                        ImportKind::SymbolImport,
                        &path,
                        target_name,
                        false,
                        is_static,
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

    /// Check if a declaration has public modifier
    fn is_public_declaration(node: &tree_sitter::Node, source: &str) -> bool {
        node.children(&mut node.walk()).any(|child| {
            child.kind() == "modifiers" && {
                child
                    .children(&mut child.walk())
                    .any(|mod_child| node_text(&mod_child, source) == "public")
            }
        })
    }

    /// Create a standardized export for a declaration
    fn create_export(
        name: String,
        kind: TargetKind,
        node: &tree_sitter::Node,
    ) -> StandardizedExport {
        StandardizedExport {
            kind: ExportKind::Named,
            target: ExportTarget {
                name,
                original_name: None,
                kind,
                source_module: None,
            },
            is_reexport: false,
            span: Some(node_to_span(node)),
        }
    }

    /// Recursively find and process export nodes (class/interface/enum definitions)
    fn find_exports_recursive(
        node: &tree_sitter::Node,
        source: &str,
        exports: &mut Vec<StandardizedExport>,
    ) {
        // Check for class/interface/enum definitions
        let (declaration_kind, target_kind) = match node.kind() {
            "class_declaration" => ("class_declaration", TargetKind::Class),
            "interface_declaration" => ("interface_declaration", TargetKind::Interface),
            "enum_declaration" => ("enum_declaration", TargetKind::Class),
            _ => ("", TargetKind::Other),
        };

        if !declaration_kind.is_empty() && Self::is_public_declaration(node, source) {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = node_text(&name_node, source);
                if !name.is_empty() {
                    exports.push(Self::create_export(name, target_kind, node));
                }
            }
        }

        // Check for method declarations (public methods)
        if node.kind() == "method_declaration" && Self::is_public_declaration(node, source) {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = node_text(&name_node, source);
                if !name.is_empty() {
                    exports.push(Self::create_export(name, TargetKind::Function, node));
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

impl Default for JavaExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::AstParser;

    #[test]
    fn test_java_import_extraction() {
        let code = r#"
import java.util.ArrayList;
import java.util.*;
"#;
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::Java);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = JavaExtractor::new();
        let imports = extractor.extract_imports(&tree, code);

        assert!(!imports.is_empty());
        // Should have wildcard import
        assert!(imports.iter().any(|i| i.is_wildcard));
    }

    #[test]
    fn test_java_stdlib_import() {
        let code = r#"
import java.util.List;
import javax.swing.JFrame;
"#;
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::Java);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = JavaExtractor::new();
        let imports = extractor.extract_imports(&tree, code);

        assert_eq!(imports.len(), 2);
        assert!(imports[0].is_system_header);
        assert!(imports[1].is_system_header);
    }

    #[test]
    fn test_java_static_import() {
        let code = r#"
import static java.lang.Math.PI;
import static java.util.Collections.*;
"#;
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::Java);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = JavaExtractor::new();
        let imports = extractor.extract_imports(&tree, code);

        assert_eq!(imports.len(), 2);
        // Check that static imports are marked with alias
        assert!(imports[0].alias.is_some());
        assert!(imports[0].alias.as_ref().unwrap().starts_with("static:"));
        // Check wildcard static import
        assert!(imports[1].is_wildcard);
        assert!(imports[1].alias.as_ref().unwrap().starts_with("static:"));
    }

    #[test]
    fn test_java_export_extraction() {
        let code = r#"
public class MyClass {
    public void method() {}
}

class PrivateClass {}

public interface MyInterface {
    void interfaceMethod();
}

public enum MyEnum {
    VALUE_A, VALUE_B
}
"#;
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::Java);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = JavaExtractor::new();
        let exports = extractor.extract_exports(&tree, code);

        // Should have public exports: MyClass, MyInterface, MyEnum, and method
        assert!(exports.len() >= 3);
        assert!(exports.iter().any(|e| e.target.name == "MyClass"));
        assert!(exports.iter().any(|e| e.target.name == "MyInterface"));
        assert!(exports.iter().any(|e| e.target.name == "MyEnum"));
    }

    #[test]
    fn test_package_declaration_extraction() {
        let code = r#"package com.example.myapp;

public class MyClass {}
"#;
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::Java);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");

        // Test the extraction
        let package = JavaExtractor::extract_package_declaration(&tree, code);
        assert!(package.is_some());
        assert_eq!(package.unwrap(), "com.example.myapp");
    }

    #[test]
    fn test_package_declaration_simple() {
        let code = r#"package mypackage;

class MyClass {}
"#;
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::Java);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let package = JavaExtractor::extract_package_declaration(&tree, code);
        assert!(package.is_some());
        assert_eq!(package.unwrap(), "mypackage");
    }

    #[test]
    fn test_package_declaration_deep() {
        let code = r#"package org.apache.commons.lang3;

class MyClass {}
"#;
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::Java);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let package = JavaExtractor::extract_package_declaration(&tree, code);
        assert!(package.is_some());
        assert_eq!(package.unwrap(), "org.apache.commons.lang3");
    }
}
