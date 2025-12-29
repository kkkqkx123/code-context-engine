/*
TSX Types and Hooks-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; Interface Declarations
(interface_declaration
  name: (type_identifier) @name.definition.interface) @definition.interface

; Type Alias Declarations
(type_alias_declaration
  name: (type_identifier) @name.definition.type_alias) @definition.type_alias

; React hooks
(call_expression
  function: (member_expression
    object: (identifier) @_react
    property: (property_identifier) @name.definition.react_hook
    (#eq? @_react "React")
    (#match? @name.definition.react_hook "^use[A-Z]"))) @definition.react_hook
`;