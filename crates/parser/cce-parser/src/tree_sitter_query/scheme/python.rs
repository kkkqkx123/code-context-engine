//! Python language query schemes
//!
//! Provides Tree-sitter query patterns for identifying and analyzing
//! Python code entities, call relationships, and dependencies.

/// Get entity query for Python
///
/// Returns Tree-sitter query patterns for identifying Python code entities:
/// - Classes (including decorated classes)
/// - Functions (including async and decorated functions)
/// - Methods (class methods)
/// - Lambda expressions
/// - Generator functions
/// - Comprehensions (list, dict, set)
/// - Type annotations
pub fn entity_query() -> &'static str {
    r#"
; ============================================
; Import Statements
; ============================================

; Simple import statement
(import_statement
  name: (dotted_name
    (identifier) @entity.import.name
  )
) @entity.import

; Import with alias
(import_statement
  name: (aliased_import
    name: (dotted_name
      (identifier) @entity.import.name
    )
    alias: (identifier) @entity.import.alias
  )
) @entity.import

; From import statement
(import_from_statement
  name: (_) @entity.import.name
) @entity.import

; Future import statement (from __future__ import ...)
(future_import_statement
  name: (dotted_name
    (identifier) @entity.import.name
  )
) @entity.import

; ============================================
; Class Definitions
; ============================================

; Class definition
(class_definition
  name: (identifier) @entity.class.name
  superclasses: (argument_list
    [
      (identifier) @entity.class.base
      (attribute) @entity.class.base
      (keyword_argument value: (_) @entity.class.base)
    ]
  )?
  body: (block) @entity.class.body
) @entity.class

; Decorated class definition
(decorated_definition
  (class_definition
    name: (identifier) @entity.class.name
    superclasses: (argument_list
      [
        (identifier) @entity.class.base
        (attribute) @entity.class.base
        (keyword_argument value: (_) @entity.class.base)
      ]
    )?
    body: (block) @entity.class.body
  )
) @entity.class

; ============================================
; Function Definitions
; ============================================

; Function definition
(function_definition
  name: (identifier) @entity.function.name
  parameters: (parameters) @entity.function.params
  return_type: (type)? @entity.function.return_type
  body: (block) @entity.function.body
) @entity.function

; Decorated function definition
(decorated_definition
  (function_definition
    name: (identifier) @entity.function.name
    parameters: (parameters) @entity.function.params
    return_type: (type)? @entity.function.return_type
    body: (block) @entity.function.body
  )
) @entity.function

; Async function definition
(function_definition
  name: (identifier) @entity.function.async.name
  parameters: (parameters) @entity.function.async.params
  return_type: (type)? @entity.function.return_type
  body: (block) @entity.function.async.body
) @entity.function.async

; ============================================
; Method Definitions (within classes)
; ============================================

; Method definition in class (main capture on the function so the
; method span covers only the definition, matching the generic
; function pattern span for same-span dedup)
(class_definition
  body: (block
    (function_definition
      name: (identifier) @entity.method.name
      parameters: (parameters) @entity.method.params
      body: (block) @entity.method.body
    ) @entity.method
  )
)

; Decorated method definition (main capture on the decorated node so the
; span matches the generic decorated-function pattern for dedup)
(class_definition
  body: (block
    (decorated_definition
      (function_definition
        name: (identifier) @entity.method.name
        parameters: (parameters) @entity.method.params
        body: (block) @entity.method.body
      )
    ) @entity.method
  )
)

; Class method (with cls parameter)
(class_definition
  body: (block
    (function_definition
      name: (identifier) @entity.method.class.name
      parameters: (parameters
        (identifier) @entity.method.class.cls_param
      )
    ) @entity.method.class
  )
)

; Instance method (with self parameter)
(class_definition
  body: (block
    (function_definition
      name: (identifier) @entity.method.instance.name
      parameters: (parameters
        (identifier) @entity.method.instance.self_param
      )
    ) @entity.method.instance
  )
)

; Static method
(class_definition
  body: (block
    (decorated_definition
      (decorator (identifier) @entity.method.static.decorator)
      (function_definition
        name: (identifier) @entity.method.static.name
      )
    ) @entity.method.static
  )
)

; Property method (getter)
(class_definition
  body: (block
    (decorated_definition
      (decorator (identifier) @entity.method.getter.decorator)
      (function_definition
        name: (identifier) @entity.method.getter.name
      )
    ) @entity.method.getter
  )
)

; ============================================
; Lambda Expressions
; ============================================

; Lambda expression assigned to variable
(expression_statement
  (assignment
    left: (identifier) @entity.lambda.name
    right: (lambda
      parameters: (lambda_parameters) @entity.lambda.params
    )
  )
) @entity.lambda

