//! Dart language import/export extractor
//!
//! Extracts import/export directives from Dart source files.
//!
//! # Supported Import Syntax
//!
//! - `import 'package:path/file.dart';` - Standard import
//! - `import 'package:path/file.dart' as prefix;` - Prefixed import
//! - `import 'package:path/file.dart' show Class1, Class2;` - Selective import
//! - `import 'package:path/file.dart' hide privateClass;` - Hiding import
//! - `import 'dart:core';` - Dart SDK import
//! - `part 'file.dart';` - Part file
//! - `part of 'library.dart';` - Part of declaration
//!
//! # Supported Export Syntax
//!
//! - `export 'package:path/file.dart';` - Export directive

use super::traits::SymbolExtractor;
use crate::tree_sitter_query::{find_children_by_kind, node_text, node_to_span};
use cce_types::import::{
    ExportKind, ExportTarget, ImportKind, ImportTarget, StandardizedExport, StandardizedImport,
    TargetKind,
};
use cce_types::language::Language;
use tree_sitter::Tree;

/// Dart language import/export extractor
pub struct DartExtractor;

impl DartExtractor {
    pub fn new() -> Self {
        Self
    }

    /// Check if an import path is a relative import
    fn is_relative_import(path: &str) -> bool {
        path.starts_with("./") || path.starts_with("../") || path == "." || path == ".."
    }

    /// Check if an import is a Dart SDK import
    fn is_dart_sdk_import(path: &str) -> bool {
        path.starts_with("dart:")
    }

    /// Extract URI from import/export specification
    fn extract_uri(node: &tree_sitter::Node, source: &str) -> Option<String> {
        // For import_specification and library_export: configurable_uri -> uri -> string_literal
        // For part_directive and part_of_directive: uri -> string_literal

        // Try configurable_uri first (for import/export)
        if let Some(config_uri) = find_children_by_kind(node, "configurable_uri")
            .first()
            .copied()
        {
            if let Some(uri) = find_children_by_kind(&config_uri, "uri").first().copied() {
                if let Some(str_lit) = find_children_by_kind(&uri, "string_literal")
                    .first()
                    .copied()
                {
                    let text = node_text(&str_lit, source);
                    return Some(
                        text.trim_matches('"')
                            .trim_matches('\'')
                            .trim_start_matches('r')
                            .to_string(),
                    );
                }
            }
        }

        // Try direct uri (for part directives)
        if let Some(uri) = find_children_by_kind(node, "uri").first().copied() {
            if let Some(str_lit) = find_children_by_kind(&uri, "string_literal")
                .first()
                .copied()
            {
                let text = node_text(&str_lit, source);
                return Some(
                    text.trim_matches('"')
                        .trim_matches('\'')
                        .trim_start_matches('r')
                        .to_string(),
                );
            }
        }

        None
    }

    /// Create a standardized import from Dart import
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
            is_wildcard: false,
            is_default: false,
            is_system_header: Self::is_dart_sdk_import(source_path),
            is_relative: Self::is_relative_import(source_path),
            span: Some(node_to_span(node)),
        }
    }
}

impl SymbolExtractor for DartExtractor {
    fn extract_imports(&self, tree: &Tree, source: &str) -> Vec<StandardizedImport> {
        let mut imports = Vec::new();
        let root_node = tree.root_node();

        // Recursively find all library_import nodes
        Self::find_imports_recursive(&root_node, source, &mut imports);

        imports
    }

    fn extract_exports(&self, tree: &Tree, source: &str) -> Vec<StandardizedExport> {
        let mut exports = Vec::new();
        let root_node = tree.root_node();

        // Recursively find all library_export nodes
        Self::find_exports_recursive(&root_node, source, &mut exports);

        exports
    }

    fn language(&self) -> Language {
        Language::Dart
    }
}

