//! C++ language import/export extractor
//!
//! Extracts #include directives, using declarations, using namespace, and module imports.

use super::traits::SymbolExtractor;
use crate::parser::stdlib::cpp::CppStdlibDetector;
use crate::tree_sitter_query::{find_children_by_kind, node_text, node_to_span};
use cce_types::import::{
    ExportKind, ExportTarget, ImportKind, ImportTarget, StandardizedExport, StandardizedImport,
    TargetKind,
};
use cce_types::language::Language;
use tree_sitter::Tree;

/// C++ language import/export extractor
pub struct CppExtractor;

impl CppExtractor {
    pub fn new() -> Self {
        Self
    }

    /// Check if an include is a system header (angle brackets)
    fn is_system_header(include_path: &str) -> bool {
        include_path.starts_with('<') && include_path.ends_with('>')
    }

    /// Check if an include is a local header (double quotes)
    fn is_local_header(include_path: &str) -> bool {
        include_path.starts_with('"') && include_path.ends_with('"')
    }

    /// Extract the path from an include directive
    fn extract_include_path(include_path: &str) -> String {
        let trimmed = include_path.trim();
        let path = if (trimmed.starts_with('<') && trimmed.ends_with('>'))
            || (trimmed.starts_with('"') && trimmed.ends_with('"'))
        {
            &trimmed[1..trimmed.len() - 1]
        } else {
            trimmed
        };
        path.to_string()
    }

    /// Check if a header is a standard library header
    fn is_stdlib_header(header_path: &str) -> bool {
        CppStdlibDetector::is_stdlib_header(header_path)
    }

    /// Create a standardized import from a C++ include
    fn create_include_import(include_path: &str, node: &tree_sitter::Node) -> StandardizedImport {
        let cleaned_path = Self::extract_include_path(include_path);
        let is_system_syntax = Self::is_system_header(include_path);
        let is_local_syntax = Self::is_local_header(include_path);

        // Determine if it's a standard library header
        let is_stdlib = Self::is_stdlib_header(&cleaned_path);

        // Determine if it's a relative/local import
        let is_relative = is_local_syntax && !is_stdlib;

        // System header flag: either system syntax OR stdlib header
        let is_system_header = is_system_syntax || is_stdlib;

        StandardizedImport {
            kind: ImportKind::Include,
            source: cleaned_path,
            target: Default::default(),
            alias: None,
            is_wildcard: false,
            is_default: false,
            is_system_header,
            is_relative,
            span: Some(node_to_span(node)),
        }
    }

    /// Parse a using declaration to extract the symbol and potential alias
    /// Examples: "using std::vector" -> ("std::vector", None)
    ///           "using Vec = std::vector<int>" -> ("std::vector<int>", Some("Vec"))
    fn parse_using_declaration(declaration: &str) -> (String, Option<String>) {
        let trimmed = declaration.trim();

        // Check for type alias: using Alias = OriginalType;
        if trimmed.contains(" = ") {
            let parts: Vec<&str> = trimmed.split(" = ").collect();
            if parts.len() == 2 {
                let alias = parts[0].trim().to_string();
                let original = parts[1].trim().to_string();
                return (original, Some(alias));
            }
        }

        // Regular using declaration: using std::vector;
        (trimmed.to_string(), None)
    }

    /// Check if a using declaration is a namespace import
    /// Example: "using namespace std" -> true
    fn is_using_namespace(declaration: &str) -> bool {
        declaration.trim().starts_with("namespace ")
    }

    /// Extract namespace name from "using namespace X"
    fn extract_namespace_name(declaration: &str) -> String {
        declaration
            .trim()
            .strip_prefix("namespace ")
            .unwrap_or(declaration)
            .trim()
            .to_string()
    }

