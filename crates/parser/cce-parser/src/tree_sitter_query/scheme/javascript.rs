//! JavaScript language query schemes
//!
//! Provides Tree-sitter query patterns for identifying and analyzing
//! JavaScript code entities, call relationships, and dependencies.
//!
//! Shared patterns are exposed as separate functions for reuse by TypeScript.

// ============================================================================
// Shared Entity Query Patterns (used by both JS and TS)
// ============================================================================

/// Get shared entity query patterns for JavaScript/TypeScript
///
/// Returns patterns that are identical between JS and TS:
/// - Methods (definition, constructor, getter, setter)
/// - Functions (declaration, generator, arrow, expression)
/// - Variables (const, let, var)
/// - Decorators
/// - Export entities
pub fn entity_shared() -> String {
    let mut q = String::new();
    q.push_str(entity_function_method_patterns());
    q.push_str(entity_non_function_patterns());
    q
}

/// Function and method patterns shared between JS and TS.
///
/// TypeScript overrides this with patterns that include `return_type` captures
/// for type annotations. For JavaScript, return type is inferred from return
/// statement values.
pub fn entity_function_method_patterns() -> &'static str {
    r#"
; ============================================
; Methods
; ============================================

; Method definition (excluding constructor)
(method_definition
  name: (property_identifier) @entity.method.name
  parameters: (formal_parameters)
  body: (statement_block
    (return_statement (_) @entity.method.return_type)?
  )
) @entity.method

; Constructor method
(method_definition
  name: (property_identifier) @entity.constructor.name
  (#eq? @entity.constructor.name "constructor")
) @entity.constructor

; Getter method
(method_definition
  name: (property_identifier) @entity.method.getter.name
  parameters: (formal_parameters)
  body: (statement_block
    (return_statement (_) @entity.method.getter.return_type)?
  )
) @entity.method.getter

; Setter method
(method_definition
  name: (property_identifier) @entity.method.setter.name
) @entity.method.setter

; ============================================
; Functions
; ============================================

; Named function declaration
(function_declaration
  name: (identifier) @entity.function.name
  parameters: (formal_parameters) @entity.function.params
  body: (statement_block
    (return_statement (_) @entity.function.return_type)?
  ) @entity.function.body
) @entity.function

; Generator function declaration
(generator_function_declaration
  name: (identifier) @entity.function.generator.name
  parameters: (formal_parameters)
  body: (statement_block
    (return_statement (_) @entity.function.generator.return_type)?
  )
) @entity.function.generator

; Arrow function assigned to variable (lexical declaration)
(lexical_declaration
  (variable_declarator
    name: (identifier) @entity.function.arrow.name
    value: (arrow_function)
  )
) @entity.function.arrow

; Arrow function assigned to variable (var declaration)
(variable_declaration
  (variable_declarator
    name: (identifier) @entity.function.arrow_var.name
    value: (arrow_function)
  )
) @entity.function.arrow_var

; Function expression assigned to variable (lexical declaration)
(lexical_declaration
  (variable_declarator
    name: (identifier) @entity.function.expression.name
    value: (function_expression
      body: (statement_block
        (return_statement (_) @entity.function.expression.return_type)?
      )
    )
  )
) @entity.function.expression

; Function expression assigned to variable (var declaration)
(variable_declaration
  (variable_declarator
    name: (identifier) @entity.function.expression_var.name
    value: (function_expression
      body: (statement_block
        (return_statement (_) @entity.function.expression_var.return_type)?
      )
    )
  )
) @entity.function.expression_var
"#
}

/// Non-function/method patterns shared between JS and TS.
pub fn entity_non_function_patterns() -> &'static str {
    r#"
; ============================================
; Imports and Requires
; ============================================

; ES6 import statement
(import_statement
  source: (string) @entity.import.name
) @entity.import

