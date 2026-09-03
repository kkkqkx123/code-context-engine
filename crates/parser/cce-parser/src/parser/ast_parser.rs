//! AST parser using tree-sitter
//!
//! Parses source code into AST using language-specific tree-sitter grammars.
//! Supports C, C++, C#, JavaScript, TypeScript, Rust, Go, Java, and Python.
//!
//! # Performance Optimization
//!
//! Reuses the global language cache from utils::tree_sitter_init to avoid redundant initialization.
//! Each programming language has exactly one tree-sitter Language instance shared across
//! all AstParser instances.

use crate::tree_sitter_init;
use cce_types::language::Language;

// Re-export AstNode from cce_parser_core to avoid duplication
pub use cce_parser_core::AstNode;

/// AST parser using tree-sitter
///
/// Manages language-specific parser instances and converts
/// tree-sitter trees to our AstNode structure.
///
/// This is a thin wrapper around `cce_parser_core::AstParser` that uses
/// `tree_sitter_init::get_tree_sitter_language` as the language resolver.
pub struct AstParser {
    inner: cce_parser_core::AstParser,
}

impl AstParser {
    /// Create a new AST parser
    pub fn new() -> Self {
        Self {
            inner: cce_parser_core::AstParser::with_resolver(
                tree_sitter_init::get_tree_sitter_language,
            ),
        }
    }

    /// Parse and return both tree and AstNode
    ///
    /// This is useful when you need the tree-sitter tree for queries
    /// and the AstNode for other operations.
    pub fn parse_with_tree(
        &mut self,
        content: &str,
        language: &Language,
    ) -> Result<(tree_sitter::Tree, AstNode), cce_types::ParseError> {
        self.inner.parse_with_tree(content, language)
    }

    /// Check if a language is supported for AST parsing
    pub fn is_supported(language: &Language) -> bool {
        cce_parser_core::AstParser::is_supported(language)
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

    #[test]
    fn test_parse_with_tree_c_code() {
        let mut parser = AstParser::new();
        let code = r#"
int main() {
    return 0;
}
"#;
        let result = parser.parse_with_tree(code, &Language::C);
        assert!(result.is_ok());

        let (_tree, ast) = result.expect("Failed to parse");
        assert!(!ast.children.is_empty());
    }

    #[test]
    fn test_parse_with_tree_python_code() {
        let mut parser = AstParser::new();
        let code = r#"
def hello():
    print("Hello, World!")
"#;
        let result = parser.parse_with_tree(code, &Language::Python);
        assert!(result.is_ok());

        let (_tree, ast) = result.expect("Failed to parse");
        assert!(!ast.children.is_empty());
    }

    #[test]
    fn test_parse_with_tree_rust_code() {
        let mut parser = AstParser::new();
        let code = r#"
fn main() {
    
}
"#;
        let result = parser.parse_with_tree(code, &Language::Rust);
        assert!(result.is_ok());

        let (_tree, ast) = result.expect("Failed to parse");
        assert!(!ast.children.is_empty());
    }

    #[test]
    fn test_is_supported() {
        assert!(AstParser::is_supported(&Language::C));
        assert!(AstParser::is_supported(&Language::Python));
        assert!(AstParser::is_supported(&Language::Rust));
        assert!(!AstParser::is_supported(&Language::Unknown));
    }
}