    /// Recursively extract using declarations from namespace nodes
    fn extract_using_from_namespace(
        namespace_node: &tree_sitter::Node,
        source: &str,
    ) -> Vec<StandardizedImport> {
        let mut imports = Vec::new();

        // Find using declarations within this namespace
        for using_node in find_children_by_kind(namespace_node, "using_declaration") {
            if let Some(scope) = using_node.child_by_field_name("scope") {
                let scope_text = node_text(&scope, source);
                if !scope_text.is_empty() {
                    if Self::is_using_namespace(&scope_text) {
                        let namespace = Self::extract_namespace_name(&scope_text);
                        let is_stdlib = namespace.starts_with("std");

                        let import = StandardizedImport {
                            kind: ImportKind::NamespaceImport,
                            source: namespace,
                            target: Default::default(),
                            alias: None,
                            is_wildcard: true,
                            is_default: false,
                            is_system_header: is_stdlib,
                            is_relative: false,
                            span: Some(node_to_span(&using_node)),
                        };
                        imports.push(import);
                    } else {
                        let (original, alias) = Self::parse_using_declaration(&scope_text);
                        let is_stdlib = original.starts_with("std::");

                        let target = ImportTarget {
                            local_name: alias.as_ref().unwrap_or(&original).clone(),
                            original_name: if alias.is_some() {
                                Some(original.clone())
                            } else {
                                None
                            },
                            kind: TargetKind::Other,
                        };

                        let import = StandardizedImport {
                            kind: ImportKind::SymbolImport,
                            source: original,
                            target,
                            alias,
                            is_wildcard: false,
                            is_default: false,
                            is_system_header: is_stdlib,
                            is_relative: false,
                            span: Some(node_to_span(&using_node)),
                        };
                        imports.push(import);
                    }
                }
            }
        }

        // Recursively process nested namespaces
        for nested_ns in find_children_by_kind(namespace_node, "namespace_definition") {
            imports.extend(Self::extract_using_from_namespace(&nested_ns, source));
        }

        imports
    }

    /// Check if a declaration has static storage class
    fn is_static_declaration(node: &tree_sitter::Node, source: &str) -> bool {
        for child in node.children(&mut node.walk()) {
            if child.kind() == "storage_class_specifier" {
                let storage = node_text(&child, source);
                if storage == "static" {
                    return true;
                }
            }
        }
        false
    }

    /// Extract macro name from a preproc_define node
    fn extract_macro_name(define_node: &tree_sitter::Node, source: &str) -> Option<String> {
        define_node
            .child_by_field_name("name")
            .map(|name| node_text(&name, source))
            .filter(|name| !name.is_empty())
    }

    /// Extract function name from a declarator node
    fn extract_function_name(declarator: &tree_sitter::Node, source: &str) -> String {
        // Check if this is a function declarator
        if declarator.kind() == "function_declarator" {
            if let Some(inner_declarator) = declarator.child_by_field_name("declarator") {
                return Self::extract_function_name(&inner_declarator, source);
            }
        }

        // Check for identifier
        if declarator.kind() == "identifier" {
            return node_text(declarator, source);
        }

        // Check for qualified identifier (e.g., MyClass::method)
        if declarator.kind() == "qualified_identifier" {
            for child in declarator.children(&mut declarator.walk()) {
                if child.kind() == "identifier" {
                    return node_text(&child, source);
                }
            }
        }

        // Try to find identifier child
        for child in declarator.children(&mut declarator.walk()) {
            if child.kind() == "identifier" {
                return node_text(&child, source);
            }
        }

        String::new()
    }
}

