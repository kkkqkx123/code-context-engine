/*
Python Type and Decorator-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; Type annotations with field names for better context
[
  (typed_parameter
    name: (identifier)? @parameter.name
    type: (type) @parameter.type)
  (typed_default_parameter
    name: (identifier)? @parameter.name
    type: (type) @parameter.type)
  (type) @type.hint
  (type_alias_statement
    name: (identifier) @type.alias.name
    value: (type) @type.alias.value)
] @definition.type

; Generic and union types with anchor for precise matching
[
  (generic_type
    (identifier) @generic.type.name)
  (union_type
    (type) @union.type.element)
] @definition.complex.type

; Documentation elements
[
  (comment) @documentation.comment
  (expression_statement
    (string) @docstring.content) @definition.docstring
] @definition.documentation

; Parameter types with field names
[
  (parameters
    (identifier) @parameter.name)
  (default_parameter
    name: (identifier) @parameter.name
    value: (expression) @parameter.default)
] @definition.parameter
`;