; ============================================
; Generator Functions
; ============================================

; Generator function (containing yield)
(function_definition
  name: (identifier) @entity.function.generator.name
  body: (block
    (expression_statement
      (yield)
    )
  )
) @entity.function.generator

; ============================================
; Comprehensions
; ============================================

; List comprehension assigned to variable
(expression_statement
  (assignment
    left: (identifier) @entity.comprehension.list.name
    right: (list_comprehension)
  )
) @entity.comprehension.list

; Dictionary comprehension assigned to variable
(expression_statement
  (assignment
    left: (identifier) @entity.comprehension.dict.name
    right: (dictionary_comprehension)
  )
) @entity.comprehension.dict

; Set comprehension assigned to variable
(expression_statement
  (assignment
    left: (identifier) @entity.comprehension.set.name
    right: (set_comprehension)
  )
) @entity.comprehension.set

; ============================================
; Variable Declarations
; ============================================

; Variable assignment
(expression_statement
  (assignment
    left: (identifier) @entity.variable.name
    right: (_) @entity.variable.value
  )
) @entity.variable

; Multiple assignment via tuple_pattern (kept for grammar variants)
(expression_statement
  (assignment
    left: (tuple_pattern
      (identifier) @entity.variable.multiple.name
    )
    right: (_) @entity.variable.multiple.value
  )
) @entity.variable.multiple

; Tuple unpacking: `first, second = pair` uses pattern_list in this grammar.
; All bound names are captured; the extractor joins them into one
; comma-separated entity with the right-hand side as its source.
(expression_statement
  (assignment
    left: (pattern_list
      (identifier) @entity.variable.multiple.name
    )
    right: (_) @entity.variable.multiple.value
  )
) @entity.variable.multiple

; For-loop binding: `for item in items` and `for k, v in items`.
; The iterable is recorded as provenance only: loop variables hold
; elements, so inference must never bind them to the iterable type itself.
(for_statement
  left: (identifier) @entity.variable.loop.name
  right: (_) @entity.variable.loop.source
) @entity.variable.loop

(for_statement
  left: (pattern_list
    (identifier) @entity.variable.loop.name
  )
  right: (_) @entity.variable.loop.source
) @entity.variable.loop

; Except-as binding: `except ValueError as e` binds e to the exception type.
(except_clause
  value: (as_pattern
    (identifier) @entity.variable.except.source
    alias: (as_pattern_target
      (identifier) @entity.variable.except.name
    )
  )
) @entity.variable.except

; With-as binding: `with open('f') as fh` binds fh to the call result.
(with_item
  value: (as_pattern
    (_) @entity.variable.with.value
    alias: (as_pattern_target
      (identifier) @entity.variable.with.name
    )
  )
) @entity.variable.with

; Match-case binding: `case x:` binds x to the subject.
(match_statement
  subject: (identifier) @entity.variable.case.source
  body: (block
    (case_clause
      (case_pattern
        (dotted_name
          (identifier) @entity.variable.case.name
        )
      )
    )
  )
) @entity.variable.case

; Match-case tuple binding: `case (x, 0)` binds x to the subject.
; Deeper nesting stays uncovered (documented limitation).
(match_statement
  subject: (identifier) @entity.variable.case.source
  body: (block
    (case_clause
      (case_pattern
        (tuple_pattern
          (case_pattern
            (dotted_name
              (identifier) @entity.variable.case.name
            )
          )
        )
      )
    )
  )
) @entity.variable.case

; ============================================
; Type Annotations
; ============================================

; Variable with type annotation
(expression_statement
  (assignment
    left: (identifier) @entity.variable.typed.name
    type: (type) @entity.variable.typed.type
  )
) @entity.variable.typed

; ============================================
; Instance Attributes (self.x = ...)
; ============================================

