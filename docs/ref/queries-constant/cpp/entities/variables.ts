/*
C++ Variable-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; Variable declarations with initialization - primary code structure
(declaration
  type: (_)
  declarator: (init_declarator
    declarator: (identifier) @name.definition.variable)) @definition.variable

; Structured bindings (C++17) - important for unpacking
(declaration
  (structured_binding_declarator
    (identifier) @name.definition.binding)) @definition.binding

`;