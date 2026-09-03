//! Python language import/export extractor
//!
//! Extracts import statements and module-level symbol exports.

use super::common::helpers::path::{extract_base_module, is_nested_path, is_relative_python};
use super::traits::SymbolExtractor;
use crate::parser::stdlib::PythonStdlibDetector;
use crate::tree_sitter_query::{find_children_by_kind, node_text, node_to_span};
use cce_types::import::{
    ExportKind, ExportTarget, ImportKind, ImportTarget, StandardizedExport, StandardizedImport,
    TargetKind,
};
use cce_types::language::Language;
use tree_sitter::{Node, Tree};

/// Python language import/export extractor
pub struct PythonExtractor;

impl PythonExtractor {
    pub fn new() -> Self {
        Self
    }

    /// Check if an import is from the standard library
    fn is_stdlib_import(path: &str) -> bool {
        let module = path.split('.').next().unwrap_or(path);
        PythonStdlibDetector::is_stdlib_module(module)
    }

    /// Process a simple import statement: `import module` or `import module as alias`
    fn process_import_node(import_node: Node, source: &str, imports: &mut Vec<StandardizedImport>) {
        if let Some(name_node) = import_node.child_by_field_name("name") {
            // Check if this is an aliased import (import x as y)
            let (module_name, alias) = if name_node.kind() == "aliased_import" {
                // Extract the actual name and alias
                let actual_name = if let Some(dotted_name) = name_node.child_by_field_name("name") {
                    node_text(&dotted_name, source)
                } else {
                    node_text(&name_node, source)
                };
                let alias_text = name_node
                    .child_by_field_name("alias")
                    .map(|a| node_text(&a, source));
                (actual_name, alias_text)
            } else {
                (node_text(&name_node, source), None)
            };

            if !module_name.is_empty() {
                let local_name = alias.clone().unwrap_or_else(|| module_name.clone());
                let original_name = if alias.is_some() {
                    Some(module_name.clone())
                } else {
                    None
                };

                let is_nested = is_nested_path(&module_name);
                let base_module = if is_nested {
                    extract_base_module(&module_name).to_string()
                } else {
                    module_name.clone()
                };

                let target = ImportTarget {
                    local_name,
                    original_name,
                    kind: TargetKind::Module,
                };

                let is_stdlib = Self::is_stdlib_import(&base_module);
                let is_relative = is_relative_python(&module_name);
                let span = node_to_span(&import_node);
                imports.push(StandardizedImport {
                    kind: ImportKind::ModuleImport,
                    source: module_name,
                    target,
                    alias,
                    is_wildcard: false,
                    is_default: false,
                    is_system_header: is_stdlib,
                    is_relative,
                    span: Some(span),
                });
            }
        }
    }

    /// Process a from-import statement: `from module import name`
    fn process_from_import_node(
        from_node: Node,
        source: &str,
        imports: &mut Vec<StandardizedImport>,
    ) {
        // Extract module name (could be dotted_name or relative_import)
        let module_node_opt = from_node.child_by_field_name("module_name");
        let module_name = if let Some(ref module_node) = module_node_opt {
            node_text(module_node, source)
        } else {
            String::new()
        };

        if module_name.is_empty() {
            return;
        }

        let is_stdlib = Self::is_stdlib_import(&module_name);
        let is_relative = is_relative_python(&module_name);
        let span = node_to_span(&from_node);

        // Check for wildcard import: from module import *
        if Self::process_wildcard_import(
            &from_node,
            &module_name,
            is_stdlib,
            is_relative,
            span,
            imports,
        ) {
            return;
        }

        // Process regular imports - find all dotted_name and aliased_import nodes
        // Use node identity comparison instead of text comparison for reliability
        for child in from_node.children(&mut from_node.walk()) {
            match child.kind() {
                "dotted_name" => {
                    // Skip if this is the module_name field by comparing node identity
                    let is_module_name_node = module_node_opt
                        .as_ref()
                        .map(|mn| child.id() == mn.id())
                        .unwrap_or(false);

                    if !is_module_name_node {
                        Self::process_dotted_import(
                            &child,
                            source,
                            &module_name,
                            is_stdlib,
                            is_relative,
                            span,
                            imports,
                        );
                    }
                }
                "aliased_import" => {
                    Self::process_aliased_import(
                        &child,
                        source,
                        &module_name,
                        is_stdlib,
                        is_relative,
                        span,
                        imports,
                    );
                }
                _ => {}
            }
        }
    }

