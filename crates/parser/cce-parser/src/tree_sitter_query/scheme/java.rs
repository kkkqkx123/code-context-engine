//! Java language query schemes
//!
//! Provides Tree-sitter query patterns for identifying and analyzing
//! Java code entities, call relationships, and dependencies.

/// Get entity query for Java
///
/// Returns Tree-sitter query patterns for identifying Java code entities:
/// - Type definitions (class, interface, enum, record, annotation)
/// - Methods (including constructors)
/// - Fields and variables
/// - Package declarations
/// - Type parameters
pub fn entity_query() -> &'static str {
    r#"
; ============================================
; 0. Imports
; ============================================

; Import declaration
(import_declaration
  (scoped_identifier) @entity.import.name
) @entity.import

; ============================================
; 1. Types
; ============================================

; Class definition
(class_declaration
  name: (identifier) @entity.class.name
  body: (class_body) @entity.class.body
  superclass: (type_identifier)? @entity.class.base
) @entity.class

; Interface definition
(interface_declaration
  name: (identifier) @entity.interface.name
) @entity.interface

; Enum definition
(enum_declaration
  name: (identifier) @entity.enum.name
) @entity.enum

; Enum constant
(enum_constant
  name: (identifier) @entity.enum_constant.name
) @entity.enum_constant

; Record definition (Java 14+)
(record_declaration
  name: (identifier) @entity.record.name
) @entity.record

; Record components (Java 14+, implicitly private final fields)
; e.g., record Point(String name, int age)
(record_declaration
  parameters: (formal_parameters
    (formal_parameter
      type: (_) @entity.field.type
      name: (identifier) @entity.field.name
    ) @entity.field
  )
)

; Annotation type definition
(annotation_type_declaration
  name: (identifier) @entity.annotation.name
) @entity.annotation

; Annotation usage (e.g., @Override, @Entity, @Test)
; Marker annotation (no arguments): @Override
(marker_annotation
  (identifier) @entity.annotation.name
) @entity.annotation

; Normal annotation (with arguments): @Entity(name = "User")
(annotation
  (identifier) @entity.annotation.name
  (annotation_argument_list) @entity.annotation.body
) @entity.annotation

; ============================================
; 2. Methods
; ============================================

; Method definition
(method_declaration
  type: (_) @entity.method.return_type
  name: (identifier) @entity.method.name
  parameters: (formal_parameters) @entity.method.params
  body: (block) @entity.method.body
) @entity.method

; Constructor definition
(constructor_declaration
  name: (identifier) @entity.constructor.name
  parameters: (formal_parameters) @entity.constructor.params
  body: (block)? @entity.constructor.body
) @entity.constructor

; ============================================
; 3. Lambda Expressions
; ============================================

; Lambda expression assigned to local variable
; e.g., Runnable handler = () -> System.out.println("hello")
(local_variable_declaration
  (variable_declarator
    name: (identifier) @entity.lambda.name
    value: (lambda_expression) @entity.lambda.params
  )
) @entity.lambda

; ============================================
; 4. Fields and Variables
; ============================================

; Field declaration
(field_declaration
  declarator: (variable_declarator
    name: (identifier) @entity.field.name
  )
) @entity.field

; Local variable declaration (including `var` inference: the declared type
; text lands in `.type` and the initializer in `.value` so the metadata
; layer can record `type_annotation` / `constructor_type` / `literal_type`
; / `call_target`).
(local_variable_declaration
  type: (_) @entity.variable.type
  declarator: (variable_declarator
    name: (identifier) @entity.variable.name
    value: (_)? @entity.variable.value
  )
) @entity.variable

; instanceof pattern variable
; e.g., if (obj instanceof String s)
; Note: tree-sitter-java validates `name:` against any sibling type child,
; so the pattern binds the name only; the type comes from control-flow
; narrowing, which parses the same condition text.
(instanceof_expression
  name: (identifier) @entity.variable.case.name
) @entity.variable.case

; Enhanced-for loop variable
; e.g., for (String current : args)
(enhanced_for_statement
  name: (identifier) @entity.variable.loop.name
  value: (_) @entity.variable.loop.source
) @entity.variable.loop

; ============================================
; 4. Package and Module
; ============================================

; Package declaration
(package_declaration
  (scoped_identifier) @entity.package.name
) @entity.package

; Module declaration (Java 9+)
(module_declaration
  name: (scoped_identifier) @entity.module.name
) @entity.module

"#
}

/// Get comment query for Java
///
/// Returns Tree-sitter query patterns for identifying Java comments.
/// Java has:
/// - Line comments (// ...)
/// - Block comments (/* ... */)
/// - Javadoc comments (/** ... */)
pub fn comment_query() -> &'static str {
    r#"
; ============================================
; Comments (Meta-information)
; ============================================

; Line comment (// ...)
(line_comment) @comment.line

; Block comment (/* ... */)[include doc comment]
(block_comment) @comment.doc
"#
}

/// Get call query for Java
///
/// Returns Tree-sitter query patterns for identifying Java call relationships:
/// - Direct method calls
/// - Object method calls
/// - Static method calls
/// - Constructor calls (object creation)
/// - Super constructor calls
/// - Method reference
pub fn call_query() -> &'static str {
    r#"
; ============================================
; 1. Direct Method Calls
; ============================================

; Direct method call (e.g., method())
(method_invocation
  name: (identifier) @call.function.name
) @call.function

; ============================================
; 2. Object Method Calls
; ============================================

; Object method call (e.g., obj.method())
(method_invocation
  object: (_) @call.method.object
  name: (identifier) @call.method.function
  arguments: (argument_list) @call.method.arguments
) @call.method

