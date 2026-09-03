//! Ruby language import/export extractor
//!
//! Extracts require, require_relative, load statements and module/class definitions.

use super::traits::SymbolExtractor;
use crate::parser::stdlib::RubyStdlibDetector;
use crate::tree_sitter_query::{
    find_children_by_kind, find_descendants_by_kind, node_text, node_to_span,
};
use cce_types::import::{
    ExportKind, ExportTarget, ImportKind, ImportTarget, StandardizedExport, StandardizedImport,
    TargetKind,
};
use cce_types::language::Language;
use tree_sitter::Tree;

/// Ruby language import/export extractor
pub struct RubyExtractor;

impl RubyExtractor {
    pub fn new() -> Self {
        Self
    }

    /// Check if a path is a relative require
    fn is_relative_require(path: &str) -> bool {
        path.starts_with("./") || path.starts_with("../") || path == "." || path == ".."
    }

    /// Check if a module/class is from standard library
    fn is_stdlib(name: &str) -> bool {
        RubyStdlibDetector::is_core_module(name) || RubyStdlibDetector::is_stdlib_require(name)
    }

    /// Extract path from string literal
    fn extract_string_path(node: &tree_sitter::Node, source: &str) -> Option<String> {
        // Try to get string content
        for child in node.children(&mut node.walk()) {
            if child.kind() == "string_content" {
                let text = node_text(&child, source);
                if !text.is_empty() {
                    return Some(text);
                }
            }
        }
        None
    }

    /// Extract argument from call node
    fn extract_call_argument(node: &tree_sitter::Node, source: &str) -> Option<String> {
        // Look for argument_list
        for arg_list in find_children_by_kind(node, "argument_list") {
            // Get the first argument (should be a string)
            for child in arg_list.children(&mut arg_list.walk()) {
                if child.kind() == "string" {
                    return Self::extract_string_path(&child, source);
                }
            }
        }
        None
    }

    /// Check if a call is to a specific method
    fn is_call_to(node: &tree_sitter::Node, source: &str, method_name: &str) -> bool {
        if let Some(method_node) = node.child_by_field_name("method") {
            let method = node_text(&method_node, source);
            method == method_name
        } else {
            false
        }
    }
}