    /// Process wildcard import: `from module import *`
    /// Returns true if wildcard was found and processed
    fn process_wildcard_import(
        from_node: &Node,
        module_name: &str,
        is_stdlib: bool,
        is_relative: bool,
        span: cce_types::position::Span,
        imports: &mut Vec<StandardizedImport>,
    ) -> bool {
        // Check for wildcard_import node
        if !find_children_by_kind(from_node, "wildcard_import").is_empty() {
            imports.push(StandardizedImport {
                kind: ImportKind::NamespaceImport,
                source: module_name.to_string(),
                target: Default::default(),
                alias: None,
                is_wildcard: true,
                is_default: false,
                is_system_header: is_stdlib,
                is_relative,
                span: Some(span),
            });
            return true;
        }
        false
    }

    /// Process a dotted name import: `from module import name`
    fn process_dotted_import(
        import_item: &Node,
        source: &str,
        module_name: &str,
        is_stdlib: bool,
        is_relative: bool,
        span: cce_types::position::Span,
        imports: &mut Vec<StandardizedImport>,
    ) {
        let name = node_text(import_item, source);
        if !name.is_empty() {
            imports.push(StandardizedImport {
                kind: ImportKind::SymbolImport,
                source: module_name.to_string(),
                target: ImportTarget {
                    local_name: name,
                    original_name: None,
                    kind: TargetKind::Other,
                },
                alias: None,
                is_wildcard: false,
                is_default: false,
                is_system_header: is_stdlib,
                is_relative,
                span: Some(span),
            });
        }
    }

    /// Process an aliased import: `from module import name as alias`
    fn process_aliased_import(
        import_item: &Node,
        source: &str,
        module_name: &str,
        is_stdlib: bool,
        is_relative: bool,
        span: cce_types::position::Span,
        imports: &mut Vec<StandardizedImport>,
    ) {
        if let Some(name_node) = import_item.child_by_field_name("name") {
            let name = node_text(&name_node, source);
            if let Some(alias_node) = import_item.child_by_field_name("alias") {
                let alias = node_text(&alias_node, source);
                if !name.is_empty() && !alias.is_empty() {
                    imports.push(StandardizedImport {
                        kind: ImportKind::SymbolImport,
                        source: module_name.to_string(),
                        target: ImportTarget {
                            local_name: alias.clone(),
                            original_name: Some(name),
                            kind: TargetKind::Other,
                        },
                        alias: Some(alias),
                        is_wildcard: false,
                        is_default: false,
                        is_system_header: is_stdlib,
                        is_relative,
                        span: Some(span),
                    });
                }
            }
        }
    }

    /// Recursively find imports in nested blocks (if statements, try blocks, etc.)
    fn find_imports_in_block(
        &self,
        node: &Node,
        source: &str,
        imports: &mut Vec<StandardizedImport>,
    ) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "import_statement" => Self::process_import_node(child, source, imports),
                "import_from_statement" => Self::process_from_import_node(child, source, imports),
                "if_statement" | "try_statement" | "with_statement" | "block" | "module" => {
                    self.find_imports_in_block(&child, source, imports);
                }
                _ => {
                    if child.child_count() > 0 {
                        self.find_imports_in_block(&child, source, imports);
                    }
                }
            }
        }
    }

    /// Check if a name should be exported (doesn't start with underscore)
    fn is_exported_name(name: &str) -> bool {
        !name.starts_with('_') && !name.is_empty()
    }

    /// Create a standardized export for a function
    fn create_function_export(name: String, span: cce_types::position::Span) -> StandardizedExport {
        StandardizedExport {
            kind: ExportKind::Named,
            target: ExportTarget {
                name,
                original_name: None,
                kind: TargetKind::Function,
                source_module: None,
            },
            is_reexport: false,
            span: Some(span),
        }
    }

    /// Create a standardized export for a class
    fn create_class_export(name: String, span: cce_types::position::Span) -> StandardizedExport {
        StandardizedExport {
            kind: ExportKind::Named,
            target: ExportTarget {
                name,
                original_name: None,
                kind: TargetKind::Class,
                source_module: None,
            },
            is_reexport: false,
            span: Some(span),
        }
    }

    /// Check if a value represents a type alias
    fn is_type_alias_value(value_text: &str) -> bool {
        value_text.contains("Type[")
            || value_text.contains("typing.")
            || value_text.contains("Optional[")
            || value_text.contains("Union[")
            || value_text.contains("List[")
            || value_text.contains("Dict[")
            || value_text.contains("Tuple[")
            || value_text.contains("Callable[")
            || value_text.contains("Any")
            || value_text.starts_with("Literal[")
            || value_text.starts_with("Annotated[")
    }
}

