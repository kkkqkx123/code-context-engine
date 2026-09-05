//! Rust language query schemes
//!
//! Provides Tree-sitter query patterns for identifying and analyzing
//! Rust code entities, call relationships, dependencies, behavior, and
//! control flow.

/// Get entity query for Rust
///
/// Returns Tree-sitter query patterns for identifying Rust code entities:
/// - Type definitions (struct, enum, trait, type alias, union)
/// - Functions (including methods, associated functions)
/// - Modules and use declarations
/// - Constants and static items
/// - Macro definitions
/// - Impl blocks
pub fn entity_query() -> &'static str {
    r#"
; ============================================
; 1. Imports
; ============================================

; Use declaration
(use_declaration
  argument: (_) @entity.import.name
) @entity.import

; Use declaration with alias
(use_declaration
  argument: (use_as_clause
    alias: (identifier) @entity.import.name
  )
) @entity.import

; ============================================
; 2. Types
; ============================================

; Struct definition
(struct_item
  name: (type_identifier) @entity.struct.name
  type_parameters: (_)? @entity.struct.signature.type_params
  body: (_) @entity.struct.body
) @entity.struct

; Enum definition
(enum_item
  name: (type_identifier) @entity.enum.name
) @entity.enum

; Enum variant
(enum_variant
  name: (identifier) @entity.enum_variant.name
) @entity.enum_variant

; Union definition
(union_item
  name: (type_identifier) @entity.union.name
) @entity.union

; Trait definition
(trait_item
  name: (type_identifier) @entity.trait.name
) @entity.trait

; Type alias
(type_item
  name: (type_identifier) @entity.type.name
) @entity.type

; ============================================
; 3. Functions
; ============================================

; Function definition
(function_item
  name: (identifier) @entity.function.signature.name
  parameters: (parameters) @entity.function.signature.params
  return_type: (_)? @entity.function.signature.return_type
  body: (_) @entity.function.body
) @entity.function

; Function signature item (trait method declaration)
(function_signature_item
  name: (identifier) @entity.function.name
) @entity.function

; ============================================
; 4. Modules and Imports
; ============================================

; Module definition
(mod_item
  name: (identifier) @entity.module.name
) @entity.module

; Use declaration
(use_declaration
  argument: (_) @dependency.import.path
) @dependency.import

; Use declaration with alias
(use_declaration
  argument: (use_as_clause
    alias: (identifier) @dependency.import.alias.name
  )
) @dependency.import.alias

; ============================================
; 5. Constants and Static
; ============================================

; Constant definition
(const_item
  name: (identifier) @entity.constant.name
) @entity.constant

; Static item
(static_item
  name: (identifier) @entity.static.name
) @entity.static

; ============================================
; 6. Macros
; ============================================

; Macro definition (macro_rules!)
(macro_definition
  name: (identifier) @entity.macro.name
) @entity.macro

