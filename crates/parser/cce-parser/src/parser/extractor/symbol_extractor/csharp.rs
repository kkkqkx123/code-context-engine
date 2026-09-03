//! C# language import/export extractor
//!
//! Extracts using directives, namespace declarations, and public type exports.

use super::traits::SymbolExtractor;
use crate::parser::stdlib::csharp::CSharpStdlibDetector;
use crate::tree_sitter_query::{find_children_by_kind, node_text, node_to_span};
use cce_types::import::{
    ExportKind, ExportTarget, ImportKind, ImportTarget, StandardizedExport, StandardizedImport,
    TargetKind,
};
use cce_types::language::Language;
use tree_sitter::Tree;

/// C# language import/export extractor
pub struct CSharpExtractor;

impl CSharpExtractor {
    pub fn new() -> Self {
        Self
    }

    /// Check if a using directive is a static import (C# 6.0+)
    /// Example: "using static System.Math" -> true
    fn is_static_using(directive: &str) -> bool {
        directive.trim().starts_with("static ")
    }

    /// Check if a using directive has an alias
    /// Example: "using Project = MyCompany.Project;" -> true
    fn has_alias(directive: &str) -> bool {
        directive.trim().contains(" = ")
    }

    /// Parse a using directive to extract namespace, alias, and static flag
    /// Returns (namespace_or_type, alias, is_static)
    fn parse_using_directive(directive: &str) -> (String, Option<String>, bool) {
        let trimmed = directive.trim();

        // Strip "using " prefix if present
        let trimmed = if trimmed.starts_with("using ") {
            trimmed.strip_prefix("using ").unwrap_or(trimmed).trim()
        } else {
            trimmed
        };

        // Check for static import: using static System.Math;
        if Self::is_static_using(trimmed) {
            let type_name = trimmed
                .strip_prefix("static ")
                .unwrap_or(trimmed)
                .trim()
                .trim_end_matches(';')
                .to_string();
            return (type_name, None, true);
        }

        // Check for alias: using Project = MyCompany.Project;
        if Self::has_alias(trimmed) {
            let parts: Vec<&str> = trimmed.split(" = ").collect();
            if parts.len() == 2 {
                let alias = parts[0].trim().to_string();
                let namespace = parts[1].trim().trim_end_matches(';').to_string();
                return (namespace, Some(alias), false);
            }
        }

        // Regular using: using System.Collections.Generic;
        let namespace = trimmed.trim_end_matches(';').to_string();
        (namespace, None, false)
    }

    /// Check if a namespace is a .NET standard library namespace
    fn is_stdlib_namespace(namespace: &str) -> bool {
        CSharpStdlibDetector::is_dotnet_namespace(namespace)
            || CSharpStdlibDetector::is_dotnet_path(namespace)
    }

    /// Check if a type has public visibility
    fn is_public_type(node: &tree_sitter::Node, source: &str) -> bool {
        // Check for modifiers
        for child in node.children(&mut node.walk()) {
            if child.kind() == "modifier" {
                let modifier = node_text(&child, source);
                if modifier == "public" {
                    return true;
                }
            }
        }
        false
    }

    /// Get the name of a type declaration
    fn get_type_name(node: &tree_sitter::Node, source: &str) -> Option<String> {
        node.child_by_field_name("name")
            .map(|name| node_text(&name, source))
            .filter(|name| !name.is_empty())
    }

    /// Check if a method is an extension method (has 'this' parameter modifier)
    fn is_extension_method(node: &tree_sitter::Node, source: &str) -> bool {
        // Look for parameter list
        for child in node.children(&mut node.walk()) {
            if child.kind() == "parameter_list" {
                // Check first parameter for 'this' modifier
                for param in child.children(&mut child.walk()) {
                    if param.kind() == "parameter" {
                        for modifier in param.children(&mut param.walk()) {
                            if modifier.kind() == "modifier" {
                                let modifier_text = node_text(&modifier, source);
                                if modifier_text == "this" {
                                    return true;
                                }
                            }
                        }
                        // Only check first parameter
                        break;
                    }
                }
            }
        }
        false
    }

    /// Check if a field declaration is a const field
    fn is_const_field(node: &tree_sitter::Node, source: &str) -> bool {
        for child in node.children(&mut node.walk()) {
            if child.kind() == "modifier" {
                let modifier = node_text(&child, source);
                if modifier == "const" {
                    return true;
                }
            }
        }
        false
    }
}

