//! JavaScript language import/export extractor
//!
//! Extracts ES6 import/export statements and CommonJS require/exports.

use super::common::helpers::{path::is_relative_js, string::unquote};
use super::traits::SymbolExtractor;
use crate::tree_sitter_query::{
    find_child_by_kind, find_children_by_kind, find_descendants_by_kind, node_text, node_to_span,
};
use cce_types::import::{
    ExportKind, ExportTarget, ImportKind, ImportTarget, StandardizedExport, StandardizedImport,
    TargetKind,
};
use cce_types::language::Language;
use cce_types::position::Span;
use tree_sitter::Tree;

/// JavaScript language import/export extractor
pub struct JavaScriptExtractor;

impl JavaScriptExtractor {
    pub fn new() -> Self {
        Self
    }

    /// Check if an import is a Node.js standard library module
    fn is_node_stdlib(path: &str) -> bool {
        // Node.js built-in modules (Node.js 20+)
        const NODE_STDLIB: &[&str] = &[
            "assert",
            "buffer",
            "child_process",
            "cluster",
            "console",
            "constants",
            "crypto",
            "dgram",
            "dns",
            "domain",
            "events",
            "fs",
            "http",
            "https",
            "inspector",
            "module",
            "net",
            "os",
            "path",
            "perf_hooks",
            "process",
            "punycode",
            "querystring",
            "readline",
            "repl",
            "stream",
            "string_decoder",
            "sys",
            "timers",
            "tls",
            "trace_events",
            "tty",
            "url",
            "util",
            "v8",
            "vm",
            "worker_threads",
            "zlib",
            "node:assert",
            "node:buffer",
            "node:child_process",
            "node:cluster",
            "node:console",
            "node:constants",
            "node:crypto",
            "node:dgram",
            "node:dns",
            "node:domain",
            "node:events",
            "node:fs",
            "node:http",
            "node:https",
            "node:inspector",
            "node:module",
            "node:net",
            "node:os",
            "node:path",
            "node:perf_hooks",
            "node:process",
            "node:punycode",
            "node:querystring",
            "node:readline",
            "node:repl",
            "node:stream",
            "node:string_decoder",
            "node:sys",
            "node:timers",
            "node:tls",
            "node:trace_events",
            "node:tty",
            "node:url",
            "node:util",
            "node:v8",
            "node:vm",
            "node:worker_threads",
            "node:zlib",
        ];

        // Extract the base module name (before any path separator)
        let base_module = path.split('/').next().unwrap_or(path);
        NODE_STDLIB.contains(&base_module)
    }

    /// Create a standardized import from ES module import
    fn create_es_import(
        kind: ImportKind,
        source: &str,
        target_name: Option<&str>,
        alias: Option<&str>,
        span: Option<Span>,
    ) -> StandardizedImport {
        let target = if let Some(name) = target_name {
            ImportTarget {
                local_name: alias.unwrap_or(name).to_string(),
                original_name: if alias.is_some() {
                    Some(name.to_string())
                } else {
                    None
                },
                kind: TargetKind::Function, // Default to function, can be refined
            }
        } else {
            ImportTarget::default()
        };

        StandardizedImport {
            kind,
            source: source.to_string(),
            target,
            alias: alias.map(|a| a.to_string()),
            is_wildcard: matches!(kind, ImportKind::NamespaceImport | ImportKind::ModuleImport),
            is_default: matches!(kind, ImportKind::DefaultImport),
            is_system_header: Self::is_node_stdlib(source),
            is_relative: is_relative_js(source),
            span,
        }
    }

    /// Extract module specifier from import statement
    fn extract_module_specifier(node: &tree_sitter::Node, source: &str) -> Option<String> {
        node.child_by_field_name("source")
            .map(|n| unquote(&node_text(&n, source)).to_string())
    }

    /// Extract string value from a string node
    fn extract_string_value(node: &tree_sitter::Node, source: &str) -> String {
        unquote(&node_text(node, source)).to_string()
    }
}

