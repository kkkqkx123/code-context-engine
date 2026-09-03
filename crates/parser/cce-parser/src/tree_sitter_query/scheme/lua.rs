//! Lua language query schemes
//!
//! Provides Tree-sitter query patterns for identifying and analyzing
//! Lua code entities, call relationships, and dependencies.

use super::common;

/// Get entity query for Lua
///
/// Returns Tree-sitter query patterns for identifying Lua code entities:
/// - Function declarations
/// - Table fields with function values
/// - Variable assignments with function definitions
pub fn entity_query() -> &'static str {
    r#"
; ============================================
; Requires
; ============================================

; require('module_name')
(function_call
  name: (identifier) @entity.require
  arguments: (arguments
    .
    (string) @entity.require.name)
  (#eq? @entity.require "require")
) @entity.require

; ============================================
; Function Declarations
; ============================================

; Function declaration: function name(...) ... end
(function_declaration
  name: [
    (identifier) @entity.function.name
    (dot_index_expression
      field: (identifier) @entity.function.name)
  ]
  parameters: (parameters)? @entity.function.params
  body: (block)? @entity.function.body
) @entity.function

; Method declaration: function obj:method(...) ... end
(function_declaration
  name: (method_index_expression
    method: (identifier) @entity.method.name)
  parameters: (parameters)? @entity.method.params
  body: (block)? @entity.method.body
) @entity.method

; ============================================
; Anonymous Function Assignments
; ============================================

; Assignment: local func = function(...) ... end
(assignment_statement
  (variable_list
    .
    (identifier) @entity.variable.name)
  (expression_list
    .
    value: (function_definition
      parameters: (parameters)? @entity.function.params
      body: (block)? @entity.function.body
    )
  )
) @entity.function

; ============================================
; Local Variable Assignments
; ============================================

; Local variable with value: local x = value (non-function, non-table values)
(variable_declaration
  (assignment_statement
    (variable_list
      .
      (identifier) @entity.variable.name)
    (expression_list
      .
      (_) @entity.variable.value)
  )
  (#not-type? @entity.variable.value function_definition table_constructor)
) @entity.variable

; Local variable without value: local x
(variable_declaration
  (variable_list
    (identifier) @entity.variable.name)
) @entity.variable

; ============================================
; Table Constructor Fields (Function Values)
; ============================================

; Table field: { name = function(...) ... end }
(table_constructor
  (field
    name: (identifier) @entity.field.name
    value: (function_definition
      parameters: (parameters)? @entity.field.params
      body: (block)? @entity.field.body
    )
  ) @entity.field
)

; ============================================
; Table Constructor
; ============================================

; Table literal: { ... }
(assignment_statement
  (variable_list
    .
    (identifier) @entity.table.name)
  (expression_list
    .
    value: (table_constructor) @entity.table.body)
) @entity.table

; ============================================
; Field Access Expressions
; ============================================

; Chained table field: a.b.c as entity expression
(dot_index_expression
  field: (identifier) @entity.field.name
) @entity.field
"#
}

/// Get call query for Lua
///
/// Returns Tree-sitter query patterns for identifying Lua function calls.
pub fn call_query() -> &'static str {
    r#"
; ============================================
; Function Calls
; ============================================

; Direct function call: name(args)
(function_call
  name: [
    (identifier) @call.function.name
    (dot_index_expression
      field: (identifier) @call.function.name)
    (method_index_expression
      method: (identifier) @call.function.name)
  ]
) @call.function
"#
}

/// Get dependency query for Lua
///
/// Returns Tree-sitter query patterns for identifying Lua dependencies:
/// - require calls
/// - dofile/loadfile calls
pub fn dependency_query() -> &'static str {
    r#"
; ============================================
; Module Imports (require)
; ============================================

; require('module_name') or require "module_name"
(function_call
  name: (identifier) @dependency.import.keyword
  arguments: (arguments
    .
    (string) @dependency.import.path)
  (#eq? @dependency.import.keyword "require")
) @dependency.import

; ============================================
; File Includes (dofile/loadfile)
; ============================================

; dofile("path") or dofile 'path'
(function_call
  name: (identifier) @dependency.include.keyword
  arguments: (arguments
    .
    (string) @dependency.include.path)
  (#match? @dependency.include.keyword "^(dofile|loadfile)$")
) @dependency.include
"#
}

/// Get behavior query for Lua
pub fn behavior_query() -> String {
    let mut query = String::from(
        r#"
(assignment_statement) @behavior.data.bind
(variable_declaration) @behavior.data.bind
(attribute) @behavior.data.reference
(function_call) @behavior.data.statement
"#,
    );
    query.push_str(&super::common::bitwise_shift_operator_query(
        "binary_expression",
    ));
    query
}

/// Get control-flow query for Lua
pub fn control_flow_query() -> &'static str {
    r#"
(if_statement) @control.flow.if
(for_statement) @control.flow.loop
(while_statement) @control.flow.loop
(do_statement) @control.flow.loop
(repeat_statement) @control.flow.loop
(return_statement) @control.flow.return
(break_statement) @control.flow.break
"#
}

/// Get comment query for Lua
///
/// Returns Tree-sitter query patterns for identifying Lua comments.
pub fn comment_query() -> &'static str {
    common::comment_query()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Query;

    fn validate_query_syntax(query_name: &str, query_str: &str) -> Result<(), String> {
        let lang = tree_sitter_lua::LANGUAGE;
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

    #[test]
    fn test_comment_query_syntax_valid() {
        let result = validate_query_syntax("comment_query", comment_query());
        assert!(
            result.is_ok(),
            "Comment query syntax validation failed: {:?}",
            result.err()
        );
    }
}