impl SymbolExtractor for CppExtractor {
    fn extract_imports(&self, tree: &Tree, source: &str) -> Vec<StandardizedImport> {
        let mut imports = Vec::new();
        let root_node = tree.root_node();

        // Find all #include directives
        for include_node in find_children_by_kind(&root_node, "preproc_include") {
            if let Some(argument) = include_node.child(1) {
                let path = node_text(&argument, source);
                if !path.is_empty() {
                    imports.push(Self::create_include_import(&path, &include_node));
                }
            }
        }

        // Find all #include_next directives (GCC extension)
        for include_next_node in find_children_by_kind(&root_node, "preproc_include_next") {
            if let Some(argument) = include_next_node.child(1) {
                let path = node_text(&argument, source);
                if !path.is_empty() {
                    let mut import = Self::create_include_import(&path, &include_next_node);
                    import.alias = Some("include_next".to_string());
                    imports.push(import);
                }
            }
        }

        // Find all #import directives (Objective-C++ style)
        for import_node in find_children_by_kind(&root_node, "preproc_import") {
            if let Some(argument) = import_node.child(1) {
                let path = node_text(&argument, source);
                if !path.is_empty() {
                    let mut import = Self::create_include_import(&path, &import_node);
                    import.kind = ImportKind::ModuleImport;
                    imports.push(import);
                }
            }
        }

        // Find using namespace declarations (e.g., using namespace std;)
        for using_ns_node in find_children_by_kind(&root_node, "using_namespace") {
            // Get the namespace name
            for child in using_ns_node.children(&mut using_ns_node.walk()) {
                if child.kind() == "namespace_identifier" || child.kind() == "identifier" {
                    let namespace = node_text(&child, source);
                    if !namespace.is_empty() {
                        let is_stdlib = namespace.starts_with("std");

                        let import = StandardizedImport {
                            kind: ImportKind::NamespaceImport,
                            source: namespace,
                            target: Default::default(),
                            alias: None,
                            is_wildcard: true,
                            is_default: false,
                            is_system_header: is_stdlib,
                            is_relative: false,
                            span: Some(node_to_span(&using_ns_node)),
                        };
                        imports.push(import);
                        break;
                    }
                }
            }
        }

        // Find using declarations (e.g., using std::vector; or using namespace std;)
        for using_node in find_children_by_kind(&root_node, "using_declaration") {
            // Check if this is a "using namespace" declaration
            // The AST shows:
            // - "using namespace std;" -> (using_declaration using namespace identifier ;)
            // - "using std::vector;" -> (using_declaration using qualified_identifier ;)

            // Check if there's a "namespace" child node
            let has_namespace_keyword = using_node
                .children(&mut using_node.walk())
                .any(|child| child.kind() == "namespace");

            if has_namespace_keyword {
                // This is "using namespace X"
                // Find the identifier child
                for child in using_node.children(&mut using_node.walk()) {
                    if child.kind() == "identifier" {
                        let namespace = node_text(&child, source);
                        if !namespace.is_empty() {
                            let is_stdlib = namespace.starts_with("std");

                            let import = StandardizedImport {
                                kind: ImportKind::NamespaceImport,
                                source: namespace,
                                target: Default::default(),
                                alias: None,
                                is_wildcard: true,
                                is_default: false,
                                is_system_header: is_stdlib,
                                is_relative: false,
                                span: Some(node_to_span(&using_node)),
                            };
                            imports.push(import);
                            break;
                        }
                    }
                }
            } else {
                // This is a regular using declaration like "using std::vector"
                // Extract the qualified identifier
                for child in using_node.children(&mut using_node.walk()) {
                    if child.kind() == "qualified_identifier" {
                        let qualified_name = node_text(&child, source);
                        if !qualified_name.is_empty() {
                            let is_stdlib = qualified_name.starts_with("std::");

                            let target = ImportTarget {
                                local_name: qualified_name.clone(),
                                original_name: None,
                                kind: TargetKind::Other,
                            };

                            let import = StandardizedImport {
                                kind: ImportKind::SymbolImport,
                                source: qualified_name,
                                target,
                                alias: None,
                                is_wildcard: false,
                                is_default: false,
                                is_system_header: is_stdlib,
                                is_relative: false,
                                span: Some(node_to_span(&using_node)),
                            };
                            imports.push(import);
                            break;
                        }
                    }
                }
            }
        }

        // Find type alias declarations (e.g., using Vec = std::vector<int>;)
        // Note: tree-sitter uses "alias_declaration" not "type_alias_declaration"
        for alias_node in find_children_by_kind(&root_node, "alias_declaration") {
            if let Some(name) = alias_node.child_by_field_name("name") {
                let alias_name = node_text(&name, source);
                if let Some(value) = alias_node.child_by_field_name("type") {
                    let original_type = node_text(&value, source);
                    if !alias_name.is_empty() && !original_type.is_empty() {
                        let is_stdlib = original_type.starts_with("std::");

                        let target = ImportTarget {
                            local_name: alias_name.clone(),
                            original_name: Some(original_type.clone()),
                            kind: TargetKind::Type,
                        };

                        let import = StandardizedImport {
                            kind: ImportKind::SymbolImport,
                            source: original_type,
                            target,
                            alias: Some(alias_name),
                            is_wildcard: false,
                            is_default: false,
                            is_system_header: is_stdlib,
                            is_relative: false,
                            span: Some(node_to_span(&alias_node)),
                        };
                        imports.push(import);
                    }
                }
            }
        }

        // Find C++20 module imports
        for import_node in find_children_by_kind(&root_node, "import_declaration") {
            if let Some(name) = import_node.child_by_field_name("name") {
                let module_name = node_text(&name, source);
                if !module_name.is_empty() {
                    let import = StandardizedImport {
                        kind: ImportKind::ModuleImport,
                        source: module_name,
                        target: Default::default(),
                        alias: None,
                        is_wildcard: false,
                        is_default: false,
                        is_system_header: false,
                        is_relative: false,
                        span: Some(node_to_span(&import_node)),
                    };
                    imports.push(import);
                }
            }
        }

        // Extract using declarations from nested namespaces
        for namespace_node in find_children_by_kind(&root_node, "namespace_definition") {
            imports.extend(Self::extract_using_from_namespace(&namespace_node, source));
        }

        imports
    }