; Attribute macro (e.g., #[derive(...)])
(attribute_item
  (attribute) @entity.macro.attribute.name
) @entity.macro.attribute

; Inner attribute (e.g., #![...])
(inner_attribute_item
  (attribute) @entity.macro.attribute.inner.name
) @entity.macro.attribute.inner

; ============================================
; 7. Impl Blocks
; ============================================

; Inherent impl block (impl Type / impl<T> Type<T>)
; Note: !trait excludes trait impl blocks (impl Trait for Type)
(impl_item
  type: (_) @entity.impl.type.name
  !trait
) @entity.impl

; Trait impl block (impl Trait for Type / impl<T> Trait for Type<T>)
(impl_item
  trait: (_) @entity.impl.trait.name
  type: (_) @entity.impl.for.type.name
) @entity.impl.trait

; ============================================
; 8. Closures
; ============================================

; Closure expression assigned to variable
(let_declaration
  pattern: (identifier) @entity.closure.name
  value: (closure_expression) @entity.closure.params
) @entity.closure

; ============================================
; 9. Variables and Parameters
; ============================================

; Let declaration
(let_declaration
  pattern: (identifier) @entity.variable.name
) @entity.variable

; Tuple destructuring: let (a, b) = pair
(let_declaration
  pattern: (tuple_pattern
    (identifier) @entity.variable.multiple.name
  )
  value: (_) @entity.variable.multiple.value
) @entity.variable.multiple

; Struct destructuring: let Point { x, y } = p
(let_declaration
  pattern: (struct_pattern
    (field_pattern
      (shorthand_field_identifier) @entity.variable.multiple.name
    )
  )
  value: (_) @entity.variable.multiple.value
) @entity.variable.multiple

; Tuple-struct destructuring: let Some(v) = opt
(let_declaration
  pattern: (tuple_struct_pattern
    type: (_)
    (identifier) @entity.variable.multiple.name
  )
  value: (_) @entity.variable.multiple.value
) @entity.variable.multiple

; If-let binding: if let Some(v) = opt
(let_condition
  pattern: (tuple_struct_pattern
    type: (_)
    (identifier) @entity.variable.multiple.name
  )
  value: (_) @entity.variable.multiple.value
) @entity.variable.multiple

; Struct field
(field_declaration
  name: (field_identifier) @entity.field.name
  type: (_) @entity.field.type
) @entity.field

"#
}

/// Get comment query for Rust
///
/// Returns Tree-sitter query patterns for identifying Rust comments.
/// Rust has line comments (//) and doc comments (/// and //!).
pub fn comment_query() -> &'static str {
    r#"
; ============================================
; Comments (Meta-information)
; ============================================

; Line comment
(line_comment) @comment.line

; Doc comment (outer /// and inner //!)
(doc_comment) @comment.doc

; Block comment (/* ... */)
(block_comment) @comment.block
"#
}

/// Get call query for Rust
///
/// Returns Tree-sitter query patterns for identifying Rust call relationships:
/// - Direct function calls
/// - Method calls
/// - Associated function calls (Type::function())
/// - Macro calls
/// - Closure calls
pub fn call_query() -> &'static str {
    r#"
; ============================================
; 1. Direct Function Calls
; ============================================

; Direct function call
(call_expression
  function: (identifier) @call.function.name
) @call.function

; ============================================
; 2. Method Calls
; ============================================

; Method call (obj.method())
(call_expression
  function: (field_expression
    value: (_) @call.method.object
    field: (field_identifier) @call.method.function
  )
) @call.method

; Chained method call (obj.method1().method2())
(call_expression
  function: (field_expression
    value: (call_expression) @call.method.chained.from
    field: (field_identifier) @call.method.chained.to
  )
) @call.method.chained

; ============================================
; 3. Associated Function Calls
; ============================================

; Associated function call (Type::function())
(call_expression
  function: (scoped_identifier
    path: (identifier) @call.associated.type.name
    name: (identifier) @call.associated.function.name
  )
) @call.associated

; Associated function call with nested path (mod::Type::function())
(call_expression
  function: (scoped_identifier
    path: (scoped_identifier) @call.associated.nested.path
    name: (identifier) @call.associated.nested.function.name
  )
) @call.associated.nested

; ============================================
; 4. Macro Calls
; ============================================

; Macro call (macro_name!())
(macro_invocation
  macro: (identifier) @call.macro.name
) @call.macro

; Macro call with scoped identifier (path::macro!())
(macro_invocation
  macro: (scoped_identifier) @call.macro.scoped.name
) @call.macro.scoped

; ============================================
; 5. Closure Calls
; ============================================

; Closure call via variable (let f = |x| x; f(42))
(call_expression
  function: (identifier) @call.closure_variable.name
) @call.closure_variable

; Inline closure call (|| {}())
(call_expression
  function: (closure_expression) @call.closure_inline
) @call.closure_inline

; ============================================
; 6. Generic Function Calls
; ============================================

; Generic function call (function::<T>())
(call_expression
  function: (generic_function
    function: (identifier) @call.generic.function.name
  )
) @call.generic

; Generic method call (obj.method::<T>())
(call_expression
  function: (generic_function
    function: (field_expression
      field: (field_identifier) @call.generic.method.name
    )
  )
) @call.generic.method
"#
}