impl SymbolExtractor for CSharpExtractor {
    fn extract_imports(&self, tree: &Tree, source: &str) -> Vec<StandardizedImport> {
        let mut imports = Vec::new();
        let root_node = tree.root_node();

        // Find all using directives
        for using_node in find_children_by_kind(&root_node, "using_directive") {
            // Get the full text of the using directive
            let using_text = node_text(&using_node, source);
            if using_text.is_empty() {
                continue;
            }

            // Check for global using (C# 10)
            let is_global = using_text.trim().starts_with("global ");
            let directive_text = if is_global {
                using_text
                    .trim()
                    .strip_prefix("global ")
                    .unwrap_or(&using_text)
            } else {
                &using_text
            };

            // Parse the directive
            let (namespace_or_type, alias, is_static) = Self::parse_using_directive(directive_text);
            if namespace_or_type.is_empty() {
                continue;
            }

            let is_stdlib = Self::is_stdlib_namespace(&namespace_or_type);

            let kind = if is_static {
                ImportKind::SymbolImport
            } else {
                ImportKind::NamespaceImport
            };

            let target = if let Some(ref alias_name) = alias {
                ImportTarget {
                    local_name: alias_name.clone(),
                    original_name: Some(namespace_or_type.clone()),
                    kind: if is_static {
                        TargetKind::Other
                    } else {
                        TargetKind::Module
                    },
                }
            } else {
                ImportTarget {
                    local_name: namespace_or_type.clone(),
                    original_name: None,
                    kind: if is_static {
                        TargetKind::Other
                    } else {
                        TargetKind::Module
                    },
                }
            };

            let import = StandardizedImport {
                kind,
                source: namespace_or_type,
                target,
                alias,
                is_wildcard: !is_static, // namespace imports are wildcard
                is_default: false,
                is_system_header: is_stdlib,
                is_relative: false,
                span: Some(node_to_span(&using_node)),
            };
            imports.push(import);
        }

        imports
    }