    fn extract_exports(&self, tree: &Tree, source: &str) -> Vec<StandardizedExport> {
        let mut exports = Vec::new();
        let root_node = tree.root_node();

        // Extract class/struct definitions
        for class_node in find_children_by_kind(&root_node, "class_specifier") {
            if let Some(name) = class_node.child_by_field_name("name") {
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
                        span: Some(node_to_span(&class_node)),
                    });
                }
            }
        }

        // Extract struct definitions
        for struct_node in find_children_by_kind(&root_node, "struct_specifier") {
            if let Some(name) = struct_node.child_by_field_name("name") {
                let struct_name = node_text(&name, source);
                if !struct_name.is_empty() {
                    exports.push(StandardizedExport {
                        kind: ExportKind::Named,
                        target: ExportTarget {
                            name: struct_name,
                            original_name: None,
                            kind: TargetKind::Class,
                            source_module: None,
                        },
                        is_reexport: false,
                        span: Some(node_to_span(&struct_node)),
                    });
                }
            }
        }

        // Extract function definitions (function_definition in C++)
        for func_node in find_children_by_kind(&root_node, "function_definition") {
            // Skip static functions (file-local scope)
            if Self::is_static_declaration(&func_node, source) {
                continue;
            }

            if let Some(declarator) = func_node.child_by_field_name("declarator") {
                let func_name = Self::extract_function_name(&declarator, source);
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
                        span: Some(node_to_span(&func_node)),
                    });
                }
            }
        }

        // Extract template declarations
        for template_node in find_children_by_kind(&root_node, "template_declaration") {
            // Extract class/function from template
            for child in template_node.children(&mut template_node.walk()) {
                if child.kind() == "class_specifier" {
                    if let Some(name) = child.child_by_field_name("name") {
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
                                span: Some(node_to_span(&template_node)),
                            });
                        }
                    }
                } else if child.kind() == "function_definition" {
                    if let Some(declarator) = child.child_by_field_name("declarator") {
                        let func_name = Self::extract_function_name(&declarator, source);
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
                                span: Some(node_to_span(&template_node)),
                            });
                        }
                    }
                }
            }
        }

        // Extract namespace definitions
        for ns_node in find_children_by_kind(&root_node, "namespace_definition") {
            if let Some(name) = ns_node.child_by_field_name("name") {
                let ns_name = node_text(&name, source);
                if !ns_name.is_empty() {
                    exports.push(StandardizedExport {
                        kind: ExportKind::Named,
                        target: ExportTarget {
                            name: ns_name,
                            original_name: None,
                            kind: TargetKind::Module,
                            source_module: None,
                        },
                        is_reexport: false,
                        span: Some(node_to_span(&ns_node)),
                    });
                }
            }
        }

        // Extract enum definitions
        for enum_node in find_children_by_kind(&root_node, "enum_specifier") {
            if let Some(name) = enum_node.child_by_field_name("name") {
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
                        span: Some(node_to_span(&enum_node)),
                    });
                }
            }
        }

        // Extract typedef/type alias declarations
        for typedef_node in find_children_by_kind(&root_node, "type_definition") {
            if let Some(name) = typedef_node.child_by_field_name("name") {
                let typedef_name = node_text(&name, source);
                if !typedef_name.is_empty() {
                    exports.push(StandardizedExport {
                        kind: ExportKind::Named,
                        target: ExportTarget {
                            name: typedef_name,
                            original_name: None,
                            kind: TargetKind::Type,
                            source_module: None,
                        },
                        is_reexport: false,
                        span: Some(node_to_span(&typedef_node)),
                    });
                }
            }
        }

        // Extract C++20 module exports
        for export_node in find_children_by_kind(&root_node, "export_declaration") {
            // Check for module export: export module MyModule;
            for child in export_node.children(&mut export_node.walk()) {
                if child.kind() == "module_declaration" {
                    if let Some(name) = child.child_by_field_name("name") {
                        let module_name = node_text(&name, source);
                        if !module_name.is_empty() {
                            exports.push(StandardizedExport {
                                kind: ExportKind::Named,
                                target: ExportTarget {
                                    name: module_name,
                                    original_name: None,
                                    kind: TargetKind::Module,
                                    source_module: None,
                                },
                                is_reexport: false,
                                span: Some(node_to_span(&export_node)),
                            });
                        }
                    }
                }
            }
        }

        // Extract C++20 concept definitions
        for concept_node in find_children_by_kind(&root_node, "concept_definition") {
            if let Some(name) = concept_node.child_by_field_name("name") {
                let concept_name = node_text(&name, source);
                if !concept_name.is_empty() {
                    exports.push(StandardizedExport {
                        kind: ExportKind::Named,
                        target: ExportTarget {
                            name: concept_name,
                            original_name: None,
                            kind: TargetKind::Interface,
                            source_module: None,
                        },
                        is_reexport: false,
                        span: Some(node_to_span(&concept_node)),
                    });
                }
            }
        }

        // Extract macro definitions (#define)
        for define_node in find_children_by_kind(&root_node, "preproc_def") {
            if let Some(macro_name) = Self::extract_macro_name(&define_node, source) {
                exports.push(StandardizedExport {
                    kind: ExportKind::Named,
                    target: ExportTarget {
                        name: macro_name,
                        original_name: None,
                        kind: TargetKind::Other,
                        source_module: None,
                    },
                    is_reexport: false,
                    span: Some(node_to_span(&define_node)),
                });
            }
        }

        // Extract function-like macro definitions (#define MACRO(...))
        for func_define_node in find_children_by_kind(&root_node, "preproc_function_def") {
            if let Some(macro_name) = Self::extract_macro_name(&func_define_node, source) {
                exports.push(StandardizedExport {
                    kind: ExportKind::Named,
                    target: ExportTarget {
                        name: macro_name,
                        original_name: None,
                        kind: TargetKind::Function,
                        source_module: None,
                    },
                    is_reexport: false,
                    span: Some(node_to_span(&func_define_node)),
                });
            }
        }

        exports
    }

    fn language(&self) -> Language {
        Language::Cpp
    }
}

