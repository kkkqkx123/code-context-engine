//! Lua language import/export extractor
//!
//! Extracts `require` calls as imports and exposes function and method
//! declarations as exports.

use super::common::helpers::string::unquote;
use super::traits::SymbolExtractor;
use crate::parser::stdlib::LuaStdlibDetector;
use crate::tree_sitter_query::{find_descendants_by_kind, node_text, node_to_span};
use cce_types::import::{
    ExportKind, ExportTarget, ImportKind, ImportTarget, StandardizedExport, StandardizedImport,
    TargetKind,
};
use cce_types::language::Language;
use tree_sitter::{Node, Tree};

/// Lua language import/export extractor
pub struct LuaExtractor;

impl LuaExtractor {
    pub fn new() -> Self {
        Self
    }

    /// Check whether a required module path is relative.
    fn is_relative_require(path: &str) -> bool {
        path.starts_with("./") || path.starts_with("../") || path == "." || path == ".."
    }

    /// Check whether a required module is part of the standard library.
    fn is_stdlib(path: &str) -> bool {
        if LuaStdlibDetector::is_stdlib_module(path) {
            return true;
        }
        // Dotted requires (`require "pkg.mod"`) resolve against the first
        // segment for stdlib classification.
        path.split('.')
            .next()
            .is_some_and(LuaStdlibDetector::is_stdlib_module)
    }

    /// Extract the module path from a string argument node.
    fn string_argument_text(node: &Node, source: &str) -> Option<String> {
        if node.kind() == "string" {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "string_content" {
                    let text = node_text(&child, source);
                    if !text.is_empty() {
                        return Some(text);
                    }
                }
            }
        }
        let text = node_text(node, source);
        if text.is_empty() {
            return None;
        }
        Some(unquote(&text).to_string())
    }

    /// Extract the first string argument of a `require` call node.
    fn require_path(call: &Node, source: &str) -> Option<String> {
        let arguments = call.child_by_field_name("arguments")?;
        let mut cursor = arguments.walk();
        for child in arguments.children(&mut cursor) {
            if !child.is_named() {
                continue;
            }
            if child.kind() == "string" {
                if let Some(path) = Self::string_argument_text(&child, source) {
                    if !path.is_empty() {
                        return Some(path);
                    }
                }
            }
        }
        None
    }

    /// Check whether a function call node invokes `require`.
    fn is_require_call(call: &Node, source: &str) -> bool {
        call.child_by_field_name("name")
            .map(|name| node_text(&name, source))
            .is_some_and(|name| name == "require")
    }

    /// Extract the declared name of a function declaration node.
    fn declaration_name(decl: &Node, source: &str) -> Option<String> {
        let name = decl.child_by_field_name("name")?;
        match name.kind() {
            "identifier" => {
                let text = node_text(&name, source);
                (!text.is_empty()).then_some(text)
            }
            "dot_index_expression" => name
                .child_by_field_name("field")
                .map(|field| node_text(&field, source))
                .filter(|text| !text.is_empty()),
            "method_index_expression" => name
                .child_by_field_name("method")
                .map(|method| node_text(&method, source))
                .filter(|text| !text.is_empty()),
            _ => None,
        }
    }
}

impl SymbolExtractor for LuaExtractor {
    fn extract_imports(&self, tree: &Tree, source: &str) -> Vec<StandardizedImport> {
        let mut imports = Vec::new();
        let root_node = tree.root_node();

        for call in find_descendants_by_kind(&root_node, "function_call") {
            if !Self::is_require_call(&call, source) {
                continue;
            }
            if let Some(path) = Self::require_path(&call, source) {
                let is_relative = Self::is_relative_require(&path);
                let is_system_header = Self::is_stdlib(&path);
                imports.push(StandardizedImport {
                    kind: ImportKind::CommonJSRequire,
                    source: path.clone(),
                    target: ImportTarget {
                        local_name: path,
                        original_name: None,
                        kind: TargetKind::Module,
                    },
                    alias: None,
                    is_wildcard: false,
                    is_default: false,
                    is_system_header,
                    is_relative,
                    span: Some(node_to_span(&call)),
                });
            }
        }

        imports
    }

    fn extract_exports(&self, tree: &Tree, source: &str) -> Vec<StandardizedExport> {
        let mut exports = Vec::new();
        let root_node = tree.root_node();

        for decl in find_descendants_by_kind(&root_node, "function_declaration") {
            if let Some(name) = Self::declaration_name(&decl, source) {
                exports.push(StandardizedExport {
                    kind: ExportKind::Named,
                    target: ExportTarget {
                        name,
                        original_name: None,
                        kind: TargetKind::Function,
                        source_module: None,
                    },
                    is_reexport: false,
                    span: Some(node_to_span(&decl)),
                });
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
        Language::Lua
    }
}

impl Default for LuaExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::AstParser;

    fn parse(code: &str) -> Tree {
        let mut parser = AstParser::new();
        parser
            .parse_with_tree(code, &Language::Lua)
            .expect("lua snippet should parse")
            .0
    }

    #[test]
    fn test_lua_require_statement() {
        let code = "local helper = require(\"helper\")\nlocal path = require(\"pkg.utils\")\n";
        let tree = parse(code);
        let extractor = LuaExtractor::new();
        let imports = extractor.extract_imports(&tree, code);

        assert_eq!(imports.len(), 2);
        assert!(imports.iter().any(|i| i.source == "helper"));
        assert!(imports.iter().any(|i| i.source == "pkg.utils"));
    }

    #[test]
    fn test_lua_require_relative() {
        let code = "local helper = require(\"./helper\")\n";
        let tree = parse(code);
        let extractor = LuaExtractor::new();
        let imports = extractor.extract_imports(&tree, code);

        assert_eq!(imports.len(), 1);
        assert!(imports[0].is_relative);
    }

    #[test]
    fn test_lua_stdlib_require() {
        let code = "local s = require(\"string\")\n";
        let tree = parse(code);
        let extractor = LuaExtractor::new();
        let imports = extractor.extract_imports(&tree, code);

        assert_eq!(imports.len(), 1);
        assert!(imports[0].is_system_header);
    }

    #[test]
    fn test_lua_function_and_method_exports() {
        let code = "local function greet(name)\n  return name\nend\nfunction User:save()\nend\n";
        let tree = parse(code);
        let extractor = LuaExtractor::new();
        let exports = extractor.extract_exports(&tree, code);

        assert!(exports.iter().any(|e| e.target.name == "greet"));
        assert!(exports.iter().any(|e| e.target.name == "save"));
    }

    #[test]
    fn test_lua_plain_calls_are_not_imports() {
        let code = "print(\"hi\")\nhelper.format(\"x\")\n";
        let tree = parse(code);
        let extractor = LuaExtractor::new();
        let imports = extractor.extract_imports(&tree, code);

        assert!(imports.is_empty());
    }
}
