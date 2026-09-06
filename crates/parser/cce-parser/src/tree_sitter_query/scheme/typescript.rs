//! TypeScript language query schemes
//!
//! TypeScript extends JavaScript with additional type-related constructs:
//! - Interfaces
//! - Type aliases
//! - Enums
//! - Abstract classes
//! - Generic type parameters
//! - Type annotations

fn entity_ts_only() -> &'static str {
    r#"
; ============================================
; TypeScript-specific Entity Definitions
; ============================================

; Class declaration (name is type_identifier in TS grammar; heritage optional)
(class_declaration
  name: (type_identifier) @entity.class.name
  (class_heritage
    (extends_clause
      value: (_) @entity.class.base
    )
  )?
  body: (class_body) @entity.class.body
) @entity.class

; Class expression
(class
  name: (type_identifier)? @entity.class_expression.name
) @entity.class_expression

; Interface declaration
(interface_declaration
  name: (type_identifier) @entity.interface.name
  body: (interface_body) @entity.interface.body
) @entity.interface

; Type alias
(type_alias_declaration
  name: (type_identifier) @entity.type_alias.name
  value: (_) @entity.type_alias.value
) @entity.type_alias

; Enum declaration
(enum_declaration
  name: (identifier) @entity.enum.name
  body: (enum_body) @entity.enum.body
) @entity.enum

; Enum member
(enum_body
  (property_identifier) @entity.enum_member.name
) @entity.enum_member

; Properties and Fields
; ============================================

; Public field definition
(public_field_definition
  name: (property_identifier) @entity.property.name
) @entity.property

; Interface property signature
(interface_body
  (property_signature
    name: (property_identifier) @entity.property.name
  ) @entity.property
)

; Annotated variable declarations (const/let/var with a type annotation).
; These duplicate the shared untyped patterns with an extra `type` capture;
; same-span dedup keeps the first (annotated) match because ts-only patterns
; precede the shared ones in the composed query.
(lexical_declaration
  (variable_declarator
    name: (identifier) @entity.variable.const.name
    type: (type_annotation (_) @entity.variable.const.type)
    value: (_)? @entity.variable.const.value
  )
) @entity.variable.const

(lexical_declaration
  (variable_declarator
    name: (identifier) @entity.variable.let.name
    type: (type_annotation (_) @entity.variable.let.type)
    value: (_)? @entity.variable.let.value
  )
) @entity.variable.let

(variable_declaration
  (variable_declarator
    name: (identifier) @entity.variable.var.name
    type: (type_annotation (_) @entity.variable.var.type)
    value: (_)? @entity.variable.var.value
  )
) @entity.variable.var

; ============================================
; Namespaces and Modules
; ============================================

; Namespace declaration
(internal_module
  name: (identifier) @entity.namespace.name
) @entity.namespace

; Module declaration
(module
  name: (identifier) @entity.module.name
) @entity.module

"#
}

/// TypeScript-specific function and method patterns with return_type captures.
///
/// These override the shared JS patterns so that TypeScript return type
/// annotations (`: type`) are captured into `entity.return_type`.
fn entity_ts_function_method_patterns() -> &'static str {
    r#"
; ============================================
; Methods (with return_type capture)
; ============================================

; Method definition (excluding constructor)
(method_definition
  name: (property_identifier) @entity.method.name
  return_type: (type_annotation (_)? @entity.method.return_type)?
) @entity.method

