//! AST parser types using tree-sitter.
//!
//! Provides `AstNode` (a portable AST representation) and `AstParser`
//! (a thin wrapper around tree-sitter that converts trees to `AstNode`).

use cce_types::ParseError;
use cce_types::language::Language;
use cce_types::position::Span;
use std::sync::OnceLock;
use tracing::warn;
use tree_sitter::{Node, Parser as TsParser, Tree};

/// Portable AST node representation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct AstNode {
    /// Node kind (e.g. `"function_definition"`, `"class_declaration"`).
    pub kind: String,
    /// Source code text of this node.
    pub text: String,
    /// Byte offsets and line/column positions.
    pub span: Span,
    /// Child nodes.
    pub children: Vec<AstNode>,
}

/// Function signature for resolving a [`Language`] to a tree-sitter
/// `Language`.
pub type LanguageResolverFn = fn(&Language) -> Option<tree_sitter::Language>;

static RESOLVER: OnceLock<LanguageResolverFn> = OnceLock::new();

/// Register the global language resolver.
///
/// Must be called once during init (e.g. in `tree_sitter_init`) before
/// any `AstParser::parse_with_tree` call.
pub fn set_language_resolver(resolver: LanguageResolverFn) {
    let _ = RESOLVER.set(resolver);
}

fn resolve_language(language: &Language) -> Option<tree_sitter::Language> {
    RESOLVER.get().and_then(|f| f(language))
}

/// AST parser using tree-sitter.
pub struct AstParser {
    parser: TsParser,
    /// Custom language resolver (if set, overrides the global resolver).
    custom_resolver: Option<LanguageResolverFn>,
}

impl AstParser {
    /// Create a new AST parser.
    pub fn new() -> Self {
        Self {
            parser: TsParser::new(),
            custom_resolver: None,
        }
    }

    /// Create a new AST parser with a custom language resolver.
    ///
    /// This allows using a different language resolution strategy than the
    /// global resolver (e.g., using `tree_sitter_init::get_tree_sitter_language`).
    pub fn with_resolver(resolver: LanguageResolverFn) -> Self {
        Self {
            parser: TsParser::new(),
            custom_resolver: Some(resolver),
        }
    }

    /// Parse source code and return both the tree-sitter [`Tree`] and an
    /// [`AstNode`] root.
    pub fn parse_with_tree(
        &mut self,
        content: &str,
        language: &Language,
    ) -> Result<(Tree, AstNode), ParseError> {
        // Use custom resolver if set, otherwise fall back to global resolver
        let ts_language = if let Some(resolver) = self.custom_resolver {
            resolver(language)
        } else {
            resolve_language(language)
        }
        .ok_or_else(|| {
            warn!(language = ?language, "Tree-sitter language not found in registry");
            ParseError::ast_parsing(format!(
                "Tree-sitter language not available for {}",
                language
            ))
        })?;

        self.parser
            .set_language(&ts_language)
            .map_err(|e| ParseError::ast_parsing(format!("Failed to set language: {}", e)))?;

        let tree = self
            .parser
            .parse(content, None)
            .ok_or_else(|| ParseError::ast_parsing("Parsing failed".to_string()))?;

        let ast_node = self.convert_node(tree.root_node(), content);

        Ok((tree, ast_node))
    }

    /// Check if a language is supported for AST parsing.
    pub fn is_supported(language: &Language) -> bool {
        matches!(
            language,
            Language::C
                | Language::Cpp
                | Language::CSharp
                | Language::JavaScript
                | Language::TypeScript
                | Language::Rust
                | Language::Go
                | Language::Java
                | Language::Python
                | Language::Dart
                | Language::Scala
                | Language::Kotlin
                | Language::Ruby
                | Language::Php
                | Language::Bash
                | Language::Lua
        )
    }

    fn convert_node(&self, node: Node, source: &str) -> AstNode {
        let kind = node.kind().to_string();
        let start_byte = node.start_byte();
        let end_byte = node.end_byte();
        let start_pos = node.start_position();
        let end_pos = node.end_position();
        let text = node.utf8_text(source.as_bytes()).unwrap_or("").to_string();
        let children: Vec<AstNode> = node
            .children(&mut node.walk())
            .map(|child| self.convert_node(child, source))
            .collect();

        AstNode {
            kind,
            text,
            span: Span::new(
                start_byte,
                end_byte,
                start_pos.row,
                start_pos.column,
                end_pos.row,
                end_pos.column,
            ),
            children,
        }
    }
}

impl Default for AstParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::language::Language;

    #[test]
    fn test_ast_node_default() {
        let node = AstNode::default();
        assert!(node.kind.is_empty());
        assert!(node.children.is_empty());
    }

    #[test]
    fn test_is_supported() {
        assert!(AstParser::is_supported(&Language::C));
        assert!(AstParser::is_supported(&Language::Python));
        assert!(AstParser::is_supported(&Language::Rust));
        assert!(!AstParser::is_supported(&Language::Unknown));
    }
}
