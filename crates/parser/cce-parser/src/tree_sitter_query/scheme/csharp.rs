//! C# language query schemes
//!
//! Provides Tree-sitter query patterns for identifying and analyzing
//! C# code entities, call relationships, and dependencies.

/// Get entity query for C#
///
/// Returns Tree-sitter query patterns for identifying C# code entities:
/// - Type definitions (class, interface, struct, enum, record)
/// - Methods (including constructors, destructors, operators)
/// - Properties and events
/// - Namespaces and using directives
/// - Delegates and attributes
pub fn entity_query() -> &'static str {
    r#"
; ============================================
; 0. Imports
; ============================================

; Using directive (namespace import). The `!name` guard excludes the alias
; form below, which carries its alias in the `name` field.
(using_directive
  !name
  (identifier) @entity.import.name
) @entity.import

; Using directive with qualified name
(using_directive
  !name
  (qualified_name) @entity.import.name
) @entity.import

; Using directive with alias (using Alias = Namespace.Path;).
; The alias is inert metadata; the entity name is the real path.
(using_directive
  name: (identifier) @entity.import.alias
  "="
  (_) @entity.import.name
) @entity.import

; ============================================
; 1. Types
; ============================================

; Class definition (including generic, static, abstract, partial, nested).
; Base list entries feed `@entity.class.base` so `base_classes` metadata is
; recorded for hierarchy-aware narrowing (mirrors the dependency captures).
(class_declaration
  name: (identifier) @entity.class.name @entity.class.signature.name
  (type_parameter_list)? @entity.class.signature.type_params
  (base_list
    (_) @entity.class.base @entity.class.signature.base
  )?
  body: (declaration_list)? @entity.class.body
) @entity.class

; Interface definition
(interface_declaration
  name: (identifier) @entity.interface.name @entity.interface.signature.name
  body: (declaration_list)? @entity.interface.body
) @entity.interface

; Struct definition
(struct_declaration
  name: (identifier) @entity.struct.name @entity.struct.signature.name
  (type_parameter_list)? @entity.struct.signature.type_params
  body: (declaration_list)? @entity.struct.body
) @entity.struct

; Enum definition
(enum_declaration
  name: (identifier) @entity.enum.name @entity.enum.signature.name
) @entity.enum

; Enum member
(enum_member_declaration
  name: (identifier) @entity.enum_member.name
) @entity.enum_member

; Record definition
(record_declaration
  name: (identifier) @entity.record.name @entity.record.signature.name
  body: (declaration_list)? @entity.record.body
) @entity.record

; ============================================
; 2. Methods
; ============================================

; Method definition (including async, static, generic)
; Note: tree-sitter-c-sharp only honors the `returns:` field constraint when
; it precedes `name:` in the pattern; placing it after `name:` silently drops
; the capture (or raises a Structure error without the `?` quantifier).
; The body is optional so bodyless declarations (abstract methods,
; interface members, extern methods) are extracted as entities too: they
; are the dispatch targets member-call resolution must bind, and their
; signatures compose from the name/params/return captures alone.
(method_declaration
  returns: (_)? @entity.method.return_type @entity.method.signature.return_type
  name: (identifier) @entity.method.name @entity.method.signature.name
  parameters: (parameter_list) @entity.method.params @entity.method.signature.params
  body: (_)? @entity.method.body
) @entity.method

; Constructor definition
(constructor_declaration
  name: (identifier) @entity.constructor.name @entity.constructor.signature.name
  (parameter_list) @entity.constructor.params @entity.constructor.signature.params
  (block)? @entity.constructor.body
) @entity.constructor

; Destructor definition
(destructor_declaration
  name: (identifier) @entity.destructor.name @entity.destructor.signature.name
  (block)? @entity.destructor.body
) @entity.destructor

; Operator overload
(operator_declaration
  type: (_) @entity.method.operator.return_type
) @entity.method.operator

; ============================================
; 3. Properties and Events
; ============================================

; Property declaration
(property_declaration
  type: (_) @entity.property.type @entity.property.signature.type
  name: (identifier) @entity.property.name @entity.property.signature.name
  accessors: (accessor_list)? @entity.property.body
) @entity.property

; Event declaration
(event_declaration
  name: (identifier) @entity.event.name
) @entity.event

; ============================================
; 4. Namespaces
; ============================================

; Namespace declaration (simple name)
(namespace_declaration
  name: (identifier) @entity.namespace.name @entity.namespace.signature.name
  body: (declaration_list) @entity.namespace.body
) @entity.namespace

