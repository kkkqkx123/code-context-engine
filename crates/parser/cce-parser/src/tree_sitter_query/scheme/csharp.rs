//! C# language query schemes
//!
//! Provides Tree-sitter query patterns for identifying and analyzing
//! C# code entities, call relationships, and dependencies.

/// Get entity query for C#
///
/// Returns Tree-sitter query patterns for identifying C# code entities:
/// - Type definitions (class, interface, struct, enum, record)
/// - Methods (including constructors, destructors, operators)
/// - Properties and events
/// - Namespaces and using directives
/// - Delegates and attributes
pub fn entity_query() -> &'static str {
    r#"
; ============================================
; 0. Imports
; ============================================

; Using directive (namespace import)
(using_directive
  name: (identifier) @entity.import.name
) @entity.import

; Using directive with qualified name
(using_directive
  name: (_) @entity.import.name
) @entity.import

; ============================================
; 1. Types
; ============================================

; Class definition (including generic, static, abstract, partial, nested)
(class_declaration
  name: (identifier) @entity.class.name
) @entity.class

; Interface definition
(interface_declaration
  name: (identifier) @entity.interface.name
) @entity.interface

; Struct definition
(struct_declaration
  name: (identifier) @entity.struct.name
) @entity.struct

; Enum definition
(enum_declaration
  name: (identifier) @entity.enum.name
) @entity.enum

; Enum member
(enum_member_declaration
  name: (identifier) @entity.enum_member.name
) @entity.enum_member

; Record definition
(record_declaration
  name: (identifier) @entity.record.name
) @entity.record

; ============================================
; 2. Methods
; ============================================

; Method definition (including async, static, generic)
(method_declaration
  name: (identifier) @entity.method.name
  parameters: (parameter_list) @entity.method.params
  body: (_) @entity.method.body
) @entity.method

; Constructor definition
(constructor_declaration
  name: (identifier) @entity.constructor.name
) @entity.constructor

; Destructor definition
(destructor_declaration
  name: (identifier) @entity.destructor.name
) @entity.destructor

; Operator overload
(operator_declaration) @entity.method.operator

; ============================================
; 3. Properties and Events
; ============================================

; Property declaration
(property_declaration
  name: (identifier) @entity.property.name
) @entity.property

; Event declaration
(event_declaration
  name: (identifier) @entity.event.name
) @entity.event

; ============================================
; 4. Namespaces
; ============================================

; Namespace declaration (simple name)
(namespace_declaration
  name: (identifier) @entity.namespace.name
) @entity.namespace

; Namespace declaration (qualified name)
(namespace_declaration
  name: (qualified_name) @entity.namespace.qualified_name
) @entity.namespace.qualified

; File-scoped namespace declaration (simple name)
(file_scoped_namespace_declaration
  name: (identifier) @entity.namespace.file_scoped.name
) @entity.namespace.file_scoped

; File-scoped namespace declaration (qualified name)
(file_scoped_namespace_declaration
  name: (qualified_name) @entity.namespace.file_scoped.qualified_name
) @entity.namespace.file_scoped.qualified

; ============================================
; 5. Delegates and Attributes
; ============================================

; Delegate declaration
(delegate_declaration
  name: (identifier) @entity.delegate.name
) @entity.delegate

; Attribute declaration
(attribute
  name: (identifier) @entity.attribute.name
) @entity.attribute

; ============================================
; 6. Generic Type Parameters
; ============================================

; ============================================
; 7. Variables and Fields
; ============================================

; Field declaration
(field_declaration
  (variable_declaration
    (variable_declarator
      (identifier) @entity.field.name
    )
  )
) @entity.field

; Local variable declaration
(local_declaration_statement
  (variable_declaration
    (variable_declarator
      (identifier) @entity.variable.name
    )
  )
) @entity.variable

; Tuple deconstruction, e.g. `var (a, b) = (1, 2);`
(local_declaration_statement
  (variable_declaration
    (variable_declarator
      (tuple_pattern
        (identifier) @entity.variable.multiple.name
      )
      (tuple_expression) @entity.variable.multiple.value
    )
  )
) @entity.variable.multiple

; foreach loop variable, e.g. `foreach (var current in items)`
(foreach_statement
  left: (identifier) @entity.variable.loop.name
  right: (_) @entity.variable.loop.source
) @entity.variable.loop

; is-pattern declaration, e.g. `if (obj is string s)`
(is_pattern_expression
  (declaration_pattern
    type: (_) @entity.variable.case.source
    name: (identifier) @entity.variable.case.name
  )
) @entity.variable.case

; out-var declaration, e.g. `int.TryParse("1", out var result)`
(argument
  (declaration_expression
    type: (_) @entity.variable.type
    name: (identifier) @entity.variable.name
  ) @entity.variable
)

"#
}

/// Get call query for C#
///
/// Returns Tree-sitter query patterns for identifying C# call relationships:
/// - Direct method/function calls
/// - Object method calls
/// - Static method calls
/// - Constructor calls (object creation)
/// - Delegate invocations
pub fn call_query() -> &'static str {
    r#"
; ============================================
; 1. Direct Calls
; ============================================