    fn extract_exports(&self, tree: &Tree, source: &str) -> Vec<StandardizedExport> {
        let mut exports = Vec::new();
        let root_node = tree.root_node();

        // Find namespace declarations
        for namespace_node in find_children_by_kind(&root_node, "namespace_declaration") {
            if let Some(name) = namespace_node.child_by_field_name("name") {
                let namespace_name = node_text(&name, source);
                if !namespace_name.is_empty() {
                    let export = StandardizedExport {
                        kind: ExportKind::Named,
                        target: ExportTarget {
                            name: namespace_name,
                            original_name: None,
                            kind: TargetKind::Module,
                            source_module: None,
                        },
                        is_reexport: false,
                        span: Some(node_to_span(&namespace_node)),
                    };
                    exports.push(export);
                }
            }
        }

        // Find public type declarations
        let type_kinds = [
            ("class_declaration", TargetKind::Class),
            ("interface_declaration", TargetKind::Interface),
            ("struct_declaration", TargetKind::Class),
            ("enum_declaration", TargetKind::Other),
            ("delegate_declaration", TargetKind::Function),
            ("record_declaration", TargetKind::Class), // C# 9.0+ record types
            ("record_struct_declaration", TargetKind::Class), // C# 10.0+ record struct
        ];

        for (node_kind, target_kind) in type_kinds {
            for type_node in find_children_by_kind(&root_node, node_kind) {
                if Self::is_public_type(&type_node, source) {
                    if let Some(type_name) = Self::get_type_name(&type_node, source) {
                        let export = StandardizedExport {
                            kind: ExportKind::Named,
                            target: ExportTarget {
                                name: type_name,
                                original_name: None,
                                kind: target_kind,
                                source_module: None,
                            },
                            is_reexport: false,
                            span: Some(node_to_span(&type_node)),
                        };
                        exports.push(export);
                    }
                }
            }
        }

        // Extract public method declarations (including extension methods)
        for method_node in find_children_by_kind(&root_node, "method_declaration") {
            if Self::is_public_type(&method_node, source) {
                if let Some(name) = method_node.child_by_field_name("name") {
                    let method_name = node_text(&name, source);
                    if !method_name.is_empty() {
                        // Check if this is an extension method (has 'this' parameter modifier)
                        let _is_extension = Self::is_extension_method(&method_node, source);

                        exports.push(StandardizedExport {
                            kind: ExportKind::Named,
                            target: ExportTarget {
                                name: method_name,
                                original_name: None,
                                kind: TargetKind::Function,
                                source_module: None,
                            },
                            is_reexport: false,
                            span: Some(node_to_span(&method_node)),
                        });
                    }
                }
            }
        }

        // Extract public property declarations
        for prop_node in find_children_by_kind(&root_node, "property_declaration") {
            if Self::is_public_type(&prop_node, source) {
                if let Some(name) = prop_node.child_by_field_name("name") {
                    let prop_name = node_text(&name, source);
                    if !prop_name.is_empty() {
                        exports.push(StandardizedExport {
                            kind: ExportKind::Named,
                            target: ExportTarget {
                                name: prop_name,
                                original_name: None,
                                kind: TargetKind::Variable,
                                source_module: None,
                            },
                            is_reexport: false,
                            span: Some(node_to_span(&prop_node)),
                        });
                    }
                }
            }
        }

        // Extract public event declarations
        for event_node in find_children_by_kind(&root_node, "event_declaration") {
            if Self::is_public_type(&event_node, source) {
                if let Some(name) = event_node.child_by_field_name("name") {
                    let event_name = node_text(&name, source);
                    if !event_name.is_empty() {
                        exports.push(StandardizedExport {
                            kind: ExportKind::Named,
                            target: ExportTarget {
                                name: event_name,
                                original_name: None,
                                kind: TargetKind::Other,
                                source_module: None,
                            },
                            is_reexport: false,
                            span: Some(node_to_span(&event_node)),
                        });
                    }
                }
            }
        }

        // Extract public field/constant declarations
        for field_node in find_children_by_kind(&root_node, "field_declaration") {
            if Self::is_public_type(&field_node, source) {
                // Check if it's a const field
                let _is_const = Self::is_const_field(&field_node, source);

                // Get variable declarations within the field declaration
                for var_node in find_children_by_kind(&field_node, "variable_declaration") {
                    for declarator in find_children_by_kind(&var_node, "variable_declarator") {
                        if let Some(name) = declarator.child_by_field_name("name") {
                            let field_name = node_text(&name, source);
                            if !field_name.is_empty() {
                                exports.push(StandardizedExport {
                                    kind: ExportKind::Named,
                                    target: ExportTarget {
                                        name: field_name,
                                        original_name: None,
                                        kind: TargetKind::Variable,
                                        source_module: None,
                                    },
                                    is_reexport: false,
                                    span: Some(node_to_span(&field_node)),
                                });
                            }
                        }
                    }
                }
            }
        }

        exports
    }

    fn language(&self) -> Language {
        Language::CSharp
    }
}

impl Default for CSharpExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_static_using_detection() {
        assert!(CSharpExtractor::is_static_using("static System.Math"));
        assert!(!CSharpExtractor::is_static_using("System.Collections"));
    }

    #[test]
    fn test_alias_detection() {
        assert!(CSharpExtractor::has_alias("Project = MyCompany.Project"));
        assert!(!CSharpExtractor::has_alias("System.Collections"));
    }

    #[test]
    fn test_parse_using_regular() {
        let (namespace, alias, is_static) =
            CSharpExtractor::parse_using_directive("System.Collections.Generic");
        assert_eq!(namespace, "System.Collections.Generic");
        assert!(alias.is_none());
        assert!(!is_static);
    }

    #[test]
    fn test_parse_using_static() {
        let (namespace, alias, is_static) =
            CSharpExtractor::parse_using_directive("static System.Math");
        assert_eq!(namespace, "System.Math");
        assert!(alias.is_none());
        assert!(is_static);
    }

    #[test]
    fn test_parse_using_alias() {
        let (namespace, alias, is_static) =
            CSharpExtractor::parse_using_directive("Project = MyCompany.Project;");
        assert_eq!(namespace, "MyCompany.Project");
        assert_eq!(alias, Some("Project".to_string()));
        assert!(!is_static);
    }

    #[test]
    fn test_stdlib_namespace_detection() {
        assert!(CSharpExtractor::is_stdlib_namespace("System"));
        assert!(CSharpExtractor::is_stdlib_namespace(
            "System.Collections.Generic"
        ));
        assert!(CSharpExtractor::is_stdlib_namespace(
            "Microsoft.Extensions.Logging"
        ));
        assert!(!CSharpExtractor::is_stdlib_namespace("MyCompany.MyProject"));
    }
}