; Namespace declaration (qualified name)
(namespace_declaration
  name: (qualified_name) @entity.namespace.qualified_name @entity.namespace.signature.name
  body: (declaration_list) @entity.namespace.body
) @entity.namespace.qualified

; File-scoped namespace declaration (simple name)
(file_scoped_namespace_declaration
  name: (identifier) @entity.namespace.file_scoped.name @entity.namespace.file_scoped.signature.name
) @entity.namespace.file_scoped

; File-scoped namespace declaration (qualified name)
(file_scoped_namespace_declaration
  name: (qualified_name) @entity.namespace.file_scoped.qualified_name @entity.namespace.file_scoped.signature.name
) @entity.namespace.file_scoped.qualified

; ============================================
; 5. Delegates and Attributes
; ============================================

; Delegate declaration
(delegate_declaration
  name: (identifier) @entity.delegate.name
) @entity.delegate

; Attribute declaration
(attribute
  name: (identifier) @entity.attribute.name
) @entity.attribute

; ============================================
; 6. Generic Type Parameters
; ============================================

; ============================================
; 7. Variables and Fields
; ============================================

; Field declaration (the declared type lands in `.type` and the initializer
; in `.value` so the metadata layer can record `type_annotation` /
; `constructor_type` / `literal_type` / `call_target`)
(field_declaration
  (variable_declaration
    type: (_) @entity.field.type
    (variable_declarator
      (identifier) @entity.field.name
      (_)? @entity.field.value
    )
  )
) @entity.field

; Local variable declaration (same `.type` / `.value` convention as fields)
(local_declaration_statement
  (variable_declaration
    type: (_) @entity.variable.type
    (variable_declarator
      (identifier) @entity.variable.name
      (_)? @entity.variable.value
    )
  )
) @entity.variable

; Tuple deconstruction, e.g. `var (a, b) = (1, 2);`
(local_declaration_statement
  (variable_declaration
    (variable_declarator
      (tuple_pattern
        (identifier) @entity.variable.multiple.name
      )
      (tuple_expression) @entity.variable.multiple.value
    )
  )
) @entity.variable.multiple

; foreach loop variable, e.g. `foreach (var current in items)`
(foreach_statement
  left: (identifier) @entity.variable.loop.name @entity.variable.loop.signature.name
  right: (_) @entity.variable.loop.source @entity.variable.loop.signature.source
) @entity.variable.loop

; is-pattern declaration, e.g. `if (obj is string s)`
(is_pattern_expression
  (declaration_pattern
    type: (_) @entity.variable.case.source
    name: (identifier) @entity.variable.case.name
  )
) @entity.variable.case

; out-var declaration, e.g. `int.TryParse("1", out var result)`
(argument
  (declaration_expression
    type: (_) @entity.variable.type
    name: (identifier) @entity.variable.name
  ) @entity.variable
)

"#
}

/// Get call query for C#
///
/// Returns Tree-sitter query patterns for identifying C# call relationships:
/// - Direct method/function calls
/// - Object method calls
/// - Static method calls
/// - Constructor calls (object creation)
/// - Delegate invocations
pub fn call_query() -> &'static str {
    r#"
; ============================================
; 1. Direct Calls
; ============================================

; Direct method/function call
(invocation_expression
  function: (identifier) @call.function.name
) @call.function

; ============================================
; 2. Object Method Calls
; ============================================

; Object method call (e.g., obj.Method())
(invocation_expression
  function: (member_access_expression
    expression: (_) @call.method.object
    name: (identifier) @call.method.function
  )
  arguments: (argument_list) @call.method.arguments
) @call.method

; Chained method call (e.g., obj.Method1().Method2())
(invocation_expression
  function: (member_access_expression
    expression: (invocation_expression) @call.method.chained.from
    name: (identifier) @call.method.chained.to
  )
) @call.method.chained

; ============================================
; 3. Static Method Calls
; ============================================

; Static method call with qualified name (e.g., ClassName.StaticMethod())
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @call.method.static.object
    name: (identifier) @call.method.static.function
  )
) @call.method.static

; Static method call with namespace qualification (e.g., Namespace.Class.Method())
(invocation_expression
  function: (member_access_expression
    expression: (member_access_expression) @call.method.static.qualified.expression
    name: (identifier) @call.method.static.qualified.function
  )
) @call.method.static.qualified

; ============================================
; 4. Constructor Calls
; ============================================

; Object creation expression (constructor call)
(object_creation_expression
  type: (identifier) @call.constructor.name
) @call.constructor

; Object creation with qualified type name
(object_creation_expression
  type: (qualified_name) @call.constructor.qualified.name
) @call.constructor.qualified

