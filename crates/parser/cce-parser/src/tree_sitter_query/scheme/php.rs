//! PHP language query schemes
//!
//! Provides Tree-sitter query patterns for identifying and analyzing
//! PHP code entities, call relationships, and dependencies.

/// Get entity query for PHP
///
/// Returns Tree-sitter query patterns for identifying PHP code entities:
/// - Classes, interfaces, traits, enums
/// - Functions and methods
/// - Properties and constants
/// - Namespaces
pub fn entity_query() -> &'static str {
    r#"
; ============================================
; Imports and Includes
; ============================================

; Namespace use declaration
(namespace_use_declaration
  (namespace_use_clause
    (name) @entity.import.name
  )
) @entity.import

; Include expression
(include_expression
  (string) @entity.include.name
) @entity.include

; Include once expression
(include_once_expression
  (string) @entity.include.name
) @entity.include

; Require expression
(require_expression
  (string) @entity.require.name
) @entity.require

; Require once expression
(require_once_expression
  (string) @entity.require.name
) @entity.require

; ============================================
; Class Definitions
; ============================================

; Class declaration
(class_declaration
  name: (name) @entity.class.name
  (base_clause
    (name) @entity.class.extends
  )?
  (class_interface_clause
    (name) @entity.class.implements
  )?
  body: (declaration_list) @entity.class.body
) @entity.class

; ============================================
; Interface Definitions
; ============================================

; Interface declaration
(interface_declaration
  name: (name) @entity.interface.name
  (base_clause
    (name) @entity.interface.extends
  )?
  body: (declaration_list) @entity.interface.body
) @entity.interface

; ============================================
; Trait Definitions
; ============================================

; Trait declaration
(trait_declaration
  name: (name) @entity.trait.name
  body: (declaration_list) @entity.trait.body
) @entity.trait

; ============================================
; Enum Definitions
; ============================================

; Enum declaration
(enum_declaration
  name: (name) @entity.enum.name
  (enum_declaration_list
    (enum_case
      name: (name) @entity.enum.case.name
    ) @entity.enum.case
  ) @entity.enum.body
) @entity.enum

; ============================================
; Function Definitions
; ============================================

; Function definition
(function_definition
  name: (name) @entity.function.name
  parameters: (formal_parameters) @entity.function.params
  return_type: (_)? @entity.function.return_type
  body: (compound_statement) @entity.function.body
) @entity.function

; ============================================
; Method Definitions
; ============================================

; Method declaration in class
(method_declaration
  (visibility_modifier)? @entity.method.visibility
  (static_modifier)? @entity.method.static
  (abstract_modifier)? @entity.method.abstract
  (final_modifier)? @entity.method.final
  name: (name) @entity.method.name
  parameters: (formal_parameters) @entity.method.params
  return_type: (_)? @entity.method.return_type
  body: (compound_statement)? @entity.method.body
) @entity.method

