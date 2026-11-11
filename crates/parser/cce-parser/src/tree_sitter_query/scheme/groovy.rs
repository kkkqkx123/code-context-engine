//! Groovy language query schemes
//!
//! Targets the vendored `murtaza64` grammar (`crates/tree-sitter/tree-sitter-groovy`):
//! classes own their body closure, enums own an `enum_body` holding constants
//! plus members after the separator, methods are function definitions nested in
//! either body, fields and locals share the `declaration` node with
//! name/type/value fields, and calls split into parenthesized `function_call`
//! plus juxtaposed `juxt_function_call` command calls. Block and line comments
//! share the `comment` node, so both feed line spans.

/// Get entity query for Groovy
///
/// Returns Tree-sitter query patterns for identifying Groovy code entities:
/// - Class, interface and annotation-type definitions (told apart by keyword)
/// - Enum definitions (told apart by keyword) and enum constants
/// - Method definitions nested in the class body closure or the enum body
/// - Top-level function definitions
/// - Field declarations (class body closure or enum body) and local declarations
/// - Annotation usages (buffered for test detection)
/// - Package declarations
pub fn entity_query() -> &'static str {
    r#"
(class_definition
  "class"
  name: (identifier) @entity.class.name @entity.class.signature.name
  superclass: [(identifier) (qualified_name) (dotted_identifier)]? @entity.class.signature.base
  body: (closure) @entity.class.body) @entity.class

; Interface definition (shares the class node, told apart by keyword)
(class_definition
  "interface"
  name: (identifier) @entity.interface.name @entity.interface.signature.name
  body: (closure) @entity.interface.body) @entity.interface

; Annotation type definition (shares the class node, told apart by keyword)
(class_definition
  "@interface"
  name: (identifier) @entity.annotation.name @entity.annotation.signature.name
  body: (closure) @entity.annotation.body) @entity.annotation

; Methods with bodies
(class_definition
  body: (closure
    (function_definition
      function: (identifier) @entity.method.name @entity.method.signature.name
      parameters: (parameter_list) @entity.method.params @entity.method.signature.params
      body: (closure)? @entity.method.body) @entity.method))

; Body-less declarations (interface and abstract methods)
(class_definition
  body: (closure
    (function_declaration
      function: (identifier) @entity.method.name @entity.method.signature.name
      parameters: (parameter_list) @entity.method.params @entity.method.signature.params) @entity.method))

; Enum definition, told apart by keyword rather than body shape: the
; current grammar still parses the enum body as a closure (constants
; recovered inside an error node), while a disambiguated grammar yields
; an enum body, so both shapes are accepted here.
(class_definition
  "enum"
  name: (identifier) @entity.enum.name @entity.enum.signature.name
  body: [(closure) (enum_body)] @entity.enum.body) @entity.enum

; Enum constants, with or without construction arguments
(enum_constant
  name: (identifier) @entity.enum_constant.name @entity.enum_constant.signature.name
  args: (argument_list)? @entity.enum_constant.signature.arguments) @entity.enum_constant

; Methods with bodies nested in the enum body
(class_definition
  body: (enum_body
    (function_definition
      function: (identifier) @entity.method.name @entity.method.signature.name
      parameters: (parameter_list) @entity.method.params @entity.method.signature.params
      body: (closure)? @entity.method.body) @entity.method))

; Body-less declarations nested in the enum body
(class_definition
  body: (enum_body
    (function_declaration
      function: (identifier) @entity.method.name @entity.method.signature.name
      parameters: (parameter_list) @entity.method.params @entity.method.signature.params) @entity.method))

; Fields declared in the enum body
(class_definition
  body: (enum_body
    (declaration
      name: (identifier) @entity.field.name @entity.field.signature.name) @entity.field))

; Annotation usage, buffered as pending annotations so `@Test`
; methods are promoted to `TestCase` like in Java and Kotlin
(annotation
  (identifier) @entity.annotation.name) @entity.annotation

(source_file
  (function_definition
    function: (identifier) @entity.function.name @entity.function.signature.name
    parameters: (parameter_list) @entity.function.params @entity.function.signature.params
    body: (closure)? @entity.function.body) @entity.function)

(class_definition
  body: (closure
    (declaration
      name: (identifier) @entity.field.name @entity.field.signature.name) @entity.field))

(function_definition
  body: (closure
    (declaration
      name: (identifier) @entity.variable.name @entity.variable.signature.name) @entity.variable))

(groovy_package
  (qualified_name) @entity.package.name) @entity.package
"#
}

/// Get call query for Groovy
///
/// Returns Tree-sitter query patterns for identifying Groovy call relationships:
/// - Parenthesized calls (`foo(...)`)
/// - Juxtaposed command calls (`implementation 'g:a:v'`)
/// - Higher-order calls passing a trailing closure
pub fn call_query() -> &'static str {
    r#"
(function_call
  function: [(identifier) (dotted_identifier)] @call.function.name
  (argument_list)? @call.function.arguments) @call.function

(function_call
  function: [(identifier) (dotted_identifier)] @call.hof.method.name
  (argument_list
    (closure) @call.hof.callback)) @call.hof

(juxt_function_call
  [(identifier) (dotted_identifier)] @call.function.name
  (argument_list)? @call.function.arguments) @call.function

(juxt_function_call
  [(identifier) (dotted_identifier)] @call.hof.method.name
  (argument_list
    (closure) @call.hof.callback)) @call.hof
"#
}

/// Get dependency query for Groovy
///
/// Returns Tree-sitter query patterns for identifying Groovy dependencies:
/// - Import declarations (qualified and wildcard)
/// - Superclass references (extends inheritance)
pub fn dependency_query() -> &'static str {
    r#"
(groovy_import
  (qualified_name) @dependency.import.name) @dependency.import

(groovy_import
  (wildcard_import) @dependency.import.wildcard) @dependency.import.wildcard

; Superclass clause (extends inheritance)
(class_definition
  superclass: [(identifier) (qualified_name) (dotted_identifier)] @dependency.extend.name) @dependency.extend
"#
}

/// Get comment query for Groovy
///
/// Returns Tree-sitter query patterns for identifying Groovy comments.
/// Line and block comments share the `comment` node, so both feed line spans.
pub fn comment_query() -> &'static str {
    r#"
(comment) @comment.line
"#
}

/// Get behavior query for Groovy
pub fn behavior_query() -> &'static str {
    r#"
(assignment) @behavior.data.bind
(declaration) @behavior.data.bind
(closure) @behavior.data.bind
"#
}

/// Get control-flow query for Groovy
pub fn control_flow_query() -> &'static str {
    r#"
(if_statement) @control.flow.if
(for_loop) @control.flow.loop
(for_in_loop) @control.flow.loop
(while_loop) @control.flow.loop
(do_while_loop) @control.flow.loop
(switch_statement) @control.flow.match
(try_statement) @control.flow.try
(return) @control.flow.return
"#
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Query;

    /// Validate query syntax and return detailed error information
    fn validate_query_syntax(query_name: &str, query_str: &str) -> Result<(), String> {
        let lang: tree_sitter::Language = tree_sitter_groovy::language();
        match Query::new(&lang, query_str) {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("Query '{query_name}' syntax error: {e:?}")),
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
        let result = validate_query_syntax("behavior_query", behavior_query());
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