; Constructor method
(method_definition
  name: (property_identifier) @entity.constructor.name
  (#eq? @entity.constructor.name "constructor")
) @entity.constructor

; Getter method
(method_definition
  name: (property_identifier) @entity.method.getter.name
  return_type: (type_annotation (_)? @entity.method.getter.return_type)?
) @entity.method.getter

; Setter method
(method_definition
  name: (property_identifier) @entity.method.setter.name
) @entity.method.setter

; ============================================
; Functions (with return_type capture)
; ============================================

; Named function declaration
(function_declaration
  name: (identifier) @entity.function.name
  parameters: (formal_parameters) @entity.function.params
  return_type: (type_annotation (_)? @entity.function.return_type)?
  body: (_) @entity.function.body
) @entity.function

; Overload signatures (no body): `function combine(a: number): number;`
; The implementation pattern above requires `body`, so signatures would be
; dropped and overload sets would collapse to the implementation. Keep each
; signature as its own callable entity so overload resolution can select by
; argument types.
(function_declaration
  name: (identifier) @entity.function.overload.name
  parameters: (formal_parameters) @entity.function.overload.params
  return_type: (type_annotation (_)? @entity.function.overload.return_type)?
) @entity.function.overload

; Generator function declaration
(generator_function_declaration
  name: (identifier) @entity.function.generator.name
  return_type: (type_annotation (_)? @entity.function.generator.return_type)?
) @entity.function.generator

; Arrow function assigned to variable (lexical declaration)
(lexical_declaration
  (variable_declarator
    name: (identifier) @entity.function.arrow.name
    value: (arrow_function
      return_type: (type_annotation (_)? @entity.function.arrow.return_type)?
    )
  )
) @entity.function.arrow

; Arrow function assigned to variable (var declaration)
(variable_declaration
  (variable_declarator
    name: (identifier) @entity.function.arrow_var.name
    value: (arrow_function
      return_type: (type_annotation (_)? @entity.function.arrow_var.return_type)?
    )
  )
) @entity.function.arrow_var

; Function expression assigned to variable (lexical declaration)
(lexical_declaration
  (variable_declarator
    name: (identifier) @entity.function.expression.name
    value: (function_expression
      return_type: (type_annotation (_)? @entity.function.expression.return_type)?
    )
  )
) @entity.function.expression

; Function expression assigned to variable (var declaration)
(variable_declaration
  (variable_declarator
    name: (identifier) @entity.function.expression_var.name
    value: (function_expression
      return_type: (type_annotation (_)? @entity.function.expression_var.return_type)?
    )
  )
) @entity.function.expression_var
"#
}

/// Get entity query for TypeScript
///
/// Returns Tree-sitter query patterns for identifying TypeScript code entities:
/// - TypeScript-specific: interfaces, type aliases, enums, namespaces
/// - TypeScript functions/methods with return_type captures
/// - All other JS-shared entities (imports, variables, properties, etc.)
pub fn entity_query() -> String {
    let mut query = String::new();
    query.push_str(entity_ts_only());
    query.push_str(entity_ts_function_method_patterns());
    query.push_str(javascript::entity_non_function_patterns());
    query
}

/// Get TypeScript-specific call query patterns
///
/// Returns patterns unique to TypeScript:
/// - Generic function calls with type_arguments
fn call_ts_only() -> &'static str {
    r#"
; ============================================
; Generic Method Calls (TypeScript-specific)
; ============================================

; Generic function call (e.g., func<T>())
(call_expression
  function: (identifier) @call.generic.function.name
  type_arguments: (type_arguments)? @call.generic.type_args
) @call.generic
"#
}

/// Get call query for TypeScript
///
/// Returns Tree-sitter query patterns for identifying TypeScript call relationships:
/// - All JavaScript call patterns
/// - Generic method calls
pub fn call_query() -> String {
    let mut query = String::new();
    query.push_str(javascript::call_shared());
    query.push_str(call_ts_only());
    query
}

/// Get TypeScript-specific dependency query patterns
fn dependency_ts_only() -> &'static str {
    r#"
; ============================================
; Import-Require Declaration (import x = require("m"))
; ============================================

; TypeScript `import x = require("m")` parses as an `import_require_clause`
; (not a `call_expression`), so the shared `dependency.require` pattern
; never fires on it. Capture the module path directly.
; (Verified with `cargo run --example parse_js_require -p cce-parser`.)
(import_statement
  (import_require_clause
    source: (string) @dependency.require.ts_import.path
  )
) @dependency.require.ts_import

