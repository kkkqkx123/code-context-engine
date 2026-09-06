//! Go language query schemes
//!
//! Provides Tree-sitter query patterns for identifying and analyzing
//! Go code entities, call relationships, and dependencies.

/// Get entity query for Go
///
/// Returns Tree-sitter query patterns for identifying Go code entities:
/// - Type definitions (struct, interface, type aliases)
/// - Functions (definitions, methods)
/// - Variables (normal, package, const)
/// - Entity components (fields, parameters, methods)
/// - Package and import declarations
/// - Other constructs (selectors, labels)
pub fn entity_query() -> &'static str {
    r#"
; ============================================
; 0. Imports
; ============================================

; Import declaration
(import_declaration
  (import_spec
    path: (interpreted_string_literal) @entity.import.name
  )
) @entity.import

; ============================================
; 1. Types
; ============================================

; Type alias declaration
(type_declaration
  (type_spec
    name: (type_identifier) @entity.type.name
    type: (_) @entity.type_alias.underlying_type
  ) @entity.type
)

; Struct type definition
(type_declaration
  (type_spec
    name: (type_identifier) @entity.struct.name
    type: (struct_type) @entity.struct.body
  ) @entity.struct
)

; Interface type definition
(type_declaration
  (type_spec
    name: (type_identifier) @entity.interface.name
    type: (interface_type) @entity.interface.body
  ) @entity.interface
)

; Interface method specification
(interface_type
  (method_elem
    name: (field_identifier) @entity.interface_method.name
    parameters: (parameter_list) @entity.interface_method.params
    result: (_)? @entity.interface_method.return_type
  ) @entity.interface_method
)

; ============================================
; 2. Functions
; ============================================

; Function declaration
(function_declaration
  name: (identifier) @entity.function.name
  parameters: (parameter_list) @entity.function.params
  result: (_)? @entity.function.return_type
  body: (block) @entity.function.body
) @entity.function

; Method declaration (receiver). Field order follows source order:
; `func <receiver> <name> <params> <result> <body>`.
(method_declaration
  receiver: (parameter_list) @entity.method.receiver
  name: (field_identifier) @entity.method.name
  parameters: (parameter_list) @entity.method.params
  result: (_)? @entity.method.return_type
  body: (block) @entity.method.body
) @entity.method

; ============================================
; 3. Function Literals (Closures)
; ============================================

; Function literal assigned to variable via short var declaration
; e.g., f := func(x int) int { return x + 1 }
(short_var_declaration
  left: (expression_list
    (identifier) @entity.function_literal.name
  )
  right: (expression_list
    (func_literal) @entity.function_literal.params
  )
) @entity.function_literal

; ============================================
; 4. Variables
; ============================================

; Variable declaration
(var_declaration
  (var_spec
    name: (identifier) @entity.variable.name
    type: (_)? @entity.variable.type
    value: (_)? @entity.variable.value
  ) @entity.variable
)

; Package variable declaration
(var_declaration
  (var_spec
    name: (identifier) @entity.variable.name
    type: (_)? @entity.variable.type
    value: (_)? @entity.variable.value
  ) @entity.variable
)

; Short variable declaration (:=)
(short_var_declaration
  left: (expression_list
    (identifier) @entity.variable.name
  )
  right: (_) @entity.variable.value
) @entity.variable

; Const declaration
(const_declaration
  (const_spec
    name: (identifier) @entity.constant.name
    type: (_)? @entity.constant.type
    value: (_)? @entity.constant.value
  ) @entity.constant
)

; Range loop variables
; e.g., `for i, v := range items` binds `i` and `v` to the index and
; element types. Each name fans out as a sibling entity sharing the
; iterated collection as provenance.
(for_statement
  (range_clause
    left: (expression_list
      (identifier) @entity.variable.loop.name
    )
    right: (_) @entity.variable.loop.source
  )
) @entity.variable.loop

