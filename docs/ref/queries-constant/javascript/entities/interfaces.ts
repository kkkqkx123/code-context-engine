/*
JavaScript Interface-like Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
Note: JavaScript doesn't have native interfaces, but this captures similar constructs
*/
export default `
; Object patterns that act like interfaces
(assignment_expression
  left: (identifier) @name.definition.interface
  right: (object)) @definition.interface

; Class declarations that could be interface-like
(class_declaration
  name: (identifier) @name.definition.interface) @definition.interface

; Function declarations that define API contracts
(function_declaration
  name: (identifier) @name.definition.interface
  (#match? @name.definition.interface "^(create|define|spec|contract|protocol).*$")) @definition.interface

; Variable declarations with object literals that define interfaces
(lexical_declaration
  (variable_declarator
    name: (identifier) @name.definition.interface
    value: (object))) @definition.interface

; Export statements that define interface-like objects
(export_statement
  (lexical_declaration
    (variable_declarator
      name: (identifier) @name.definition.interface
      value: (object)))) @definition.interface
`;