/// Get dependency query for Rust
///
/// Returns Tree-sitter query patterns for identifying Rust dependencies:
/// - Use declarations (imports)
/// - Trait bounds
/// - Module declarations
pub fn dependency_query() -> &'static str {
    r#"
; ============================================
; 1. Use Declarations (Imports)
; ============================================

; Simple use declaration (use path::module;)
(use_declaration
  argument: (scoped_identifier) @dependency.use.path
) @dependency.use

; Use declaration with wildcard (use path::*;)
(use_declaration
  argument: (use_wildcard) @dependency.use.wildcard
) @dependency.use.wildcard

; Use declaration with list (use path::{item1, item2};)
(use_declaration
  argument: (use_list) @dependency.use.list
) @dependency.use.list

; Use declaration with alias (use path::module as alias;)
(use_declaration
  argument: (use_as_clause
    path: (scoped_identifier) @dependency.use.alias.path
    alias: (identifier) @dependency.use.alias.name
  )
) @dependency.use.alias

; Scoped use list (use path::module::{item1, item2};)
(use_declaration
  argument: (scoped_use_list) @dependency.use.scoped_list
) @dependency.use.scoped_list

; ============================================
; 2. Trait Bounds
; ============================================

; Trait bound in where clause
(where_clause
  (where_predicate) @dependency.where_predicate
) @dependency.trait_bound

; Trait bound in type parameter
(type_parameter
  name: (type_identifier) @dependency.type_parameter.name
) @dependency.type_parameter.bound

; Note: impl blocks are intentionally NOT matched here. Their structural
; relations (Implementation, ImplAssociation) are derived from parsed
; TraitImpl/InherentImpl entities, which are the single source of truth.

; ============================================
; 3. Module Dependencies
; ============================================

; Module declaration
(mod_item
  name: (identifier) @dependency.module.name
) @dependency.module

; External crate declaration (extern crate)
(extern_crate_declaration
  name: (identifier) @dependency.extern_crate.name
) @dependency.extern_crate

; ============================================
; 4. Scoped References
; ============================================

; Scoped identifier reference
(scoped_identifier
  path: (identifier) @dependency.reference.path
) @dependency.reference.scoped

; Scoped type identifier reference
; path: (_) accepts a nested scoped_identifier so type-position references
; keep the full scoped path (`std::collections::HashMap`), not just the
; first segment (`std`).
(scoped_type_identifier
  path: (_) @dependency.reference.type_path
) @dependency.reference.scoped_type

; ============================================
; 5. Type References
; ============================================

; Type annotation in let binding (let x: Type)
(let_declaration
  type: (type_identifier) @dependency.reference.type
) @dependency.reference.annotation

; Function parameter type
(parameter
  type: (type_identifier) @dependency.reference.type
) @dependency.reference.param_type

; Function return type
(function_item
  return_type: (type_identifier) @dependency.reference.type
) @dependency.reference.return_type

; Struct field type
(field_declaration
  type: (type_identifier) @dependency.reference.type
) @dependency.reference.field_type

; Generic type arguments (nested in generic_type)
(generic_type
  (type_arguments
    (type_identifier) @dependency.reference.nested_type
  )
) @dependency.reference.generic_args

; Reference type inner type
(reference_type
  type: (type_identifier) @dependency.reference.ref_type
) @dependency.reference.reference

; Tuple type elements
(tuple_type
  (type_identifier) @dependency.reference.tuple_element
) @dependency.reference.tuple
"#
}