impl SymbolExtractor for JavaScriptExtractor {
    fn extract_imports(&self, tree: &Tree, source: &str) -> Vec<StandardizedImport> {
        let mut imports = Vec::new();
        let root_node = tree.root_node();

        // ES6 imports: import x from 'mod', import { x } from 'mod', import * as ns from 'mod'
        for import_node in find_children_by_kind(&root_node, "import_statement") {
            if let Some(specifier) = Self::extract_module_specifier(&import_node, source) {
                let span = node_to_span(&import_node);

                // Find import_clause node
                if let Some(import_clause) = find_child_by_kind(&import_node, "import_clause") {
                    // Look for default import (import x from 'mod')
                    // The identifier is a direct child of import_clause
                    if let Some(identifier) = find_child_by_kind(&import_clause, "identifier") {
                        let name = node_text(&identifier, source);
                        if !name.is_empty() {
                            // Check if this is a default import (no named_imports or namespace_import)
                            if find_child_by_kind(&import_clause, "named_imports").is_none()
                                && find_child_by_kind(&import_clause, "namespace_import").is_none()
                            {
                                imports.push(Self::create_es_import(
                                    ImportKind::DefaultImport,
                                    &specifier,
                                    Some(&name),
                                    None,
                                    Some(span),
                                ));
                            }
                        }
                    }

                    // Look for named imports (import { x, y as z } from 'mod')
                    if let Some(named_imports) = find_child_by_kind(&import_clause, "named_imports")
                    {
                        for import_specifier in
                            find_children_by_kind(&named_imports, "import_specifier")
                        {
                            let local_name = import_specifier
                                .child_by_field_name("name")
                                .map(|n| node_text(&n, source))
                                .unwrap_or_default();

                            let alias = import_specifier
                                .child_by_field_name("alias")
                                .map(|n| node_text(&n, source))
                                .unwrap_or_default();

                            if !local_name.is_empty() {
                                imports.push(Self::create_es_import(
                                    ImportKind::SymbolImport,
                                    &specifier,
                                    Some(&local_name),
                                    if alias.is_empty() { None } else { Some(&alias) },
                                    Some(span),
                                ));
                            }
                        }
                    }

                    // Look for namespace imports (import * as ns from 'mod')
                    if let Some(namespace_import) =
                        find_child_by_kind(&import_clause, "namespace_import")
                    {
                        // The identifier is a child of namespace_import
                        if let Some(identifier) =
                            find_child_by_kind(&namespace_import, "identifier")
                        {
                            let ns_name = node_text(&identifier, source);
                            if !ns_name.is_empty() {
                                imports.push(Self::create_es_import(
                                    ImportKind::NamespaceImport,
                                    &specifier,
                                    None,
                                    Some(&ns_name),
                                    Some(span),
                                ));
                            }
                        }
                    }
                } else {
                    // Side-effect import (import 'mod') - no import_clause
                    imports.push(StandardizedImport {
                        kind: ImportKind::SideEffectImport,
                        source: specifier.clone(),
                        target: ImportTarget::default(),
                        alias: None,
                        is_wildcard: false,
                        is_default: false,
                        is_system_header: false,
                        is_relative: is_relative_js(&specifier),
                        span: Some(span),
                    });
                }
            }
        }

        // Dynamic imports: import('module')
        for call_node in find_descendants_by_kind(&root_node, "call_expression") {
            if let Some(callee) = call_node.child_by_field_name("function") {
                // Check if callee is "import" (kind is "import", not identifier)
                if callee.kind() == "import" {
                    if let Some(args) = call_node.child_by_field_name("arguments") {
                        // Find string argument in arguments node
                        if let Some(arg) = find_children_by_kind(&args, "string").into_iter().next()
                        {
                            let specifier = Self::extract_string_value(&arg, source);
                            let span = node_to_span(&call_node);
                            imports.push(StandardizedImport {
                                kind: ImportKind::DynamicImport,
                                source: specifier.clone(),
                                target: ImportTarget::default(),
                                alias: None,
                                is_wildcard: false,
                                is_default: false,
                                is_system_header: false,
                                is_relative: is_relative_js(&specifier),
                                span: Some(span),
                            });
                        }
                    }
                }
            }
        }

        // CommonJS require: require('module'), require("module")
        for call_node in find_descendants_by_kind(&root_node, "call_expression") {
            if let Some(callee) = call_node.child_by_field_name("function") {
                if node_text(&callee, source) == "require" {
                    if let Some(args) = call_node.child_by_field_name("arguments") {
                        // Find string argument in arguments node
                        if let Some(arg) = find_children_by_kind(&args, "string").into_iter().next()
                        {
                            let specifier = Self::extract_string_value(&arg, source);
                            let span = node_to_span(&call_node);

                            // Check if this is a destructured require
                            // const { x, y } = require('module')
                            let mut imported_names = Vec::new();

                            // Look for parent assignment to check for destructuring
                            let mut parent = call_node.parent();
                            while let Some(p) = parent {
                                if p.kind() == "variable_declarator" {
                                    if let Some(name_field) = p.child_by_field_name("name") {
                                        if name_field.kind() == "object_pattern" {
                                            // Destructured require: const { x, y } = require('module')
                                            // Iterate through all children of object_pattern
                                            let mut cursor = name_field.walk();
                                            for child in name_field.children(&mut cursor) {
                                                let child_kind = child.kind();
                                                if child_kind
                                                    == "shorthand_property_identifier_pattern"
                                                {
                                                    // { x } - shorthand property
                                                    let name = node_text(&child, source);
                                                    if !name.is_empty() {
                                                        imported_names.push(name.to_string());
                                                    }
                                                } else if child_kind == "property_identifier" {
                                                    // Property identifier in object pattern
                                                    let name = node_text(&child, source);
                                                    if !name.is_empty() {
                                                        imported_names.push(name.to_string());
                                                    }
                                                } else if child_kind == "pair_pattern" {
                                                    // { original: alias }
                                                    if let Some(key) =
                                                        child.child_by_field_name("key")
                                                    {
                                                        let key_name = node_text(&key, source);
                                                        if !key_name.is_empty() {
                                                            imported_names
                                                                .push(key_name.to_string());
                                                        }
                                                    }
                                                }
                                            }
                                        } else {
                                            // Simple require: const x = require('module')
                                            let local_name = node_text(&name_field, source);
                                            if !local_name.is_empty() {
                                                imports.push(StandardizedImport {
                                                    kind: ImportKind::CommonJSRequire,
                                                    source: specifier.clone(),
                                                    target: ImportTarget {
                                                        local_name: local_name.to_string(),
                                                        original_name: None,
                                                        kind: TargetKind::Module,
                                                    },
                                                    alias: None,
                                                    is_wildcard: false,
                                                    is_default: false,
                                                    is_system_header: false,
                                                    is_relative: is_relative_js(&specifier),
                                                    span: Some(span),
                                                });
                                            }
                                        }
                                    }
                                    break;
                                }
                                parent = p.parent();
                            }

                            // Add destructured imports
                            for name in imported_names {
                                imports.push(StandardizedImport {
                                    kind: ImportKind::CommonJSRequire,
                                    source: specifier.clone(),
                                    target: ImportTarget {
                                        local_name: name.clone(),
                                        original_name: None,
                                        kind: TargetKind::Other,
                                    },
                                    alias: None,
                                    is_wildcard: false,
                                    is_default: false,
                                    is_system_header: false,
                                    is_relative: is_relative_js(&specifier),
                                    span: Some(span),
                                });
                            }
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

        // ES6 exports: export default x, export { x }, export * from 'mod'
        for export_node in find_children_by_kind(&root_node, "export_statement") {
            let span = node_to_span(&export_node);

            // Check for default export (field name is "value", not "default")
            if let Some(default_val) = export_node.child_by_field_name("value") {
                let _name = node_text(&default_val, source);
                let export = StandardizedExport {
                    kind: ExportKind::Default,
                    target: ExportTarget {
                        name: "default".to_string(),
                        original_name: None,
                        kind: TargetKind::Other,
                        source_module: None,
                    },
                    is_reexport: false,
                    span: Some(span),
                };
                exports.push(export);
            }

            // Check for named exports (export { x, y }) - use export_clause, not named_exports
            if let Some(clause) = find_child_by_kind(&export_node, "export_clause") {
                // Check if this is a re-export (has source)
                let is_reexport = export_node.child_by_field_name("source").is_some();
                let source_module = if is_reexport {
                    Self::extract_module_specifier(&export_node, source)
                } else {
                    None
                };

                for export_specifier in find_children_by_kind(&clause, "export_specifier") {
                    let name = export_specifier
                        .child_by_field_name("name")
                        .map(|n| node_text(&n, source))
                        .unwrap_or_default();

                    let alias = export_specifier
                        .child_by_field_name("alias")
                        .map(|n| node_text(&n, source))
                        .unwrap_or_default();

                    if !name.is_empty() {
                        let kind = if is_reexport {
                            ExportKind::Reexport
                        } else {
                            ExportKind::Named
                        };

                        let export = StandardizedExport {
                            kind,
                            target: ExportTarget {
                                name: if alias.is_empty() {
                                    name.clone()
                                } else {
                                    alias.clone()
                                },
                                original_name: if alias.is_empty() { None } else { Some(name) },
                                kind: TargetKind::Other,
                                source_module: source_module.clone(),
                            },
                            is_reexport,
                            span: Some(span),
                        };
                        exports.push(export);
                    }
                }
            }

            // Check for wildcard re-export (export * from 'mod')
            if let Some(_wildcard) = find_child_by_kind(&export_node, "*") {
                if let Some(specifier) = Self::extract_module_specifier(&export_node, source) {
                    let export = StandardizedExport {
                        kind: ExportKind::Wildcard,
                        target: ExportTarget {
                            name: "*".to_string(),
                            original_name: None,
                            kind: TargetKind::Module,
                            source_module: Some(specifier),
                        },
                        is_reexport: true,
                        span: Some(span),
                    };
                    exports.push(export);
                }
            }

            // Check for direct exports: export const/let/var/class/function
            if export_node.child_by_field_name("declaration").is_some() {
                if let Some(decl) = export_node.child_by_field_name("declaration") {
                    let export_name = match decl.kind() {
                        "lexical_declaration" | "variable_declaration" => {
                            // export const x = 1 or export let y = 2
                            // Find variable_declarator child
                            let mut name = String::new();
                            for declarator in find_children_by_kind(&decl, "variable_declarator") {
                                if let Some(name_node) = declarator.child_by_field_name("name") {
                                    name = node_text(&name_node, source);
                                    break;
                                }
                            }
                            name
                        }
                        "function_declaration" => decl
                            .child_by_field_name("name")
                            .map(|n| node_text(&n, source))
                            .unwrap_or_default(),
                        "class_declaration" => decl
                            .child_by_field_name("name")
                            .map(|n| node_text(&n, source))
                            .unwrap_or_default(),
                        _ => String::new(),
                    };

                    if !export_name.is_empty() {
                        let export = StandardizedExport {
                            kind: ExportKind::Named,
                            target: ExportTarget {
                                name: export_name.clone(),
                                original_name: None,
                                kind: match decl.kind() {
                                    "function_declaration" => TargetKind::Function,
                                    "class_declaration" => TargetKind::Class,
                                    _ => TargetKind::Variable,
                                },
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

        // CommonJS exports: module.exports = x, exports.x = y
        // Need to search recursively for assignment_expression
        for assignment_node in find_descendants_by_kind(&root_node, "assignment_expression") {
            if let Some(left) = assignment_node.child_by_field_name("left") {
                // Check for module.exports = x
                if left.kind() == "member_expression" {
                    if let Some(object) = left.child_by_field_name("object") {
                        if let Some(property) = left.child_by_field_name("property") {
                            let obj_text = node_text(&object, source);
                            let prop_text = node_text(&property, source);
                            let span = node_to_span(&assignment_node);

                            if obj_text == "module" && prop_text == "exports" {
                                // module.exports = x
                                // We can't easily extract the exported name, so use a placeholder
                                let export = StandardizedExport {
                                    kind: ExportKind::CommonJSExport,
                                    target: ExportTarget {
                                        name: "module.exports".to_string(),
                                        original_name: None,
                                        kind: TargetKind::Other,
                                        source_module: None,
                                    },
                                    is_reexport: false,
                                    span: Some(span),
                                };
                                exports.push(export);
                            } else if obj_text == "exports" {
                                // exports.x = y
                                let export_name = prop_text;
                                let export = StandardizedExport {
                                    kind: ExportKind::CommonJSExport,
                                    target: ExportTarget {
                                        name: export_name.clone(),
                                        original_name: None,
                                        kind: TargetKind::Other,
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
            }
        }

        exports
    }

    fn language(&self) -> Language {
        Language::JavaScript
    }
}

impl Default for JavaScriptExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_relative_import() {
        assert!(is_relative_js("./module"));
        assert!(is_relative_js("../parent"));
        assert!(is_relative_js("."));
        assert!(is_relative_js(".."));
        assert!(!is_relative_js("lodash"));
        assert!(!is_relative_js("react"));
    }

    #[test]
    fn test_is_node_stdlib() {
        assert!(JavaScriptExtractor::is_node_stdlib("fs"));
        assert!(JavaScriptExtractor::is_node_stdlib("path"));
        assert!(JavaScriptExtractor::is_node_stdlib("http"));
        assert!(JavaScriptExtractor::is_node_stdlib("node:fs"));
        assert!(JavaScriptExtractor::is_node_stdlib("node:path"));
        assert!(!JavaScriptExtractor::is_node_stdlib("lodash"));
        assert!(!JavaScriptExtractor::is_node_stdlib("react"));
        assert!(!JavaScriptExtractor::is_node_stdlib("express"));
    }

    #[test]
    fn test_create_es_import_default() {
        let import = JavaScriptExtractor::create_es_import(
            ImportKind::DefaultImport,
            "react",
            Some("React"),
            None,
            None,
        );
        assert_eq!(import.source, "react");
        assert_eq!(import.target.local_name, "React");
        assert!(import.is_default);
        assert!(!import.is_wildcard);
        assert!(!import.is_relative);
    }

    #[test]
    fn test_create_es_import_named() {
        let import = JavaScriptExtractor::create_es_import(
            ImportKind::SymbolImport,
            "lodash",
            Some("map"),
            None,
            None,
        );
        assert_eq!(import.source, "lodash");
        assert_eq!(import.target.local_name, "map");
        assert!(!import.is_default);
        assert!(!import.is_wildcard);
    }

    #[test]
    fn test_create_es_import_namespace() {
        let import = JavaScriptExtractor::create_es_import(
            ImportKind::NamespaceImport,
            "lodash",
            None,
            Some("_"),
            None,
        );
        assert_eq!(import.source, "lodash");
        assert!(import.is_wildcard);
        assert!(!import.is_default);
    }

    #[test]
    fn test_create_es_import_relative() {
        let import = JavaScriptExtractor::create_es_import(
            ImportKind::SymbolImport,
            "./utils",
            Some("helper"),
            None,
            None,
        );
        assert!(import.is_relative);
    }

    #[test]
    fn test_create_es_import_with_alias() {
        let import = JavaScriptExtractor::create_es_import(
            ImportKind::SymbolImport,
            "lodash",
            Some("map"),
            Some("myMap"),
            None,
        );
        assert_eq!(import.target.local_name, "myMap");
        assert_eq!(import.target.original_name, Some("map".to_string()));
        assert_eq!(import.alias, Some("myMap".to_string()));
    }
}