; Direct method/function call
(invocation_expression
  function: (identifier) @call.function.name
) @call.function

; ============================================
; 2. Object Method Calls
; ============================================

; Object method call (e.g., obj.Method())
(invocation_expression
  function: (member_access_expression
    expression: (_) @call.method.object
    name: (identifier) @call.method.function
  )
  arguments: (argument_list) @call.method.arguments
) @call.method

; Chained method call (e.g., obj.Method1().Method2())
(invocation_expression
  function: (member_access_expression
    expression: (invocation_expression) @call.method.chained.from
    name: (identifier) @call.method.chained.to
  )
) @call.method.chained

; ============================================
; 3. Static Method Calls
; ============================================

; Static method call with qualified name (e.g., ClassName.StaticMethod())
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @call.method.static.object
    name: (identifier) @call.method.static.function
  )
) @call.method.static

; Static method call with namespace qualification (e.g., Namespace.Class.Method())
(invocation_expression
  function: (member_access_expression
    expression: (member_access_expression) @call.method.static.qualified.expression
    name: (identifier) @call.method.static.qualified.function
  )
) @call.method.static.qualified

; ============================================
; 4. Constructor Calls
; ============================================

; Object creation expression (constructor call)
(object_creation_expression
  type: (identifier) @call.constructor.name
) @call.constructor

; Object creation with qualified type name
(object_creation_expression
  type: (qualified_name) @call.constructor.qualified.name
) @call.constructor.qualified

; ============================================
; 5. Delegate Invocations
; ============================================

; Delegate invocation (distinguished from function calls by type context)
; Note: At syntax level, delegate calls are indistinguishable from function calls.
; The Delegate category is mapped to CallbackCall in determine_call_relation_type,
; while Function category maps to DirectCall. This produces different edge types.
; We keep the pattern but it may produce duplicate edges with call.function.
(invocation_expression
  function: (identifier) @call.delegate.name
) @call.delegate

; ============================================
; 6. Generic Method Calls
; ============================================

; Generic method call
(invocation_expression
  function: (generic_name) @call.generic.function
) @call.generic

; Generic method call on object
(invocation_expression
  function: (member_access_expression
    expression: (_) @call.generic.object.name
    name: (generic_name) @call.generic.method.name
  )
) @call.generic.method
"#
}

/// Get dependency query for C#
///
/// Returns Tree-sitter query patterns for identifying C# dependencies:
/// - Using directives (namespace imports)
/// - Namespace references
/// - Type references
pub fn dependency_query() -> &'static str {
    r#"
; ============================================
; 1. Using Directives
; ============================================

; using directive (namespace import)
(using_directive
  name: (identifier) @dependency.using.namespace.name
) @dependency.using

; using directive with qualified name
(using_directive
  name: (_) @dependency.using.qualified.name
) @dependency.using.qualified

; ============================================
; 2. Namespace References
; ============================================

; Qualified identifier access (e.g., System.Console)
(qualified_name
  name: (identifier) @dependency.namespace.qualified.name
) @dependency.namespace.qualified

; ============================================
; 3. Type References
; ============================================

; Class base types (`class Circle : Shape`): inheritance. A class base list
; may mix one base class with interfaces, but the first entry is the base
; class; the resolver records inheritance and implementations resolve via
; the symbol table.
(class_declaration
  (base_list
    (_) @dependency.extend.name
  )
) @dependency.extend

; Struct base types (all are interface implementations: `struct S : I`)
(struct_declaration
  (base_list
    (_) @dependency.implement.name
  )
) @dependency.implement

; Record base types (records support inheritance like classes)
(record_declaration
  (base_list
    (_) @dependency.extend.name
  )
) @dependency.extend

; Interface base types (all are extends)
(interface_declaration
  (base_list
    (type) @dependency.extend.name
  )
) @dependency.extend
"#
}

/// Get behavior query for C#
pub fn behavior_query() -> String {
    let mut query = String::from(
        r#"
(local_declaration_statement) @behavior.data.bind
(variable_declaration) @behavior.data.bind
(assignment_expression) @behavior.data.bind
(member_access_expression) @behavior.data.reference
(element_access_expression) @behavior.data.reference
(query_expression) @behavior.data.query
(expression_statement) @behavior.data.statement
(try_statement) @behavior.effect.error
(throw_expression) @behavior.effect.error
"#,
    );
    query.push_str(&super::common::bitwise_shift_operator_query(
        "binary_expression",
    ));
    query
}

/// Get control-flow query for C#
pub fn control_flow_query() -> &'static str {
    r#"
(if_statement) @control.flow.if
(for_statement) @control.flow.loop
(foreach_statement) @control.flow.loop
(while_statement) @control.flow.loop
(do_statement) @control.flow.loop
(switch_statement) @control.flow.match
(try_statement) @control.flow.try
(return_statement) @control.flow.return
(break_statement) @control.flow.break
(continue_statement) @control.flow.continue
(yield_statement) @control.flow.yield
"#
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Query;

    /// Validate query syntax and return detailed error information
    fn validate_query_syntax(query_name: &str, query_str: &str) -> Result<(), String> {
        let lang = tree_sitter_c_sharp::LANGUAGE;
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
