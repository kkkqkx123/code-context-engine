//! Kotlin language query schemes
//!
//! Provides Tree-sitter query patterns for identifying and analyzing
//! Kotlin code entities, call relationships, and dependencies.

/// Get entity query for Kotlin
///
/// Returns Tree-sitter query patterns for identifying Kotlin code entities:
/// - Type definitions (class, interface, object, enum, data class, sealed class)
/// - Functions (including extension functions)
/// - Properties and variables
/// - Constructors
/// - Package declarations
/// - Type parameters and aliases
pub fn entity_query() -> &'static str {
    r#"
; ============================================
; 0. Imports
; ============================================

; Import declaration
(import
  (identifier) @entity.import.name
) @entity.import

; Import with qualified identifier
(import
  (qualified_identifier) @entity.import.name
) @entity.import

; ============================================
; 1. Types
; ============================================

; Annotation usage (e.g., @Test, @Suppress); buffered as pending annotations
; by the entity extractor (`language_has_annotation_semantics`), so `@Test`
; methods are promoted to `TestCase` and reach the grouper test detector.
; Bare `@Test` is `(annotation (user_type (identifier)))`; parameterized
; `@Test(...)` wraps the type in a `constructor_invocation`.
(annotation
  (user_type (identifier) @entity.annotation.name)
) @entity.annotation

; Class definition
(class_declaration
  name: (identifier) @entity.class.name
  (class_body)? @entity.class.body
) @entity.class

; Interface definition (class_declaration with interface inheritance)
(class_declaration
  name: (identifier) @entity.interface.name
  (class_body)? @entity.interface.body
) @entity.interface

; Object declaration (singleton)
(object_declaration
  name: (identifier) @entity.object.name
  (class_body)? @entity.object.body
) @entity.object

; Companion object
(companion_object
  name: (identifier)? @entity.companion.name
  (class_body)? @entity.companion.body
) @entity.companion

; Enum entry
(enum_entry
  (identifier) @entity.enum_member.name
) @entity.enum_member

; Type alias
(type_alias
  type: (identifier) @entity.type.name
) @entity.type

; ============================================
; 2. Functions
; ============================================

; Function declaration
(function_declaration
  name: (identifier) @entity.function.name
  (function_value_parameters) @entity.function.params
  (type)? @entity.function.return_type
  (function_body)? @entity.function.body
) @entity.function

; ============================================
; 3. Methods (functions inside classes)
; ============================================

; Method inside class
(class_declaration
  (class_body
    (class_member_declaration
      (function_declaration
        name: (identifier) @entity.method.name
        (function_value_parameters) @entity.method.params
        (type)? @entity.method.return_type
        (function_body)? @entity.method.body
      ) @entity.method
    )
  )
)

; Constructor (secondary)
(secondary_constructor
  (function_value_parameters) @entity.constructor.params
) @entity.constructor

; ============================================
; 4. Lambda Expressions
; ============================================

; Lambda expression assigned to property
; e.g., val handler = { x: Int -> x + 1 }
(property_declaration
  (variable_declaration
    (identifier) @entity.lambda.name
  )
  (lambda_literal) @entity.lambda.params
) @entity.lambda

; ============================================
; 5. Properties and Variables
; ============================================

; Property declaration (val/var in class)
(property_declaration
  (variable_declaration
    (identifier) @entity.property.name
    (type)? @entity.property.type
  )
  (_)? @entity.property.value
) @entity.property

; Property getter
(getter
  (function_body)? @entity.method.getter.body
) @entity.method.getter

; Property setter
(setter
  (function_body)? @entity.method.setter.body
) @entity.method.setter

; Variable declaration (val/var in function/block)
(variable_declaration
  (identifier) @entity.variable.name
  (type)? @entity.variable.type
) @entity.variable

; Multi-variable declaration
(multi_variable_declaration
  (variable_declaration
    (identifier) @entity.variable.name
    (type)? @entity.variable.type
  )
) @entity.variable.multi