; require() call
(call_expression
  function: (identifier) @entity.require
  arguments: (arguments
    (string) @entity.require.name
  )
  (#eq? @entity.require "require")
) @entity.require

; ============================================
; Variables
; ============================================

; Variable declaration with const
(lexical_declaration
  (variable_declarator
    name: (identifier) @entity.variable.const.name
    value: (_)? @entity.variable.const.value
  )
) @entity.variable.const

; Variable declaration with let
(lexical_declaration
  (variable_declarator
    name: (identifier) @entity.variable.let.name
    value: (_)? @entity.variable.let.value
  )
) @entity.variable.let

; Variable declaration with var
(variable_declaration
  (variable_declarator
    name: (identifier) @entity.variable.var.name
    value: (_)? @entity.variable.var.value
  )
) @entity.variable.var

; ============================================
; Object Properties
; ============================================

; Object property
(pair
  key: (property_identifier) @entity.property.name
  value: (_)? @entity.property.value
) @entity.property

; ============================================
; Decorators
; ============================================

; Decorator on class or method
(decorator
  (identifier) @entity.decorator.name
) @entity.decorator

; Decorator with arguments
(decorator
  (call_expression
    function: (identifier) @entity.decorator.call.name
  )
) @entity.decorator.call

; ============================================
; Export and Import Entities
; ============================================

; Export declaration
(export_statement
  declaration: (_) @entity.export.declaration
) @entity.export

; Default export
(export_statement
  value: (identifier) @entity.export.default.name
) @entity.export.default
"#
}

/// Get JavaScript-specific entity query patterns
///
/// Returns patterns unique to JavaScript:
/// - Class declaration with identifier name
/// - Class expression with optional name
pub fn entity_js_only() -> &'static str {
    r#"
; ============================================
; Types (Classes)
; ============================================

; Class declaration
(class_declaration
  name: (identifier) @entity.class.name
  (class_heritage
    (identifier) @entity.class.base
  )
  body: (class_body) @entity.class.body
) @entity.class

; Class expression
(class
  name: (identifier)? @entity.class_expression.name
) @entity.class_expression
"#
}

// ============================================================================
// Shared Call Query Patterns (used by both JS and TS)
// ============================================================================

/// Get shared call query patterns for JavaScript/TypeScript
///
/// Returns patterns that are identical between JS and TS:
/// - Direct function calls
/// - Method calls (object.method(), chained)
/// - Constructor calls (new ClassName())
/// - Special function calls (call, apply, bind)
/// - Async/Promise calls
pub fn call_shared() -> &'static str {
    r#"
; ============================================
; 1. Direct Calls
; ============================================

; Direct function call
(call_expression
  function: (identifier) @call.function.name
  arguments: (arguments)? @call.function.arguments
) @call.function

; ============================================
; 2. Method Calls
; ============================================

; Object method call (e.g., obj.method())
(call_expression
  function: (member_expression
    object: (_) @call.method.object
    property: (property_identifier) @call.method.function
  )
) @call.method

; Chained method call (e.g., obj.method1().method2())
(call_expression
  function: (member_expression
    object: (call_expression) @call.method.chained.from
    property: (property_identifier) @call.method.chained.to.name
  )
) @call.method.chained

; ============================================
; 3. Constructor Calls
; ============================================

; New expression (constructor call)
(new_expression
  constructor: (identifier) @call.constructor.name
  arguments: (arguments)? @call.constructor.arguments
) @call.constructor

; New expression with member expression
(new_expression
  constructor: (member_expression
    object: (_) @call.constructor.member.object
    property: (property_identifier) @call.constructor.member.property
  )
) @call.constructor.member

; ============================================
; 4. Special Function Calls
; ============================================

