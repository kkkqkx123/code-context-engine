//! TypeScript language import/export extractor
//!
//! Extracts ES module import/export statements with TypeScript-specific features
//! (type imports, type exports, etc.).

use super::javascript::JavaScriptExtractor;
use super::traits::SymbolExtractor;
use crate::tree_sitter_query::{
    find_child_by_kind, find_children_by_kind, node_text, node_to_span,
};
use cce_types::import::{
    ExportKind, ExportTarget, StandardizedExport, StandardizedImport, TargetKind,
};
use cce_types::language::Language;
use tree_sitter::Tree;

/// TypeScript language import/export extractor
///
/// TypeScript shares most import/export syntax with JavaScript, with additional
/// support for type-only imports and exports.
pub struct TypeScriptExtractor {
    js_extractor: JavaScriptExtractor,
}

impl TypeScriptExtractor {
    pub fn new() -> Self {
        Self {
            js_extractor: JavaScriptExtractor::new(),
        }
    }

    /// Extract module specifier from import statement
    fn extract_module_specifier(node: &tree_sitter::Node, source: &str) -> Option<String> {
        node.child_by_field_name("source").map(|n| {
            node_text(&n, source)
                .trim_matches('"')
                .trim_matches('\'')
                .to_string()
        })
    }

    /// Check if a clause has a direct `type` keyword (`import type` / `export type`).
    /// Also checks inside ERROR nodes for cases where the grammar doesn't fully support
    /// the syntax (e.g., `export type * from 'mod'`).
    fn has_direct_type_keyword(node: &tree_sitter::Node, source: &str) -> bool {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "type" {
                let text = node_text(&child, source);
                if text == "type" {
                    return true;
                }
            }
            // Check ERROR nodes for the type keyword (grammar may not support all TS syntax)
            if child.kind() == "ERROR" {
                let mut cursor2 = child.walk();
                for subchild in child.children(&mut cursor2) {
                    if subchild.kind() == "type" {
                        let text = node_text(&subchild, source);
                        if text == "type" {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    fn local_names_from_import_clause(clause: &tree_sitter::Node, source: &str) -> Vec<String> {
        let mut names = Vec::new();

        if let Some(default_name) = clause.child_by_field_name("name") {
            let name = node_text(&default_name, source);
            if !name.is_empty() {
                names.push(name);
            }
        }

        if let Some(named_imports) = find_child_by_kind(clause, "named_imports") {
            for specifier in find_children_by_kind(&named_imports, "import_specifier") {
                let local_name = specifier
                    .child_by_field_name("alias")
                    .or_else(|| specifier.child_by_field_name("name"))
                    .map(|node| node_text(&node, source))
                    .unwrap_or_default();
                if !local_name.is_empty() {
                    names.push(local_name);
                }
            }
        }

        if let Some(namespace_import) = find_child_by_kind(clause, "namespace_import") {
            if let Some(name) = find_child_by_kind(&namespace_import, "identifier") {
                names.push(node_text(&name, source));
            }
        }

        names
    }
}

impl SymbolExtractor for TypeScriptExtractor {
    fn extract_imports(&self, tree: &Tree, source: &str) -> Vec<StandardizedImport> {
        let mut imports = self.js_extractor.extract_imports(tree, source);
        let root_node = tree.root_node();

        // TypeScript-specific: For import type statements, update the TargetKind to Type
        for import_node in find_children_by_kind(&root_node, "import_statement") {
            let Some(clause) = find_child_by_kind(&import_node, "import_clause") else {
                continue;
            };
            let is_type_import = Self::has_direct_type_keyword(&import_node, source);

            if is_type_import {
                if let Some(specifier) = Self::extract_module_specifier(&import_node, source) {
                    let type_import_names = Self::local_names_from_import_clause(&clause, source);
                    let span = node_to_span(&import_node);

                    // Match the exact statement so another import from the same module is not changed.
                    for import in &mut imports {
                        if import.source == specifier
                            && import.span == Some(span)
                            && type_import_names.contains(&import.target.local_name)
                        {
                            import.target.kind = TargetKind::Type;
                        }
                    }
                }
            }

            // `import { type User, value }` marks only the selected specifier.
            if let Some(named_imports) = find_child_by_kind(&clause, "named_imports") {
                let span = node_to_span(&import_node);
                let specifier = Self::extract_module_specifier(&import_node, source);
                for import_specifier in find_children_by_kind(&named_imports, "import_specifier") {
                    if !Self::has_direct_type_keyword(&import_specifier, source) {
                        continue;
                    }
                    let local_name = import_specifier
                        .child_by_field_name("alias")
                        .or_else(|| import_specifier.child_by_field_name("name"))
                        .map(|node| node_text(&node, source))
                        .unwrap_or_default();
                    for import in &mut imports {
                        if Some(import.source.as_str()) == specifier.as_deref()
                            && import.span == Some(span)
                            && import.target.local_name == local_name
                        {
                            import.target.kind = TargetKind::Type;
                        }
                    }
                }
            }
        }

        imports
    }

    fn extract_exports(&self, tree: &Tree, source: &str) -> Vec<StandardizedExport> {
        let mut exports = self.js_extractor.extract_exports(tree, source);
        let root_node = tree.root_node();

        // TypeScript-specific: Handle export type/interface/enum statements
        for export_node in find_children_by_kind(&root_node, "export_statement") {
            let is_type_export = Self::has_direct_type_keyword(&export_node, source);

            // Handle export type { T } or export type { T as U } from 'mod'
            if is_type_export {
                if let Some(specifier) = Self::extract_module_specifier(&export_node, source) {
                    let span = node_to_span(&export_node);
                    // Match the exact statement so another re-export from the same module is not changed.
                    let mut updated = false;
                    for export in &mut exports {
                        if export
                            .target
                            .source_module
                            .as_ref()
                            .is_some_and(|s| s == &specifier)
                            && export.span == Some(span)
                        {
                            export.target.kind = TargetKind::Type;
                            updated = true;
                        }
                    }

                    if !updated {
                        // Handle export type { T } or export type { T as U } from 'mod'
                        if let Some(clause) = find_child_by_kind(&export_node, "export_clause") {
                            if let Some(named_exports) =
                                find_child_by_kind(&clause, "named_exports")
                            {
                                for export_specifier in
                                    find_children_by_kind(&named_exports, "export_specifier")
                                {
                                    let name = export_specifier
                                        .child_by_field_name("name")
                                        .map(|n| node_text(&n, source))
                                        .unwrap_or_default();

                                    let alias = export_specifier
                                        .child_by_field_name("alias")
                                        .map(|n| node_text(&n, source))
                                        .unwrap_or_default();

                                    if !name.is_empty() {
                                        exports.push(StandardizedExport {
                                            kind: ExportKind::Reexport,
                                            target: ExportTarget {
                                                name: alias.clone(),
                                                original_name: if alias != name {
                                                    Some(name)
                                                } else {
                                                    None
                                                },
                                                kind: TargetKind::Type,
                                                source_module: Some(specifier.clone()),
                                            },
                                            is_reexport: true,
                                            span: Some(span),
                                        });
                                    }
                                }
                            }

                            if find_child_by_kind(&clause, "*").is_some() {
                                exports.push(StandardizedExport {
                                    kind: ExportKind::Wildcard,
                                    target: ExportTarget {
                                        name: "*".to_string(),
                                        original_name: None,
                                        kind: TargetKind::Type,
                                        source_module: Some(specifier.clone()),
                                    },
                                    is_reexport: true,
                                    span: Some(span),
                                });
                            }
                        }
                    }
                }
            }

            // `export { type User, value } from 'module'` marks only one specifier.
            if let (Some(specifier), Some(clause)) = (
                Self::extract_module_specifier(&export_node, source),
                find_child_by_kind(&export_node, "export_clause"),
            ) {
                if let Some(named_exports) = find_child_by_kind(&clause, "named_exports") {
                    let span = node_to_span(&export_node);
                    for export_specifier in
                        find_children_by_kind(&named_exports, "export_specifier")
                    {
                        if !Self::has_direct_type_keyword(&export_specifier, source) {
                            continue;
                        }
                        let exported_name = export_specifier
                            .child_by_field_name("alias")
                            .or_else(|| export_specifier.child_by_field_name("name"))
                            .map(|node| node_text(&node, source))
                            .unwrap_or_default();
                        for export in &mut exports {
                            if export.target.source_module.as_deref() == Some(specifier.as_str())
                                && export.span == Some(span)
                                && export.target.name == exported_name
                            {
                                export.target.kind = TargetKind::Type;
                            }
                        }
                    }
                }
            }

            // Handle direct type exports: export type T = ..., export interface I {}, export enum E {}
            if let Some(declaration) = export_node.child_by_field_name("declaration") {
                let (export_name, target_kind) = match declaration.kind() {
                    "type_alias_declaration" => (
                        declaration
                            .child_by_field_name("name")
                            .map(|n| node_text(&n, source))
                            .unwrap_or_default(),
                        TargetKind::Type,
                    ),
                    "interface_declaration" => (
                        declaration
                            .child_by_field_name("name")
                            .map(|n| node_text(&n, source))
                            .unwrap_or_default(),
                        TargetKind::Interface,
                    ),
                    "enum_declaration" => (
                        declaration
                            .child_by_field_name("name")
                            .map(|n| node_text(&n, source))
                            .unwrap_or_default(),
                        TargetKind::Other,
                    ),
                    _ => (String::new(), TargetKind::Other),
                };

                if !export_name.is_empty() {
                    exports.push(StandardizedExport {
                        kind: ExportKind::Named,
                        target: ExportTarget {
                            name: export_name.clone(),
                            original_name: None,
                            kind: target_kind,
                            source_module: None,
                        },
                        is_reexport: false,
                        span: Some(node_to_span(&export_node)),
                    });
                }
            }
        }

        exports
    }

    fn language(&self) -> Language {
        Language::TypeScript
    }
}

impl Default for TypeScriptExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::AstParser;

    #[test]
    fn test_typescript_type_only_import() {
        let code = "import type { User } from './user';";
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::TypeScript);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = TypeScriptExtractor::new();
        let imports = extractor.extract_imports(&tree, code);

        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].source, "./user");
        assert_eq!(imports[0].target.local_name, "User");
        assert_eq!(imports[0].target.kind, TargetKind::Type);
    }

    #[test]
    fn test_typescript_type_only_export() {
        let code = "export type { UserProps } from './types';";
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::TypeScript);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = TypeScriptExtractor::new();
        let exports = extractor.extract_exports(&tree, code);

        assert_eq!(exports.len(), 1);
        assert_eq!(exports[0].target.name, "UserProps");
        assert_eq!(exports[0].target.kind, TargetKind::Type);
        assert!(exports[0].is_reexport);
    }

