//! Query executor for running Tree-sitter queries
//!
//! Executes Tree-sitter queries on parsed trees and returns structured results.

use super::error::{QueryError, Result};
use crate::tree_sitter_query::loader::{QueryLoader, QueryType};

use cce_types::language::Language;
use tree_sitter::{Query, QueryCursor, Tree};

/// Query match result
#[derive(Debug, Clone)]
pub struct QueryMatch {
    /// Match index
    pub index: usize,
    /// Pattern index
    pub pattern_index: usize,
    /// Captures
    pub captures: Vec<Capture>,
}

/// Capture result
#[derive(Debug, Clone)]
pub struct Capture {
    /// Capture name (e.g., "@entity.function.name")
    pub name: String,
    /// Capture text
    pub text: String,
    /// Start byte position
    pub start_byte: usize,
    /// End byte position
    pub end_byte: usize,
    /// Start point (row, column)
    pub start_point: (usize, usize),
    /// End point (row, column)
    pub end_point: (usize, usize),
}

/// Query executor
///
/// Executes Tree-sitter queries on parsed trees and returns structured results.
pub struct QueryExecutor {
    /// Query loader for compiling and caching queries
    loader: QueryLoader,
}

impl QueryExecutor {
    /// Create a new query executor
    pub fn new() -> Self {
        Self {
            loader: QueryLoader::new(),
        }
    }

    /// Get the query loader
    pub fn loader(&self) -> &QueryLoader {
        &self.loader
    }

