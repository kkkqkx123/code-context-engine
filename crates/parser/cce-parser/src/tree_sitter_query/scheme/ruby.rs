//! Ruby language query schemes
//!
//! Provides Tree-sitter query patterns for identifying and analyzing
//! Ruby code entities, call relationships, and dependencies.

/// Get entity query for Ruby
///
/// Returns Tree-sitter query patterns for identifying Ruby code entities:
/// - Classes and modules
/// - Methods (instance, singleton, class)
/// - Constants and variables
pub fn entity_query() -> &'static str {
    r#"
; ============================================
; Requires and Includes
; ============================================

; require
(call
  method: (identifier) @entity.require
  (#eq? @entity.require "require")
  arguments: (argument_list
    (string
      (string_content) @entity.require.name
    )
  )
) @entity.require

; require_relative
(call
  method: (identifier) @entity.require
  (#eq? @entity.require "require_relative")
  arguments: (argument_list
    (string
      (string_content) @entity.require.name
    )
  )
) @entity.require

; include
(call
  method: (identifier) @entity.include
  (#eq? @entity.include "include")
  arguments: (argument_list
    (constant) @entity.include.name
  )
) @entity.include

; ============================================
; Class Definitions
; ============================================

; Class definition
(class
  name: (constant) @entity.class.name
  superclass: (constant)? @entity.class.superclass
  body: (body_statement) @entity.class.body
) @entity.class

; Singleton class
(singleton_class
  value: (self) @entity.singleton.target
  body: (body_statement) @entity.singleton.body
) @entity.singleton

; ============================================
; Module Definitions
; ============================================

; Module definition
(module
  name: (constant) @entity.module.name
  body: (body_statement) @entity.module.body
) @entity.module

; ============================================
; Method Definitions
; ============================================

; Instance method definition
(method
  name: (identifier) @entity.method.instance.name
  parameters: (method_parameters)? @entity.method.instance.params
  body: (body_statement)? @entity.method.instance.body
) @entity.method.instance

; Singleton method definition
(singleton_method
  object: (_) @entity.method.singleton.object
  name: (identifier) @entity.method.singleton.name
  parameters: (method_parameters)? @entity.method.singleton.params
  body: (body_statement)? @entity.method.singleton.body
) @entity.method.singleton

; ============================================
; Constant Definitions
; ============================================

; Constant assignment
(assignment
  left: (constant) @entity.const.name
  right: (_) @entity.const.value
) @entity.const

; ============================================
; Variable Declarations
; ============================================

; Global variable
(global_variable) @entity.variable.global

; Local variable assignment
(assignment
  left: (identifier) @entity.variable.local.name
  right: (_) @entity.variable.local.value
) @entity.variable.local

; ============================================
; Accessor Declarations
; ============================================

; attr_reader
(call
  method: (identifier) @entity.attr.reader
  (#eq? @entity.attr.reader "attr_reader")
  arguments: (argument_list
    (simple_symbol) @entity.attr.reader.name
  )
) @entity.attr.reader

; attr_writer
(call
  method: (identifier) @entity.attr.writer
  (#eq? @entity.attr.writer "attr_writer")
  arguments: (argument_list
    (simple_symbol) @entity.attr.writer.name
  )
) @entity.attr.writer

; attr_accessor
(call
  method: (identifier) @entity.attr.accessor
  (#eq? @entity.attr.accessor "attr_accessor")
  arguments: (argument_list
    (simple_symbol) @entity.attr.accessor.name
  )
) @entity.attr.accessor

; ============================================
; Alias and Undef
; ============================================

; Alias
(alias
  (identifier) @entity.alias.new
  (identifier) @entity.alias.old
) @entity.alias

; Undef
(undef
  (identifier) @entity.undef.name
) @entity.undef
"#
}

/// Get call query for Ruby
///
/// Returns Tree-sitter query patterns for identifying Ruby call relationships:
/// - Direct method calls
/// - Chained method calls
/// - Block calls (yield)
/// - Super calls
pub fn call_query() -> &'static str {
    r#"
; ============================================
; Direct Method Calls
; ============================================

; Direct method call
(call
  method: (identifier) @call.method.name
  arguments: (argument_list)? @call.method.arguments
) @call.method

; Method call with receiver
(call
  receiver: (_) @call.method.receiver
  method: (identifier) @call.method.name
  arguments: (argument_list)? @call.method.arguments
) @call.method

; Method call with operator
(binary
  left: (_) @call.binary.left
  operator: (_) @call.binary.operator
  right: (_) @call.binary.right
) @call.binary

; ============================================
; Chained Method Calls
; ============================================

; Chained method call
(call
  receiver: (call) @call.method.chained.from
  method: (identifier) @call.method.chained.to
) @call.method.chained

; ============================================
; Special Calls
; ============================================

; Yield expression
(yield) @call.yield

; Yield with arguments
(yield
  (argument_list) @call.yield.arguments
) @call.yield.args

; Super call
(super) @call.super

; Return expression
(return
  (_)? @call.return.value
) @call.return

; ============================================
; Scope Resolution
; ============================================

; Scope resolution (Module::Class)
(scope_resolution
  scope: (constant)? @call.scope.scope
  name: (constant) @call.scope.name
) @call.scope

; Scope resolution with method call
(call
  receiver: (scope_resolution
    scope: (constant)? @call.scope.method.scope
    name: (constant) @call.scope.method.class
  )
  method: (identifier) @call.scope.method.name
) @call.scope.method
"#
}

/// Get dependency query for Ruby
///
/// Returns Tree-sitter query patterns for identifying Ruby dependencies:
/// - Require statements
/// - Load statements
/// - Include/extend/prepend
/// - Autoload
pub fn dependency_query() -> &'static str {
    r#"
; ============================================
; Require Statements
; ============================================

; require
(call
  method: (identifier) @dependency.require
  (#eq? @dependency.require "require")
  arguments: (argument_list
    (string
      (string_content) @dependency.require.path
    )
  )
) @dependency.require

; require_relative
(call
  method: (identifier) @dependency.require_relative
  (#eq? @dependency.require_relative "require_relative")
  arguments: (argument_list
    (string
      (string_content) @dependency.require_relative.path
    )
  )
) @dependency.require_relative

; ============================================
; Load Statement
; ============================================

; load
(call
  method: (identifier) @dependency.load
  (#eq? @dependency.load "load")
  arguments: (argument_list
    (string
      (string_content) @dependency.load.path
    )
  )
) @dependency.load

; ============================================
; Module Inclusion
; ============================================

; include
(call
  method: (identifier) @dependency.include
  (#eq? @dependency.include "include")
  arguments: (argument_list
    (constant) @dependency.include.module
  )
) @dependency.include

; extend
(call
  method: (identifier) @dependency.extend
  (#eq? @dependency.extend "extend")
  arguments: (argument_list
    (constant) @dependency.extend.module
  )
) @dependency.extend

; prepend
(call
  method: (identifier) @dependency.prepend
  (#eq? @dependency.prepend "prepend")
  arguments: (argument_list
    (constant) @dependency.prepend.module
  )
) @dependency.prepend

; ============================================
; Autoload
; ============================================

; autoload
(call
  method: (identifier) @dependency.autoload
  (#eq? @dependency.autoload "autoload")
  arguments: (argument_list
    (simple_symbol) @dependency.autoload.const
    (string
      (string_content) @dependency.autoload.path
    )
  )
) @dependency.autoload

; ============================================
; Inheritance
; ============================================

; Class inheritance
(class
  (constant) @dependency.inheritance.super
) @dependency.inheritance

; ============================================
; Gem Dependencies
; ============================================

; gem method (in Gemfile)
(call
  method: (identifier) @dependency.gem
  (#eq? @dependency.gem "gem")
  arguments: (argument_list
    (string
      (string_content) @dependency.gem.name
    )
  )
) @dependency.gem
"#
}

/// Get behavior query for Ruby
pub fn behavior_query() -> String {
    let mut query = String::from(
        r#"
(assignment) @behavior.data.bind
(binary) @behavior.data.reference
(instance_variable) @behavior.data.reference
(class_variable) @behavior.data.reference
(lambda) @behavior.data.bind
(block) @behavior.data.bind
(do_block) @behavior.data.bind
(call) @behavior.data.statement
(begin) @behavior.effect.error
(rescue) @behavior.effect.error
"#,
    );
    query.push_str(&super::common::bitwise_shift_operator_query("binary"));
    query
}

/// Get control-flow query for Ruby
pub fn control_flow_query() -> &'static str {
    r#"
(if) @control.flow.if
(unless) @control.flow.if
(for) @control.flow.loop
(while) @control.flow.loop
(until) @control.flow.loop
(case) @control.flow.match
(return) @control.flow.return
(break) @control.flow.break
(next) @control.flow.continue
(yield) @control.flow.yield
"#
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Query;

    /// Validate query syntax and return detailed error information
    fn validate_query_syntax(query_name: &str, query_str: &str) -> Result<(), String> {
        let lang = tree_sitter_ruby::LANGUAGE;
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
