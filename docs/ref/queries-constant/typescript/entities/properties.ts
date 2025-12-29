/*
TypeScript Property-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; Public field definitions - important for class structure
(public_field_definition
  (property_identifier) @name.definition.property) @definition.property
`;