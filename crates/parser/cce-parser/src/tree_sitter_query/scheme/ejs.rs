//! EJS (Embedded JavaScript Templates) language query schemes
//!
//! Provides Tree-sitter query patterns for identifying and analyzing
//! EJS template entities, embedded blocks, and dependencies.
//!
//! The `tree-sitter-embedded-template` grammar splits EJS files into:
//! - `content` nodes (raw HTML text)
//! - `directive` nodes (`<% code %>`)
//! - `output_directive` nodes (`<%= expr %>`)
//! - `comment_directive` nodes (`<%# comment %>`)
//!
//! HTML and JavaScript entities are extracted via injection queries
//! (content → HTML grammar, code → JavaScript grammar).

/// Get entity query for EJS
///
/// The embedded-template grammar treats HTML as raw `content` nodes.
/// HTML entity extraction relies on injection queries, so this query
/// is intentionally minimal.
pub fn entity_query() -> &'static str {
    ""
}

/// Get comment query for EJS
///
/// Matches `comment_directive` nodes (`<%# ... %>`).
pub fn comment_query() -> &'static str {
    r#"
; ============================================
; EJS Comments
; ============================================

(comment_directive) @comment.block
"#
}

/// Get dependency query for EJS
///
/// The `code` nodes inside EJS directives are raw text, not parsed JavaScript AST.
/// Dependency resolution (e.g., `include()` calls) must happen at a higher level
/// after the JavaScript code blocks are parsed. This query is intentionally empty.
pub fn dependency_query() -> &'static str {
    ""
}

/// Get embedded block query for EJS
///
/// Matches directive and output_directive nodes as embedded script blocks.
/// The `code` child contains the raw JavaScript source.
pub fn embedded_block_query() -> &'static str {
    r#"
; ============================================
; Embedded Script Block (directive: <% code %>)
; ============================================

(directive
  (code)? @embedded.script.content
) @embedded.script

; ============================================
; Embedded Output Block (output_directive: <%= expr %>)
; ============================================

(output_directive
  (code)? @embedded.script.content
) @embedded.script
"#
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Query;

    fn validate_query_syntax(query_name: &str, query_str: &str) -> Result<(), String> {
        if query_str.is_empty() {
            return Ok(());
        }
        let lang = tree_sitter_embedded_template::LANGUAGE;
        match Query::new(&lang.into(), query_str) {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("Query '{}' syntax error: {:?}", query_name, e)),
        }
    }

    #[test]
    fn test_comment_query_syntax_valid() {
        let result = validate_query_syntax("comment_query", comment_query());
        assert!(
            result.is_ok(),
            "Comment query syntax validation failed: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_dependency_query_syntax_valid() {
        let result = validate_query_syntax("dependency_query", dependency_query());
        assert!(
            result.is_ok(),
            "Dependency query syntax validation failed: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_embedded_block_query_syntax_valid() {
        let result = validate_query_syntax("embedded_block_query", embedded_block_query());
        assert!(
            result.is_ok(),
            "Embedded block query syntax validation failed: {:?}",
            result.err()
        );
    }
}