impl DartExtractor {
    /// Recursively find and process import nodes
    fn find_imports_recursive(
        node: &tree_sitter::Node,
        source: &str,
        imports: &mut Vec<StandardizedImport>,
    ) {
        // Check if this node is a library_import
        if node.kind() == "library_import" {
            // Find import_specification within library_import
            for import_node in find_children_by_kind(node, "import_specification") {
                // Extract URI
                if let Some(uri) = Self::extract_uri(&import_node, source) {
                    // Check for alias (import '...' as alias;)
                    let alias = import_node
                        .child_by_field_name("alias")
                        .map(|n| node_text(&n, source));

                    // Check for show/hide combinators
                    let has_show =
                        !find_children_by_kind(&import_node, "show_combinator").is_empty();
                    let has_hide =
                        !find_children_by_kind(&import_node, "hide_combinator").is_empty();

                    // Determine import kind
                    let kind = if has_show || has_hide {
                        ImportKind::SymbolImport
                    } else if alias.is_some() {
                        ImportKind::NamespaceImport
                    } else {
                        ImportKind::ModuleImport
                    };

                    imports.push(Self::create_import(
                        kind,
                        &uri,
                        None,
                        alias.as_deref(),
                        node,
                    ));
                }
            }
        }

        // Check if this node is a part_directive
        if node.kind() == "part_directive" {
            if let Some(uri) = Self::extract_uri(node, source) {
                let is_relative = Self::is_relative_import(&uri);
                imports.push(StandardizedImport {
                    kind: ImportKind::SymbolImport,
                    source: uri,
                    target: ImportTarget::default(),
                    alias: None,
                    is_wildcard: false,
                    is_default: false,
                    is_system_header: false,
                    is_relative,
                    span: Some(node_to_span(node)),
                });
            }
        }

        // Check if this node is a part_of_directive
        if node.kind() == "part_of_directive" {
            if let Some(uri) = Self::extract_uri(node, source) {
                let is_relative = Self::is_relative_import(&uri);
                imports.push(StandardizedImport {
                    kind: ImportKind::SymbolImport,
                    source: uri,
                    target: ImportTarget::default(),
                    alias: None,
                    is_wildcard: false,
                    is_default: false,
                    is_system_header: false,
                    is_relative,
                    span: Some(node_to_span(node)),
                });
            }
        }

        // Recursively process children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            Self::find_imports_recursive(&child, source, imports);
        }
    }

    /// Recursively find and process export nodes
    fn find_exports_recursive(
        node: &tree_sitter::Node,
        source: &str,
        exports: &mut Vec<StandardizedExport>,
    ) {
        // Check if this node is a library_export (re-export)
        if node.kind() == "library_export" {
            if let Some(uri) = Self::extract_uri(node, source) {
                exports.push(StandardizedExport {
                    kind: ExportKind::Wildcard,
                    target: ExportTarget {
                        name: "*".to_string(),
                        original_name: None,
                        kind: TargetKind::Module,
                        source_module: Some(uri),
                    },
                    is_reexport: true,
                    span: Some(node_to_span(node)),
                });
            }
        }

        // In Dart, all top-level definitions are automatically exported
        // Extract class definitions
        if node.kind() == "class_declaration" {
            if let Some(name) = node.child_by_field_name("name") {
                let class_name = node_text(&name, source);
                if !class_name.is_empty() {
                    exports.push(StandardizedExport {
                        kind: ExportKind::Named,
                        target: ExportTarget {
                            name: class_name,
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

        // Extract function definitions (top-level function_signature)
        if node.kind() == "function_signature" {
            if let Some(name) = node.child_by_field_name("name") {
                let func_name = node_text(&name, source);
                if !func_name.is_empty() {
                    exports.push(StandardizedExport {
                        kind: ExportKind::Named,
                        target: ExportTarget {
                            name: func_name,
                            original_name: None,
                            kind: TargetKind::Function,
                            source_module: None,
                        },
                        is_reexport: false,
                        span: Some(node_to_span(node)),
                    });
                }
            }
        }

        // Extract mixin definitions
        if node.kind() == "mixin_declaration" {
            if let Some(name) = node.child_by_field_name("name") {
                let mixin_name = node_text(&name, source);
                if !mixin_name.is_empty() {
                    exports.push(StandardizedExport {
                        kind: ExportKind::Named,
                        target: ExportTarget {
                            name: mixin_name,
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

        // Extract extension definitions
        if node.kind() == "extension_declaration" {
            if let Some(name) = node.child_by_field_name("name") {
                let ext_name = node_text(&name, source);
                if !ext_name.is_empty() {
                    exports.push(StandardizedExport {
                        kind: ExportKind::Named,
                        target: ExportTarget {
                            name: ext_name,
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

        // Extract enum definitions
        if node.kind() == "enum_declaration" {
            if let Some(name) = node.child_by_field_name("name") {
                let enum_name = node_text(&name, source);
                if !enum_name.is_empty() {
                    exports.push(StandardizedExport {
                        kind: ExportKind::Named,
                        target: ExportTarget {
                            name: enum_name,
                            original_name: None,
                            kind: TargetKind::Other,
                            source_module: None,
                        },
                        is_reexport: false,
                        span: Some(node_to_span(node)),
                    });
                }
            }
        }

        // Extract typedef definitions
        if node.kind() == "type_alias" {
            if let Some(name) = node.child_by_field_name("name") {
                let type_name = node_text(&name, source);
                if !type_name.is_empty() {
                    exports.push(StandardizedExport {
                        kind: ExportKind::Named,
                        target: ExportTarget {
                            name: type_name,
                            original_name: None,
                            kind: TargetKind::Type,
                            source_module: None,
                        },
                        is_reexport: false,
                        span: Some(node_to_span(node)),
                    });
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

impl Default for DartExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::AstParser;

    #[test]
    fn test_dart_import_extraction() {
        let code = r#"
import 'package:flutter/material.dart';
import 'dart:io' as io;
import 'package:test/test.dart' show test, expect;

void main() {}
"#;
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::Dart);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = DartExtractor::new();
        let imports = extractor.extract_imports(&tree, code);

        assert_eq!(imports.len(), 3);
        assert!(imports[0].source.contains("flutter"));
        assert!(imports[1].alias.is_some());
        assert_eq!(imports[1].alias.as_deref(), Some("io"));
    }

    #[test]
    fn test_dart_sdk_import() {
        let code = r#"
import 'dart:core';
import 'dart:async';

void main() {}
"#;
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::Dart);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = DartExtractor::new();
        let imports = extractor.extract_imports(&tree, code);

        assert_eq!(imports.len(), 2);
        assert!(imports[0].is_system_header);
        assert!(imports[1].is_system_header);
    }

    #[test]
    fn test_dart_part_directive() {
        let code = r#"
part 'src/part1.dart';
part 'src/part2.dart';
"#;
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::Dart);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = DartExtractor::new();
        let imports = extractor.extract_imports(&tree, code);

        assert_eq!(imports.len(), 2);
    }

    #[test]
    fn test_dart_export_extraction() {
        let code = r#"
export 'src/public_api.dart';

void main() {}
"#;
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::Dart);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = DartExtractor::new();
        let exports = extractor.extract_exports(&tree, code);

        // Should have 2 exports:
        // 1. The re-export of 'src/public_api.dart'
        // 2. The top-level function 'main' (Dart auto-exports all top-level definitions)
        assert_eq!(exports.len(), 2);

        // Check for the re-export
        assert!(
            exports.iter().any(|e| e.is_reexport
                && e.target.source_module.as_deref() == Some("src/public_api.dart"))
        );

        // Check for the function export
        assert!(exports.iter().any(|e| !e.is_reexport
            && e.target.name == "main"
            && e.target.kind == TargetKind::Function));
    }
}