; ============================================
; 5. Delegate Invocations
; ============================================

; Delegate invocation (distinguished from function calls by type context)
; Note: At syntax level, delegate calls are indistinguishable from function calls.
; The Delegate category is mapped to CallbackCall in determine_call_relation_type,
; while Function category maps to DirectCall. This produces different edge types.
; We keep the pattern but it may produce duplicate edges with call.function.
(invocation_expression
  function: (identifier) @call.delegate.name
) @call.delegate

; ============================================
; 6. Member Call With Anonymous Receiver
; ============================================

; Member call whose receiver is an anonymous token (`this.H()`,
; `base.M()`). The object patterns above require a named receiver
; expression, so this form would otherwise go missing. No receiver is
; captured: the bare function name resolves against same-file methods.
; Appended last so overlapping ordinary calls keep their earlier winners
; in downstream per-site deduplication.
(invocation_expression
  function: (member_access_expression
    name: (identifier) @call.method.function
  )
  arguments: (argument_list)? @call.method.arguments
) @call.method

; ============================================
; 7. Generic Method Calls
; ============================================

; Generic method call
(invocation_expression
  function: (generic_name) @call.generic.function
) @call.generic

; Generic method call on object
(invocation_expression
  function: (member_access_expression
    expression: (_) @call.generic.object.name
    name: (generic_name) @call.generic.method.name
  )
) @call.generic.method
"#
}

/// Get dependency query for C#
///
/// Returns Tree-sitter query patterns for identifying C# dependencies:
/// - Using directives (namespace imports)
/// - Namespace references
/// - Type references
pub fn dependency_query() -> &'static str {
    r#"
; ============================================
; 1. Using Directives
; ============================================

; using directive (namespace import). The `!name` guard excludes the alias
; form below, which carries its alias in the `name` field.
(using_directive
  !name
  (identifier) @dependency.using.namespace.name
) @dependency.using

; using directive with qualified name (also covers `using static`)
(using_directive
  !name
  (qualified_name) @dependency.using.qualified.name
) @dependency.using.qualified

; using directive with alias (using Alias = Namespace.Path;).
; The alias capture is inert metadata; the `.path` rule selects the real
; path as the relation target so the alias never becomes a dependency edge.
(using_directive
  name: (identifier) @dependency.using.alias.name
  "="
  (_) @dependency.using.alias.path
) @dependency.using.alias

; ============================================
; 2. Namespace References
; ============================================

; Qualified identifier access (e.g., System.Console)
(qualified_name
  name: (identifier) @dependency.namespace.qualified.name
) @dependency.namespace.qualified

; ============================================
; 3. Type References
; ============================================

; Class base types (`class Circle : Shape`): inheritance. A class base list
; may mix one base class with interfaces, but the first entry is the base
; class; the resolver records inheritance and implementations resolve via
; the symbol table.
(class_declaration
  (base_list
    (_) @dependency.extend.name
  )
) @dependency.extend

; Struct base types (all are interface implementations: `struct S : I`)
(struct_declaration
  (base_list
    (_) @dependency.implement.name
  )
) @dependency.implement

; Record base types (records support inheritance like classes)
(record_declaration
  (base_list
    (_) @dependency.extend.name
  )
) @dependency.extend

; Interface base types (all are extends)
(interface_declaration
  (base_list
    (type) @dependency.extend.name
  )
) @dependency.extend
"#
}

/// Get behavior query for C#
pub fn behavior_query() -> String {
    let mut query = String::from(
        r#"
(local_declaration_statement) @behavior.data.bind
(variable_declaration) @behavior.data.bind
(assignment_expression) @behavior.data.bind
(member_access_expression) @behavior.data.reference
(element_access_expression) @behavior.data.reference
(query_expression) @behavior.data.query
(expression_statement) @behavior.data.statement
(try_statement) @behavior.effect.error
(throw_expression) @behavior.effect.error
"#,
    );
    query.push_str(&super::common::bitwise_shift_operator_query(
        "binary_expression",
    ));
    query
}

/// Get control-flow query for C#
pub fn control_flow_query() -> &'static str {
    r#"
(if_statement) @control.flow.if
(for_statement) @control.flow.loop
(foreach_statement) @control.flow.loop
(while_statement) @control.flow.loop
(do_statement) @control.flow.loop
(switch_statement) @control.flow.match
(try_statement) @control.flow.try
(return_statement) @control.flow.return
(break_statement) @control.flow.break
(continue_statement) @control.flow.continue
(yield_statement) @control.flow.yield
"#
}

