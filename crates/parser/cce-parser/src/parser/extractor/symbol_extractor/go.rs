//! Go language import/export extractor
//!
//! Extracts import declarations and exported symbols.

use super::traits::SymbolExtractor;
use crate::parser::stdlib::GoStdlibDetector;
use crate::tree_sitter_query::{
    find_children_by_kind, find_descendants_by_kind, node_text, node_to_span,
};
use cce_types::import::{
    ExportKind, ExportTarget, ImportKind, StandardizedExport, StandardizedImport, TargetKind,
};
use cce_types::language::Language;
use tree_sitter::Tree;

/// Go language import/export extractor
pub struct GoExtractor;

impl GoExtractor {
    pub fn new() -> Self {
        Self
    }

    /// Extract package declaration from Go source
    ///
    /// Example: `package main` -> `main`
    /// Example: `package mypackage` -> `mypackage`
    fn extract_package_declaration(tree: &Tree, source: &str) -> Option<String> {
        let root_node = tree.root_node();

        // Find package clause
        for child in root_node.children(&mut root_node.walk()) {
            if child.kind() == "package_clause" {
                // Find the package identifier
                for node in child.children(&mut child.walk()) {
                    if node.kind() == "package_identifier" || node.kind() == "identifier" {
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

    /// Check if an import path is from the standard library
    fn is_stdlib_import(&self, path: &str) -> bool {
        // Extract the package name (first component of the path)
        let package = path.split('/').next().unwrap_or(path);
        GoStdlibDetector::is_stdlib_package(package)
    }

    /// Determine the import kind based on alias
    ///
    /// Returns (kind, is_wildcard, should_skip)
    ///
    /// - Dot import (. "math"): imports all symbols into current namespace
    /// - Blank import (_ "lib"): for side effects only, should be skipped
    /// - Regular import: normal module import
    fn determine_import_kind(alias: &Option<String>) -> (ImportKind, bool, bool) {
        match alias.as_deref() {
            Some(".") => {
                // Dot import: import . "math"
                // This imports all exported symbols into the current namespace
                (ImportKind::NamespaceImport, true, false)
            }
            Some("_") => {
                // Blank import: import _ "github.com/lib"
                // This is for side effects only (init functions)
                // We skip it as it doesn't import any symbols
                (ImportKind::SideEffectImport, false, true)
            }
            _ => {
                // Regular import (with or without alias)
                (ImportKind::ModuleImport, false, false)
            }
        }
    }
}

impl SymbolExtractor for GoExtractor {
    fn extract_imports(&self, tree: &Tree, source: &str) -> Vec<StandardizedImport> {
        let mut imports = Vec::new();
        let root_node = tree.root_node();

        // Find all import declarations
        for import_node in find_children_by_kind(&root_node, "import_declaration") {
            // Go imports can be:
            // 1. Single import: import "fmt"
            // 2. Aliased import: import f "fmt"
            // 3. Dot import: import . "math"  (imports all symbols into current namespace)
            // 4. Blank import: import _ "github.com/lib"  (for side effects only)
            // 5. Multiple imports: import ( "fmt"; "math" )

            // First, check if import_declaration itself has a path (single import)
            if let Some(path) = import_node.child_by_field_name("path") {
                let path_text = node_text(&path, source).trim_matches('"').to_string();
                if !path_text.is_empty() {
                    let alias = import_node
                        .child_by_field_name("name")
                        .map(|n| node_text(&n, source));
                    let is_stdlib = self.is_stdlib_import(&path_text);
                    let span = node_to_span(&import_node);

                    // Check for special imports
                    let (kind, is_wildcard, skip_import) = Self::determine_import_kind(&alias);

                    if !skip_import {
                        let import = StandardizedImport {
                            kind,
                            source: path_text,
                            target: Default::default(),
                            alias: alias.filter(|a| !a.is_empty() && a != "." && a != "_"),
                            is_wildcard,
                            is_default: false,
                            is_system_header: is_stdlib,
                            is_relative: false,
                            span: Some(span),
                        };
                        imports.push(import);
                    }
                }
            }

            // Then, find import_spec nodes (for grouped imports) - use descendants for nested specs
            for spec_node in find_descendants_by_kind(&import_node, "import_spec") {
                // Check for alias (e.g., f "fmt", . "math", _ "lib")
                let alias = spec_node
                    .child_by_field_name("name")
                    .map(|n| node_text(&n, source));

                // Get the path
                if let Some(path) = spec_node.child_by_field_name("path") {
                    let path_text = node_text(&path, source).trim_matches('"').to_string();

                    if !path_text.is_empty() {
                        let is_stdlib = self.is_stdlib_import(&path_text);
                        let span = node_to_span(&spec_node);

                        // Check for special imports
                        let (kind, is_wildcard, skip_import) = Self::determine_import_kind(&alias);

                        if !skip_import {
                            let import = StandardizedImport {
                                kind,
                                source: path_text,
                                target: Default::default(),
                                alias: alias.filter(|a| !a.is_empty() && a != "." && a != "_"),
                                is_wildcard,
                                is_default: false,
                                is_system_header: is_stdlib,
                                is_relative: false,
                                span: Some(span),
                            };
                            imports.push(import);
                        }
                    }
                }
            }
        }

        imports
    }

    fn extract_exports(&self, tree: &Tree, source: &str) -> Vec<StandardizedExport> {
        let mut exports = Vec::new();
        let root_node = tree.root_node();

        // In Go, exported symbols are those starting with uppercase letter
        // We need to find all top-level declarations with uppercase names

        let mut cursor = root_node.walk();
        for child in root_node.children(&mut cursor) {
            match child.kind() {
                "function_declaration" => {
                    if let Some(name) = child.child_by_field_name("name") {
                        let fn_name = node_text(&name, source);
                        // Check if starts with uppercase (exported in Go)
                        if !fn_name.is_empty()
                            && fn_name
                                .chars()
                                .next()
                                .map(|c| c.is_uppercase())
                                .unwrap_or(false)
                        {
                            let span = node_to_span(&child);
                            let export = StandardizedExport {
                                kind: ExportKind::Named,
                                target: ExportTarget {
                                    name: fn_name,
                                    original_name: None,
                                    kind: TargetKind::Function,
                                    source_module: None,
                                },
                                is_reexport: false,
                                span: Some(span),
                            };
                            exports.push(export);
                        }
                    }
                }
                "type_declaration" => {
                    // Find all type_spec nodes within type_declaration
                    for spec in find_children_by_kind(&child, "type_spec") {
                        if let Some(name) = spec.child_by_field_name("name") {
                            let type_name = node_text(&name, source);
                            // Check if starts with uppercase (exported in Go)
                            if !type_name.is_empty()
                                && type_name
                                    .chars()
                                    .next()
                                    .map(|c| c.is_uppercase())
                                    .unwrap_or(false)
                            {
                                // Determine the target kind based on the type
                                let target_kind =
                                    if let Some(type_node) = spec.child_by_field_name("type") {
                                        if type_node.kind() == "interface_type" {
                                            TargetKind::Interface
                                        } else {
                                            TargetKind::Class
                                        }
                                    } else {
                                        TargetKind::Class
                                    };

                                let span = node_to_span(&spec);
                                let export = StandardizedExport {
                                    kind: ExportKind::Named,
                                    target: ExportTarget {
                                        name: type_name,
                                        original_name: None,
                                        kind: target_kind,
                                        source_module: None,
                                    },
                                    is_reexport: false,
                                    span: Some(span),
                                };
                                exports.push(export);
                            }
                        }
                    }
                }
                "const_declaration" => {
                    for decl in find_descendants_by_kind(&child, "const_spec") {
                        if let Some(name) = decl.child_by_field_name("name") {
                            let const_name = node_text(&name, source);
                            // Check if starts with uppercase (exported in Go)
                            if !const_name.is_empty()
                                && const_name
                                    .chars()
                                    .next()
                                    .map(|c| c.is_uppercase())
                                    .unwrap_or(false)
                            {
                                let span = node_to_span(&decl);
                                let export = StandardizedExport {
                                    kind: ExportKind::Named,
                                    target: ExportTarget {
                                        name: const_name,
                                        original_name: None,
                                        kind: TargetKind::Variable,
                                        source_module: None,
                                    },
                                    is_reexport: false,
                                    span: Some(span),
                                };
                                exports.push(export);
                            }
                        }
                    }
                }
                "var_declaration" => {
                    for decl in find_descendants_by_kind(&child, "var_spec") {
                        if let Some(name) = decl.child_by_field_name("name") {
                            let var_name = node_text(&name, source);
                            // Check if starts with uppercase (exported in Go)
                            if !var_name.is_empty()
                                && var_name
                                    .chars()
                                    .next()
                                    .map(|c| c.is_uppercase())
                                    .unwrap_or(false)
                            {
                                let span = node_to_span(&decl);
                                let export = StandardizedExport {
                                    kind: ExportKind::Named,
                                    target: ExportTarget {
                                        name: var_name,
                                        original_name: None,
                                        kind: TargetKind::Variable,
                                        source_module: None,
                                    },
                                    is_reexport: false,
                                    span: Some(span),
                                };
                                exports.push(export);
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        exports
    }

    fn language(&self) -> Language {
        Language::Go
    }

    fn extract_package_declaration(&self, tree: &Tree, source: &str) -> Option<String> {
        Self::extract_package_declaration(tree, source)
    }
}

impl Default for GoExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::AstParser;

    #[test]
    fn test_go_import_extraction() {
        let code = r#"
package main

import "fmt"
import "math"
"#;
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::Go);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = GoExtractor::new();
        let imports = extractor.extract_imports(&tree, code);

        assert!(!imports.is_empty());
        // Check for stdlib imports
        assert!(
            imports
                .iter()
                .any(|i| i.source == "fmt" && i.is_system_header)
        );
        assert!(
            imports
                .iter()
                .any(|i| i.source == "math" && i.is_system_header)
        );
    }

    #[test]
    fn test_go_grouped_imports() {
        let code = r#"
package main

import (
    "fmt"
    "os"
    "github.com/gin-gonic/gin"
)
"#;
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::Go);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = GoExtractor::new();
        let imports = extractor.extract_imports(&tree, code);

        assert!(!imports.is_empty());
        // Check for stdlib
        assert!(
            imports
                .iter()
                .any(|i| i.source == "fmt" && i.is_system_header)
        );
        // Check for external package
        assert!(
            imports
                .iter()
                .any(|i| i.source == "github.com/gin-gonic/gin" && !i.is_system_header)
        );
    }

    #[test]
    fn test_go_export_extraction() {
        let code = r#"
package main

func PublicFunction() {}
func privateFunction() {}

type PublicStruct struct {}
type privateStruct struct {}
"#;
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::Go);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = GoExtractor::new();
        let exports = extractor.extract_exports(&tree, code);

        // Should only export capitalized names
        assert!(exports.iter().any(|e| e.target.name == "PublicFunction"));
        assert!(exports.iter().any(|e| e.target.name == "PublicStruct"));
        assert!(!exports.iter().any(|e| e.target.name == "privateFunction"));
        assert!(!exports.iter().any(|e| e.target.name == "privateStruct"));
    }
}