; Chained method call (e.g., obj.method1().method2())
(method_invocation
  object: (method_invocation) @call.method.chained.from
  name: (identifier) @call.method.chained.to
) @call.method.chained

; ============================================
; 3. Static Method Calls
; ============================================

; Static method call with class name (e.g., ClassName.staticMethod())
(method_invocation
  object: (identifier) @call.method.static.object
  name: (identifier) @call.method.static.function
) @call.method.static

; ============================================
; 4. Constructor Calls
; ============================================

; Object creation expression (constructor call)
(object_creation_expression) @call.constructor

; ============================================
; 5. Super Constructor Calls
; ============================================

; Super constructor call
(explicit_constructor_invocation
  (super)
) @call.constructor.super

; ============================================
; 6. Method Reference
; ============================================

; Method reference (e.g., ClassName::methodName or ClassName::new)
; Note: Java's method_reference node has unnamed children (receiver, ::, name),
; so we capture the entire node and parse the raw text in relation_extractor.
(method_reference) @call.reference

; ============================================
; 7. Higher-Order Function Calls
; ============================================

; Lambda as method argument
(method_invocation
  name: (identifier) @call.hof.method.name
  arguments: (argument_list
    (lambda_expression) @call.hof.callback
  )
) @call.hof.lambda

; Method reference as method argument
(method_invocation
  name: (identifier) @call.hof.method.name
  arguments: (argument_list
    (method_reference) @call.hof.callback
  )
) @call.hof.method_ref
"#
}

/// Get dependency query for Java
///
/// Returns Tree-sitter query patterns for identifying Java dependencies:
/// - Import declarations
/// - Package references
/// - Extends/implements clauses
///
/// ## Extends vs Implements distinction
///
/// *Current realization scenarios:**
///
/// Distinguish between extends and implements by matching different AST node types:
///
/// 1. **Extends relation**: matched via the `superclass` node
///    - AST structure: `class_declaration` → `superclass` → `type_identifier`
///    - Example: `class Dog extends Animal {}`
///    - Captures: `@dependency.extend.name` = "Animal"
///
/// 2. **Implements relation**: matched via the `super_interfaces` node
///    - AST structure: `class_declaration` → `super_interfaces` → `type_list` → `type_identifier`
///    - Example: `class Dog implements Runnable, Serializable {}`
///    - Captures: `@dependency.implement.name` = "Runnable", "Serializable"
///
/// 3. **Interface inheritance**: also uses the `super_interfaces` node
///    - AST structure: `interface_declaration` → `super_interfaces` → `type_list` → `type_identifier`
///    - Example: `interface MyInterface extends ParentInterface {}`
///    - Captures: `@dependency.implement.name` = "ParentInterface"
///
/// * Description of the query pattern:**
///
/// - `@dependency.extend.name`: captures the parent class name of an extends clause
/// - `@dependency.implement.name`: captures the interface name of an implements clause (or the parent interface of an interface inheritance)
///
/// * Technical constraints:**
///
/// The Tree-sitter query language does not support field-access syntax (e.g. `superclass:`), so matching is done directly on node types.
/// This approach correctly distinguishes between extends and implements relationships and requires no post-processing.
pub fn dependency_query() -> &'static str {
    r#"
; ============================================
; 1. Import Declarations
; ============================================

; Single type import (import pkg.ClassName;)
(import_declaration
  (scoped_identifier) @dependency.import.name
) @dependency.import

; ============================================
; 2. Type References (Extends and Implements)
; ============================================

; Note: the Tree-sitter query language does not support field-access syntax (e.g. superclass:)
; so extends and implements must be distinguished by matching different node structures

; Superclass node (extends relationship)
; In class_declaration, the superclass node represents an extends relationship
(superclass
  (type_identifier) @dependency.extend.name
) @dependency.extend

; Super_interfaces node (implements or interface extends)
; A type_identifier inside super_interfaces means implements (for classes/enums/records)
; or extends (for interfaces)
(super_interfaces
  (type_list
    (type_identifier) @dependency.implement.name
  )
) @dependency.implement

; ============================================
; 3. Module Dependencies (Java 9+)
; ============================================

; Requires module
(requires_module_directive
  (scoped_identifier) @dependency.module.requires.name
) @dependency.module.requires

; Exports module
(exports_module_directive
  (scoped_identifier) @dependency.module.exports.name
) @dependency.module.exports

; Opens module
(opens_module_directive
  (scoped_identifier) @dependency.module.opens.name
) @dependency.module.opens

; Uses service
(uses_module_directive
  (scoped_identifier) @dependency.module.uses.name
) @dependency.module.uses

; Provides service
(provides_module_directive
  (scoped_identifier) @dependency.module.provides.name
) @dependency.module.provides
"#
}

/// Get behavior query for Java
pub fn behavior_query() -> String {
    let mut query = String::from(
        r#"
(local_variable_declaration) @behavior.data.bind
(assignment_expression) @behavior.data.bind
(lambda_expression) @behavior.data.bind
(field_access) @behavior.data.reference
(expression_statement) @behavior.data.statement
(try_statement) @behavior.effect.error
(throw_statement) @behavior.effect.error
"#,
    );
    query.push_str(&super::common::bitwise_shift_operator_query(
        "binary_expression",
    ));
    query
}

/// Get control-flow query for Java
pub fn control_flow_query() -> &'static str {
    r#"
(if_statement) @control.flow.if
(for_statement) @control.flow.loop
(enhanced_for_statement) @control.flow.loop
(while_statement) @control.flow.loop
(do_statement) @control.flow.loop
(switch_expression) @control.flow.match
(try_statement) @control.flow.try
(try_with_resources_statement) @control.flow.try
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
        let lang = tree_sitter_java::LANGUAGE;
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
