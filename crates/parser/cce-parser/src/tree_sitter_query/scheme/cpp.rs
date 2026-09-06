//! C++ language query schemes
//!
//! Provides Tree-sitter query patterns for identifying and analyzing
//! C++ code entities, call relationships, and dependencies.
//!
//! C++ queries extend C queries with additional C++ specific constructs.

use super::c;

/// Get entity query for C++
///
/// Returns Tree-sitter query patterns for identifying C++ code entities.
/// This combines C entity queries with C++ specific constructs:
/// - All C entities (struct, union, enum, typedef, functions, variables, etc.)
/// - C++ specific: classes, methods, namespaces, templates, constructors, destructors
pub fn entity_query() -> String {
    let mut query = String::new();

    // Include all C entity queries
    query.push_str(c::entity_query());

    // Add C++ specific entity queries
    query.push_str(
        r#"
; ============================================
; C++ Specific Extensions
; ============================================

; Class definition
(class_specifier
  name: (type_identifier) @entity.class.name
  body: (field_declaration_list) @entity.class.body
) @entity.class

; ============================================
; Methods (C++ specific)
; ============================================

; Method definition
; Note: `type:` must precede `declarator:` — tree-sitter-cpp only honors
; the return-type field constraint in this order (same constraint-ordering
; quirk as tree-sitter-c-sharp `returns:`).
(function_definition
  type: (_) @entity.method.return_type
  declarator: (function_declarator
    declarator: (field_identifier) @entity.method.name
    parameters: (parameter_list) @entity.method.params
  )
  body: (compound_statement) @entity.method.body
) @entity.method

; Method declaration
(declaration
  declarator: (function_declarator
    declarator: (field_identifier) @entity.method.prototype.name
    parameters: (parameter_list) @entity.method.prototype.params
  )
) @entity.method.prototype

; Method prototype in member-declaration form, e.g. `int add(int a);`
(class_specifier
  body: (field_declaration_list
    (field_declaration
      type: (_) @entity.method.prototype.return_type
      declarator: (function_declarator
        declarator: (field_identifier) @entity.method.prototype.name
        parameters: (parameter_list) @entity.method.prototype.params
      )
    ) @entity.method.prototype
  )
)

; Constructor prototype inside class, e.g. `C();`
(class_specifier
  body: (field_declaration_list
    (declaration
      declarator: (function_declarator
        declarator: (identifier) @entity.constructor.prototype.name
        parameters: (parameter_list) @entity.constructor.prototype.params
      )
    ) @entity.constructor.prototype
  )
)

; Destructor prototype inside class, e.g. `~C();`
(class_specifier
  body: (field_declaration_list
    (declaration
      declarator: (function_declarator
        declarator: (destructor_name) @entity.destructor.prototype.name
        parameters: (parameter_list) @entity.destructor.prototype.params
      )
    ) @entity.destructor.prototype
  )
)

; Constructor definition
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @entity.constructor.name
    parameters: (parameter_list) @entity.constructor.params
  )
  body: (compound_statement) @entity.constructor.body
) @entity.constructor

; Destructor definition
(function_definition
  declarator: (function_declarator
    declarator: (destructor_name) @entity.destructor.name
    parameters: (parameter_list) @entity.destructor.params
  )
  body: (compound_statement) @entity.destructor.body
) @entity.destructor

; Operator overload
(function_definition
  type: (_) @entity.method.operator.return_type
  declarator: (function_declarator
    declarator: (operator_name) @entity.method.operator.name
    parameters: (parameter_list) @entity.method.operator.params
  )
  body: (compound_statement) @entity.method.operator.body
) @entity.method.operator

; ============================================
; Namespaces (C++ specific)
; ============================================

; Namespace definition
(namespace_definition
  name: (namespace_identifier) @entity.namespace.definition.name
  body: (declaration_list) @entity.namespace.definition.body
) @entity.namespace.definition

; Nested namespace definition
(namespace_definition
  body: (declaration_list
    (namespace_definition
      name: (namespace_identifier) @entity.namespace.nested.name
    )
  )
) @entity.namespace.nested

; ============================================
; Templates (C++ specific)
; ============================================

; Template class declaration
(template_declaration
  parameters: (template_parameter_list) @entity.template.class.params
  (class_specifier
    name: (type_identifier) @entity.template.class.name
    body: (field_declaration_list) @entity.template.class.body
  )
) @entity.template.class

; Template struct declaration
(template_declaration
  parameters: (template_parameter_list) @entity.template.struct.params
  (struct_specifier
    name: (type_identifier) @entity.template.struct.name
    body: (field_declaration_list) @entity.template.struct.body
  )
) @entity.template.struct