/// Get behavior query for Rust
///
/// Returns Tree-sitter query patterns for capturing Rust function-body behavior:
/// - Data flow: binding, reference
/// - Effects: error propagation
/// - Special operations: bitwise shifts
pub fn behavior_query() -> String {
    let mut query = String::from(
        r#"
; ============================================
; 1. Data Binding and References
; ============================================

; let binding
(let_declaration
  pattern: (_) @behavior.data.bind.pattern
  value: (_)? @behavior.data.bind.value
) @behavior.data.bind

; reference expression
(reference_expression
  value: (_) @behavior.data.reference.value
) @behavior.data.reference

; standalone expression statement
(expression_statement) @behavior.data.statement

; ============================================
; 2. Effects
; ============================================

; Unsafe block
(unsafe_block) @behavior.effect.error

; Foreign module (FFI)
(foreign_mod_item) @behavior.effect.error

"#,
    );
    query.push_str(
        &crate::tree_sitter_query::scheme::common::bitwise_shift_operator_query(
            "binary_expression",
        ),
    );
    query
}

/// Get control-flow query for Rust
///
/// Returns Tree-sitter query patterns for capturing Rust control-flow structures:
/// - Control flow: if, match, loop, break/continue, early return
pub fn control_flow_query() -> &'static str {
    r#"
; ============================================
; 1. Control Flow
; ============================================

; if / else
(if_expression
  condition: (_) @control.flow.if.condition
  consequence: (block) @control.flow.if.consequence
  alternative: (else_clause)? @control.flow.if.alternative
) @control.flow.if

; match
(match_expression
  value: (_) @control.flow.match.value
  body: (match_block) @control.flow.match.body
) @control.flow.match

; for loop
(for_expression
  pattern: (_) @control.flow.loop.for.pattern
  value: (_) @control.flow.loop.for.value
  body: (block) @control.flow.loop.for.body
) @control.flow.loop

; while loop
(while_expression
  condition: (_) @control.flow.loop.while.condition
  body: (block) @control.flow.loop.while.body
) @control.flow.loop

; infinite loop
(loop_expression
  body: (block) @control.flow.loop.body
) @control.flow.loop

; return / break / continue / yield / try
(return_expression) @control.flow.return
(break_expression) @control.flow.break
(continue_expression) @control.flow.continue
(yield_expression) @control.flow.yield
(try_expression) @control.flow.try
"#
}

#[cfg(test)]
mod tests {
    use super::{
        behavior_query, call_query, comment_query, control_flow_query, dependency_query,
        entity_query,
    };
    use tree_sitter::Query;
    use tree_sitter::{Parser, QueryCursor};

