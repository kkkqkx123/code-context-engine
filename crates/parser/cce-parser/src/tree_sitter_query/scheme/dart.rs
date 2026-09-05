//! Dart language query schemes
//!
//! Provides Tree-sitter query patterns for identifying and analyzing
//! Dart code entities, call relationships, and dependencies.

/// Get entity query for Dart
///
/// Returns Tree-sitter query patterns for identifying Dart code entities:
/// - Type definitions (class, mixin, enum, extension)
/// - Methods (including constructors, getters, setters)
/// - Functions (top-level)
/// - Variables (top-level, local, fields)
/// - Type aliases (typedef)
pub fn entity_query() -> &'static str {
    r#"
; ============================================
; 0. Imports
; ============================================

; Library import
(import_specification
  uri: (configurable_uri
    (uri
      (string_literal) @entity.import.name
    )
  )
) @entity.import

; ============================================
; 1. Types
; ============================================

; Class definition
(class_declaration
  name: (identifier) @entity.class.name
  body: (class_body) @entity.class.body
) @entity.class

; Mixin definition
(mixin_declaration
  name: (identifier) @entity.mixin.name
) @entity.mixin

; Enum definition
(enum_declaration
  name: (identifier) @entity.enum.name
  body: (enum_body) @entity.enum.body
) @entity.enum

; Enum constant
(enum_constant
  name: (identifier) @entity.enum_constant.name
) @entity.enum_constant

; Extension definition
(extension_declaration
  name: (identifier)? @entity.extension.name
) @entity.extension

; ============================================
; 2. Methods
; ============================================

; Method signature (method definition)
(method_signature
  (function_signature
    return_type: (_)? @entity.method.return_type
    name: (identifier) @entity.method.name
    (formal_parameter_list) @entity.method.params
  )
) @entity.method

; Constructor signature
(constructor_signature
  name: (identifier) @entity.constructor.name
  (formal_parameter_list) @entity.constructor.params
) @entity.constructor

; Factory constructor
(factory_constructor_signature
  name: (identifier) @entity.constructor.factory.name
) @entity.constructor.factory

; Getter signature
(getter_signature
  name: (identifier) @entity.method.getter.name
) @entity.method.getter

; Setter signature
(setter_signature
  name: (identifier) @entity.method.setter.name
) @entity.method.setter

; ============================================
; 3. Functions
; ============================================

; Top-level function signature
(function_signature
  return_type: (_)? @entity.function.return_type
  name: (identifier) @entity.function.name
  (formal_parameter_list) @entity.function.params
) @entity.function

; ============================================
; 4. Variables and Fields
; ============================================

; Top-level variable declaration (var)
(initialized_identifier
  name: (identifier) @entity.variable.name
  value: (_)? @entity.variable.value
) @entity.variable

; Local variable declaration inside function bodies, e.g.
; `var count = 42;` or `String explicit = 'typed';`
(local_variable_declaration
  (initialized_variable_definition
    (type_identifier)? @entity.variable.type
    name: (identifier) @entity.variable.name
    value: (_)? @entity.variable.value
  )
) @entity.variable

; Static final declaration (final/const)
(static_final_declaration
  name: (identifier) @entity.constant.name
) @entity.constant

; Field declaration (inside class)
(declaration
  (type_identifier)? @entity.field.type
  (initialized_identifier_list
    (initialized_identifier
      name: (identifier) @entity.field.name
      value: (_)? @entity.field.value
    )
  )
) @entity.field

; ============================================
; 5. Type Aliases
; ============================================

; Typedef declaration
(type_alias
  (type_identifier) @entity.typedef.name
) @entity.typedef

; ============================================
; 6. Parameters
; ============================================

"#
}

/// Get comment query for Dart
///
/// Returns Tree-sitter query patterns for identifying Dart comments.
/// Dart has:
/// - Line comments (// ...)
/// - Documentation comments (/// ...)
/// - Block comments (/* ... */)
/// - Documentation block comments (/** ... */)
pub fn comment_query() -> &'static str {
    r#"
; ============================================
; Comments
; ============================================

; Comment (includes line, block, and doc comments)
(comment) @comment.line
"#
}

/// Get call query for Dart
///
/// Returns Tree-sitter query patterns for identifying Dart call relationships:
/// - Method calls (obj.method())
/// - Constructor calls (new ClassName())
/// - Function calls (function())
/// - Selector access (obj.property)
pub fn call_query() -> &'static str {
    r#"
; ============================================
; 1. Method Calls
; ============================================

; Method call with selector (obj.method())
(expression_statement
  (identifier) @call.method.object
  (selector
    (unconditional_assignable_selector
      (identifier) @call.method.function
    )
  )
) @call.method

; ============================================
; 2. Constructor Calls
; ============================================

; New expression (new ClassName())
(new_expression
  type: (type_identifier) @call.constructor.name
  arguments: (arguments) @call.constructor.arguments
) @call.constructor

; ============================================
; 3. Function Calls
; ============================================

; Function call (function())
(expression_statement
  (identifier) @call.function.name
  (selector
    (argument_part
      (arguments) @call.function.arguments
    )
  )
) @call.function

