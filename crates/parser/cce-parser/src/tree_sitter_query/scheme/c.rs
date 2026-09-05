//! C language query schemes
//!
//! Provides Tree-sitter query patterns for identifying and analyzing
//! C code entities, call relationships, and dependencies.

use super::common;

/// Get entity query for C
///
/// Returns Tree-sitter query patterns for identifying C code entities:
/// - Type definitions (struct, union, enum, typedef)
/// - Functions (definitions, prototypes, pointers)
/// - Variables (normal, array, pointer)
/// - Entity components (fields, parameters)
/// - Preprocessor directives (macros)
/// - Other constructs (attributes, labels, comments)
pub fn entity_query() -> &'static str {
    r#"
; ============================================
; 1. Types
; ============================================

; Struct definition
(struct_specifier
  name: (type_identifier) @entity.struct.name
  body: (field_declaration_list) @entity.struct.body
) @entity.struct

; Anonymous struct definition
(struct_specifier
  body: (field_declaration_list) @entity.struct_anon.body
) @entity.struct_anon

; Union definition
(union_specifier
  name: (type_identifier) @entity.union.name
  body: (field_declaration_list) @entity.union.body
) @entity.union

; Anonymous union definition
(union_specifier
  body: (field_declaration_list) @entity.union_anon.body
) @entity.union_anon

; Enum definition
(enum_specifier
  name: (type_identifier) @entity.enum.name
  body: (enumerator_list) @entity.enum.body
) @entity.enum

; Anonymous enum definition
(enum_specifier
  body: (enumerator_list) @entity.enum_anon.body
) @entity.enum_anon

; Enum member (enumerator constant, e.g. RED in enum Color { RED, GREEN = 2 })
(enumerator
  name: (identifier) @entity.enum_member.name
) @entity.enum_member

; typedef type definition
(type_definition
  type: (_) @entity.typedef.original_type
  declarator: (type_identifier) @entity.typedef.name
) @entity.typedef

; typedef struct definition
(type_definition
  type: (struct_specifier
    name: (type_identifier)? @entity.typedef_struct.original_name
    body: (field_declaration_list) @entity.typedef_struct.body
  )
  declarator: (type_identifier) @entity.typedef_struct.name
) @entity.typedef_struct

; typedef union definition
(type_definition
  type: (union_specifier
    name: (type_identifier)? @entity.typedef_union.original_name
    body: (field_declaration_list) @entity.typedef_union.body
  )
  declarator: (type_identifier) @entity.typedef_union.name
) @entity.typedef_union

; typedef enum definition
(type_definition
  type: (enum_specifier
    name: (type_identifier)? @entity.typedef_enum.original_name
    body: (enumerator_list) @entity.typedef_enum.body
  )
  declarator: (type_identifier) @entity.typedef_enum.name
) @entity.typedef_enum

; typedef function pointer definition
(type_definition
  type: (pointer_declarator
    declarator: (function_declarator
      parameters: (parameter_list) @entity.typedef_function_pointer.params
    )
  )
  declarator: (type_identifier) @entity.typedef_function_pointer.name
) @entity.typedef_function_pointer

; ============================================
; 2. Functions
; ============================================

; Function definition
(function_definition
  type: (_) @entity.function.return_type
  declarator: (function_declarator
    declarator: (identifier) @entity.function.name
    parameters: (parameter_list) @entity.function.params
  )
  body: (compound_statement) @entity.function.body
) @entity.function

; Function declaration (prototype)
(declaration
  type: (_) @entity.function.prototype.return_type
  declarator: (function_declarator
    declarator: (identifier) @entity.function.prototype.name
    parameters: (parameter_list) @entity.function.prototype.params
  )
) @entity.function.prototype

; Function pointer declaration
(declaration
  type: (_) @entity.function.pointer.return_type
  declarator: (pointer_declarator
    declarator: (function_declarator
      declarator: (identifier) @entity.function.pointer.name
      parameters: (parameter_list) @entity.function.pointer.params
    )
  )
) @entity.function.pointer

; Function pointer array declaration
(declaration
  type: (_) @entity.function.pointer_array.return_type
  declarator: (array_declarator
    declarator: (pointer_declarator
      declarator: (function_declarator
        declarator: (identifier) @entity.function.pointer_array.name
        parameters: (parameter_list) @entity.function.pointer_array.params
      )
    )
    size: (_) @entity.function.pointer_array.size
  )
) @entity.function.pointer_array

; ============================================
; 3. Variables
; ============================================

; Normal variable declaration (including global, static, extern, const)
(declaration
  type: (_) @entity.variable.normal.type
  declarator: (identifier) @entity.variable.normal.name
  value: (_) @entity.variable.normal.value?
) @entity.variable.normal

; Initialized variable declaration, e.g. `int x = 1;`
(declaration
  type: (_) @entity.variable.init.type
  declarator: (init_declarator
    declarator: (identifier) @entity.variable.init.name
    value: (_) @entity.variable.init.value
  )
) @entity.variable.init