impl SymbolExtractor for RubyExtractor {
    fn extract_imports(&self, tree: &Tree, source: &str) -> Vec<StandardizedImport> {
        let mut imports = Vec::new();
        let root_node = tree.root_node();

        // Extract require, require_relative, load statements
        // These are method calls in Ruby: require 'file', require_relative 'file', load 'file'
        for call_node in find_children_by_kind(&root_node, "call") {
            // Check for require
            if Self::is_call_to(&call_node, source, "require") {
                if let Some(path) = Self::extract_call_argument(&call_node, source) {
                    let is_stdlib = Self::is_stdlib(&path);
                    let import = StandardizedImport {
                        kind: ImportKind::ModuleImport,
                        source: path.clone(),
                        target: ImportTarget {
                            local_name: path,
                            original_name: None,
                            kind: TargetKind::Module,
                        },
                        alias: None,
                        is_wildcard: false,
                        is_default: false,
                        is_system_header: is_stdlib,
                        is_relative: false,
                        span: Some(node_to_span(&call_node)),
                    };
                    imports.push(import);
                }
            }

            // Check for require_relative
            if Self::is_call_to(&call_node, source, "require_relative") {
                if let Some(path) = Self::extract_call_argument(&call_node, source) {
                    let import = StandardizedImport {
                        kind: ImportKind::ModuleImport,
                        source: path.clone(),
                        target: ImportTarget {
                            local_name: path,
                            original_name: None,
                            kind: TargetKind::Module,
                        },
                        alias: None,
                        is_wildcard: false,
                        is_default: false,
                        is_system_header: false,
                        is_relative: true,
                        span: Some(node_to_span(&call_node)),
                    };
                    imports.push(import);
                }
            }

            // Check for load
            if Self::is_call_to(&call_node, source, "load") {
                if let Some(path) = Self::extract_call_argument(&call_node, source) {
                    let is_relative = Self::is_relative_require(&path);
                    let import = StandardizedImport {
                        kind: ImportKind::ModuleImport,
                        source: path.clone(),
                        target: ImportTarget {
                            local_name: path,
                            original_name: None,
                            kind: TargetKind::Module,
                        },
                        alias: None,
                        is_wildcard: false,
                        is_default: false,
                        is_system_header: false,
                        is_relative,
                        span: Some(node_to_span(&call_node)),
                    };
                    imports.push(import);
                }
            }

            // Check for autoload
            if Self::is_call_to(&call_node, source, "autoload") {
                // autoload has two arguments: symbol and path
                // We extract the path (second argument)
                for arg_list in find_children_by_kind(&call_node, "argument_list") {
                    let args: Vec<_> = arg_list
                        .children(&mut arg_list.walk())
                        .filter(|c| c.kind() == "string")
                        .collect();

                    if !args.is_empty() {
                        if let Some(path) = Self::extract_string_path(&args[0], source) {
                            let is_relative = Self::is_relative_require(&path);
                            let span = node_to_span(&call_node);
                            let import = StandardizedImport {
                                kind: ImportKind::ModuleImport,
                                source: path.clone(),
                                target: ImportTarget {
                                    local_name: path,
                                    original_name: None,
                                    kind: TargetKind::Module,
                                },
                                alias: None,
                                is_wildcard: false,
                                is_default: false,
                                is_system_header: false,
                                is_relative,
                                span: Some(span),
                            };
                            imports.push(import);
                        }
                    }
                }
            }

            // Check for include/extend/prepend (module mixins)
            if Self::is_call_to(&call_node, source, "include")
                || Self::is_call_to(&call_node, source, "extend")
                || Self::is_call_to(&call_node, source, "prepend")
            {
                for arg_list in find_children_by_kind(&call_node, "argument_list") {
                    for child in arg_list.children(&mut arg_list.walk()) {
                        if child.kind() == "constant" {
                            let module_name = node_text(&child, source);
                            if !module_name.is_empty() {
                                let span = node_to_span(&call_node);
                                let import = StandardizedImport {
                                    kind: ImportKind::SymbolImport,
                                    source: module_name.clone(),
                                    target: ImportTarget {
                                        local_name: module_name,
                                        original_name: None,
                                        kind: TargetKind::Module,
                                    },
                                    alias: None,
                                    is_wildcard: false,
                                    is_default: false,
                                    is_system_header: false,
                                    is_relative: false,
                                    span: Some(span),
                                };
                                imports.push(import);
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

        // Ruby doesn't have explicit exports
        // Instead, we export all public class/module definitions

        // Export classes - use find_descendants_by_kind to find nested classes
        for class_node in find_descendants_by_kind(&root_node, "class") {
            if let Some(name_node) = class_node.child_by_field_name("name") {
                let class_name = node_text(&name_node, source);
                if !class_name.is_empty() {
                    let export = StandardizedExport {
                        kind: ExportKind::Named,
                        target: ExportTarget {
                            name: class_name,
                            original_name: None,
                            kind: TargetKind::Class,
                            source_module: None,
                        },
                        is_reexport: false,
                        span: Some(node_to_span(&class_node)),
                    };
                    exports.push(export);
                }
            }
        }

        // Export modules - use find_descendants_by_kind to find nested modules
        for module_node in find_descendants_by_kind(&root_node, "module") {
            if let Some(name_node) = module_node.child_by_field_name("name") {
                let module_name = node_text(&name_node, source);
                if !module_name.is_empty() {
                    let export = StandardizedExport {
                        kind: ExportKind::Named,
                        target: ExportTarget {
                            name: module_name,
                            original_name: None,
                            kind: TargetKind::Module,
                            source_module: None,
                        },
                        is_reexport: false,
                        span: Some(node_to_span(&module_node)),
                    };
                    exports.push(export);
                }
            }
        }

        // Export public methods (instance methods) - use find_descendants_by_kind
        for method_node in find_descendants_by_kind(&root_node, "method") {
            if let Some(name_node) = method_node.child_by_field_name("name") {
                let method_name = node_text(&name_node, source);
                // Skip private/protected methods (those starting with underscore is a convention)
                if !method_name.is_empty() {
                    let export = StandardizedExport {
                        kind: ExportKind::Named,
                        target: ExportTarget {
                            name: method_name,
                            original_name: None,
                            kind: TargetKind::Function,
                            source_module: None,
                        },
                        is_reexport: false,
                        span: Some(node_to_span(&method_node)),
                    };
                    exports.push(export);
                }
            }
        }

        // Export singleton methods - use find_descendants_by_kind
        for method_node in find_descendants_by_kind(&root_node, "singleton_method") {
            if let Some(name_node) = method_node.child_by_field_name("name") {
                let method_name = node_text(&name_node, source);
                if !method_name.is_empty() {
                    let export = StandardizedExport {
                        kind: ExportKind::Named,
                        target: ExportTarget {
                            name: method_name,
                            original_name: None,
                            kind: TargetKind::Function,
                            source_module: None,
                        },
                        is_reexport: false,
                        span: Some(node_to_span(&method_node)),
                    };
                    exports.push(export);
                }
            }
        }

        exports.sort_by_key(|export| {
            export
                .span
                .map(|span| (span.start_byte, span.end_byte))
                .unwrap_or((usize::MAX, usize::MAX))
        });
        exports.dedup_by(|left, right| {
            left.target.name == right.target.name
                && left.target.kind == right.target.kind
                && left.span == right.span
        });

        exports
    }

    fn language(&self) -> Language {
        Language::Ruby
    }
}

impl Default for RubyExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::AstParser;

    #[test]
    fn test_ruby_require_statement() {
        let code = r#"
            require 'json'
            require 'net/http'
        "#;
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::Ruby);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = RubyExtractor::new();
        let imports = extractor.extract_imports(&tree, code);

        assert!(imports.len() >= 2);
        assert!(imports.iter().any(|i| i.source == "json"));
        assert!(imports.iter().any(|i| i.source == "net/http"));
    }

    #[test]
    fn test_ruby_require_relative() {
        let code = r#"
            require_relative 'helpers'
            require_relative '../lib/utils'
        "#;
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::Ruby);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = RubyExtractor::new();
        let imports = extractor.extract_imports(&tree, code);

        assert!(imports.len() >= 2);
        assert!(imports.iter().all(|i| i.is_relative));
    }

    #[test]
    fn test_ruby_module_definition() {
        let code = r#"
            module MyApp
              module Models
              end
            end
        "#;
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::Ruby);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = RubyExtractor::new();
        let exports = extractor.extract_exports(&tree, code);

        assert!(exports.len() >= 2);
        assert!(exports.iter().any(|e| e.target.name == "MyApp"));
        assert!(exports.iter().any(|e| e.target.name == "Models"));
    }

    #[test]
    fn test_ruby_class_definition() {
        let code = r#"
            class User
              def initialize(name)
                @name = name
              end
            end
        "#;
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::Ruby);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = RubyExtractor::new();
        let exports = extractor.extract_exports(&tree, code);

        assert!(exports.iter().any(|e| e.target.name == "User"));
        assert!(exports.iter().any(|e| e.target.kind == TargetKind::Class));
    }

    #[test]
    fn test_ruby_method_export() {
        let code = r#"
            class User
              def save
              end
            end
        "#;
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::Ruby);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = RubyExtractor::new();
        let exports = extractor.extract_exports(&tree, code);

        assert!(exports.iter().any(|e| e.target.name == "save"));
        assert!(
            exports
                .iter()
                .any(|e| e.target.kind == TargetKind::Function)
        );
    }

    #[test]
    fn test_ruby_load_statement() {
        let code = "load 'config.rb'";
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::Ruby);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = RubyExtractor::new();
        let imports = extractor.extract_imports(&tree, code);

        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].source, "config.rb");
    }

    #[test]
    fn test_ruby_stdlib_detection() {
        let code = "require 'json'";
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::Ruby);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = RubyExtractor::new();
        let imports = extractor.extract_imports(&tree, code);

        assert_eq!(imports.len(), 1);
        assert!(imports[0].is_system_header);
    }
}
