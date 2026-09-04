//! Scala language query schemes
//!
//! Provides Tree-sitter query patterns for identifying and analyzing
//! Scala code entities, call relationships, and dependencies.

/// Get entity query for Scala
///
/// Returns Tree-sitter query patterns for identifying Scala code entities:
/// - Type definitions (class, trait, object, enum, case class)
/// - Methods and functions
/// - Properties and variables
/// - Package declarations
/// - Type parameters
pub fn entity_query() -> &'static str {
    r#"
; ============================================
; 0. Imports
; ============================================

; Import declaration
(import_declaration
  path: (identifier) @entity.import.name
) @entity.import

; ============================================
; 1. Types
; ============================================

; Class definition
(class_definition
  name: (identifier) @entity.class.name
  body: (template_body)? @entity.class.body
) @entity.class

; Trait definition
(trait_definition
  name: (identifier) @entity.trait.name
  body: (template_body)? @entity.trait.body
) @entity.trait

; Object definition (singleton)
(object_definition
  name: (identifier) @entity.object.name
  body: (template_body)? @entity.object.body
) @entity.object

; Enum definition (Scala 3)
(enum_definition
  name: (identifier) @entity.enum.name
  body: (enum_body)? @entity.enum.body
) @entity.enum

; Enum case (simple_enum_case in Scala 3)
(simple_enum_case
  name: (identifier) @entity.enum_case.name
) @entity.enum_case

; Given definition (Scala 3)
(given_definition
  name: (identifier)? @entity.given.name
) @entity.given

; ============================================
; 2. Functions and Methods
; ============================================

; Function definition (def)
(function_definition
  name: (identifier) @entity.function.name
  parameters: (parameters)? @entity.function.params
  return_type: (_)? @entity.function.return_type
  body: (_)? @entity.function.body
) @entity.function

; ============================================
; 3. Properties and Variables
; ============================================

; Val definition (immutable variable)
(val_definition
  pattern: (identifier) @entity.variable.name
  type: (_)? @entity.variable.type
  value: (_)? @entity.variable.value
) @entity.variable

; Var definition (mutable variable)
(var_definition
  pattern: (identifier) @entity.variable.name
  type: (_)? @entity.variable.type
  value: (_)? @entity.variable.value
) @entity.variable

; ============================================
; 4. Package and Imports
; ============================================

; Package declaration
(package_clause) @entity.package

; ============================================
; 4.5 Annotations
; ============================================

; Annotation (e.g. @Test, @tailrec)
(annotation
  name: (type_identifier) @entity.annotation.name
) @entity.annotation

; ============================================
; 5. Type Parameters
; ============================================

; Type parameter in generic type definition
; Note: tree-sitter-scala v0.23 uses different node names
; This is a simplified pattern that captures type identifiers in type parameter contexts

"#
}

/// Get comment query for Scala
///
/// Returns Tree-sitter query patterns for identifying Scala comments.
/// Scala has:
/// - Line comments (// ...)
/// - Block comments (/* ... */)
/// - Scaladoc comments (/** ... */)
pub fn comment_query() -> &'static str {
    r#"
; ============================================
; Comments (Meta-information)
; ============================================

; Line comment (// ...)
(comment) @comment.line

; Block comment and Scaladoc (/* ... */ and /** ... */)
(block_comment) @comment.doc
"#
}

/// Get call query for Scala
///
/// Returns Tree-sitter query patterns for identifying Scala call relationships:
/// - Direct function calls
/// - Method calls (instance and static)
/// - Constructor calls
/// - Infix method calls
/// - Apply/update calls
pub fn call_query() -> &'static str {
    r#"
; ============================================
; 1. Direct Function Calls
; ============================================

; Direct function call (e.g., functionName())
(call_expression
  function: (identifier) @call.function.name
  arguments: (arguments) @call.function.arguments
) @call.function

; ============================================
; 2. Instance Method Calls
; ============================================

; Method call on object (e.g., obj.method())
(call_expression
  function: (field_expression
    value: (_) @call.method.instance.object
    field: (identifier) @call.method.instance.function
  )
  arguments: (arguments) @call.method.instance.arguments
) @call.method.instance

; ============================================
; 3. Static/Object Method Calls
; ============================================

; Static method call via object name (e.g., ClassName.method())
(call_expression
  function: (field_expression
    value: (identifier) @call.method.static.object
    field: (identifier) @call.method.static.function
  )
  arguments: (arguments) @call.method.static.arguments
) @call.method.static

; ============================================
; 4. Constructor Calls
; ============================================

; Constructor invocation (e.g., new ClassName())
(instance_expression
  (type_identifier) @call.constructor.type.name
  arguments: (arguments)? @call.constructor.arguments
) @call.constructor