impl Default for CppExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_header_detection() {
        assert!(CppExtractor::is_system_header("<vector>"));
        assert!(!CppExtractor::is_system_header("\"local.h\""));
    }

    #[test]
    fn test_local_header_detection() {
        assert!(CppExtractor::is_local_header("\"myheader.h\""));
        assert!(!CppExtractor::is_local_header("<iostream>"));
    }

    #[test]
    fn test_stdlib_header_detection() {
        assert!(CppExtractor::is_stdlib_header("vector"));
        assert!(CppExtractor::is_stdlib_header("iostream"));
        assert!(CppExtractor::is_stdlib_header("string"));
        assert!(!CppExtractor::is_stdlib_header("myheader.h"));
    }

    #[test]
    fn test_using_namespace_detection() {
        assert!(CppExtractor::is_using_namespace("namespace std"));
        assert!(CppExtractor::is_using_namespace("namespace std::chrono"));
        assert!(!CppExtractor::is_using_namespace("std::vector"));
    }

    #[test]
    fn test_extract_namespace_name() {
        assert_eq!(CppExtractor::extract_namespace_name("namespace std"), "std");
        assert_eq!(
            CppExtractor::extract_namespace_name("namespace std::chrono"),
            "std::chrono"
        );
    }

    #[test]
    fn test_parse_using_declaration_regular() {
        let (original, alias) = CppExtractor::parse_using_declaration("std::vector");
        assert_eq!(original, "std::vector");
        assert!(alias.is_none());
    }

    #[test]
    fn test_parse_using_declaration_alias() {
        let (original, alias) = CppExtractor::parse_using_declaration("Vec = std::vector<int>");
        assert_eq!(original, "std::vector<int>");
        assert_eq!(alias, Some("Vec".to_string()));
    }

    #[test]
    fn test_cpp_include_extraction() {
        let code = r#"
#include <vector>
#include <iostream>
#include "local_header.h"
"#;
        let mut parser = crate::parser::AstParser::new();
        let result = parser.parse_with_tree(code, &Language::Cpp);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = CppExtractor::new();
        let imports = extractor.extract_imports(&tree, code);

        assert!(!imports.is_empty());
        // Check for system header
        assert!(
            imports
                .iter()
                .any(|i| i.source == "vector" && i.is_system_header)
        );
        // Check for local header
        assert!(
            imports
                .iter()
                .any(|i| i.source == "local_header.h" && i.is_relative)
        );
    }

    #[test]
    fn test_cpp_using_declaration() {
        let code = r#"
using namespace std;
using std::vector;
using Vec = std::vector<int>;
"#;
        let mut parser = crate::parser::AstParser::new();
        let result = parser.parse_with_tree(code, &Language::Cpp);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = CppExtractor::new();
        let imports = extractor.extract_imports(&tree, code);

        assert!(!imports.is_empty());
        // Check for namespace import
        assert!(imports.iter().any(|i| i.is_wildcard && i.source == "std"));
        // Check for using declaration
        assert!(imports.iter().any(|i| i.source.contains("std::vector")));
    }
}