; Function call in local variable initializer (final x = combine(1, 2))
(initialized_variable_definition
  value: (identifier) @call.function.name
  value: (selector
    (argument_part
      (arguments) @call.function.arguments
    )
  )
) @call.function

; Function call in top-level variable initializer (final x = combine(1, 2))
(initialized_identifier
  value: (identifier) @call.function.name
  value: (selector
    (argument_part
      (arguments) @call.function.arguments
    )
  )
) @call.function

; ============================================
; 4. Property Access
; ============================================

; Selector access (obj.property)
(selector
  (unconditional_assignable_selector
    (identifier) @call.getter.name
  )
) @call.getter
"#
}

/// Get dependency query for Dart
///
/// Returns Tree-sitter query patterns for identifying Dart dependencies:
/// - Import directives (import 'package:...';)
/// - Export directives (export '...';)
/// - Part directives (part 'file.dart';)
/// - Extends/implements/with clauses
pub fn dependency_query() -> &'static str {
    r#"
; ============================================
; 1. Import Directives
; ============================================

; Library import
(import_specification
  uri: (configurable_uri
    (uri
      (string_literal) @dependency.import.path
    )
  )
) @dependency.import

; Import with alias (import '...' as alias;)
(import_specification
  uri: (configurable_uri
    (uri
      (string_literal) @dependency.import.alias.path
    )
  )
  alias: (identifier) @dependency.import.alias.name
) @dependency.import.alias

; ============================================
; 2. Export Directives
; ============================================

; Export directive
(library_export
  uri: (configurable_uri
    (uri
      (string_literal) @dependency.export.path
    )
  )
) @dependency.export

; ============================================
; 3. Part Directives
; ============================================

; Part directive (part 'file.dart';)
(part_directive
  uri: (uri
    (string_literal) @dependency.part.path
  )
) @dependency.part

; Part of directive (part of 'library';)
(part_of_directive
  (uri
    (string_literal) @dependency.part_of.path
  )
) @dependency.part_of

; ============================================
; 4. Type Inheritance
; ============================================

; Superclass (extends)
(superclass
  type: (type_identifier) @dependency.extend.name
) @dependency.extend

; Interfaces (implements)
(interfaces
  (type_identifier) @dependency.implement.name
) @dependency.implement

; Mixins (with)
(mixins
  (type_identifier) @dependency.mixin.name
) @dependency.mixin
"#
}

/// Get behavior query for Dart
pub fn behavior_query() -> String {
    let mut query = String::from(
        r#"
(assignment_expression) @behavior.data.bind
(declaration) @behavior.data.bind
(postfix_expression) @behavior.data.reference
(expression_statement) @behavior.data.statement
(try_statement) @behavior.effect.error
(throw_expression) @behavior.effect.error
"#,
    );
    query.push_str(&super::common::bitwise_shift_operator_query_fallback(
        "shift_expression",
    ));
    query
}

/// Get control-flow query for Dart
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
(yield_statement) @control.flow.yield
"#
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Query;

    /// Validate query syntax and return detailed error information
    fn validate_query_syntax(query_name: &str, query_str: &str) -> Result<(), String> {
        let lang = tree_sitter_dart::LANGUAGE;
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

    /// Verify that the Dart behavior query correctly distinguishes `<<` from `>>`
    /// in shift expressions, despite Dart's grammar lacking named `operator` fields.
    ///
    /// This confirms the fallback approach (literal anonymous token matching)
    /// works where the standard field-based `(#eq?)` approach cannot.
    #[test]
    fn test_behavior_query_distinguishes_shift_operators() {
        use streaming_iterator::StreamingIterator;

        let mut parser = tree_sitter::Parser::new();
        let language = tree_sitter_dart::LANGUAGE;
        parser.set_language(&language.into()).unwrap();

        let cases: Vec<(&str, &str, bool, bool)> = vec![
            ("void f() { int x = 1 << 2; }", "shift_left", true, false),
            ("void f() { int x = 8 >> 1; }", "shift_right", false, true),
        ];

        let query = Query::new(&language.into(), &behavior_query())
            .expect("Behavior query should be valid");

        for (code, desc, expect_left, expect_right) in &cases {
            let tree = parser.parse(code, None).unwrap();
            let root = tree.root_node();

            let mut cursor = tree_sitter::QueryCursor::new();
            let mut cursor_matches = cursor.matches(&query, root, code.as_bytes());
            let mut matched_left = false;
            let mut matched_right = false;

            while let Some(m) = cursor_matches.next() {
                for c in m.captures {
                    let name = query.capture_names()[c.index as usize];
                    match name {
                        "behavior.op.shift_left" => matched_left = true,
                        "behavior.op.shift_right" => matched_right = true,
                        _ => {}
                    }
                }
            }

            assert_eq!(
                *expect_left, matched_left,
                "Expected shift_left={expect_left} for {desc}, got {matched_left}"
            );
            assert_eq!(
                *expect_right, matched_right,
                "Expected shift_right={expect_right} for {desc}, got {matched_right}"
            );
        }
    }
}
