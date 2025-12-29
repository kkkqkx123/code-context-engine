/*
TypeScript Interface-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; Interface declarations - important for type definitions
(interface_declaration
  name: (type_identifier) @name.definition.interface) @definition.interface

; Generic interface declarations - important for generic type understanding
(interface_declaration
  name: (type_identifier) @name.definition.interface
  type_parameters: (type_parameters)) @definition.generic_interface
`;