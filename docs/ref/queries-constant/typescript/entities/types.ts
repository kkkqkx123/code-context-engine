/*
TypeScript Type-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; Type alias declarations - important for type definitions
(type_alias_declaration
  name: (type_identifier) @name.definition.type) @definition.type

; Generic type alias declarations - important for generic type understanding
(type_alias_declaration
  name: (type_identifier) @name.definition.type
  type_parameters: (type_parameters)) @definition.generic_type_alias

; Enum declarations - important for understanding enumerated types
(enum_declaration
  name: (identifier) @name.definition.enum) @definition.enum

; Namespace declarations - important for understanding code organization
(internal_module
  name: (identifier) @name.definition.namespace) @definition.namespace
`;