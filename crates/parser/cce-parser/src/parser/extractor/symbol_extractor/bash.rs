//! Bash language import/export extractor
//!
//! Extracts `source`/`.` load statements as imports and exposes top-level
//! function definitions and variable assignments as exports.

use super::common::helpers::string::unquote;
use super::traits::SymbolExtractor;
use crate::tree_sitter_query::{find_descendants_by_kind, node_text, node_to_span};
use cce_types::import::{
    ExportKind, ExportTarget, ImportKind, ImportTarget, StandardizedExport, StandardizedImport,
    TargetKind,
};
use cce_types::language::Language;
use tree_sitter::{Node, Tree};

/// Bash language import/export extractor
pub struct BashExtractor;

impl BashExtractor {
    pub fn new() -> Self {
        Self
    }

    /// Check whether a sourced path is relative (anything not absolute).
    fn is_relative_path(path: &str) -> bool {
        !path.starts_with('/')
    }

    /// Extract the text of a command argument node, unwrapping quotes.
    fn argument_text(node: &Node, source: &str) -> Option<String> {
        // `string` nodes wrap their content in a `string_content` child.
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

    /// Find the first path argument of a `source`/`.` command node.
    fn source_path(command: &Node, source: &str) -> Option<String> {
        let mut cursor = command.walk();
        for child in command.children(&mut cursor) {
            if !child.is_named() || child.kind() == "command_name" {
                continue;
            }
            match child.kind() {
                "word" | "string" | "raw_string" | "concatenation" | "number" => {
                    if let Some(path) = Self::argument_text(&child, source) {
                        if !path.is_empty() {
                            return Some(path);
                        }
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// Check whether a command node invokes `source` or `.`.
    fn is_source_command(command: &Node, source: &str) -> bool {
        command
            .child_by_field_name("name")
            .map(|name| node_text(&name, source))
            .is_some_and(|name| name == "source" || name == ".")
    }
}

impl SymbolExtractor for BashExtractor {
    fn extract_imports(&self, tree: &Tree, source: &str) -> Vec<StandardizedImport> {
        let mut imports = Vec::new();
        let root_node = tree.root_node();

        for command in find_descendants_by_kind(&root_node, "command") {
            if !Self::is_source_command(&command, source) {
                continue;
            }
            if let Some(path) = Self::source_path(&command, source) {
                let is_relative = Self::is_relative_path(&path);
                imports.push(StandardizedImport {
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
                    span: Some(node_to_span(&command)),
                });
            }
        }

        imports
    }

    fn extract_exports(&self, tree: &Tree, source: &str) -> Vec<StandardizedExport> {
        let mut exports = Vec::new();
        let root_node = tree.root_node();

        // Function definitions are the primary cross-file symbols.
        for func in find_descendants_by_kind(&root_node, "function_definition") {
            if let Some(name_node) = func.child_by_field_name("name") {
                let name = node_text(&name_node, source);
                if !name.is_empty() {
                    exports.push(StandardizedExport {
                        kind: ExportKind::Named,
                        target: ExportTarget {
                            name,
                            original_name: None,
                            kind: TargetKind::Function,
                            source_module: None,
                        },
                        is_reexport: false,
                        span: Some(node_to_span(&func)),
                    });
                }
            }
        }

        // Top-level variable assignments are visible to sourced files.
        for assignment in find_descendants_by_kind(&root_node, "variable_assignment") {
            if let Some(name_node) = assignment.child_by_field_name("name") {
                let name = node_text(&name_node, source);
                if !name.is_empty() {
                    exports.push(StandardizedExport {
                        kind: ExportKind::Named,
                        target: ExportTarget {
                            name,
                            original_name: None,
                            kind: TargetKind::Variable,
                            source_module: None,
                        },
                        is_reexport: false,
                        span: Some(node_to_span(&assignment)),
                    });
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
        Language::Bash
    }
}

impl Default for BashExtractor {
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
            .parse_with_tree(code, &Language::Bash)
            .expect("bash snippet should parse")
            .0
    }

    #[test]
    fn test_bash_source_statement() {
        let tree = parse("source ./lib.sh\nsource /opt/common.sh\n");
        let extractor = BashExtractor::new();
        let imports = extractor.extract_imports(&tree, "source ./lib.sh\nsource /opt/common.sh\n");

        assert_eq!(imports.len(), 2);
        assert!(
            imports
                .iter()
                .any(|i| i.source == "./lib.sh" && i.is_relative)
        );
        assert!(
            imports
                .iter()
                .any(|i| i.source == "/opt/common.sh" && !i.is_relative)
        );
    }

    #[test]
    fn test_bash_dot_statement() {
        let code = ". ./helpers.sh\n";
        let tree = parse(code);
        let extractor = BashExtractor::new();
        let imports = extractor.extract_imports(&tree, code);

        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].source, "./helpers.sh");
        assert!(imports[0].is_relative);
    }

    #[test]
    fn test_bash_quoted_source_path() {
        let code = "source \"$BASE/lib.sh\"\n";
        let tree = parse(code);
        let extractor = BashExtractor::new();
        let imports = extractor.extract_imports(&tree, code);

        assert_eq!(imports.len(), 1);
        assert!(!imports[0].source.contains('"'));
    }

    #[test]
    fn test_bash_function_and_variable_exports() {
        let code = "greet() {\n  echo \"hi $1\"\n}\nCOUNT=3\n";
        let tree = parse(code);
        let extractor = BashExtractor::new();
        let exports = extractor.extract_exports(&tree, code);

        assert!(
            exports
                .iter()
                .any(|e| e.target.name == "greet" && e.target.kind == TargetKind::Function)
        );
        assert!(
            exports
                .iter()
                .any(|e| e.target.name == "COUNT" && e.target.kind == TargetKind::Variable)
        );
    }

    #[test]
    fn test_bash_plain_commands_are_not_imports() {
        let code = "greet \"bob\"\necho done\n";
        let tree = parse(code);
        let extractor = BashExtractor::new();
        let imports = extractor.extract_imports(&tree, code);

        assert!(imports.is_empty());
    }
}
