//! PHP language import/export extractor
//!
//! Extracts use statements, include/require, and namespace declarations.

use super::traits::SymbolExtractor;
use crate::parser::stdlib::PhpStdlibDetector;
use crate::tree_sitter_query::{
    find_child_by_kind, find_children_by_kind, find_descendants_by_kind, node_text, node_to_span,
};
use cce_types::import::{
    ExportKind, ExportTarget, ImportKind, ImportTarget, StandardizedExport, StandardizedImport,
    TargetKind,
};
use cce_types::language::Language;
use tree_sitter::Tree;

/// PHP language import/export extractor
pub struct PhpExtractor;

impl PhpExtractor {
    pub fn new() -> Self {
        Self
    }

    /// Check if a path is a relative include
    fn is_relative_include(path: &str) -> bool {
        path.starts_with("./") || path.starts_with("../") || path == "." || path == ".."
    }

    /// Check if a namespace/class is from standard library
    fn is_stdlib(name: &str) -> bool {
        PhpStdlibDetector::is_builtin_class(name) || PhpStdlibDetector::is_core_extension(name)
    }

    /// Extract path from string literal
    fn extract_string_path(node: &tree_sitter::Node, source: &str) -> Option<String> {
        let text = node_text(node, source);
        // Remove quotes
        let trimmed = text
            .trim_start_matches('"')
            .trim_end_matches('"')
            .trim_start_matches('\'')
            .trim_end_matches('\'')
            .to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    }

    fn use_target_kind(node: &tree_sitter::Node, source: &str) -> TargetKind {
        let use_text = node_text(node, source);
        if use_text.trim_start().starts_with("use function") {
            TargetKind::Function
        } else if use_text.trim_start().starts_with("use const") {
            TargetKind::Variable
        } else {
            TargetKind::Class
        }
    }
}