    /// Execute entity query
    ///
    /// Extracts entity definitions (functions, classes, structs, etc.) from source code.
    ///
    /// # Arguments
    ///
    /// * `tree` - Parsed tree-sitter tree
    /// * `source` - Source code string
    /// * `language` - Programming language
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<QueryMatch>)` - List of entity matches
    /// * `Err(QueryError)` - If query execution fails
    pub fn execute_entity_query(
        &self,
        tree: &Tree,
        source: &str,
        language: &Language,
    ) -> Result<Vec<QueryMatch>> {
        let query = self.loader.get_entity_query(language)?;
        self.execute_query_internal(&query, tree, source)
    }

    /// Execute call query
    ///
    /// Extracts function call relationships from source code.
    ///
    /// # Arguments
    ///
    /// * `tree` - Parsed tree-sitter tree
    /// * `source` - Source code string
    /// * `language` - Programming language
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<QueryMatch>)` - List of call matches
    /// * `Err(QueryError)` - If query execution fails
    pub fn execute_call_query(
        &self,
        tree: &Tree,
        source: &str,
        language: &Language,
    ) -> Result<Vec<QueryMatch>> {
        let query = self.loader.get_call_query(language)?;
        self.execute_query_internal(&query, tree, source)
    }

    /// Execute control-flow query
    ///
    /// Extracts function-body control-flow captures from source code.
    pub fn execute_control_flow_query(
        &self,
        tree: &Tree,
        source: &str,
        language: &Language,
    ) -> Result<Vec<QueryMatch>> {
        let query = self.loader.get_control_flow_query(language)?;
        self.execute_query_internal(&query, tree, source)
    }

    /// Execute behavior query
    ///
    /// Extracts function-body raw behavior snippets from source code.
    pub fn execute_behavior_query(
        &self,
        tree: &Tree,
        source: &str,
        language: &Language,
    ) -> Result<Vec<QueryMatch>> {
        let query = self.loader.get_behavior_query(language)?;
        self.execute_query_internal(&query, tree, source)
    }

    /// Execute dependency query
    ///
    /// Extracts dependencies (imports, includes, type references) from source code.
    ///
    /// # Arguments
    ///
    /// * `tree` - Parsed tree-sitter tree
    /// * `source` - Source code string
    /// * `language` - Programming language
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<QueryMatch>)` - List of dependency matches
    /// * `Err(QueryError)` - If query execution fails
    pub fn execute_dependency_query(
        &self,
        tree: &Tree,
        source: &str,
        language: &Language,
    ) -> Result<Vec<QueryMatch>> {
        let query = self.loader.get_dependency_query(language)?;
        self.execute_query_internal(&query, tree, source)
    }

    /// Execute comment query
    ///
    /// Extracts comments from source code.
    ///
    /// # Arguments
    ///
    /// * `tree` - Parsed tree-sitter tree
    /// * `source` - Source code string
    /// * `language` - Programming language
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<QueryMatch>)` - List of comment matches
    /// * `Err(QueryError)` - If query execution fails
    pub fn execute_comment_query(
        &self,
        tree: &Tree,
        source: &str,
        language: &Language,
    ) -> Result<Vec<QueryMatch>> {
        let query = self.loader.get_comment_query(language)?;
        self.execute_query_internal(&query, tree, source)
    }

    /// Execute structural query
    ///
    /// Extracts structural containment relationships (e.g., component hierarchy, element containment).
    /// These relationships describe how entities contain other entities beyond simple nesting.
    ///
    /// # Arguments
    ///
    /// * `tree` - Parsed tree-sitter tree
    /// * `source` - Source code string
    /// * `language` - Programming language
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<QueryMatch>)` - List of structural matches with containment captures
    /// * `Err(QueryError)` - If query execution fails
    pub fn execute_structural_query(
        &self,
        tree: &Tree,
        source: &str,
        language: &Language,
    ) -> Result<Vec<QueryMatch>> {
        let query = self.loader.get_query(language, QueryType::Structural)?;
        self.execute_query_internal(&query, tree, source)
    }

    /// Execute a query and return matches
    ///
    /// # Arguments
    ///
    /// * `query` - Tree-sitter query to execute
    /// * `tree` - Parsed tree-sitter tree
    /// * `source` - Source code string
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<QueryMatch>)` - List of query matches
    /// * `Err(QueryError)` - If query execution fails
    pub fn execute_query(
        &self,
        query: &Query,
        tree: &Tree,
        source: &str,
    ) -> Result<Vec<QueryMatch>> {
        self.execute_query_internal(query, tree, source)
    }

    /// Internal method to execute a query and return matches
    fn execute_query_internal(
        &self,
        query: &Query,
        tree: &Tree,
        source: &str,
    ) -> Result<Vec<QueryMatch>> {
        use streaming_iterator::StreamingIterator;

        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(query, tree.root_node(), source.as_bytes());

        let mut results = Vec::new();

        while let Some(mat) = matches.next() {
            let index = results.len();
            let mut captures = Vec::new();

            for capture in mat.captures {
                let node = capture.node;

                // Get capture name
                let capture_name = query.capture_names()[capture.index as usize].to_string();

                // Get node text
                let text = node
                    .utf8_text(source.as_bytes())
                    .map_err(|e| {
                        QueryError::InvalidQuery(format!("Failed to extract text: {}", e))
                    })?
                    .to_string();

                // Get capture details
                let start_byte = node.start_byte();
                let end_byte = node.end_byte();
                let start_point = (node.start_position().row, node.start_position().column);
                let end_point = (node.end_position().row, node.end_position().column);

                captures.push(Capture {
                    name: capture_name,
                    text,
                    start_byte,
                    end_byte,
                    start_point,
                    end_point,
                });
            }

            results.push(QueryMatch {
                index,
                pattern_index: mat.pattern_index,
                captures,
            });
        }

        Ok(results)
    }

    /// Execute query and filter by capture name
    ///
    /// Returns only captures that match the specified capture name pattern.
    ///
    /// # Arguments
    ///
    /// * `query_type` - Type of query to execute
    /// * `tree` - Parsed tree-sitter tree
    /// * `source` - Source code string
    /// * `language` - Programming language
    /// * `capture_pattern` - Capture name pattern to filter (e.g., "entity.function.name")
    ///
    /// # Returns
    ///
    /// * `Ok<Vec<Capture>)` - List of matching captures
    /// * `Err(QueryError)` - If query execution fails
    pub fn execute_query_with_capture_filter(
        &self,
        query_type: QueryType,
        tree: &Tree,
        source: &str,
        language: &Language,
        capture_pattern: &str,
    ) -> Result<Vec<Capture>> {
        let matches = match query_type {
            QueryType::Entity => self.execute_entity_query(tree, source, language)?,
            QueryType::Call => self.execute_call_query(tree, source, language)?,
            QueryType::ControlFlow => self.execute_control_flow_query(tree, source, language)?,
            QueryType::Behavior => self.execute_behavior_query(tree, source, language)?,
            QueryType::Dependency => self.execute_dependency_query(tree, source, language)?,
            QueryType::Comment => self.execute_comment_query(tree, source, language)?,
            QueryType::Embedded => {
                return Err(QueryError::InvalidQuery(
                    "Embedded query type does not support capture filtering".to_string(),
                ));
            }
            QueryType::Structural => self.execute_structural_query(tree, source, language)?,
        };

        let mut filtered = Vec::new();

        for mat in matches {
            for capture in mat.captures {
                if capture.name.contains(capture_pattern) {
                    filtered.push(capture);
                }
            }
        }

        Ok(filtered)
    }

    /// Extract capture text by name from query results
    ///
    /// Returns the text of the first capture that matches the given name.
    ///
    /// # Arguments
    ///
    /// * `matches` - Query match results
    /// * `capture_name` - Name of the capture to extract
    ///
    /// # Returns
    ///
    /// * `Some(String)` - Text of the first matching capture
    /// * `None` - If no capture matches the name
    pub fn extract_capture_text(matches: &[QueryMatch], capture_name: &str) -> Option<String> {
        for mat in matches {
            for capture in &mat.captures {
                if capture.name == capture_name {
                    return Some(capture.text.clone());
                }
            }
        }
        None
    }

    /// Extract all capture texts by name from query results
    ///
    /// Returns all texts of captures that match the given name.
    ///
    /// # Arguments
    ///
    /// * `matches` - Query match results
    /// * `capture_name` - Name of the captures to extract
    ///
    /// # Returns
    ///
    /// * `Vec<String>` - List of texts from all matching captures
    pub fn extract_all_capture_texts(matches: &[QueryMatch], capture_name: &str) -> Vec<String> {
        let mut texts = Vec::new();

        for mat in matches {
            for capture in &mat.captures {
                if capture.name == capture_name {
                    texts.push(capture.text.clone());
                }
            }
        }

        texts
    }
}

