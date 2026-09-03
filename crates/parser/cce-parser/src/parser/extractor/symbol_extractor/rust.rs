//! Rust language import/export extractor
//!
//! Extracts use statements and pub declarations.

use super::common::helpers::path::is_relative_rust;
use super::traits::SymbolExtractor;
use crate::parser::stdlib::RustStdlibDetector;
use crate::tree_sitter_query::{
    find_children_by_kind, find_descendants_by_kind, node_text, node_to_span,
};
use cce_types::import::{
    ExportKind, ExportTarget, ImportKind, ImportTarget, StandardizedExport, StandardizedImport,
    TargetKind,
};
use cce_types::language::Language;
use tree_sitter::Tree;

/// Rust language import/export extractor
pub struct RustExtractor;

impl RustExtractor {
    pub fn new() -> Self {
        Self
    }

    /// Process a use tree node recursively
    ///
    /// Handles various use patterns:
    /// - Simple: `use std::collections::HashMap`
    /// - Multiple: `use std::collections::{HashMap, HashSet}`
    /// - Self: `use crate::module::{self, Item}`
    /// - Wildcard: `use std::collections::*`
    /// - Renamed: `use std::io as stdio`
    fn process_use_tree(
        &self,
        node: &tree_sitter::Node,
        source: &str,
        base_path: &str,
        span: cce_types::position::Span,
        imports: &mut Vec<StandardizedImport>,
    ) {
        let node_kind = node.kind();

        match node_kind {
            "scoped_identifier" | "identifier" => {
                // Simple path: std::collections::HashMap
                let path = node_text(node, source);
                let full_path = if base_path.is_empty() {
                    path.to_string()
                } else {
                    format!("{}::{}", base_path, path)
                };

                if !full_path.is_empty() {
                    let target_name = full_path.split("::").last().unwrap_or(&full_path);
                    imports.push(StandardizedImport {
                        kind: ImportKind::SymbolImport,
                        source: full_path.clone(),
                        target: ImportTarget {
                            local_name: target_name.to_string(),
                            original_name: None,
                            kind: TargetKind::Other,
                        },
                        alias: None,
                        is_wildcard: false,
                        is_default: false,
                        is_system_header: self.is_stdlib_path(&full_path),
                        is_relative: is_relative_rust(&full_path),
                        span: Some(span),
                    });
                }
            }
            "use_list" => {
                // Multiple items: {HashMap, HashSet, self}
                for child in node.children(&mut node.walk()) {
                    if child.kind() != "," && child.kind() != "{" && child.kind() != "}" {
                        self.process_use_tree(&child, source, base_path, span, imports);
                    }
                }
            }
            "scoped_use_list" => {
                // Scoped list: std::collections::{HashMap, HashSet}
                // Extract the scope (e.g., "std::collections")
                let mut scope = String::new();
                let mut list_node = None;

                for child in node.children(&mut node.walk()) {
                    match child.kind() {
                        "scoped_identifier" | "identifier" => {
                            if !scope.is_empty() {
                                scope.push_str("::");
                            }
                            scope.push_str(&node_text(&child, source));
                        }
                        "use_list" => {
                            list_node = Some(child);
                        }
                        _ => {}
                    }
                }

                let full_base = if base_path.is_empty() {
                    scope
                } else {
                    format!("{}::{}", base_path, scope)
                };

                if let Some(list) = list_node {
                    self.process_use_tree(&list, source, &full_base, span, imports);
                }
            }
            "use_wildcard" => {
                // Wildcard import: use std::collections::*
                // The use_wildcard node contains scoped_identifier as child
                let mut source_path = base_path.to_string();

                // Extract the path from scoped_identifier child
                for child in node.children(&mut node.walk()) {
                    if child.kind() == "scoped_identifier" || child.kind() == "identifier" {
                        let path = node_text(&child, source);
                        if !source_path.is_empty() {
                            source_path = format!("{}::{}", source_path, path);
                        } else {
                            source_path = path.to_string();
                        }
                    }
                }

                if !source_path.is_empty() {
                    imports.push(StandardizedImport {
                        kind: ImportKind::NamespaceImport,
                        source: source_path.clone(),
                        target: Default::default(),
                        alias: None,
                        is_wildcard: true,
                        is_default: false,
                        is_system_header: self.is_stdlib_path(&source_path),
                        is_relative: is_relative_rust(&source_path),
                        span: Some(span),
                    });
                }
            }
            "use_self" => {
                // Self import: use crate::module::{self}
                if !base_path.is_empty() {
                    imports.push(StandardizedImport {
                        kind: ImportKind::SymbolImport,
                        source: base_path.to_string(),
                        target: ImportTarget {
                            local_name: base_path
                                .split("::")
                                .last()
                                .unwrap_or(base_path)
                                .to_string(),
                            original_name: None,
                            kind: TargetKind::Module,
                        },
                        alias: None,
                        is_wildcard: false,
                        is_default: false,
                        is_system_header: self.is_stdlib_path(base_path),
                        is_relative: is_relative_rust(base_path),
                        span: Some(span),
                    });
                }
            }
            "use_as" => {
                // Renamed import: use std::io as stdio
                let mut original_path = String::new();
                let mut alias = String::new();

                // Find the "as" keyword and extract original path and alias
                let mut found_as = false;
                for child in node.children(&mut node.walk()) {
                    let child_text = node_text(&child, source);
                    if child_text == "as" {
                        found_as = true;
                    } else if child.kind() == "scoped_identifier" || child.kind() == "identifier" {
                        if !found_as && original_path.is_empty() {
                            original_path = child_text;
                        } else if found_as && alias.is_empty() {
                            alias = child_text;
                        }
                    }
                }

                let full_path = if base_path.is_empty() {
                    original_path.clone()
                } else {
                    format!("{}::{}", base_path, original_path)
                };

                if !full_path.is_empty() && !alias.is_empty() {
                    imports.push(StandardizedImport {
                        kind: ImportKind::SymbolImport,
                        source: full_path.clone(),
                        target: ImportTarget {
                            local_name: alias.clone(),
                            original_name: Some(full_path.clone()),
                            kind: TargetKind::Other,
                        },
                        alias: Some(alias),
                        is_wildcard: false,
                        is_default: false,
                        is_system_header: self.is_stdlib_path(&full_path),
                        is_relative: is_relative_rust(&full_path),
                        span: Some(span),
                    });
                }
            }
            _ => {
                // For other node types, try to extract text and process
                let text = node_text(node, source);
                if !text.is_empty() && text.contains("::") {
                    // This might be a simple scoped identifier as text
                    let full_path = if base_path.is_empty() {
                        text.to_string()
                    } else {
                        format!("{}::{}", base_path, text)
                    };

                    // Check for wildcard
                    if full_path.ends_with("::*") {
                        let source_path = full_path.trim_end_matches("::*").to_string();
                        imports.push(StandardizedImport {
                            kind: ImportKind::NamespaceImport,
                            source: source_path.clone(),
                            target: Default::default(),
                            alias: None,
                            is_wildcard: true,
                            is_default: false,
                            is_system_header: self.is_stdlib_path(&source_path),
                            is_relative: is_relative_rust(&source_path),
                            span: Some(span),
                        });
                    } else if full_path.contains(" as ") {
                        // Handle "path as alias" pattern
                        let parts: Vec<&str> = full_path.split(" as ").collect();
                        if parts.len() == 2 {
                            let original = parts[0].trim();
                            let alias_name = parts[1].trim();
                            imports.push(StandardizedImport {
                                kind: ImportKind::SymbolImport,
                                source: original.to_string(),
                                target: ImportTarget {
                                    local_name: alias_name.to_string(),
                                    original_name: Some(original.to_string()),
                                    kind: TargetKind::Other,
                                },
                                alias: Some(alias_name.to_string()),
                                is_wildcard: false,
                                is_default: false,
                                is_system_header: self.is_stdlib_path(original),
                                is_relative: is_relative_rust(original),
                                span: Some(span),
                            });
                        }
                    } else {
                        let target_name = full_path.split("::").last().unwrap_or(&full_path);
                        imports.push(StandardizedImport {
                            kind: ImportKind::SymbolImport,
                            source: full_path.clone(),
                            target: ImportTarget {
                                local_name: target_name.to_string(),
                                original_name: None,
                                kind: TargetKind::Other,
                            },
                            alias: None,
                            is_wildcard: false,
                            is_default: false,
                            is_system_header: self.is_stdlib_path(&full_path),
                            is_relative: is_relative_rust(&full_path),
                            span: Some(span),
                        });
                    }
                }
            }
        }
    }

