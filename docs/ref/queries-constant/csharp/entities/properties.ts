/*
C# Property and Variable-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; Property declarations - important for encapsulation
(property_declaration
  name: (identifier) @name.definition.property) @definition.property

; Indexer declarations - important for indexed access
(indexer_declaration
  name: (identifier) @name.definition.indexer) @definition.indexer

; Event declarations - important for event handling
(event_declaration
  name: (identifier) @name.definition.event) @definition.event

; Delegate declarations - important for function pointers
(delegate_declaration
  name: (identifier) @name.definition.delegate) @definition.delegate
`;