; Constructor method
(method_declaration
  (visibility_modifier)? @entity.constructor.visibility
  name: (name) @entity.constructor.name
  (#eq? @entity.constructor.name "__construct")
  parameters: (formal_parameters) @entity.constructor.params
) @entity.constructor

; Destructor method
(method_declaration
  (visibility_modifier)? @entity.destructor.visibility
  name: (name) @entity.destructor.name
  (#eq? @entity.destructor.name "__destruct")
) @entity.destructor

; ============================================
; Property Declarations
; ============================================

; Property declaration
(property_declaration
  (visibility_modifier)? @entity.property.visibility
  (static_modifier)? @entity.property.static
  (readonly_modifier)? @entity.property.readonly
  (property_element
    name: (variable_name
      (name) @entity.property.name
    )
    default_value: (_)? @entity.property.default
  ) @entity.property.element
) @entity.property

; ============================================
; Constant Declarations
; ============================================

; Class constant declaration
(const_declaration
  (visibility_modifier)? @entity.const.visibility
  (const_element
    (name) @entity.const.name
  ) @entity.const.element
) @entity.const

; ============================================
; Namespace Definitions
; ============================================

; Namespace definition
(namespace_definition
  name: (namespace_name
    (name) @entity.namespace.name
  )
) @entity.namespace

; ============================================
; Variables
; ============================================

; Variable assignment
(assignment_expression
  left: (variable_name
    (name) @entity.variable.name
  )
  right: (_) @entity.variable.value
) @entity.variable

; ============================================
; Attributes (PHP 8+)
; ============================================

; Attribute
(attribute
  (name) @entity.attribute.name
) @entity.attribute
"#
}

/// Get call query for PHP
///
/// Returns Tree-sitter query patterns for identifying PHP call relationships:
/// - Direct function calls
/// - Method calls (instance, static, nullsafe)
/// - Constructor calls
pub fn call_query() -> &'static str {
    r#"
; ============================================
; Direct Function Calls
; ============================================

; Direct function call
(function_call_expression
  function: (name) @call.function.name
  arguments: (arguments) @call.function.arguments
) @call.function

; Function call with qualified name
(function_call_expression
  function: (qualified_name
    (name) @call.function.qualified.name
  )
  arguments: (arguments) @call.function.arguments
) @call.function.qualified

; ============================================
; Method Calls
; ============================================

; Instance method call
(member_call_expression
  object: (_) @call.method.instance.object
  name: (name) @call.method.instance.function
  arguments: (arguments) @call.method.arguments
) @call.method.instance

; Nullsafe method call
(nullsafe_member_call_expression
  object: (_) @call.method.nullsafe.object
  name: (name) @call.method.nullsafe.function
  arguments: (arguments) @call.method.arguments
) @call.method.nullsafe

; Static method call
(scoped_call_expression
  scope: (_) @call.method.static.class
  name: (name) @call.method.static.function
  arguments: (arguments) @call.method.arguments
) @call.method.static

; ============================================
; Constructor Calls
; ============================================

; Object creation
(object_creation_expression
  (name) @call.constructor.name
  (arguments)? @call.constructor.arguments
) @call.constructor

; Object creation with qualified name
(object_creation_expression
  (qualified_name
    (name) @call.constructor.qualified.name
  )
  (arguments)? @call.constructor.arguments
) @call.constructor.qualified

; ============================================
; Chained Method Calls
; ============================================

; Chained method call
(member_call_expression
  object: (member_call_expression) @call.method.chained.from
  name: (name) @call.method.chained.to
) @call.method.chained

; ============================================
; Special Calls
; ============================================

; Parent method call
(scoped_call_expression
  scope: (relative_scope) @call.parent.scope
  (#eq? @call.parent.scope "parent")
  name: (name) @call.parent.function
) @call.parent

; Self method call
(scoped_call_expression
  scope: (relative_scope) @call.self.scope
  (#eq? @call.self.scope "self")
  name: (name) @call.self.function
) @call.self

; Static method call
(scoped_call_expression
  name: (name) @call.static.function
) @call.static
"#
}

/// Get dependency query for PHP
///
/// Returns Tree-sitter query patterns for identifying PHP dependencies:
/// - Namespace imports (use statements)
/// - Include/require expressions
/// - Trait usage
pub fn dependency_query() -> &'static str {
    r#"
; ============================================
; Namespace Use Declarations
; ============================================

; Simple use statement
(namespace_use_declaration
  (namespace_use_clause
    (name) @dependency.use.name
  ) @dependency.use
)

; Use statement with alias
(namespace_use_declaration
  (namespace_use_clause
    (qualified_name
      (name) @dependency.use.qualified.name
    )
    (name) @dependency.use.alias
  ) @dependency.use.alias
)

; Group use statement
(namespace_use_declaration
  (namespace_use_group
    (namespace_use_clause
      (name) @dependency.use.group.name
    )
  ) @dependency.use.group
)

; ============================================
; Include/Require
; ============================================

; Include expression
(include_expression
  (string) @dependency.include.path
) @dependency.include

; Include once expression
(include_once_expression
  (string) @dependency.include_once.path
) @dependency.include_once

; Require expression
(require_expression
  (string) @dependency.require.path
) @dependency.require

; Require once expression
(require_once_expression
  (string) @dependency.require_once.path
) @dependency.require_once

; ============================================
; Inheritance Dependencies
; ============================================

; Class extends
(class_declaration
  (base_clause
    (name) @dependency.extend.name
  )
) @dependency.extend

; Class implements
(class_declaration
  (class_interface_clause
    (name) @dependency.implement.name
  )
) @dependency.implement

; Interface extends
(interface_declaration
  (base_clause
    (name) @dependency.interface.extends.name
  )
) @dependency.interface.extends

; ============================================
; Trait Usage (Mixin)
; ============================================

; Trait use in class body
(use_declaration
  (name) @dependency.trait.name
) @dependency.trait

(use_declaration
  (qualified_name) @dependency.trait.name
) @dependency.trait
"#
}

/// Get behavior query for PHP
pub fn behavior_query() -> String {
    let mut query = String::from(
        r#"
(assignment_expression) @behavior.data.bind
(variable_name) @behavior.data.bind
(anonymous_function) @behavior.data.bind
(arrow_function) @behavior.data.bind
(member_access_expression) @behavior.data.reference
(subscript_expression) @behavior.data.reference
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

/// Get control-flow query for PHP
pub fn control_flow_query() -> &'static str {
    r#"
(if_statement) @control.flow.if
(for_statement) @control.flow.loop
(while_statement) @control.flow.loop
(do_statement) @control.flow.loop
(switch_statement) @control.flow.match
(match_expression) @control.flow.match
(return_statement) @control.flow.return
(break_statement) @control.flow.break
(continue_statement) @control.flow.continue
(yield_expression) @control.flow.yield
"#
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Query;

    /// Validate query syntax and return detailed error information
    fn validate_query_syntax(query_name: &str, query_str: &str) -> Result<(), String> {
        let lang = tree_sitter_php::LANGUAGE_PHP;
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
