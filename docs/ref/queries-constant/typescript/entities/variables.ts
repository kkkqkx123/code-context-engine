/*
TypeScript Variable-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; Variable declarations with var - important for understanding state
(variable_declaration
  (variable_declarator
    name: (identifier) @name.definition.variable)) @definition.variable

; Lexical declarations with let/const - important for modern JavaScript
(lexical_declaration
  (variable_declarator
    name: (identifier) @name.definition.variable)) @definition.variable

; Const declarations - important for understanding constants
(lexical_declaration
  "const"
  (variable_declarator
    name: (identifier) @name.definition.constant)) @definition.constant

; Let declarations - important for block-scoped variables
(lexical_declaration
  "let"
  (variable_declarator
    name: (identifier) @name.definition.let_variable)) @definition.variable



; Assignment expressions - important for understanding state changes
(assignment_expression
  left: (identifier) @name.assignment) @definition.assignment
`;