impl SymbolExtractor for PythonExtractor {
    fn extract_imports(&self, tree: &Tree, source: &str) -> Vec<StandardizedImport> {
        let mut imports = Vec::new();
        let root_node = tree.root_node();

        // Find all import statements at top level
        for import_node in find_children_by_kind(&root_node, "import_statement") {
            Self::process_import_node(import_node, source, &mut imports);
        }

        // Find all "from ... import ..." statements at top level
        for from_node in find_children_by_kind(&root_node, "import_from_statement") {
            Self::process_from_import_node(from_node, source, &mut imports);
        }

        // Find imports inside conditional blocks
        let mut cursor = root_node.walk();
        for child in root_node.children(&mut cursor) {
            if matches!(
                child.kind(),
                "if_statement" | "try_statement" | "with_statement"
            ) {
                self.find_imports_in_block(&child, source, &mut imports);
            }
        }

        imports
    }

    fn extract_exports(&self, tree: &Tree, source: &str) -> Vec<StandardizedExport> {
        let mut exports = Vec::new();
        let root_node = tree.root_node();

        // Python uses __all__ to explicitly define public API
        if Self::extract_all_exports(&root_node, source, &mut exports) {
            return exports;
        }

        // If no __all__ is defined, export all public names
        Self::extract_implicit_exports(&root_node, source, &mut exports);

        exports
    }

    fn language(&self) -> Language {
        Language::Python
    }
}