    /// Validate query syntax and return detailed error information
    fn validate_query_syntax(query_name: &str, query_str: &str) -> Result<(), String> {
        let lang = tree_sitter_rust::LANGUAGE;
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
    fn test_entity_query_contains_signature_subcaptures() {
        let query = entity_query();
        // Verify signature sub-captures are embedded in entity captures
        assert!(query.contains("@entity.struct.signature.type_params"));
        assert!(query.contains("@entity.function.signature.name"));
        assert!(query.contains("@entity.function.signature.params"));
        assert!(query.contains("@entity.function.signature.return_type"));
        // No separate whole-node signature patterns should exist: every
        // `.signature` capture must be a sub-capture (e.g. `.signature.name`).
        // The check is line-anchored so sub-capture lines such as
        // `name: (identifier) @entity.function.signature.name` do not match.
        let has_bare_signature_pattern = query.lines().any(|line| {
            let line = line.trim_end();
            line.ends_with(") @entity.struct.signature")
                || line.ends_with(") @entity.function.signature")
                || line.ends_with(") @entity.impl.signature")
        });
        assert!(
            !has_bare_signature_pattern,
            "separate whole-node signature patterns must not exist"
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
    fn test_behavior_query_contains_main_captures() {
        let query = behavior_query();
        assert!(query.contains("@behavior.data.bind"));
        assert!(query.contains("@behavior.data.reference"));
        assert!(query.contains("@behavior.op.shift_left"));
        assert!(query.contains("@behavior.op.shift_right"));
        assert!(!query.contains("@behavior.control."));
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
    fn test_control_flow_query_contains_main_captures() {
        let query = control_flow_query();
        assert!(query.contains("@control.flow.if"));
        assert!(query.contains("@control.flow.match"));
        assert!(query.contains("@control.flow.loop"));
        assert!(query.contains("@control.flow.return"));
        assert!(query.contains("@control.flow.try"));
        assert!(!query.contains("@control.flow.op"));
        assert!(!query.contains("@behavior."));
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
    fn test_signature_extraction_struct() {
        use streaming_iterator::StreamingIterator;

        let mut parser = Parser::new();
        let language = tree_sitter_rust::LANGUAGE;
        parser
            .set_language(&language.into())
            .expect("Failed to set language");

        let code = r#"pub struct OnceCell<T> {
    initialized: bool,
    value: Option<T>,
}"#;

        let tree = parser.parse(code, None).expect("Failed to parse");
        let query = Query::new(&language.into(), entity_query()).expect("Failed to create query");

        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), code.as_bytes());

        while let Some(mat) = matches.next() {
            // Find the entity.struct match
            let mut has_struct = false;
            let mut has_type_params = false;
            for capture in mat.captures {
                let capture_name = &query.capture_names()[capture.index as usize];
                if *capture_name == "@entity.struct" {
                    has_struct = true;
                }
                if *capture_name == "@entity.struct.signature.type_params" {
                    has_type_params = true;
                }
            }
            // The struct entity should have a type_params sub-capture
            if has_struct {
                assert!(
                    has_type_params,
                    "Struct should have type_params sub-capture"
                );
            }
        }
    }

    #[test]
    fn test_signature_extraction_function() {
        use streaming_iterator::StreamingIterator;

        let mut parser = Parser::new();
        let language = tree_sitter_rust::LANGUAGE;
        parser
            .set_language(&language.into())
            .expect("Failed to set language");

        let code = r#"pub fn new() -> OnceCell<T> {
    OnceCell { initialized: false, value: None }
}"#;

        let tree = parser.parse(code, None).expect("Failed to parse");
        let query = Query::new(&language.into(), entity_query()).expect("Failed to create query");

        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), code.as_bytes());

        while let Some(mat) = matches.next() {
            // Find the entity.function match
            let mut has_function = false;
            let mut has_name = false;
            let mut has_params = false;
            let mut has_return_type = false;
            for capture in mat.captures {
                let capture_name = &query.capture_names()[capture.index as usize];
                if *capture_name == "@entity.function" {
                    has_function = true;
                }
                if *capture_name == "@entity.function.signature.name" {
                    has_name = true;
                }
                if *capture_name == "@entity.function.signature.params" {
                    has_params = true;
                }
                if *capture_name == "@entity.function.signature.return_type" {
                    has_return_type = true;
                }
            }
            // The function entity should have signature sub-captures
            if has_function {
                assert!(has_name, "Function should have signature.name sub-capture");
                assert!(
                    has_params,
                    "Function should have signature.params sub-capture"
                );
                assert!(
                    has_return_type,
                    "Function should have signature.return_type sub-capture"
                );
            }
        }
    }

    #[test]
    fn test_signature_extraction_impl() {
        use streaming_iterator::StreamingIterator;

        let mut parser = Parser::new();
        let language = tree_sitter_rust::LANGUAGE;
        parser
            .set_language(&language.into())
            .expect("Failed to set language");

        let code = r#"impl<T> OnceCell<T> {
    pub fn new() -> Self {
        Self { initialized: false, value: None }
    }
}"#;

        let tree = parser.parse(code, None).expect("Failed to parse");
        let query = Query::new(&language.into(), entity_query()).expect("Failed to create query");

        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), code.as_bytes());

        while let Some(mat) = matches.next() {
            // Find the entity.impl match
            let mut has_impl = false;
            let mut has_type = false;
            for capture in mat.captures {
                let capture_name = &query.capture_names()[capture.index as usize];
                if *capture_name == "@entity.impl" {
                    has_impl = true;
                }
                if *capture_name == "@entity.impl.type.name" {
                    has_type = true;
                }
            }
            // The impl entity should have a type.name capture
            if has_impl {
                assert!(has_type, "Impl should have type.name capture");
            }
        }
    }
}