; ============================================
; 5. Package and Imports
; ============================================

; Package declaration
(package_header
  (qualified_identifier) @entity.package.name
) @entity.package

"#
}

/// Get comment query for Kotlin
///
/// Returns Tree-sitter query patterns for identifying Kotlin comments.
/// Kotlin has:
/// - Line comments (// ...)
/// - Block comments (/* ... */)
/// - KDoc comments (/** ... */)
pub fn comment_query() -> &'static str {
    r#"
; ============================================
; Comments (Meta-information)
; ============================================

; Line comment (// ...)
(line_comment) @comment.line

; Block comment and KDoc (/* ... */ and /** ... */)
(block_comment) @comment.doc
"#
}

/// Get call query for Kotlin
///
/// Returns Tree-sitter query patterns for identifying Kotlin call relationships:
/// - Direct function calls
/// - Method calls (instance and static)
/// - Constructor calls
/// - Extension function calls
/// - Lambda/closure calls
pub fn call_query() -> &'static str {
    r#"
; ============================================
; 1. Direct Function Calls
; ============================================

; Direct function call (e.g., functionName())
(call_expression
  (identifier) @call.function.name
  (value_arguments) @call.function.arguments
) @call.function

; ============================================
; 2. Instance Method Calls
; ============================================

; Method call on object (e.g., obj.method())
(call_expression
  (navigation_expression
    (expression) @call.method.instance.object
    (identifier) @call.method.instance.function
  )
  (value_arguments) @call.method.instance.arguments
) @call.method.instance

; Safe call (e.g., obj?.method())
(call_expression
  (navigation_expression
    (expression) @call.method.instance.object
    (identifier) @call.method.instance.function
  )
) @call.method.instance

; ============================================
; 3. Static/Companion Method Calls
; ============================================

; Static method call via class name (e.g., ClassName.method())
(call_expression
  (navigation_expression
    (identifier) @call.method.static.class
    (identifier) @call.method.static.function
  )
  (value_arguments) @call.method.static.arguments
) @call.method.static

; ============================================
; 4. Constructor Calls
; ============================================

; Constructor invocation (e.g., ClassName())
(constructor_invocation
  (type) @call.constructor.type.name
  (value_arguments) @call.constructor.arguments
) @call.constructor

; ============================================
; 5. Chained Calls
; ============================================

; Chained method call (e.g., obj.method1().method2())
(call_expression
  (navigation_expression
    (call_expression) @call.method.chained.from
    (identifier) @call.method.chained.to
  )
) @call.method.chained

; ============================================
; 6. Extension Function Calls
; ============================================

; Extension function call (treated as instance method)
(call_expression
  (navigation_expression
    (expression) @call.method.extension.object
    (identifier) @call.method.extension.function
  )
  (value_arguments) @call.method.extension.arguments
) @call.method.extension

; ============================================
; 7. Lambda and Closure
; ============================================

; Lambda invocation (calling a lambda)
(call_expression
  (identifier) @call.closure.name
) @call.closure

; ============================================
; 8. Callable Reference
; ============================================

; Method reference (::functionName)
; Note: Kotlin's callable_reference node has unnamed children,
; so we capture the entire node and parse the raw text in relation_extractor.
(callable_reference) @call.reference

; ============================================
; 9. Generic Function Calls
; ============================================

; Generic function call (e.g., function<Type>())
(call_expression
  (identifier) @call.generic.function.name
  (type_arguments) @call.generic.type_args
) @call.generic

; ============================================
; 10. Higher-Order Function Calls
; ============================================

; Trailing lambda (e.g., list.forEach { item -> process(item) })
(call_expression
  (identifier) @call.hof.name
  (annotated_lambda
    (lambda_literal) @call.hof.callback
  )
) @call.hof.trailing

; Lambda in arguments (e.g., list.forEach({ item -> process(item) }))
(call_expression
  (identifier) @call.hof.name
  (value_arguments
    (value_argument
      (lambda_literal) @call.hof.callback
    )
  )
) @call.hof.argument
"#
}