; ============================================
; 4. Entity Components
; ============================================

; Struct field declaration
(field_declaration
  name: (field_identifier) @entity.field.name
  type: (_) @entity.field.type
) @entity.field

; Struct field declaration with tag
(field_declaration
  name: (field_identifier) @entity.field_tagged.name
  type: (_) @entity.field_tagged.type
  tag: (raw_string_literal) @entity.field_tagged.tag
) @entity.field_tagged

; Embedded struct field (anonymous field)
; Captured here as an entity to represent the structural feature
(field_declaration
  type: (type_identifier) @entity.embedded.name
) @entity.embedded


; ============================================
; 6. Type Declarations
; ============================================
; 5. Package and Import
; ============================================

; Package clause
(package_clause
  (package_identifier) @entity.package.name
) @entity.package

; ============================================
; 5. Selector Expressions
; ============================================

; Selector expression (package.member)
(selector_expression
  operand: (identifier) @entity.selector.package.name
  field: (field_identifier) @entity.selector.member.name
) @entity.selector.package

; Selector expression (object.member)
(selector_expression
  operand: (_) @entity.selector.object.name
  field: (field_identifier) @entity.selector.member.name
) @entity.selector.object

"#
}

/// Get call query for Go
///
/// Returns Tree-sitter query patterns for identifying Go call relationships:
/// - Direct function calls
/// - Method calls
/// - Package function calls
/// - Goroutine calls
/// - Deferred calls
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

; ============================================
; 2. Method Calls
; ============================================

; Method call on object
(call_expression
  function: (selector_expression
    operand: (_) @call.method.object
    field: (field_identifier) @call.method.function
  )
  arguments: (argument_list)? @call.method.arguments
) @call.method

; Method call on package (package.Func)
(call_expression
  function: (selector_expression
    operand: (identifier) @call.method.static.object
    field: (field_identifier) @call.method.static.function
  )
  arguments: (argument_list)? @call.method.arguments
) @call.method.package

; ============================================
; 3. Chained Calls
; ============================================

; Chained method call (e.g., obj.method1().method2())
(call_expression
  function: (selector_expression
    operand: (call_expression
      function: (selector_expression
        field: (field_identifier) @call.method.chained.from
      )
    )
    field: (field_identifier) @call.method.chained.to
  )
) @call.method.chained

; ============================================
; 4. Goroutine Calls
; ============================================

; Goroutine function call
(go_statement
  (call_expression
    function: (identifier) @call.goroutine.function.name
    arguments: (argument_list)? @call.goroutine.arguments
  )
) @call.goroutine

; Goroutine method call
(go_statement
  (call_expression
    function: (selector_expression
      operand: (_) @call.goroutine.method.object.name
      field: (field_identifier) @call.goroutine.method.function.name
    )
    arguments: (argument_list)? @call.goroutine.method.arguments
  )
) @call.goroutine.method

; Goroutine with function literal (go func() { ... }())
(go_statement
  (call_expression
    function: (func_literal) @call.goroutine.function_literal
  )
) @call.goroutine.literal

; ============================================
; 5. Deferred Calls
; ============================================

; Deferred function call
(defer_statement
  (call_expression
    function: (identifier) @call.deferred.function.name
    arguments: (argument_list)? @call.deferred.arguments
  )
) @call.deferred

; Deferred method call
(defer_statement
  (call_expression
    function: (selector_expression
      operand: (_) @call.deferred.method.object.name
      field: (field_identifier) @call.deferred.method.function.name
    )
    arguments: (argument_list)? @call.deferred.method.arguments
  )
) @call.deferred.method

; Deferred function literal (defer func() { ... }())
(defer_statement
  (call_expression
    function: (func_literal) @call.deferred.function_literal
  )
) @call.deferred.literal

; ============================================
; 6. Callback Calls
; ============================================

; Callback function call via parameter
(call_expression
  function: (identifier) @call.callback.function.name
  arguments: (argument_list
    (identifier) @call.callback.argument
  )
) @call.callback