; Annotated instance attribute: self.<name>: <type> = <value>
(expression_statement
  (assignment
    left: (attribute
      object: (identifier) @entity.field.receiver (#eq? @entity.field.receiver "self")
      attribute: (identifier) @entity.field.name
    )
    type: (type) @entity.field.type
    right: (_) @entity.field.value
  )
) @entity.field

; Plain instance attribute: self.<name> = <value>
(expression_statement
  (assignment
    left: (attribute
      object: (identifier) @entity.field.receiver (#eq? @entity.field.receiver "self")
      attribute: (identifier) @entity.field.name
    )
    right: (_) @entity.field.value
  )
) @entity.field

; Augmented instance attribute: self.<name> += <value> (captures as field for inference)
(expression_statement
  (augmented_assignment
    left: (attribute
      object: (identifier) @entity.field.receiver (#eq? @entity.field.receiver "self")
      attribute: (identifier) @entity.field.name
    )
  )
) @entity.field

; ============================================
; Special Statements
; ============================================


; Global statement
(global_statement
  (identifier) @entity.variable.global.name
) @entity.variable.global

; Nonlocal statement
(nonlocal_statement
  (identifier) @entity.variable.nonlocal.name
) @entity.variable.nonlocal

; ============================================
; Decorators
; ============================================

; Decorator
(decorator
  (identifier) @entity.decorator.name
) @entity.decorator

; Decorator with arguments
(decorator
  (call
    function: (identifier) @entity.decorator.call.name
    arguments: (argument_list) @entity.decorator.call.arguments
  )
) @entity.decorator.call

; ============================================
; Enum Definitions (Python 3.4+)
; ============================================

; Enum class definition
(class_definition
  name: (identifier) @entity.enum.name
  superclasses: (argument_list
    (identifier) @entity.enum.base
  )
  body: (block) @entity.enum.body
) @entity.enum

; Enum member/variant (simple assignment in enum body)
(class_definition
  body: (block
    (expression_statement
      (assignment
        left: (identifier) @entity.enum.variant.name
        right: (_) @entity.enum.variant.value
      )
    )
  )
) @entity.enum.variant

; ============================================
; Type Annotations and Generics
; ============================================

; TypeVar definition for generic constraints
; Example: T = TypeVar('T', bound=SomeClass)
(expression_statement
  (assignment
    left: (identifier) @entity.type_constraint.param
    right: (call
      function: (identifier) @entity.type_constraint.function
      arguments: (argument_list) @entity.type_constraint.args
    )
  )
) @entity.type_constraint

; Function with type constraints
(function_definition
  name: (identifier) @entity.function.generic.name
  parameters: (parameters) @entity.function.generic.params
  return_type: (type) @entity.function.generic.return_type
  body: (block) @entity.function.generic.body
) @entity.function.generic
"#
}

/// Get comment query for Python
///
/// Returns Tree-sitter query patterns for identifying Python comments.
/// Python docstrings are string literals at the beginning of modules,
/// classes, or functions, not traditional comments.
pub fn comment_query() -> &'static str {
    r#"
; ============================================
; Comments (Meta-information)
; ============================================

; Line comment (# ...)
(comment) @comment.line

; Docstring - module/class/function documentation
; Python docstrings are expression_statement containing a string
; Capture string_content to get the actual text without quotes
(expression_statement
  (string
    (string_content) @comment.docstring
  )
)
"#
}

/// Get call query for Python
///
/// Returns Tree-sitter query patterns for identifying Python call relationships:
/// - Direct function calls
/// - Method calls (instance and class methods)
/// - Constructor calls (class instantiation)
/// - Chained method calls
/// - Super calls
pub fn call_query() -> &'static str {
    r#"
; ============================================
; Direct Function Calls
; ============================================

; Direct function call
(call
  function: (identifier) @call.function.name
  arguments: (argument_list) @call.function.arguments
) @call.function

; ============================================
; Method Calls
; ============================================

; Instance method call
(call
  function: (attribute
    object: (identifier) @call.method.object
    attribute: (identifier) @call.method.function
  )
  arguments: (argument_list) @call.method.arguments
) @call.method

; Class method call
(call
  function: (attribute
    object: (identifier) @call.method.class.object
    attribute: (identifier) @call.method.class.function
  )
) @call.method.class

; ============================================
; Constructor Calls
; ============================================

; Class instantiation (constructor call)
(call
  function: (identifier) @call.constructor.name
  arguments: (argument_list) @call.constructor.arguments
) @call.constructor

; ============================================
; Chained Method Calls
; ============================================

; Chained method call
(call
  function: (attribute
    object: (call) @call.method.chained.from
    attribute: (identifier) @call.method.chained.to
  )
) @call.method.chained

; ============================================
; Special Calls
; ============================================

; Super call
(call
  function: (identifier) @call.super.name
) @call.super

; Super method call
(call
  function: (attribute
    object: (call
      function: (identifier) @call.super.method.class
    )
    attribute: (identifier) @call.super.method.name
  )
) @call.super.method

; ============================================
; Async Calls
; ============================================

; Await expression
(await
  (call
    function: (identifier) @call.async.function.name
  )
) @call.async

; Await method call
(await
  (call
    function: (attribute
      object: (identifier) @call.async.method.object
      attribute: (identifier) @call.async.method.name
    )
  )
) @call.async.method
"#
}

/// Get dependency query for Python
///
/// Returns Tree-sitter query patterns for identifying Python dependencies:
/// - Import statements (import, from...import)
/// - Import with aliases
/// - Relative imports
/// - Wildcard imports
pub fn dependency_query() -> &'static str {
    r#"
; ============================================
; Import Statements
; ============================================

; Simple import statement
(import_statement
  name: (dotted_name
    (identifier) @dependency.import.module.name
  )
) @dependency.import.module

; Import with alias
(import_statement
  name: (aliased_import
    name: (dotted_name
      (identifier) @dependency.import.alias.module
    )
    alias: (identifier) @dependency.import.alias.name
  )
) @dependency.import.alias

; ============================================
; From Import Statements
; ============================================

; From import statement
(import_from_statement
  module_name: (dotted_name
    (identifier) @dependency.import.from.module
  )
  name: (dotted_name
    (identifier) @dependency.import.from.name
  )
) @dependency.import.from

; From import with alias
(import_from_statement
  module_name: (dotted_name
    (identifier) @dependency.import.from.alias.module
  )
  name: (aliased_import
    name: (dotted_name
      (identifier) @dependency.import.from.alias.original
    )
    alias: (identifier) @dependency.import.from.alias.name
  )
) @dependency.import.from.alias

; ============================================
; Relative Import
; ============================================

; Relative import (from . import)
(import_from_statement
  module_name: (relative_import
    (dotted_name
      (identifier) @dependency.import.relative.module
    )
  )
) @dependency.import.relative

; ============================================
; Wildcard Import
; ============================================

; Wildcard import (from module import *)
(wildcard_import) @dependency.import.wildcard

; ============================================
; Future Import
; ============================================

; Future import (from __future__ import ...)
(future_import_statement
  name: (dotted_name
    (identifier) @dependency.import.future.name
  )
) @dependency.import.future

; ============================================
; Inheritance Dependencies (Multiple Inheritance)
; ============================================

; Class inheritance: supports simple names, dotted names, and keyword arguments
; Examples:
;   - class Derived(Base):
;   - class Derived(module.Base):
;   - class Derived(Base1, Base2):
;   - class Derived(meta=MetaClass):
;   - class Derived(Base1, meta=MetaClass):
; Uses selector to match all forms in a single query, avoiding duplicate matches
(class_definition
  (argument_list
    [
      ; Simple identifier (e.g., Base)
      (identifier) @dependency.extend.name
      ; Dotted name (e.g., module.Base)
      (attribute (identifier) @dependency.extend.name)
      ; Keyword argument value (e.g., meta=MetaClass)
      (keyword_argument value: (identifier) @dependency.extend.name)
    ]
  )
) @dependency.extend

; ============================================
; TypeVar and Generic Constraints
; ============================================

; TypeVar definition: T = TypeVar('T', bound=Base)
; Captures the constraint information for generic type parameters
(expression_statement
  (assignment
    left: (identifier) @dependency.type_constraint.param
    right: (call
      function: (identifier) @dependency.type_constraint.function
      arguments: (argument_list) @dependency.type_constraint.args
    )
  )
) @dependency.type_constraint

; Enum Variant Dependencies
; In Python enums, variants can reference other types:
; class Status(Enum):
;   ACTIVE = 1
;   PENDING = 2
; Captured as variant assignments within enum class body
(class_definition
  superclasses: (argument_list (identifier) @dependency.enum.base)
  body: (block
    (expression_statement
      (assignment
        left: (identifier) @dependency.enum.variant.name
        right: (_) @dependency.enum.variant.value
      )
    )
  )
) @dependency.enum

"#
}

/// Get behavior query for Python
pub fn behavior_query() -> String {
    let mut query = String::from(
        r#"
(assignment) @behavior.data.bind
(augmented_assignment) @behavior.data.bind
(attribute) @behavior.data.reference
(expression_statement) @behavior.data.statement
(with_statement) @behavior.effect.error
(try_statement) @behavior.effect.error
(raise_statement) @behavior.effect.error
"#,
    );
    query.push_str(&super::common::bitwise_shift_operator_query(
        "binary_operator",
    ));
    query
}

/// Get control-flow query for Python
pub fn control_flow_query() -> &'static str {
    r#"
(if_statement) @control.flow.if
(for_statement) @control.flow.loop
(while_statement) @control.flow.loop
(match_statement) @control.flow.match
(return_statement) @control.flow.return
(break_statement) @control.flow.break
(continue_statement) @control.flow.continue
(yield) @control.flow.yield
"#
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Query;

    /// Validate query syntax and return detailed error information
    fn validate_query_syntax(query_name: &str, query_str: &str) -> Result<(), String> {
        let lang = tree_sitter_python::LANGUAGE;
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
