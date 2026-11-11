//! Groovy language import/export extractor
//!
//! Extracts import declarations from Groovy source files (including Gradle
//! build scripts parsed with the Groovy DSL).
//!
//! # Supported Import Syntax
//!
//! - `import package.Class` - Single class import
//! - `import package.*` - Wildcard import
//! - `import static package.Class.member` - Static import
//! - `import package.Class as Alias` - Aliased import
//!
//! # Export Semantics
//!
//! Groovy has no explicit export statements. All non-private classes and
//! top-level functions are automatically accessible. This extractor analyzes
//! class definitions and top-level function definitions to identify
//! exported symbols.

use super::traits::SymbolExtractor;
use crate::tree_sitter_query::{node_text, node_to_span};
use cce_types::import::{
    ExportKind, ExportTarget, ImportKind, ImportTarget, StandardizedExport, StandardizedImport,
    TargetKind,
};
use cce_types::language::Language;
use tree_sitter::Tree;

/// Groovy language import/export extractor
pub struct GroovyExtractor;

impl GroovyExtractor {
    pub fn new() -> Self {
        Self
    }

    /// Extract package declaration from Groovy source
    ///
    /// Example: `package com.example.myapp` -> `com.example.myapp`
    fn extract_package_declaration(tree: &Tree, source: &str) -> Option<String> {
        let root_node = tree.root_node();

        for child in root_node.children(&mut root_node.walk()) {
            if child.kind() == "groovy_package" {
                let mut cursor = child.walk();
                for node in child.children(&mut cursor) {
                    if node.kind() == "qualified_name" {
                        let path = node_text(&node, source);
                        if !path.is_empty() {
                            return Some(path);
                        }
                    }
                }
            }
        }

        None
    }

    /// Check if an import is a Groovy/JDK standard library import
    fn is_groovy_stdlib(path: &str) -> bool {
        path.starts_with("groovy.")
            || path.starts_with("org.codehaus.groovy.")
            || path.starts_with("java.")
            || path.starts_with("javax.")
    }

    /// Create a standardized import from a Groovy import node
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
            is_system_header: Self::is_groovy_stdlib(source_path),
            is_relative: false,
            span: Some(node_to_span(node)),
        }
    }

    /// Extract the imported path and alias from a `groovy_import` node.
    /// The alias rides the `import_alias` field (an `identifier` node),
    /// not a dedicated node type.
    fn extract_import_path(
        node: &tree_sitter::Node,
        source: &str,
    ) -> Option<(String, Option<String>)> {
        let path = node
            .child_by_field_name("import")
            .map(|n| node_text(&n, source))
            .filter(|text| !text.is_empty())?;
        let alias = node
            .child_by_field_name("import_alias")
            .map(|n| node_text(&n, source))
            .filter(|text| !text.is_empty());
        Some((path, alias))
    }
}

impl SymbolExtractor for GroovyExtractor {
    fn extract_imports(&self, tree: &Tree, source: &str) -> Vec<StandardizedImport> {
        let mut imports = Vec::new();
        let root_node = tree.root_node();

        Self::find_imports_recursive(&root_node, source, &mut imports);

        imports
    }

    fn extract_exports(&self, tree: &Tree, source: &str) -> Vec<StandardizedExport> {
        let mut exports = Vec::new();
        let root_node = tree.root_node();

        Self::find_exports_recursive(&root_node, source, &mut exports, 0);

        exports
    }

    fn language(&self) -> Language {
        Language::Groovy
    }

    fn extract_package_declaration(&self, tree: &Tree, source: &str) -> Option<String> {
        Self::extract_package_declaration(tree, source)
    }
}

impl GroovyExtractor {
    /// Recursively find and process import nodes
    fn find_imports_recursive(
        node: &tree_sitter::Node,
        source: &str,
        imports: &mut Vec<StandardizedImport>,
    ) {
        if node.kind() == "groovy_import" {
            if let Some((path, alias)) = Self::extract_import_path(node, source) {
                let mut cursor = node.walk();
                let children: Vec<_> = node.children(&mut cursor).collect();
                let is_wildcard = children.iter().any(|c| c.kind() == "wildcard_import");

                if is_wildcard {
                    imports.push(Self::create_import(
                        ImportKind::NamespaceImport,
                        &path,
                        None,
                        None,
                        true,
                        node,
                    ));
                } else {
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

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            Self::find_imports_recursive(&child, source, imports);
        }
    }

    /// Recursively find and process export nodes (class and top-level
    /// function definitions)
    fn find_exports_recursive(
        node: &tree_sitter::Node,
        source: &str,
        exports: &mut Vec<StandardizedExport>,
        depth: usize,
    ) {
        let target_kind = match node.kind() {
            "class_definition" => TargetKind::Class,
            "function_definition" if depth <= 1 => TargetKind::Function,
            _ => TargetKind::Other,
        };

        if !matches!(target_kind, TargetKind::Other) {
            let mut cursor = node.walk();
            let is_private = node.children(&mut cursor).any(|child| {
                (child.kind() == "modifier" || child.kind() == "access_modifier")
                    && node_text(&child, source) == "private"
            });
            if !is_private
                && let Some(name_node) = node
                    .child_by_field_name("function")
                    .or_else(|| node.child_by_field_name("name"))
            {
                let name = node_text(&name_node, source);
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

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            Self::find_exports_recursive(&child, source, exports, depth + 1);
        }
    }
}
