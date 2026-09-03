//! C language import/export extractor
//!
//! Extracts #include directives and any other C-specific import/export patterns.

use super::traits::SymbolExtractor;
use crate::parser::stdlib::c::CStdlibDetector;
use crate::tree_sitter_query::{find_children_by_kind, node_text, node_to_span};
use cce_types::import::{
    ExportKind, ExportTarget, ImportKind, StandardizedExport, StandardizedImport, TargetKind,
};
use cce_types::language::Language;
use cce_types::position::Span;
use tree_sitter::Tree;

/// C language import/export extractor
pub struct CExtractor;

impl CExtractor {
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
        CStdlibDetector::is_stdlib_header(header_path)
    }

    /// Create a standardized import from a C include
    fn create_include_import(include_path: &str, span: Span) -> StandardizedImport {
        let cleaned_path = Self::extract_include_path(include_path);
        let is_system_syntax = Self::is_system_header(include_path);
        let is_local_syntax = Self::is_local_header(include_path);

        // Determine if it's a standard library header
        let is_stdlib = Self::is_stdlib_header(&cleaned_path);

        // Determine if it's a relative/local import
        // Local headers (quoted) are typically relative paths
        // System headers (angle brackets) are typically absolute or system paths
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
            span: Some(span),
        }
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
    /// Handles both simple declarators and function declarators
    fn extract_function_name(declarator: &tree_sitter::Node, source: &str) -> String {
        // Check if this is a function declarator
        if declarator.kind() == "function_declarator" {
            // Get the declarator part (which contains the function name)
            if let Some(inner_declarator) = declarator.child_by_field_name("declarator") {
                return Self::extract_function_name(&inner_declarator, source);
            }
        }

        // Check for identifier node
        if declarator.kind() == "identifier" {
            return node_text(declarator, source);
        }

        // Check for parenthesized declarator
        if declarator.kind() == "parenthesized_declarator" {
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

impl SymbolExtractor for CExtractor {
    fn extract_imports(&self, tree: &Tree, source: &str) -> Vec<StandardizedImport> {
        let mut imports = Vec::new();
        let root_node = tree.root_node();

        // Find all #include directives
        for include_node in find_children_by_kind(&root_node, "preproc_include") {
            // Get the argument (the path) from the include
            if let Some(argument) = include_node.child(1) {
                let path = node_text(&argument, source);
                if !path.is_empty() {
                    let span = node_to_span(&include_node);
                    imports.push(Self::create_include_import(&path, span));
                }
            }
        }

        // Find all #include_next directives (GCC extension)
        for include_next_node in find_children_by_kind(&root_node, "preproc_include_next") {
            if let Some(argument) = include_next_node.child(1) {
                let path = node_text(&argument, source);
                if !path.is_empty() {
                    let span = node_to_span(&include_next_node);
                    let mut import = Self::create_include_import(&path, span);
                    // Mark as include_next in metadata (using alias field for now)
                    import.alias = Some("include_next".to_string());
                    imports.push(import);
                }
            }
        }

        // Find all #import directives (Objective-C style, ensures single inclusion)
        for import_node in find_children_by_kind(&root_node, "preproc_import") {
            if let Some(argument) = import_node.child(1) {
                let path = node_text(&argument, source);
                if !path.is_empty() {
                    let span = node_to_span(&import_node);
                    let mut import = Self::create_include_import(&path, span);
                    // #import ensures the file is only included once
                    import.kind = ImportKind::ModuleImport;
                    imports.push(import);
                }
            }
        }

        imports
    }

    fn extract_exports(&self, tree: &Tree, source: &str) -> Vec<StandardizedExport> {
        let mut exports = Vec::new();
        let root_node = tree.root_node();

        // Extract function definitions (function_definition in C)
        for func_node in find_children_by_kind(&root_node, "function_definition") {
            // Skip static functions (file-local scope)
            if Self::is_static_declaration(&func_node, source) {
                continue;
            }

            if let Some(declarator) = func_node.child_by_field_name("declarator") {
                // Get function name from declarator
                let func_name = Self::extract_function_name(&declarator, source);
                if !func_name.is_empty() {
                    let span = node_to_span(&func_node);
                    exports.push(StandardizedExport {
                        kind: ExportKind::Named,
                        target: ExportTarget {
                            name: func_name,
                            original_name: None,
                            kind: TargetKind::Function,
                            source_module: None,
                        },
                        is_reexport: false,
                        span: Some(span),
                    });
                }
            }
        }

        // Extract struct/union definitions
        for struct_node in find_children_by_kind(&root_node, "struct_specifier") {
            if let Some(name) = struct_node.child_by_field_name("name") {
                let struct_name = node_text(&name, source);
                if !struct_name.is_empty() {
                    let span = node_to_span(&struct_node);
                    exports.push(StandardizedExport {
                        kind: ExportKind::Named,
                        target: ExportTarget {
                            name: struct_name,
                            original_name: None,
                            kind: TargetKind::Class,
                            source_module: None,
                        },
                        is_reexport: false,
                        span: Some(span),
                    });
                }
            }
        }

        // Extract union definitions
        for union_node in find_children_by_kind(&root_node, "union_specifier") {
            if let Some(name) = union_node.child_by_field_name("name") {
                let union_name = node_text(&name, source);
                if !union_name.is_empty() {
                    let span = node_to_span(&union_node);
                    exports.push(StandardizedExport {
                        kind: ExportKind::Named,
                        target: ExportTarget {
                            name: union_name,
                            original_name: None,
                            kind: TargetKind::Class,
                            source_module: None,
                        },
                        is_reexport: false,
                        span: Some(span),
                    });
                }
            }
        }

        // Extract typedef declarations
        for typedef_node in find_children_by_kind(&root_node, "type_definition") {
            if let Some(name) = typedef_node.child_by_field_name("name") {
                let typedef_name = node_text(&name, source);
                if !typedef_name.is_empty() {
                    let span = node_to_span(&typedef_node);
                    exports.push(StandardizedExport {
                        kind: ExportKind::Named,
                        target: ExportTarget {
                            name: typedef_name,
                            original_name: None,
                            kind: TargetKind::Type,
                            source_module: None,
                        },
                        is_reexport: false,
                        span: Some(span),
                    });
                }
            }
        }

        // Extract enum definitions
        for enum_node in find_children_by_kind(&root_node, "enum_specifier") {
            if let Some(name) = enum_node.child_by_field_name("name") {
                let enum_name = node_text(&name, source);
                if !enum_name.is_empty() {
                    let span = node_to_span(&enum_node);
                    exports.push(StandardizedExport {
                        kind: ExportKind::Named,
                        target: ExportTarget {
                            name: enum_name,
                            original_name: None,
                            kind: TargetKind::Other,
                            source_module: None,
                        },
                        is_reexport: false,
                        span: Some(span),
                    });
                }
            }
        }

        // Extract macro definitions (#define)
        for define_node in find_children_by_kind(&root_node, "preproc_def") {
            if let Some(macro_name) = Self::extract_macro_name(&define_node, source) {
                let span = node_to_span(&define_node);
                exports.push(StandardizedExport {
                    kind: ExportKind::Named,
                    target: ExportTarget {
                        name: macro_name,
                        original_name: None,
                        kind: TargetKind::Other,
                        source_module: None,
                    },
                    is_reexport: false,
                    span: Some(span),
                });
            }
        }

        // Extract function-like macro definitions (#define MACRO(...))
        for func_define_node in find_children_by_kind(&root_node, "preproc_function_def") {
            if let Some(macro_name) = Self::extract_macro_name(&func_define_node, source) {
                let span = node_to_span(&func_define_node);
                exports.push(StandardizedExport {
                    kind: ExportKind::Named,
                    target: ExportTarget {
                        name: macro_name,
                        original_name: None,
                        kind: TargetKind::Function,
                        source_module: None,
                    },
                    is_reexport: false,
                    span: Some(span),
                });
            }
        }

        exports
    }

    fn language(&self) -> Language {
        Language::C
    }
}

impl Default for CExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_header_detection() {
        assert!(CExtractor::is_system_header("<stdio.h>"));
        assert!(!CExtractor::is_system_header("\"local.h\""));
    }

    #[test]
    fn test_local_header_detection() {
        assert!(CExtractor::is_local_header("\"local.h\""));
        assert!(!CExtractor::is_local_header("<stdio.h>"));
    }

    #[test]
    fn test_path_extraction() {
        assert_eq!(CExtractor::extract_include_path("<stdio.h>"), "stdio.h");
        assert_eq!(CExtractor::extract_include_path("\"local.h\""), "local.h");
    }

    #[test]
    fn test_stdlib_header_detection() {
        assert!(CExtractor::is_stdlib_header("stdio.h"));
        assert!(CExtractor::is_stdlib_header("stdlib.h"));
        assert!(CExtractor::is_stdlib_header("string.h"));
        assert!(!CExtractor::is_stdlib_header("myheader.h"));
    }

    #[test]
    fn test_include_import_system() {
        let import = CExtractor::create_include_import("<stdio.h>", Span::default());
        assert_eq!(import.source, "stdio.h");
        assert!(import.is_system_header);
        assert!(!import.is_relative);
    }

    #[test]
    fn test_include_import_local() {
        let import = CExtractor::create_include_import("\"myheader.h\"", Span::default());
        assert_eq!(import.source, "myheader.h");
        assert!(!import.is_system_header);
        assert!(import.is_relative);
    }

    #[test]
    fn test_include_import_stdlib_with_quotes() {
        // Standard library header included with quotes (unusual but valid)
        let import = CExtractor::create_include_import("\"stdio.h\"", Span::default());
        assert_eq!(import.source, "stdio.h");
        assert!(import.is_system_header); // Still marked as system because it's stdlib
        assert!(!import.is_relative); // Not relative because it's stdlib
    }
}