impl SymbolExtractor for PhpExtractor {
    fn extract_imports(&self, tree: &Tree, source: &str) -> Vec<StandardizedImport> {
        let mut imports = Vec::new();
        let root_node = tree.root_node();

        // Extract use statements
        // use Some\Namespace\Class;
        // use Some\Namespace\Class as Alias;
        // use Some\Namespace\{Class1, Class2};
        for use_node in find_descendants_by_kind(&root_node, "namespace_use_declaration") {
            // Check for simple use clause
            for use_clause in find_children_by_kind(&use_node, "namespace_use_clause") {
                // Get the qualified name - extract full path from qualified_name
                let full_name = if let Some(qualified_name) =
                    find_child_by_kind(&use_clause, "qualified_name")
                {
                    let mut parts = Vec::new();

                    // Walk through all name descendants in source order
                    let mut name_nodes: Vec<_> = find_descendants_by_kind(&qualified_name, "name")
                        .into_iter()
                        .filter(|n| {
                            let text = node_text(n, source);
                            !text.is_empty()
                        })
                        .collect();

                    // Sort by start position to get source order
                    name_nodes.sort_by_key(|n| n.start_position());

                    for name_node in name_nodes {
                        parts.push(node_text(&name_node, source));
                    }

                    parts.join("\\")
                } else {
                    String::new()
                };

                if !full_name.is_empty() {
                    // Check for alias - look for 'as' keyword followed by name
                    let alias = if let Some(alias_node) = use_clause.child_by_field_name("alias") {
                        Some(node_text(&alias_node, source))
                    } else {
                        // Look for 'as' keyword and the following name
                        let mut cursor = use_clause.walk();
                        let mut found_as = false;
                        let mut alias_name = None;
                        for child in use_clause.children(&mut cursor) {
                            if node_text(&child, source) == "as" {
                                found_as = true;
                            } else if found_as && child.kind() == "name" {
                                alias_name = Some(node_text(&child, source));
                                break;
                            }
                        }
                        alias_name
                    };

                    // Extract the last component as the local name
                    let local_name = full_name
                        .split('\\')
                        .next_back()
                        .unwrap_or(&full_name)
                        .to_string();

                    let target = ImportTarget {
                        local_name: alias.clone().unwrap_or(local_name),
                        original_name: if alias.is_some() {
                            Some(full_name.clone())
                        } else {
                            None
                        },
                        kind: Self::use_target_kind(&use_node, source),
                    };

                    let import = StandardizedImport {
                        kind: ImportKind::SymbolImport,
                        source: full_name.clone(),
                        target,
                        alias,
                        is_wildcard: false,
                        is_default: false,
                        is_system_header: Self::is_stdlib(&full_name),
                        is_relative: false,
                        span: Some(node_to_span(&use_node)),
                    };
                    imports.push(import);
                }
            }

            // Check for group use (use Some\Namespace\{Class1, Class2})
            for group_node in find_children_by_kind(&use_node, "namespace_use_group") {
                // Get the base namespace from parent
                let base_ns = find_child_by_kind(&use_node, "namespace_name")
                    .map(|n| node_text(&n, source))
                    .unwrap_or_default();

                for use_clause in find_children_by_kind(&group_node, "namespace_use_clause") {
                    if let Some(name_node) = find_child_by_kind(&use_clause, "name") {
                        let class_name = node_text(&name_node, source);
                        if !class_name.is_empty() {
                            let full_name = if base_ns.is_empty() {
                                class_name.clone()
                            } else {
                                format!("{}\\{}", base_ns, class_name)
                            };

                            let alias = use_clause
                                .child_by_field_name("alias")
                                .map(|n| node_text(&n, source))
                                .or_else(|| {
                                    let mut cursor = use_clause.walk();
                                    let mut found_as = false;
                                    let mut alias_name = None;
                                    for child in use_clause.children(&mut cursor) {
                                        if node_text(&child, source) == "as" {
                                            found_as = true;
                                        } else if found_as && child.kind() == "name" {
                                            alias_name = Some(node_text(&child, source));
                                            break;
                                        }
                                    }
                                    alias_name
                                });

                            let target = ImportTarget {
                                local_name: alias.clone().unwrap_or(class_name),
                                original_name: if alias.is_some() {
                                    Some(full_name.clone())
                                } else {
                                    None
                                },
                                kind: Self::use_target_kind(&use_node, source),
                            };

                            let import = StandardizedImport {
                                kind: ImportKind::SymbolImport,
                                source: full_name.clone(),
                                target,
                                alias,
                                is_wildcard: false,
                                is_default: false,
                                is_system_header: Self::is_stdlib(&full_name),
                                is_relative: false,
                                span: Some(node_to_span(&use_node)),
                            };
                            imports.push(import);
                        }
                    }
                }
            }
        }

        // Extract include/require statements
        for include_node in find_descendants_by_kind(&root_node, "include_expression") {
            // Try to get path from field name, or look for string child
            let path_opt = if let Some(path_node) = include_node.child_by_field_name("path") {
                Self::extract_string_path(&path_node, source)
            } else if let Some(string_node) = find_child_by_kind(&include_node, "string") {
                Self::extract_string_path(&string_node, source)
            } else {
                None
            };

            if let Some(path) = path_opt {
                let is_relative = Self::is_relative_include(&path);
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
                    span: Some(node_to_span(&include_node)),
                };
                imports.push(import);
            }
        }

