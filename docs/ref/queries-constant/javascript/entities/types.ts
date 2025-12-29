/*
JavaScript Type-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
Note: JavaScript doesn't have native types like TypeScript, but this captures similar constructs
*/
export default `
; Type annotations (TypeScript/Flow style but may appear in JS with JSDoc)
(type_annotation) @name.definition.type_annotation

; Type alias declarations (TypeScript/Flow style)
(type_alias_declaration
  name: (type_identifier) @name.definition.type_alias) @definition.type_alias

; Interface declarations (TypeScript/Flow style)
(interface_declaration
  name: (type_identifier) @name.definition.interface) @definition.interface

; Enum declarations (TypeScript style)
(enum_declaration
  name: (identifier) @name.definition.enum) @definition.enum

; Namespace declarations (TypeScript style)
(namespace_declaration
  name: (identifier) @name.definition.namespace) @definition.namespace

; Type parameters
(type_parameters) @name.definition.type_parameters
(type_arguments) @name.definition.type_arguments

; JSDoc type comments that act like type definitions
(comment
  (#match? @name.definition.jsdoc_type "^/\\*\\*[^*]*@type")) @definition.jsdoc_type

; Object patterns that define types (destructuring)
(object_pattern
  (object_pattern
    (pair
      key: (property_identifier) @name.definition.pattern_property))) @definition.pattern_type

; Array patterns that define types
(array_pattern) @name.definition.array_pattern_type

; Function type annotations
(function
  type_parameters: (type_parameters) @name.definition.function_type_parameters) @definition.function_type

; Class expressions that define types
(class_expression
  name: (identifier) @name.definition.class_type) @definition.class_type

; Generic type calls
(call_expression
  function: (identifier) @name.definition.generic_type
  type_arguments: (type_arguments)) @definition.generic_type

; Type predicates (TypeScript style)
(function_declaration
  name: (identifier) @name.definition.type_predicate
  return_type: (type_annotation
    (predicate_type))) @definition.type_predicate
`;