#[cfg(test)]
mod tests {
    use super::*;
    use streaming_iterator::StreamingIterator;
    use tree_sitter::{Parser, Query, QueryCursor};

    /// Run a query over a snippet, returning matches as capture texts.
    fn run_matches(query_str: &str, code: &str) -> Vec<Vec<(String, String)>> {
        let lang = tree_sitter_c_sharp::LANGUAGE;
        let query = Query::new(&lang.into(), query_str).expect("query should compile");
        let mut parser = Parser::new();
        parser.set_language(&lang.into()).unwrap();
        let tree = parser.parse(code, None).unwrap();
        let names = query.capture_names().to_vec();
        let mut cursor = QueryCursor::new();
        let mut out = Vec::new();
        let mut matches = cursor.matches(&query, tree.root_node(), code.as_bytes());
        while let Some(mat) = matches.next() {
            out.push(
                mat.captures
                    .iter()
                    .map(|c| {
                        (
                            names[c.index as usize].to_string(),
                            code[c.node.start_byte()..c.node.end_byte()].to_string(),
                        )
                    })
                    .collect(),
            );
        }
        out
    }

    fn texts_of(matches: &[Vec<(String, String)>], capture: &str) -> Vec<String> {
        matches
            .iter()
            .filter_map(|caps| {
                caps.iter()
                    .find(|(name, _)| name == capture)
                    .map(|(_, text)| text.clone())
            })
            .collect()
    }

    #[test]
    fn test_using_plain_and_qualified_hit_without_name_field() {
        let matches = run_matches(dependency_query(), "using System;\nusing System.IO;\n");
        assert_eq!(
            matches.len(),
            3,
            "plain, qualified and qualified-name matches"
        );
        let targets = texts_of(&matches, "dependency.using.namespace.name");
        assert_eq!(targets, vec!["System"]);
        let qualified = texts_of(&matches, "dependency.using.qualified.name");
        assert_eq!(qualified, vec!["System.IO"]);
    }

    #[test]
    fn test_using_alias_resolves_to_path_not_alias() {
        let matches = run_matches(dependency_query(), "using Foo = Bar.Baz;\n");
        assert_eq!(
            matches.len(),
            2,
            "alias using plus qualified-name reference"
        );
        let paths = texts_of(&matches, "dependency.using.alias.path");
        assert_eq!(paths, vec!["Bar.Baz"]);
        assert!(
            !matches
                .iter()
                .flatten()
                .any(|(name, text)| (name == "dependency.using.namespace.name"
                    || name == "dependency.using.qualified.name")
                    && text == "Foo"),
            "alias must not surface as a plain using name"
        );
    }

    #[test]
    fn test_using_static_and_global_hit() {
        let matches = run_matches(
            dependency_query(),
            "using static System.Math;\nglobal using System;\n",
        );
        let qualified = texts_of(&matches, "dependency.using.qualified.name");
        assert_eq!(qualified, vec!["System.Math"]);
        let plain = texts_of(&matches, "dependency.using.namespace.name");
        assert_eq!(plain, vec!["System"]);
    }

    #[test]
    fn test_this_and_base_calls_hit_with_bare_name() {
        let matches = run_matches(
            call_query(),
            "class C : B {\n void H() { }\n void G() { this.H(); }\n void F() { base.M(); }\n}\nclass B { public void M() { } }\n",
        );
        let names = texts_of(&matches, "call.method.function");
        assert!(
            names.contains(&"H".to_string()),
            "this.H() hits, got {names:?}"
        );
        assert!(
            names.contains(&"M".to_string()),
            "base.M() hits, got {names:?}"
        );
    }

    /// Validate query syntax and return detailed error information
    fn validate_query_syntax(query_name: &str, query_str: &str) -> Result<(), String> {
        let lang = tree_sitter_c_sharp::LANGUAGE;
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
        let query = entity_query().to_string();
        for expected in [
            "@entity.class.signature.name",
            "@entity.class.signature.base",
            "@entity.class.signature.type_params",
            "@entity.interface.signature.name",
            "@entity.struct.signature.name",
            "@entity.enum.signature.name",
            "@entity.method.signature.name",
            "@entity.method.signature.params",
            "@entity.constructor.signature.name",
            "@entity.namespace.signature.name",
            "@entity.namespace.body",
            "@entity.namespace.file_scoped.signature.name",
            "@entity.property.signature.name",
            "@entity.property.signature.type",
            "@entity.property.body",
        ] {
            assert!(
                query.contains(expected),
                "entity query must declare signature sub-capture {expected}"
            );
        }
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