; Template function declaration
(template_declaration
  parameters: (template_parameter_list) @entity.template.function.params
  (function_definition
    type: (_) @entity.template.function.return_type
    declarator: (function_declarator
      declarator: (identifier) @entity.template.function.name
      parameters: (parameter_list) @entity.template.function.params
    )
  )
) @entity.template.function

; Template method declaration
(template_declaration
  parameters: (template_parameter_list) @entity.template.method.params
  (function_definition
    declarator: (function_declarator
      declarator: (field_identifier) @entity.template.method.name
      parameters: (parameter_list) @entity.template.method.params
    )
  )
) @entity.template.method

; ============================================
; C++ Specific Constructs
; ============================================


; Using declaration
(using_declaration) @entity.using

; Range-based for loop variable, e.g. `for (auto& elem : items)`
(for_range_loop
  declarator: (reference_declarator
    (identifier) @entity.variable.loop.name
  )
  right: (_) @entity.variable.loop.source
) @entity.variable.loop

; Range-based for loop variable without reference, e.g. `for (auto elem : items)`
(for_range_loop
  declarator: (identifier) @entity.variable.loop.name
  right: (_) @entity.variable.loop.source
) @entity.variable.loop

; Structured binding declaration, e.g. `auto [a, b] = pair;`
(declaration
  (init_declarator
    declarator: (structured_binding_declarator
      (identifier) @entity.variable.multiple.name
    )
    value: (_) @entity.variable.multiple.value
  )
) @entity.variable.multiple

; Attribute declaration (C++11)
(attribute
  name: (identifier) @entity.attribute.name
) @entity.attribute
"#,
    );

    query
}

/// Get call query for C++
///
/// Returns Tree-sitter query patterns for identifying C++ call relationships.
/// This combines C call queries with C++ specific constructs:
/// - All C call patterns (direct calls, pointer calls, method calls)
/// - C++ specific: template function calls
pub fn call_query() -> String {
    let mut query = String::new();

    // Include all C call queries
    query.push_str(c::call_query());

    // Add C++ specific call queries
    query.push_str(
        r#"
; ============================================
; Template Function Calls (C++ specific)
; ============================================

; Template function call with explicit template arguments
(call_expression
  function: (identifier) @call.template.function.name
  arguments: (argument_list) @call.template.function.arguments
) @call.template.function

; Template method call
(call_expression
  function: (field_expression
    argument: (_) @call.template.method.object
    field: (field_identifier) @call.template.method.name
  )
  arguments: (argument_list) @call.template.method.arguments
) @call.template.method
"#,
    );

    query
}

/// Get dependency query for C++
///
/// Returns Tree-sitter query patterns for identifying C++ dependencies.
/// This combines C dependency queries with C++ specific constructs:
/// - All C dependencies (includes, macros)
/// - C++ specific: using directives, namespace dependencies
pub fn dependency_query() -> String {
    let mut query = String::new();

    // Include all C dependency queries
    query.push_str(c::dependency_query());

    // Add C++ specific dependency queries
    query.push_str(
        r#"
; ============================================
; Using Dependencies (C++ specific)
; ============================================

; using namespace directive
(using_declaration) @dependency.using.namespace

; using type directive
(using_declaration) @dependency.using.type

; ============================================
; Namespace Dependencies (C++ specific)
; ============================================

; Qualified identifier access (e.g., std::vector)
(qualified_identifier
  name: (identifier) @dependency.namespace.qualified.name
) @dependency.namespace.qualified
"#,
    );

    query
}

/// Get behavior query for C++
pub fn behavior_query() -> String {
    let mut query = String::from(
        r#"
(declaration) @behavior.data.bind
(assignment_expression) @behavior.data.bind
(field_expression) @behavior.data.reference
(subscript_expression) @behavior.data.reference
(expression_statement) @behavior.data.statement
(friend_declaration) @behavior.effect.error
(try_statement) @behavior.effect.error
(throw_statement) @behavior.effect.error
"#,
    );
    query.push_str(&super::common::bitwise_shift_operator_query(
        "binary_expression",
    ));
    query
}

/// Get control-flow query for C++
pub fn control_flow_query() -> &'static str {
    r#"
(if_statement) @control.flow.if
(for_statement) @control.flow.loop
(for_range_loop) @control.flow.loop
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
        let lang = tree_sitter_cpp::LANGUAGE;
        match Query::new(&lang.into(), query_str) {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("Query '{}' syntax error: {:?}", query_name, e)),
        }
    }

    #[test]
    fn test_entity_query_syntax_valid() {
        let result = validate_query_syntax("entity_query", &entity_query());
        assert!(
            result.is_ok(),
            "Entity query syntax validation failed: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_call_query_syntax_valid() {
        let result = validate_query_syntax("call_query", &call_query());
        assert!(
            result.is_ok(),
            "Call query syntax validation failed: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_dependency_query_syntax_valid() {
        let result = validate_query_syntax("dependency_query", &dependency_query());
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