/// Get dependency query for Kotlin
///
/// Returns Tree-sitter query patterns for identifying Kotlin dependencies:
/// - Import declarations
/// - Package references
/// - Class inheritance (extends/implements)
/// - Delegation
pub fn dependency_query() -> &'static str {
    r#"
; ============================================
; 1. Import Declarations
; ============================================

; Single import (import pkg.ClassName)
(import
  (identifier) @dependency.import.name
) @dependency.import

; Import with qualified identifier (import com.example.ClassName)
(import
  (qualified_identifier) @dependency.import.name
) @dependency.import

; ============================================
; 2. Type References (Inheritance)
; ============================================

; Delegation specifier (extends/implements in Kotlin)
(delegation_specifier
  (type) @dependency.extend.name
) @dependency.extend

; Explicit delegation (by keyword)
(explicit_delegation
  (type) @dependency.extend.name
) @dependency.extend

; ============================================
; 3. Type Constraints
; ============================================

; Type constraint in where clause
(type_constraint
  (type) @dependency.type_parameter.bound
) @dependency.type_parameter.bound
"#
}

/// Get behavior query for Kotlin
pub fn behavior_query() -> String {
    let mut query = String::from(
        r#"
(assignment) @behavior.data.bind
(variable_declaration) @behavior.data.bind
(declaration) @behavior.data.bind
(lambda_literal) @behavior.data.bind
(anonymous_function) @behavior.data.bind
(navigation_expression) @behavior.data.reference
(statement) @behavior.data.statement
(try_expression) @behavior.effect.error
(throw_expression) @behavior.effect.error
"#,
    );
    query.push_str(&super::common::bitwise_shift_operator_query(
        "binary_expression",
    ));
    query
}

/// Get control-flow query for Kotlin
pub fn control_flow_query() -> &'static str {
    r#"
(if_expression) @control.flow.if
(for_statement) @control.flow.loop
(while_statement) @control.flow.loop
(do_while_statement) @control.flow.loop
(when_expression) @control.flow.match
(return_expression) @control.flow.return
"#
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Query;

    /// Validate query syntax and return detailed error information
    fn validate_query_syntax(query_name: &str, query_str: &str) -> Result<(), String> {
        let lang: tree_sitter::Language = tree_sitter_kotlin_ng::LANGUAGE.into();
        match Query::new(&lang, query_str) {
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

    #[test]
    fn test_kotlin_return_type_capture() {
        let lang: tree_sitter::Language = tree_sitter_kotlin_ng::LANGUAGE.into();
        let query_str = r#"
(function_declaration
  name: (identifier) @entity.function.name
  (function_value_parameters) @entity.function.params
  (type)? @entity.function.return_type
  (function_body)? @entity.function.body
) @entity.function
"#;
        let result = Query::new(&lang, query_str);
        assert!(
            result.is_ok(),
            "Return type capture query failed: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_kotlin_variable_type_capture() {
        use streaming_iterator::StreamingIterator;

        let mut parser = tree_sitter::Parser::new();
        let language: tree_sitter::Language = tree_sitter_kotlin_ng::LANGUAGE.into();
        parser.set_language(&language).unwrap();

        let code = "val x: Int = 10";
        let tree = parser.parse(code, None).unwrap();

        let query_str = r#"
(variable_declaration
  (identifier) @entity.variable.name
  (type)? @entity.variable.type
) @entity.variable
"#;

        let query = tree_sitter::Query::new(&language, query_str).unwrap();
        let mut cursor = tree_sitter::QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), code.as_bytes());

        let mut found_type = false;
        while let Some(m) = matches.next() {
            for c in m.captures {
                let name = query.capture_names()[c.index as usize];
                let text = &code[c.node.start_byte()..c.node.end_byte()];
                if name == "entity.variable.type" {
                    println!("Variable type captured: {:?}", text);
                    found_type = true;
                }
            }
        }
        assert!(found_type, "Variable type should be captured");
    }
}