    /// Check if path is a standard library path
    fn is_stdlib_path(&self, path: &str) -> bool {
        RustStdlibDetector::STDLIB_CRATES
            .iter()
            .any(|&prefix| path == prefix || path.starts_with(&format!("{}::", prefix)))
    }
}

impl SymbolExtractor for RustExtractor {
    fn extract_imports(&self, tree: &Tree, source: &str) -> Vec<StandardizedImport> {
        let mut imports = Vec::new();
        let root_node = tree.root_node();

        // Find all use statements
        for use_node in find_children_by_kind(&root_node, "use_declaration") {
            if let Some(argument) = use_node.child_by_field_name("argument") {
                let span = node_to_span(&use_node);
                // Process the use tree recursively
                self.process_use_tree(&argument, source, "", span, &mut imports);
            }
        }

        imports
    }

    fn extract_exports(&self, tree: &Tree, source: &str) -> Vec<StandardizedExport> {
        let mut exports = Vec::new();
        let root_node = tree.root_node();

        // Find all public declarations
        for node in find_descendants_by_kind(&root_node, "function_item") {
            // Check if the node has a visibility_modifier child
            let is_pub = node
                .children(&mut root_node.walk())
                .any(|child| child.kind() == "visibility_modifier");

            if is_pub {
                if let Some(name) = node.child_by_field_name("name") {
                    let fn_name = node_text(&name, source);
                    if !fn_name.is_empty() {
                        let span = node_to_span(&node);
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
        }

        // Find all public struct declarations
        for node in find_descendants_by_kind(&root_node, "struct_item") {
            let is_pub = node
                .children(&mut root_node.walk())
                .any(|child| child.kind() == "visibility_modifier");

            if is_pub {
                if let Some(name) = node.child_by_field_name("name") {
                    let struct_name = node_text(&name, source);
                    if !struct_name.is_empty() {
                        let span = node_to_span(&node);
                        let export = StandardizedExport {
                            kind: ExportKind::Named,
                            target: ExportTarget {
                                name: struct_name,
                                original_name: None,
                                kind: TargetKind::Class,
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

        // Find all public enum declarations
        for node in find_descendants_by_kind(&root_node, "enum_item") {
            let is_pub = node
                .children(&mut root_node.walk())
                .any(|child| child.kind() == "visibility_modifier");

            if is_pub {
                if let Some(name) = node.child_by_field_name("name") {
                    let enum_name = node_text(&name, source);
                    if !enum_name.is_empty() {
                        let span = node_to_span(&node);
                        let export = StandardizedExport {
                            kind: ExportKind::Named,
                            target: ExportTarget {
                                name: enum_name,
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

        // Find all public trait declarations
        for node in find_descendants_by_kind(&root_node, "trait_item") {
            let is_pub = node
                .children(&mut root_node.walk())
                .any(|child| child.kind() == "visibility_modifier");

            if is_pub {
                if let Some(name) = node.child_by_field_name("name") {
                    let trait_name = node_text(&name, source);
                    if !trait_name.is_empty() {
                        let span = node_to_span(&node);
                        let export = StandardizedExport {
                            kind: ExportKind::Named,
                            target: ExportTarget {
                                name: trait_name,
                                original_name: None,
                                kind: TargetKind::Interface,
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

        // Find all public type alias declarations
        for node in find_descendants_by_kind(&root_node, "type_item") {
            let is_pub = node
                .children(&mut root_node.walk())
                .any(|child| child.kind() == "visibility_modifier");

            if is_pub {
                if let Some(name) = node.child_by_field_name("name") {
                    let type_name = node_text(&name, source);
                    if !type_name.is_empty() {
                        let span = node_to_span(&node);
                        let export = StandardizedExport {
                            kind: ExportKind::Named,
                            target: ExportTarget {
                                name: type_name,
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

        // Find all pub use re-exports
        for node in find_descendants_by_kind(&root_node, "use_declaration") {
            let is_pub = node
                .children(&mut root_node.walk())
                .any(|child| child.kind() == "visibility_modifier");

            if is_pub {
                if let Some(argument) = node.child_by_field_name("argument") {
                    let path = node_text(&argument, source);
                    if !path.is_empty() {
                        let span = node_to_span(&node);

                        // Handle different use patterns
                        if path.contains(" as ") {
                            // pub use module::Item as Alias;
                            let parts: Vec<&str> = path.split(" as ").collect();
                            let original_path = parts.first().map(|s| s.trim()).unwrap_or("");
                            let alias = parts.get(1).map(|s| s.trim()).unwrap_or("");

                            let export = StandardizedExport {
                                kind: ExportKind::Named,
                                target: ExportTarget {
                                    name: alias.to_string(),
                                    original_name: Some(original_path.to_string()),
                                    kind: TargetKind::Other,
                                    source_module: Some(original_path.to_string()),
                                },
                                is_reexport: true,
                                span: Some(span),
                            };
                            exports.push(export);
                        } else if path.contains("::{") {
                            // pub use module::{Item1, Item2};
                            // Extract items from the braces
                            if let Some(start) = path.find("::{") {
                                let module_path = &path[..start];
                                if let Some(end) = path.find('}') {
                                    let items_str = &path[start + 3..end];
                                    for item in items_str.split(',') {
                                        let item = item.trim();
                                        if !item.is_empty() && item != "self" {
                                            let export = StandardizedExport {
                                                kind: ExportKind::Named,
                                                target: ExportTarget {
                                                    name: item.to_string(),
                                                    original_name: None,
                                                    kind: TargetKind::Other,
                                                    source_module: Some(format!(
                                                        "{}::{}",
                                                        module_path, item
                                                    )),
                                                },
                                                is_reexport: true,
                                                span: Some(span),
                                            };
                                            exports.push(export);
                                        }
                                    }
                                }
                            }
                        } else if path.contains("::*") {
                            // pub use module::*; - wildcard re-export
                            let source_path = path.trim_end_matches("::*");
                            let export = StandardizedExport {
                                kind: ExportKind::Wildcard,
                                target: ExportTarget {
                                    name: "*".to_string(),
                                    original_name: None,
                                    kind: TargetKind::Other,
                                    source_module: Some(source_path.to_string()),
                                },
                                is_reexport: true,
                                span: Some(span),
                            };
                            exports.push(export);
                        } else {
                            // pub use module::Item;
                            let item_name = path.split("::").last().unwrap_or(&path);
                            let export = StandardizedExport {
                                kind: ExportKind::Named,
                                target: ExportTarget {
                                    name: item_name.to_string(),
                                    original_name: None,
                                    kind: TargetKind::Other,
                                    source_module: Some(path.to_string()),
                                },
                                is_reexport: true,
                                span: Some(span),
                            };
                            exports.push(export);
                        }
                    }
                }
            }
        }

        exports
    }

    fn language(&self) -> Language {
        Language::Rust
    }
}

impl Default for RustExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_stdlib_path() {
        let extractor = RustExtractor::new();
        assert!(extractor.is_stdlib_path("std"));
        assert!(extractor.is_stdlib_path("std::collections"));
        assert!(extractor.is_stdlib_path("core::option"));
        assert!(extractor.is_stdlib_path("alloc::vec"));
        assert!(!extractor.is_stdlib_path("serde"));
        assert!(!extractor.is_stdlib_path("my_crate"));
    }

    #[test]
    fn test_rust_pub_use_reexport() {
        use crate::parser::AstParser;

        let code = r#"
pub use std::collections::HashMap;
pub use crate::module::MyStruct;
pub use super::parent::ItemType as AliasType;
pub use internal::{ItemA, ItemB};
"#;
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::Rust);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = RustExtractor::new();
        let exports = extractor.extract_exports(&tree, code);

        // Should have re-exports
        assert!(
            exports
                .iter()
                .any(|e| e.is_reexport && e.target.name == "HashMap")
        );
        assert!(
            exports
                .iter()
                .any(|e| e.is_reexport && e.target.name == "MyStruct")
        );
        assert!(
            exports
                .iter()
                .any(|e| e.is_reexport && e.target.name == "AliasType")
        );
        assert!(
            exports
                .iter()
                .any(|e| e.is_reexport && e.target.name == "ItemA")
        );
        assert!(
            exports
                .iter()
                .any(|e| e.is_reexport && e.target.name == "ItemB")
        );
    }

    #[test]
    fn test_rust_pub_trait_and_type() {
        use crate::parser::AstParser;

        let code = r#"
pub trait MyTrait {
    fn method(&self);
}

pub type MyAlias = String;

trait PrivateTrait {}

type PrivateAlias = i32;
"#;
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::Rust);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = RustExtractor::new();
        let exports = extractor.extract_exports(&tree, code);

        // Should export public trait and type
        assert!(
            exports
                .iter()
                .any(|e| e.target.name == "MyTrait" && e.target.kind == TargetKind::Interface)
        );
        assert!(exports.iter().any(|e| e.target.name == "MyAlias"));

        // Should NOT export private trait and type
        assert!(!exports.iter().any(|e| e.target.name == "PrivateTrait"));
        assert!(!exports.iter().any(|e| e.target.name == "PrivateAlias"));
    }
}