; call() invocation
(call_expression
  function: (member_expression
    object: (identifier) @call.special.call.object
    property: (property_identifier) @call.special.call.method
  )
  (#eq? @call.special.call.method "call")
) @call.special.call

; apply() invocation
(call_expression
  function: (member_expression
    object: (identifier) @call.special.apply.object
    property: (property_identifier) @call.special.apply.method
  )
  (#eq? @call.special.apply.method "apply")
) @call.special.apply

; bind() invocation
(call_expression
  function: (member_expression
    object: (identifier) @call.special.bind.object
    property: (property_identifier) @call.special.bind.method
  )
  (#eq? @call.special.bind.method "bind")
) @call.special.bind

; ============================================
; 5. Async/Promise Calls
; ============================================

; await expression
(await_expression) @call.async

; Promise.then() call
(call_expression
  function: (member_expression
    object: (call_expression) @call.promise.then.object
    property: (property_identifier) @call.promise.then.method
  )
  (#eq? @call.promise.then.method "then")
) @call.promise.then

; Promise.catch() call
(call_expression
  function: (member_expression
    object: (call_expression) @call.promise.catch.object
    property: (property_identifier) @call.promise.catch.method
  )
  (#eq? @call.promise.catch.method "catch")
) @call.promise.catch

; ============================================
; 6. Higher-Order Function Calls
; ============================================

; Higher-order function call with arrow function argument
(call_expression
  function: (_) @call.hof.name
  arguments: (arguments
    (arrow_function) @call.hof.callback
  )
) @call.hof.arrow

; Higher-order function call with function expression argument
(call_expression
  function: (_) @call.hof.name
  arguments: (arguments
    (function_expression) @call.hof.callback
  )
) @call.hof.function_expr
"#
}

// ============================================================================
// Shared Dependency Query Patterns (used by both JS and TS)
// ============================================================================

/// Get shared dependency query patterns for JavaScript/TypeScript
///
/// Returns patterns that are identical between JS and TS:
/// - ES6 import declarations
/// - CommonJS require calls
/// - Dynamic import
/// - Export dependencies
pub fn dependency_shared() -> &'static str {
    r#"
; ============================================
; 1. ES6 Import Declarations
; ============================================

; Import entire module
(import_statement
  source: (string) @dependency.import.path
) @dependency.import

; Import with default binding
(import_statement
  (import_clause
    (identifier) @dependency.import.default.name
  )
  source: (string) @dependency.import.default.path
) @dependency.import.default

; Import with namespace binding (import * as name)
(import_statement
  (import_clause
    (namespace_import
      (identifier) @dependency.import.namespace.alias
    )
  )
  source: (string) @dependency.import.namespace.path
) @dependency.import.namespace

; Import named bindings (import { a, b })
(import_statement
  (import_clause
    (named_imports
      (import_specifier
        name: (identifier) @dependency.import.named.name
        alias: (identifier)? @dependency.import.named.alias
      )
    )
  )
  source: (string) @dependency.import.named.path
) @dependency.import.named

; ============================================
; 2. CommonJS Require
; ============================================

; require() call
(call_expression
  function: (identifier) @dependency.require.function
  arguments: (arguments
    (string) @dependency.require.path
  )
  (#eq? @dependency.require.function "require")
) @dependency.require

; require() assigned to variable
(variable_declaration
  (variable_declarator
    name: (identifier) @dependency.require.name
    value: (call_expression
      function: (identifier) @dependency.require.function
      arguments: (arguments
        (string) @dependency.require.path
      )
    )
  )
) @dependency.require.variable

; lexical declaration require()
(lexical_declaration
  (variable_declarator
    name: (identifier) @dependency.require.name
    value: (call_expression
      function: (identifier) @dependency.require.function
      arguments: (arguments
        (string) @dependency.require.path
      )
    )
  )
) @dependency.require.lexical

; ============================================
; 3. Dynamic Import
; ============================================

; import() dynamic import
(call_expression
  function: (identifier) @dependency.import.dynamic.function
  arguments: (arguments
    (string) @dependency.import.dynamic.path
  )
  (#eq? @dependency.import.dynamic.function "import")
) @dependency.import.dynamic

; ============================================
; 4. Export Dependencies
; ============================================

; Export from module
(export_statement
  source: (string) @dependency.export.from.source
) @dependency.export.from
"#
}

// ============================================================================
// JavaScript Public API
// ============================================================================

/// Get entity query for JavaScript
///
/// Returns Tree-sitter query patterns for identifying JavaScript code entities:
/// - Class definitions
/// - Method definitions
/// - Function declarations (named, arrow, generator)
/// - Variables (const, let, var)
/// - Decorators
/// - Test entities (suites, cases, hooks, assertions, mocks)
pub fn entity_query() -> String {
    let mut query = String::new();
    query.push_str(entity_js_only());
    query.push_str(&entity_shared());
    query
}

/// Get call query for JavaScript
///
/// Returns Tree-sitter query patterns for identifying JavaScript call relationships:
/// - Direct function calls
/// - Method calls (object.method())
/// - Chained method calls
/// - Constructor calls (new ClassName())
/// - Call/Apply/Bind invocations
pub fn call_query() -> &'static str {
    call_shared()
}

/// Get dependency query for JavaScript
///
/// Returns Tree-sitter query patterns for identifying JavaScript dependencies:
/// - Import declarations (ES6 modules)
/// - Require calls (CommonJS)
/// - Export declarations
pub fn dependency_query() -> &'static str {
    dependency_shared()
}

// ============================================================================
// CSS-in-JS Query Patterns
// ============================================================================

/// Get CSS-in-JS query patterns
///
/// Returns patterns for detecting CSS-in-JS usage:
/// - styled-components: styled.div`...` or styled('div')`...`
/// - emotion: css`...`
/// - template literals containing CSS-like content
pub fn css_in_js_query() -> &'static str {
    r#"
; ============================================
; styled-components Patterns
; ============================================

; styled.tag`...` (e.g., styled.div`...`)
(call_expression
  function: (member_expression
    object: (identifier) @css_in_js.styled.object
    (#eq? @css_in_js.styled.object "styled")
    property: (property_identifier) @css_in_js.styled.tag
  )
  arguments: (template_string) @css_in_js.styled.content
) @css_in_js.styled

; styled('tag')`...` or styled(Component)`...`
(call_expression
  function: (call_expression
    function: (identifier) @css_in_js.styled_func.name
    (#eq? @css_in_js.styled_func.name "styled")
    arguments: (arguments
      (_)? @css_in_js.styled_func.target
    )
  )
  arguments: (template_string) @css_in_js.styled_func.content
) @css_in_js.styled_func

; styled(Component).attrs(...)`...`
(call_expression
  function: (call_expression
    function: (member_expression
      object: (call_expression
        function: (identifier) @css_in_js.styled_attrs.styled
        (#eq? @css_in_js.styled_attrs.styled "styled")
      )
      property: (property_identifier) @css_in_js.styled_attrs.method
      (#eq? @css_in_js.styled_attrs.method "attrs")
    )
  )
  arguments: (template_string) @css_in_js.styled_attrs.content
) @css_in_js.styled_attrs

; ============================================
; Emotion Patterns
; ============================================

; css`...` template literal
(call_expression
  function: (identifier) @css_in_js.emotion.css_func
  (#eq? @css_in_js.emotion.css_func "css")
  arguments: (template_string) @css_in_js.emotion.content
) @css_in_js.emotion

; cx`...` template literal (emotion cx)
(call_expression
  function: (identifier) @css_in_js.emotion.cx_func
  (#eq? @css_in_js.emotion.cx_func "cx")
  arguments: (template_string) @css_in_js.emotion.cx_content
) @css_in_js.emotion.cx

; ============================================
; Other CSS-in-JS Libraries
; ============================================

; createGlobalStyle`...` (styled-components)
(call_expression
  function: (identifier) @css_in_js.global_style.func
  (#eq? @css_in_js.global_style.func "createGlobalStyle")
  arguments: (template_string) @css_in_js.global_style.content
) @css_in_js.global_style

; keyframes`...` (styled-components/emotion)
(call_expression
  function: (identifier) @css_in_js.keyframes.func
  (#eq? @css_in_js.keyframes.func "keyframes")
  arguments: (template_string) @css_in_js.keyframes.content
) @css_in_js.keyframes

; injectGlobal`...` (emotion)
(call_expression
  function: (identifier) @css_in_js.inject_global.func
  (#eq? @css_in_js.inject_global.func "injectGlobal")
  arguments: (template_string) @css_in_js.inject_global.content
) @css_in_js.inject_global
"#
}

/// Get behavior query for JavaScript
pub fn behavior_query() -> String {
    let mut query = String::from(
        r#"
(lexical_declaration) @behavior.data.bind
(variable_declaration) @behavior.data.bind
(assignment_expression) @behavior.data.bind
(member_expression) @behavior.data.reference
(subscript_expression) @behavior.data.reference
(object) @behavior.data.object
(array) @behavior.data.array
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

/// Get control-flow query for JavaScript
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
(yield_expression) @control.flow.yield
"#
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Query;

    /// Validate query syntax and return detailed error information
    fn validate_query_syntax(query_name: &str, query_str: &str) -> Result<(), String> {
        let lang = tree_sitter_javascript::LANGUAGE;
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
    fn test_shared_queries_syntax_valid() {
        let result = validate_query_syntax("entity_shared", &entity_shared());
        assert!(
            result.is_ok(),
            "Entity shared query syntax validation failed: {:?}",
            result.err()
        );

        let result = validate_query_syntax("call_shared", call_shared());
        assert!(
            result.is_ok(),
            "Call shared query syntax validation failed: {:?}",
            result.err()
        );

        let result = validate_query_syntax("dependency_shared", dependency_shared());
        assert!(
            result.is_ok(),
            "Dependency shared query syntax validation failed: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_css_in_js_query_syntax_valid() {
        let result = validate_query_syntax("css_in_js_query", css_in_js_query());
        assert!(
            result.is_ok(),
            "CSS-in-JS query syntax validation failed: {:?}",
            result.err()
        );
    }
}