; ============================================
; 5. Chained Calls
; ============================================

; Chained method call (e.g., obj.method1().method2())
(call_expression
  function: (field_expression
    value: (call_expression) @call.method.chained.from
    field: (identifier) @call.method.chained.to
  )
) @call.method.chained

; ============================================
; 6. Infix Method Calls
; ============================================

; Infix method call (e.g., obj method arg)
(infix_expression
  left: (_) @call.infix.left
  operator: (identifier) @call.infix.operator
  right: (_) @call.infix.right
) @call.infix

; ============================================
; 7. Apply/Update Calls
; ============================================

; Apply call (e.g., obj(args))
(call_expression
  function: (identifier) @call.apply.name
) @call.apply
"#
}

/// Get dependency query for Scala
///
/// Returns Tree-sitter query patterns for identifying Scala dependencies:
/// - Import declarations
/// - Package references
/// - Extends/with clauses (inheritance and mixins)
pub fn dependency_query() -> &'static str {
    r#"
; ============================================
; 1. Import Declarations
; ============================================

; Import declaration with path
(import_declaration
  path: (identifier) @dependency.import.name
) @dependency.import

; Import with namespace selectors (import pkg.{A, B})
(import_declaration
  (namespace_selectors
    (identifier) @dependency.import.selector.name
  )
) @dependency.import.selectors

; ============================================
; 2. Type References (Inheritance and Mixins)
; ============================================

; Extends clause (class inheritance and trait mixing)
; In Scala, extends clause contains both parent class and mixed-in traits
(extends_clause
  type: (type_identifier) @dependency.extend.name
) @dependency.extend

; ============================================
; 3. Type Constraints
; ============================================

; Type bound in context bound (e.g., [T: ClassTag])
(context_bound
  (type_identifier) @dependency.type_parameter.bound
) @dependency.type_parameter.bound

; View bound (e.g., [T <% String])
(view_bound
  (type_identifier) @dependency.type_parameter.view_bound
) @dependency.type_parameter.view_bound
"#
}

/// Get behavior query for Scala
pub fn behavior_query() -> String {
    let mut query = String::from(
        r#"
(assignment_expression) @behavior.data.bind
(var_declaration) @behavior.data.bind
(lambda_expression) @behavior.data.bind
(field_expression) @behavior.data.reference
(call_expression) @behavior.data.statement
(try_expression) @behavior.effect.error
"#,
    );
    query.push_str(&super::common::bitwise_shift_operator_query(
        "infix_expression",
    ));
    query
}

/// Get control-flow query for Scala
pub fn control_flow_query() -> &'static str {
    r#"
(if_expression) @control.flow.if
(for_expression) @control.flow.loop
(while_expression) @control.flow.loop
(do_while_expression) @control.flow.loop
(match_expression) @control.flow.match
(return_expression) @control.flow.return
"#
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Query;

    /// Validate query syntax and return detailed error information
    fn validate_query_syntax(query_name: &str, query_str: &str) -> Result<(), String> {
        let lang: tree_sitter::Language = tree_sitter_scala::LANGUAGE.into();
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

    fn print_node(node: tree_sitter::Node, source: &str, depth: usize) {
        let indent = "  ".repeat(depth);
        let text = if node.child_count() == 0 {
            let start = node.start_byte();
            let end = node.end_byte();
            &source[start..end]
        } else {
            ""
        };
        println!("{}{}: {:?}", indent, node.kind(), text);

        for i in 0..node.child_count() {
            if let Some(child) = node.child(i as u32) {
                if let Some(field_name) = node.field_name_for_child(i as u32) {
                    println!("{}  [field: {}]", indent, field_name);
                }
                print_node(child, source, depth + 1);
            }
        }
    }

    #[test]
    fn dump_scala_node_types() {
        let mut parser = tree_sitter::Parser::new();
        let language: tree_sitter::Language = tree_sitter_scala::LANGUAGE.into();
        parser.set_language(&language).unwrap();

        let cases = vec![
            (
                "Function with return type",
                "def add(a: Int, b: Int): Int = a + b",
            ),
            (
                "Function without return type",
                "def add(a: Int, b: Int) = a + b",
            ),
            ("Val with type", "val x: Int = 10"),
            ("Val without type", "val x = 10"),
            ("Var with type", "var y: String = \"hello\""),
            (
                "Match expression",
                "x match { case s: String => s.length, case n: Int => n }",
            ),
            ("Constructor call", "val user = new User(\"name\")"),
            ("Class with methods", "class Foo { def bar: String = \"\" }"),
        ];

        for (name, code) in cases {
            println!("\n=== {} ===", name);
            println!("Code: {}", code);
            let tree = parser.parse(code, None).unwrap();
            print_node(tree.root_node(), code, 0);
        }
    }
}