impl PythonExtractor {
    /// Extract exports from __all__ assignment
    /// Returns true if __all__ was found
    fn extract_all_exports(
        root_node: &Node,
        source: &str,
        exports: &mut Vec<StandardizedExport>,
    ) -> bool {
        for expr_node in find_children_by_kind(root_node, "assignment") {
            if let Some(name_node) = expr_node.child_by_field_name("left") {
                let name = node_text(&name_node, source);
                if name == "__all__" {
                    let span = node_to_span(&expr_node);
                    if let Some(value_node) = expr_node.child_by_field_name("right") {
                        for list_item in find_children_by_kind(&value_node, "list") {
                            for string_node in find_children_by_kind(&list_item, "string") {
                                let export_name = node_text(&string_node, source)
                                    .trim_matches('"')
                                    .trim_matches('\'')
                                    .to_string();
                                if !export_name.is_empty() {
                                    exports.push(StandardizedExport {
                                        kind: ExportKind::Named,
                                        target: ExportTarget {
                                            name: export_name,
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
                    }
                    return true;
                }
            }
        }
        false
    }

    /// Extract implicit exports (all public names not starting with _)
    fn extract_implicit_exports(
        root_node: &Node,
        source: &str,
        exports: &mut Vec<StandardizedExport>,
    ) {
        let mut cursor = root_node.walk();
        for child in root_node.children(&mut cursor) {
            match child.kind() {
                "function_definition" => {
                    Self::extract_function_export(&child, source, exports);
                }
                "class_definition" => {
                    Self::extract_class_export(&child, source, exports);
                }
                "expression_statement" => {
                    // Check for assignment inside expression_statement
                    for sub_child in child.children(&mut child.walk()) {
                        if sub_child.kind() == "assignment" {
                            Self::extract_assignment_export(&sub_child, source, exports);
                        }
                    }
                }
                "assignment" => {
                    Self::extract_assignment_export(&child, source, exports);
                }
                "decorated_definition" => {
                    Self::extract_decorated_export(&child, source, exports);
                }
                "async_function_definition" => {
                    Self::extract_function_export(&child, source, exports);
                }
                _ => {}
            }
        }
    }

    /// Extract export from function definition
    fn extract_function_export(node: &Node, source: &str, exports: &mut Vec<StandardizedExport>) {
        if let Some(name) = node.child_by_field_name("name") {
            let fn_name = node_text(&name, source);
            if Self::is_exported_name(&fn_name) {
                exports.push(Self::create_function_export(fn_name, node_to_span(node)));
            }
        }
    }

    /// Extract export from class definition
    fn extract_class_export(node: &Node, source: &str, exports: &mut Vec<StandardizedExport>) {
        if let Some(name) = node.child_by_field_name("name") {
            let class_name = node_text(&name, source);
            if Self::is_exported_name(&class_name) {
                exports.push(Self::create_class_export(class_name, node_to_span(node)));
            }
        }
    }

    /// Extract export from assignment (variable or type alias)
    fn extract_assignment_export(node: &Node, source: &str, exports: &mut Vec<StandardizedExport>) {
        if let Some(name_node) = node.child_by_field_name("left") {
            let name = node_text(&name_node, source);
            if name != "__all__" && Self::is_exported_name(&name) {
                let span = node_to_span(node);
                let kind = if let Some(value_node) = node.child_by_field_name("right") {
                    let value_text = node_text(&value_node, source);
                    if Self::is_type_alias_value(&value_text) {
                        TargetKind::Type
                    } else {
                        TargetKind::Variable
                    }
                } else {
                    TargetKind::Variable
                };

                exports.push(StandardizedExport {
                    kind: ExportKind::Named,
                    target: ExportTarget {
                        name,
                        original_name: None,
                        kind,
                        source_module: None,
                    },
                    is_reexport: false,
                    span: Some(span),
                });
            }
        }
    }

    /// Extract export from decorated definition
    fn extract_decorated_export(node: &Node, source: &str, exports: &mut Vec<StandardizedExport>) {
        let span = node_to_span(node);
        for def_child in node.children(&mut node.walk()) {
            match def_child.kind() {
                "function_definition" => {
                    if let Some(name) = def_child.child_by_field_name("name") {
                        let fn_name = node_text(&name, source);
                        if Self::is_exported_name(&fn_name) {
                            exports.push(StandardizedExport {
                                kind: ExportKind::Named,
                                target: ExportTarget {
                                    name: fn_name,
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
                "class_definition" => {
                    if let Some(name) = def_child.child_by_field_name("name") {
                        let class_name = node_text(&name, source);
                        if Self::is_exported_name(&class_name) {
                            exports.push(StandardizedExport {
                                kind: ExportKind::Named,
                                target: ExportTarget {
                                    name: class_name,
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
                _ => {}
            }
        }
    }
}

impl Default for PythonExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::AstParser;

    #[test]
    fn test_is_relative_import() {
        assert!(is_relative_python(".module"));
        assert!(is_relative_python("..parent"));
        assert!(!is_relative_python("os"));
    }

    #[test]
    fn test_is_stdlib_import() {
        assert!(PythonExtractor::is_stdlib_import("os"));
        assert!(PythonExtractor::is_stdlib_import("sys.path"));
        assert!(!PythonExtractor::is_stdlib_import("numpy"));
    }

    #[test]
    fn test_is_exported_name() {
        assert!(PythonExtractor::is_exported_name("public_func"));
        assert!(!PythonExtractor::is_exported_name("_private_func"));
        assert!(!PythonExtractor::is_exported_name("__dunder__"));
        assert!(!PythonExtractor::is_exported_name(""));
    }

    #[test]
    fn test_is_type_alias_value() {
        assert!(PythonExtractor::is_type_alias_value("Optional[str]"));
        assert!(PythonExtractor::is_type_alias_value("Union[int, str]"));
        assert!(PythonExtractor::is_type_alias_value("typing.List"));
        assert!(!PythonExtractor::is_type_alias_value("42"));
        assert!(!PythonExtractor::is_type_alias_value("\"hello\""));
    }

    #[test]
    fn test_python_import_extraction() {
        let code = r#"
import os
import sys.path
from collections import defaultdict
from .local_module import something
"#;
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::Python);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = PythonExtractor::new();
        let imports = extractor.extract_imports(&tree, code);

        assert!(!imports.is_empty());
        // Check for stdlib import
        assert!(
            imports
                .iter()
                .any(|i| i.source == "os" && i.is_system_header)
        );
        // Check for relative import
        assert!(imports.iter().any(|i| i.is_relative));
    }

    #[test]
    fn test_python_from_import() {
        let code = r#"
from typing import List, Dict
from numpy import array as arr
"#;
        let mut parser = AstParser::new();
        let result = parser.parse_with_tree(code, &Language::Python);
        assert!(result.is_ok());

        let (tree, _) = result.expect("Failed to parse");
        let extractor = PythonExtractor::new();
        let imports = extractor.extract_imports(&tree, code);

        assert!(!imports.is_empty());
        // Check for typing stdlib
        assert!(
            imports
                .iter()
                .any(|i| i.source == "typing" && i.is_system_header)
        );
        // Check for numpy external package
        assert!(
            imports
                .iter()
                .any(|i| i.source == "numpy" && !i.is_system_header)
        );
    }
}
