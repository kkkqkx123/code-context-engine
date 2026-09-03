//! Bash language query schemes
//!
//! Provides Tree-sitter query patterns for identifying and analyzing
//! Bash shell script code entities, call relationships, and dependencies.

use super::common;

/// Get entity query for Bash
///
/// Returns Tree-sitter query patterns for identifying Bash code entities:
/// - Function definitions
/// - Variable assignments (environment and local)
pub fn entity_query() -> &'static str {
    r#"
; ============================================
; Function Definitions
; ============================================

; Function definition: name() { ... }
(function_definition
  name: (word) @entity.function.name
  body: (compound_statement) @entity.function.body
) @entity.function

; ============================================
; Variable Assignments
; ============================================

; Variable assignment: VAR=value
(variable_assignment
  name: (variable_name) @entity.variable.name
  value: (_)? @entity.variable.value
) @entity.variable
"#
}

/// Get call query for Bash
///
/// Returns Tree-sitter query patterns for identifying Bash function calls
/// and command invocations.
pub fn call_query() -> &'static str {
    r#"
; ============================================
; Command Calls
; ============================================

; Simple command (function/program call)
(command
  name: (command_name) @call.function.name
) @call.function
"#
}

/// Get dependency query for Bash
///
/// Returns Tree-sitter query patterns for identifying Bash dependencies:
/// - Source/include statements (source file.sh or . file.sh)
/// - Export declarations
pub fn dependency_query() -> &'static str {
    r#"
; ============================================
; Source Dependencies (source file.sh or . file.sh)
; ============================================

; source command: source file.sh
(command
  name: (command_name) @dependency.source.keyword
  argument: (word) @dependency.source.path
  (#eq? @dependency.source.keyword "source")
) @dependency.source

; source command: . file.sh
(command
  name: (command_name) @dependency.dot.keyword
  argument: (word) @dependency.dot.path
  (#eq? @dependency.dot.keyword ".")
) @dependency.dot

; ============================================
; Export Declarations
; ============================================

; export command: export VAR or export VAR=value
(declaration_command
  (variable_assignment
    name: (variable_name) @dependency.export.name
  ) @dependency.export
)

; ============================================
; File Redirect Dependencies
; ============================================

; Input/Output redirect: command < file or command > file
(file_redirect
  (word) @dependency.source.path
) @dependency.source

; Heredoc redirect: command <<EOF
(heredoc_redirect
  (heredoc_end) @dependency.source.path
) @dependency.source
"#
}

/// Get behavior query for Bash
pub fn behavior_query() -> String {
    let mut query = String::from(
        r#"
(variable_assignment) @behavior.data.bind
(declaration_command) @behavior.data.bind
(command) @behavior.data.statement
"#,
    );
    query.push_str(&common::bitwise_shift_operator_query("binary_expression"));
    query
}

/// Get control-flow query for Bash
pub fn control_flow_query() -> &'static str {
    r#"
(if_statement) @control.flow.if
(for_statement) @control.flow.loop
(while_statement) @control.flow.loop
(case_statement) @control.flow.match
"#
}

/// Get comment query for Bash
///
/// Returns Tree-sitter query patterns for identifying Bash comments.
pub fn comment_query() -> &'static str {
    common::comment_query()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Query;

    fn validate_query_syntax(query_name: &str, query_str: &str) -> Result<(), String> {
        let lang = tree_sitter_bash::LANGUAGE;
        match Query::new(&lang.into(), query_str) {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("Query '{}' syntax error: {:?}", query_name, e)),
        }
    }

    #[test]
    fn test_entity_query_syntax_valid() {
        let result = validate_query_syntax("entity_query", entity_query());
        assert!(
            result.is_ok(),
            "Entity query syntax validation failed: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_call_query_syntax_valid() {
        let result = validate_query_syntax("call_query", call_query());
        assert!(
            result.is_ok(),
            "Call query syntax validation failed: {:?}",
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
    fn test_comment_query_syntax_valid() {
        let result = validate_query_syntax("comment_query", comment_query());
        assert!(
            result.is_ok(),
            "Comment query syntax validation failed: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_behavior_query_syntax_valid() {
        let result = validate_query_syntax("behavior_query", &behavior_query());
        assert!(
            result.is_ok(),
            "Behavior query syntax validation failed: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_control_flow_query_syntax_valid() {
        let result = validate_query_syntax("control_flow_query", control_flow_query());
        assert!(
            result.is_ok(),
            "Control-flow query syntax validation failed: {:?}",
            result.err()
        );
    }
}