; ============================================
; Class Inheritance (extends clause)
; ============================================

; Class extends another class
(class_declaration
  name: (type_identifier) @entity.class.name
  (class_heritage
    (extends_clause
      value: (identifier) @dependency.class_extends.name
    )
  )
) @dependency.class_extends

; ============================================
; Class Implements Interface
; ============================================

; Class implements single interface
(class_declaration
  name: (type_identifier) @entity.class.name
  (class_heritage
    (implements_clause
      (type_identifier) @dependency.class_implements.name
    )
  )
) @dependency.class_implements

; Class implements multiple interfaces
(class_declaration
  (class_heritage
    (implements_clause
      (_) @dependency.class_implements.name
    )
  )
) @dependency.class_implements

; ============================================
; Interface Extension (extends clause)
; ============================================

; Interface extends another interface
(interface_declaration
  name: (type_identifier) @entity.interface.name
  (extends_type_clause
    type: (type_identifier) @dependency.interface_extends.name
  )
) @dependency.interface_extends

; Interface extends multiple interfaces
(interface_declaration
  (extends_type_clause
    type: (_) @dependency.interface_extends.name
  )
) @dependency.interface_extends

; ============================================
; Generic Type Parameters and Constraints
; ============================================

; Class with generic type parameter
(class_declaration
  name: (type_identifier) @entity.class.name
  type_parameters: (type_parameters
    (type_parameter
      name: (type_identifier) @dependency.generic_param.name
      constraint: (constraint
          (type_identifier) @dependency.generic_constraint.bound
        )
    )
  )
) @dependency.generic_constraint
"#
}

/// Get dependency query for TypeScript
///
/// Returns Tree-sitter query patterns for identifying TypeScript dependencies:
/// - All JavaScript dependency patterns (ES6 imports, CommonJS require)
pub fn dependency_query() -> String {
    let mut query = String::new();
    query.push_str(javascript::dependency_shared());
    query.push_str(dependency_ts_only());
    query
}

/// Get behavior query for TypeScript
pub fn behavior_query() -> String {
    javascript::behavior_query()
}

/// Get control-flow query for TypeScript
pub fn control_flow_query() -> &'static str {
    javascript::control_flow_query()
}

/// Import JavaScript functions
use crate::tree_sitter_query::scheme::javascript;

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Query;

    /// Dump AST for a TypeScript code snippet to verify field names
    fn dump_ts_ast(code: &str) {
        let mut parser = tree_sitter::Parser::new();
        let language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT;
        parser.set_language(&language.into()).unwrap();
        let tree = parser.parse(code, None).unwrap();
        print_node(tree.root_node(), code, 0);
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
    fn test_ts_function_return_type_ast() {
        dump_ts_ast("function foo(): string { return 'hi'; }");
    }

    #[test]
    fn test_ts_method_return_type_ast() {
        dump_ts_ast("class Foo { bar(): number { return 42; } }");
    }

    #[test]
    fn test_ts_arrow_return_type_ast() {
        dump_ts_ast("const baz = (): boolean => true;");
    }

    /// Validate query syntax and return detailed error information
    fn validate_query_syntax(query_name: &str, query_str: &str) -> Result<(), String> {
        let lang = tree_sitter_typescript::LANGUAGE_TYPESCRIPT;
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

    #[test]
    fn test_ts_only_queries_syntax_valid() {
        let result = validate_query_syntax("entity_ts_only", entity_ts_only());
        assert!(
            result.is_ok(),
            "Entity TS-only query syntax validation failed: {:?}",
            result.err()
        );

        let result = validate_query_syntax("call_ts_only", call_ts_only());
        assert!(
            result.is_ok(),
            "Call TS-only query syntax validation failed: {:?}",
            result.err()
        );

        let result = validate_query_syntax("dependency_ts_only", dependency_ts_only());
        assert!(
            result.is_ok(),
            "Dependency TS-only query syntax validation failed: {:?}",
            result.err()
        );
    }
}