impl Default for QueryExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast_parser::AstParser;

    #[test]
    fn test_create_executor() {
        let executor = QueryExecutor::new();
        // Just verify that we can create an executor and access the loader
        let _stats = executor.loader().cache_stats();
        // Cache may or may not be empty depending on other tests
    }

    #[test]
    fn test_execute_c_entity_query() {
        let executor = QueryExecutor::new();
        let mut ast_parser = AstParser::new();
        let code = r#"
int main() {
    return 0;
}
"#;

        let tree = ast_parser
            .parse_with_tree(code, &Language::C)
            .expect("Failed to parse C code")
            .0;

        let result = executor.execute_entity_query(&tree, code, &Language::C);
        assert!(result.is_ok());

        let matches = result.expect("Failed to execute entity query");
        // Should capture the function definition
        assert!(!matches.is_empty());
    }

    #[test]
    fn test_execute_c_call_query() {
        let executor = QueryExecutor::new();
        let mut ast_parser = AstParser::new();
        let code = r#"
int foo() {
    return 1;
}

int main() {
    int x = foo();
    return 0;
}
"#;

        let tree = ast_parser
            .parse_with_tree(code, &Language::C)
            .expect("Failed to parse C code")
            .0;

        let result = executor.execute_call_query(&tree, code, &Language::C);
        assert!(result.is_ok());

        let matches = result.expect("Failed to execute call query");
        // Should capture the function call
        assert!(!matches.is_empty());
    }

    #[test]
    fn test_execute_rust_control_flow_query() {
        let executor = QueryExecutor::new();
        let mut ast_parser = AstParser::new();
        let code = r#"
fn demo(input: Option<i32>) -> Result<i32, ()> {
    if let Some(v) = input {
        return Ok(v);
    }

    match input {
        Some(v) => return Ok(v),
        None => {}
    }

    for i in 0..3 {
        if i == 1 {
            continue;
        }
        break;
    }

    loop {
        return Err(());
    }
}
"#;

        let tree = ast_parser
            .parse_with_tree(code, &Language::Rust)
            .expect("Failed to parse Rust code")
            .0;

        let result = executor.execute_control_flow_query(&tree, code, &Language::Rust);
        assert!(result.is_ok());

        let matches = result.expect("Failed to execute control flow query");
        assert!(
            !matches.is_empty(),
            "Control-flow query should capture Rust control-flow structures"
        );
    }

    #[test]
    fn test_execute_rust_behavior_query() {
        let executor = QueryExecutor::new();
        let mut ast_parser = AstParser::new();
        let code = r#"
fn demo(input: Option<i32>) -> Result<i32, ()> {
    let mut value = 0;
    let _ = &value;
    let _ = maybe_result()?;
    let _ = 1 << 2;

    Ok(value)
}
"#;

        let tree = ast_parser
            .parse_with_tree(code, &Language::Rust)
            .expect("Failed to parse Rust code")
            .0;

        let result = executor.execute_behavior_query(&tree, code, &Language::Rust);
        assert!(result.is_ok());

        let matches = result.expect("Failed to execute behavior query");
        assert!(
            !matches.is_empty(),
            "Behavior query should capture Rust function-body behavior"
        );
    }

    #[test]
    fn test_execute_c_dependency_query() {
        let executor = QueryExecutor::new();
        let mut ast_parser = AstParser::new();
        let code = r#"
#include <stdio.h>
#include "utils.h"

int main() {
    return 0;
}
"#;

        let tree = ast_parser
            .parse_with_tree(code, &Language::C)
            .expect("Failed to parse C code")
            .0;

        let result = executor.execute_dependency_query(&tree, code, &Language::C);
        assert!(result.is_ok());

        let matches = result.expect("Failed to execute dependency query");
        // Should capture the include directives
        assert!(!matches.is_empty());
    }

    #[test]
    fn test_extract_capture_text() {
        let matches = vec![QueryMatch {
            index: 0,
            pattern_index: 0,
            captures: vec![Capture {
                name: "@entity.function.name".to_string(),
                text: "main".to_string(),
                start_byte: 0,
                end_byte: 4,
                start_point: (0, 0),
                end_point: (0, 4),
            }],
        }];

        let text = QueryExecutor::extract_capture_text(&matches, "@entity.function.name");
        assert_eq!(text, Some("main".to_string()));

        let text = QueryExecutor::extract_capture_text(&matches, "@nonexistent");
        assert_eq!(text, None);
    }

    #[test]
    fn test_extract_all_capture_texts() {
        let matches = vec![
            QueryMatch {
                index: 0,
                pattern_index: 0,
                captures: vec![Capture {
                    name: "@entity.function.name".to_string(),
                    text: "foo".to_string(),
                    start_byte: 0,
                    end_byte: 3,
                    start_point: (0, 0),
                    end_point: (0, 3),
                }],
            },
            QueryMatch {
                index: 1,
                pattern_index: 0,
                captures: vec![Capture {
                    name: "@entity.function.name".to_string(),
                    text: "bar".to_string(),
                    start_byte: 10,
                    end_byte: 13,
                    start_point: (2, 0),
                    end_point: (2, 3),
                }],
            },
        ];

        let texts = QueryExecutor::extract_all_capture_texts(&matches, "@entity.function.name");
        assert_eq!(texts.len(), 2);
        assert!(texts.contains(&"foo".to_string()));
        assert!(texts.contains(&"bar".to_string()));
    }
}
