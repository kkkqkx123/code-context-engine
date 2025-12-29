/*
C++ Type-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; Type definitions - important for custom types
(type_definition
  type: (_)
  declarator: (type_identifier) @name.definition.type) @definition.type

; Enum declarations - important for enumerated types
(enum_specifier
  name: (type_identifier) @name.definition.enum) @definition.enum

; Concept definitions (C++20) - important for template constraints
(concept_definition
  name: (identifier) @name.definition.concept) @definition.concept

; Auto type deduction - important for type inference
(declaration
  type: (auto)
  declarator: (init_declarator
    declarator: (identifier) @name.definition.auto_var)) @definition.auto_var
`;