        // Extract include_once
        for include_node in find_descendants_by_kind(&root_node, "include_once_expression") {
            let path_opt = if let Some(path_node) = include_node.child_by_field_name("path") {
                Self::extract_string_path(&path_node, source)
            } else if let Some(string_node) = find_child_by_kind(&include_node, "string") {
                Self::extract_string_path(&string_node, source)
            } else {
                None
            };

            if let Some(path) = path_opt {
                let is_relative = Self::is_relative_include(&path);
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
                    span: Some(node_to_span(&include_node)),
                };
                imports.push(import);
            }
        }

        // Extract require
        for require_node in find_descendants_by_kind(&root_node, "require_expression") {
            let path_opt = if let Some(path_node) = require_node.child_by_field_name("path") {
                Self::extract_string_path(&path_node, source)
            } else if let Some(string_node) = find_child_by_kind(&require_node, "string") {
                Self::extract_string_path(&string_node, source)
            } else {
                None
            };

            if let Some(path) = path_opt {
                let is_relative = Self::is_relative_include(&path);
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
                    span: Some(node_to_span(&require_node)),
                };
                imports.push(import);
            }
        }

        // Extract require_once
        for require_node in find_descendants_by_kind(&root_node, "require_once_expression") {
            let path_opt = if let Some(path_node) = require_node.child_by_field_name("path") {
                Self::extract_string_path(&path_node, source)
            } else if let Some(string_node) = find_child_by_kind(&require_node, "string") {
                Self::extract_string_path(&string_node, source)
            } else {
                None
            };

            if let Some(path) = path_opt {
                let is_relative = Self::is_relative_include(&path);
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
                    span: Some(node_to_span(&require_node)),
                };
                imports.push(import);
            }
        }

        imports
    }

    fn extract_exports(&self, tree: &Tree, source: &str) -> Vec<StandardizedExport> {
        let mut exports = Vec::new();
        let root_node = tree.root_node();

        // PHP doesn't have explicit exports like ES6 modules
        // Instead, we export all public class/function definitions

        // Export public classes
        for class_node in find_children_by_kind(&root_node, "class_declaration") {
            if let Some(name_node) = class_node.child_by_field_name("name") {
                let class_name = node_text(&name_node, source);
                if !class_name.is_empty() {
                    // Check visibility - classes are public by default
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

        // Export public interfaces
        for interface_node in find_children_by_kind(&root_node, "interface_declaration") {
            if let Some(name_node) = interface_node.child_by_field_name("name") {
                let interface_name = node_text(&name_node, source);
                if !interface_name.is_empty() {
                    let export = StandardizedExport {
                        kind: ExportKind::Named,
                        target: ExportTarget {
                            name: interface_name,
                            original_name: None,
                            kind: TargetKind::Interface,
                            source_module: None,
                        },
                        is_reexport: false,
                        span: Some(node_to_span(&interface_node)),
                    };
                    exports.push(export);
                }
            }
        }

        // Export public traits
        for trait_node in find_children_by_kind(&root_node, "trait_declaration") {
            if let Some(name_node) = trait_node.child_by_field_name("name") {
                let trait_name = node_text(&name_node, source);
                if !trait_name.is_empty() {
                    let export = StandardizedExport {
                        kind: ExportKind::Named,
                        target: ExportTarget {
                            name: trait_name,
                            original_name: None,
                            kind: TargetKind::Other,
                            source_module: None,
                        },
                        is_reexport: false,
                        span: Some(node_to_span(&trait_node)),
                    };
                    exports.push(export);
                }
            }
        }

        // Export public functions
        for func_node in find_children_by_kind(&root_node, "function_definition") {
            if let Some(name_node) = func_node.child_by_field_name("name") {
                let func_name = node_text(&name_node, source);
                if !func_name.is_empty() {
                    let export = StandardizedExport {
                        kind: ExportKind::Named,
                        target: ExportTarget {
                            name: func_name,
                            original_name: None,
                            kind: TargetKind::Function,
                            source_module: None,
                        },
                        is_reexport: false,
                        span: Some(node_to_span(&func_node)),
                    };
                    exports.push(export);
                }
            }
        }

        // Export public enums (PHP 8.1+)
        for enum_node in find_children_by_kind(&root_node, "enum_declaration") {
            if let Some(name_node) = enum_node.child_by_field_name("name") {
                let enum_name = node_text(&name_node, source);
                if !enum_name.is_empty() {
                    let export = StandardizedExport {
                        kind: ExportKind::Named,
                        target: ExportTarget {
                            name: enum_name,
                            original_name: None,
                            kind: TargetKind::Other,
                            source_module: None,
                        },
                        is_reexport: false,
                        span: Some(node_to_span(&enum_node)),
                    };
                    exports.push(export);
                }
            }
        }

        exports
    }

    fn extract_package_declaration(&self, tree: &Tree, source: &str) -> Option<String> {
        let root_node = tree.root_node();

        for decl in find_children_by_kind(&root_node, "namespace_definition") {
            if let Some(name_node) = decl.child_by_field_name("name") {
                let name = node_text(&name_node, source);
                if !name.is_empty() {
                    return Some(name);
                }
            }
        }

        None
    }

    fn language(&self) -> Language {
        Language::Php
    }
}

impl Default for PhpExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::AstParser;

    #[test]
    fn test_php_use_statement() {
        let code = r#"
            <?php
            use App\Models\User;
            use Illuminate\Support\Facades\DB;
        "#;
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::Php);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = PhpExtractor::new();
        let imports = extractor.extract_imports(&tree, code);

        assert!(imports.iter().any(|i| i.source == "App\\Models\\User"));
        assert!(
            imports
                .iter()
                .any(|i| i.source == "Illuminate\\Support\\Facades\\DB")
        );
    }

    #[test]
    fn test_php_use_alias() {
        let code = r#"<?php use App\Models\User as UserModel;"#;
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::Php);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = PhpExtractor::new();
        let imports = extractor.extract_imports(&tree, code);

        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].alias, Some("UserModel".to_string()));
        assert_eq!(imports[0].target.local_name, "UserModel");
        assert_eq!(
            imports[0].target.original_name,
            Some("App\\Models\\User".to_string())
        );
    }

    #[test]
    fn test_php_grouped_function_and_const_use() {
        let code = r#"<?php
            use function App\Support\{format_date};
            use const App\Config\{MAX_USERS};
        "#;
        let mut parser = AstParser::new();
        let (tree, _) = parser
            .parse_with_tree(code, &Language::Php)
            .expect("Failed to parse");

        let imports = PhpExtractor::new().extract_imports(&tree, code);
        assert_eq!(imports.len(), 2);
        assert!(imports.iter().any(|import| {
            import.target.local_name == "format_date" && import.target.kind == TargetKind::Function
        }));
        assert!(imports.iter().any(|import| {
            import.target.local_name == "MAX_USERS" && import.target.kind == TargetKind::Variable
        }));
    }

    #[test]
    fn test_php_namespace_declaration() {
        let code = "<?php namespace App\\Http\\Controllers;";
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::Php);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = PhpExtractor::new();
        let package = extractor.extract_package_declaration(&tree, code);

        assert_eq!(package, Some("App\\Http\\Controllers".to_string()));
    }

    #[test]
    fn test_php_class_definition() {
        let code = r#"
            <?php
            class User extends Model
            {
                protected $fillable = ['name', 'email'];
            }
        "#;
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::Php);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = PhpExtractor::new();
        let exports = extractor.extract_exports(&tree, code);

        assert!(exports.iter().any(|e| e.target.name == "User"));
        assert!(exports.iter().any(|e| e.target.kind == TargetKind::Class));
    }

    #[test]
    fn test_php_interface_definition() {
        let code = r#"
            <?php
            interface RepositoryInterface
            {
                public function find($id);
            }
        "#;
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::Php);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = PhpExtractor::new();
        let exports = extractor.extract_exports(&tree, code);

        assert!(
            exports
                .iter()
                .any(|e| e.target.name == "RepositoryInterface")
        );
        assert!(
            exports
                .iter()
                .any(|e| e.target.kind == TargetKind::Interface)
        );
    }

    #[test]
    fn test_php_trait_definition() {
        let code = r#"
            <?php
            trait HasFactory
            {
                public static function factory()
                {
                }
            }
        "#;
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::Php);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = PhpExtractor::new();
        let exports = extractor.extract_exports(&tree, code);

        assert!(exports.iter().any(|e| e.target.name == "HasFactory"));
    }

    #[test]
    fn test_php_function_definition() {
        let code = r#"
            <?php
            function format_date($timestamp)
            {
                return date('Y-m-d', $timestamp);
            }
        "#;
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::Php);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = PhpExtractor::new();
        let exports = extractor.extract_exports(&tree, code);

        assert!(exports.iter().any(|e| e.target.name == "format_date"));
        assert!(
            exports
                .iter()
                .any(|e| e.target.kind == TargetKind::Function)
        );
    }

    #[test]
    fn test_php_enum_definition() {
        let code = r#"
            <?php
            enum Status: string
            {
                case Active = 'active';
                case Inactive = 'inactive';
            }
        "#;
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::Php);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = PhpExtractor::new();
        let exports = extractor.extract_exports(&tree, code);

        assert!(exports.iter().any(|e| e.target.name == "Status"));
    }

    #[test]
    fn test_php_include_statement() {
        let code = r#"<?php include 'config.php';"#;
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::Php);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = PhpExtractor::new();
        let imports = extractor.extract_imports(&tree, code);

        assert!(imports.iter().any(|i| i.source == "config.php"));
    }

    #[test]
    fn test_php_require_statement() {
        let code = r#"<?php require_once 'database.php';"#;
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::Php);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = PhpExtractor::new();
        let imports = extractor.extract_imports(&tree, code);

        assert!(imports.iter().any(|i| i.source == "database.php"));
    }
}