; Array declaration (single and multi-dimensional)
(declaration
  type: (_) @entity.variable.array.type
  declarator: (array_declarator
    declarator: (identifier) @entity.variable.array.name
    size: (_) @entity.variable.array.size
  )
  value: (_) @entity.variable.array.value?
) @entity.variable.array

; Pointer declaration (single and multi-level)
(declaration
  type: (_) @entity.variable.pointer.type
  declarator: (pointer_declarator
    declarator: (identifier) @entity.variable.pointer.name
  )
  value: (_) @entity.variable.pointer.value?
) @entity.variable.pointer

; ============================================
; 4. Entity Components
; ============================================

; Struct field declaration
(field_declaration
  type: (_) @entity.field.type
  declarator: (field_identifier) @entity.field.name
) @entity.field

; Struct bit field
(field_declaration
  type: (_) @entity.bitfield.type
  declarator: (field_identifier) @entity.bitfield.name
) @entity.bitfield

; ============================================
; 5. Preprocessor
; ============================================

; Macro definition
(preproc_def
  name: (identifier) @entity.preprocessor.macro.name
) @entity.preprocessor.macro

; Macro function definition
(preproc_function_def
  name: (identifier) @entity.preprocessor.macro_function.name
  parameters: (preproc_params) @entity.preprocessor.macro_function.params
) @entity.preprocessor.macro_function

; ============================================
; 6. Attribute
; ============================================

; Attribute declaration (C11)
(attribute
  name: (identifier) @entity.attribute.name
) @entity.attribute
"#
}

/// Get comment query for C
///
/// Returns Tree-sitter query patterns for identifying C comments.
/// This uses the common comment query shared across all languages.
pub fn comment_query() -> &'static str {
    common::comment_query()
}

/// Get call query for C
///
/// Returns Tree-sitter query patterns for identifying C call relationships:
/// - Direct function calls
/// - Pointer calls
/// - Method calls
pub fn call_query() -> &'static str {
    r#"
; ============================================
; 1. Direct Calls
; ============================================

; Normal function call
(call_expression
  function: (identifier) @call.function.name
  arguments: (argument_list)? @call.function.arguments
) @call.function

; Macro function call
(call_expression
  function: (identifier) @call.macro.name
  arguments: (argument_list)? @call.function.arguments
) @call.macro

; ============================================
; 2. Pointer Calls
; ============================================

; Function pointer call
(call_expression
  function: (pointer_expression
    argument: (identifier) @call.pointer.variable.name
  )
) @call.pointer

; Callback function call
(call_expression
  function: (identifier) @call.callback.function.name
  arguments: (argument_list
    (_) @call.callback.argument
  )
) @call.callback

; ============================================
; 3. Method Calls
; ============================================

; Object method call
(call_expression
  function: (field_expression
    argument: (_) @call.method.object
    field: (field_identifier) @call.method.function
  )
) @call.method

; Chained method call (e.g., obj.method1().method2())
(call_expression
  function: (field_expression
    argument: (call_expression
      function: (identifier) @call.method.chained.from
    )
    field: (field_identifier) @call.method.chained.to
  )
) @call.method.chained
"#
}

/// Get dependency query for C
///
/// Returns Tree-sitter query patterns for identifying C dependencies:
/// - #include directives
/// - #define dependencies
pub fn dependency_query() -> &'static str {
    r#"
; ============================================
; Include Dependencies
; ============================================

; #include <header> (system include)
(preproc_include
  path: (string_literal) @dependency.include.path
) @dependency.include

; #include "header" (local include)
(preproc_include
  path: (string_literal) @dependency.include.path
) @dependency.include

; ============================================
; Macro Dependencies
; ============================================

; #ifdef dependency
(preproc_ifdef
  name: (identifier) @dependency.macro.ifdef.name
) @dependency.macro.ifdef

; #ifndef dependency
(preproc_ifdef
  name: (identifier) @dependency.macro.ifndef.name
) @dependency.macro.ifndef

; #if defined() dependency
(preproc_if
  condition: (identifier) @dependency.macro.if.name
) @dependency.macro.if
"#
}

/// Get behavior query for C
pub fn behavior_query() -> String {
    let mut query = String::from(
        r#"
(declaration) @behavior.data.bind
(assignment_expression) @behavior.data.bind
(field_expression) @behavior.data.reference
(subscript_expression) @behavior.data.reference
(expression_statement) @behavior.data.statement
"#,
    );
    query.push_str(&common::bitwise_shift_operator_query("binary_expression"));
    query
}

/// Get control-flow query for C
pub fn control_flow_query() -> &'static str {
    r#"
(if_statement) @control.flow.if
(for_statement) @control.flow.loop
(while_statement) @control.flow.loop
(do_statement) @control.flow.loop
(switch_statement) @control.flow.match
(return_statement) @control.flow.return
(break_statement) @control.flow.break
(continue_statement) @control.flow.continue
"#
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Query;

    /// Validate query syntax and return detailed error information
    fn validate_query_syntax(query_name: &str, query_str: &str) -> Result<(), String> {
        let lang = tree_sitter_c::LANGUAGE;
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