    #[test]
    fn test_typescript_mixed_imports() {
        let code = r#"
            import React from 'react';
            import type { FC } from 'react';
            import { useState, useEffect } from 'react';
        "#;
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::TypeScript);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = TypeScriptExtractor::new();
        let imports = extractor.extract_imports(&tree, code);

        assert!(imports.len() >= 3);
        let type_import = imports
            .iter()
            .find(|i| i.target.kind == TargetKind::Type)
            .expect("type import should be present");
        assert_eq!(type_import.target.local_name, "FC");
        assert!(imports.iter().any(|i| {
            i.source == "react"
                && i.target.local_name == "React"
                && i.target.kind != TargetKind::Type
        }));
    }

    #[test]
    fn test_typescript_namespace_import() {
        let code = "import * as Utils from './utils';";
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::TypeScript);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = TypeScriptExtractor::new();
        let imports = extractor.extract_imports(&tree, code);

        assert_eq!(imports.len(), 1);
        assert!(imports[0].is_wildcard);
        assert_eq!(imports[0].alias, Some("Utils".to_string()));
    }

    #[test]
    fn test_typescript_re_export() {
        let code = "export { User } from './user';";
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::TypeScript);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = TypeScriptExtractor::new();
        let exports = extractor.extract_exports(&tree, code);

        assert_eq!(exports.len(), 1);
        assert!(exports[0].is_reexport);
        assert_eq!(exports[0].target.name, "User");
    }

    #[test]
    fn test_typescript_type_import_with_alias() {
        let code = "import type { User as UserModel } from './user';";
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::TypeScript);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = TypeScriptExtractor::new();
        let imports = extractor.extract_imports(&tree, code);

        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].target.local_name, "UserModel");
        assert_eq!(imports[0].target.original_name, Some("User".to_string()));
        assert_eq!(imports[0].alias, Some("UserModel".to_string()));
    }

    #[test]
    fn test_typescript_type_and_value_imports_same_module() {
        let code = r#"
            import { User } from './user';
            import type { User as UserType } from './user';
        "#;
        let mut parser = AstParser::new();
        let (tree, _) = parser
            .parse_with_tree(code, &Language::TypeScript)
            .expect("Failed to parse");
        let imports = TypeScriptExtractor::new().extract_imports(&tree, code);

        assert!(imports.iter().any(|import| {
            import.target.local_name == "User" && import.target.kind != TargetKind::Type
        }));
        assert!(imports.iter().any(|import| {
            import.target.local_name == "UserType" && import.target.kind == TargetKind::Type
        }));
    }

    #[test]
    fn test_typescript_type_wildcard_export() {
        let code = "export type * from './types';";
        let mut parser = AstParser::new();
        let (tree, _) = parser
            .parse_with_tree(code, &Language::TypeScript)
            .expect("Failed to parse");
        let exports = TypeScriptExtractor::new().extract_exports(&tree, code);

        assert!(exports.iter().any(|export| {
            export.target.name == "*"
                && export.target.kind == TargetKind::Type
                && export.is_reexport
        }));
    }

    #[test]
    fn test_typescript_mixed_type_specifiers() {
        let code = "import { type User, value } from './model';";
        let mut parser = AstParser::new();
        let (tree, _) = parser
            .parse_with_tree(code, &Language::TypeScript)
            .expect("Failed to parse");
        let imports = TypeScriptExtractor::new().extract_imports(&tree, code);

        assert!(imports.iter().any(|import| {
            import.target.local_name == "User" && import.target.kind == TargetKind::Type
        }));
        assert!(imports.iter().any(|import| {
            import.target.local_name == "value" && import.target.kind != TargetKind::Type
        }));
    }

    #[test]
    fn test_typescript_interface_export() {
        let code = r#"
            export interface IUser {
                id: number;
                name: string;
            }
        "#;
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::TypeScript);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = TypeScriptExtractor::new();
        let exports = extractor.extract_exports(&tree, code);

        assert!(exports.iter().any(|e| e.target.name == "IUser"));
        assert!(
            exports
                .iter()
                .any(|e| e.target.kind == TargetKind::Interface)
        );
    }

    #[test]
    fn test_typescript_type_alias_export() {
        let code = "export type UserProps = { id: number; name: string; };";
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::TypeScript);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = TypeScriptExtractor::new();
        let exports = extractor.extract_exports(&tree, code);

        assert!(exports.iter().any(|e| e.target.name == "UserProps"));
        assert!(exports.iter().any(|e| e.target.kind == TargetKind::Type));
    }
}