; Callback method call
(call_expression
  function: (selector_expression
    field: (field_identifier) @call.callback.method.name
  )
  arguments: (argument_list
    (_) @call.callback.argument
  )
) @call.callback.method
"#
}

/// Get dependency query for Go
///
/// Returns Tree-sitter query patterns for identifying Go dependencies:
/// - Import declarations
/// - Package references
pub fn dependency_query() -> &'static str {
    r#"
; ============================================
; Import Dependencies
; ============================================

; Standard import
(import_declaration
  (import_spec
    path: (interpreted_string_literal) @dependency.import.path
  ) @dependency.import.standard
)

; Import with alias
(import_declaration
  (import_spec
    name: (_) @dependency.import.alias.alias
    path: (interpreted_string_literal) @dependency.import.alias.path
  ) @dependency.import.alias
)

; Import with dot (.)
(import_declaration
  (import_spec
    name: (dot) @dependency.import.dot.dot
    path: (interpreted_string_literal) @dependency.import.dot.path
  ) @dependency.import.dot
)

; Import with blank identifier (_)
(import_declaration
  (import_spec
    name: (blank_identifier) @dependency.import.blank.blank
    path: (interpreted_string_literal) @dependency.import.blank.path
  ) @dependency.import.blank
)

; ============================================
; Struct Embedding (Go-specific)
; ============================================
; Embedded struct fields create composition relationships.
; Unlike regular fields (which have names), embedded fields are anonymous
; and represent type composition/embedding.
; This query captures three forms of embedding:
; 1. Simple:    struct { A }
; 2. Pointer:   struct { *A }
; 3. Qualified: struct { pkg.A }

(field_declaration
  type: [
    ; Simple type embedding: struct { A }
    (type_identifier) @dependency.embedding.name

    ; Pointer type embedding: struct { *A }
    (pointer_type
      (type_identifier) @dependency.embedding.name
    )

    ; Qualified type embedding: struct { pkg.A }
    (qualified_type
      (type_identifier) @dependency.embedding.name
    )
  ]
) @dependency.embedding

; ============================================
; Interface Embedding (Go-specific)
; ============================================
; Embedded interface fields create extension relationships.
; In Go interfaces, you can embed other interfaces:
;   type Reader interface { ... }
;   type ReadWriter interface {
;     Reader    // embedded interface
;     ...
;   }

(interface_type
  (type_elem
    [
      (type_identifier) @dependency.interface_embedding.name
      (pointer_type
        (type_identifier) @dependency.interface_embedding.name
      )
      (qualified_type
        (type_identifier) @dependency.interface_embedding.name
      )
    ]
  ) @dependency.interface_embedding
)

; ============================================
; Package Dependencies
; ============================================
; Package references are resolved from import declarations by the relation
; resolver. A bare `selector_expression` operand (e.g. `user.Greet()`,
; `u.Name`, `err.Error()`) is a local value or receiver, not a package, so it
; must not emit a module edge here; otherwise every method call produces a
; fake `file -> local` dependency.module row.
"#
}

/// Get behavior query for Go
pub fn behavior_query() -> String {
    let mut query = String::from(
        r#"
(assignment_statement) @behavior.data.bind
(short_var_declaration) @behavior.data.bind
(var_declaration) @behavior.data.bind
(selector_expression) @behavior.data.reference
(expression_statement) @behavior.data.statement
"#,
    );
    query.push_str(&super::common::bitwise_shift_operator_query(
        "binary_expression",
    ));
    query
}

/// Get control-flow query for Go
pub fn control_flow_query() -> &'static str {
    r#"
(if_statement) @control.flow.if
(for_statement) @control.flow.loop
(expression_switch_statement) @control.flow.match
(type_switch_statement) @control.flow.match
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
        let lang = tree_sitter_go::LANGUAGE;
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
}
