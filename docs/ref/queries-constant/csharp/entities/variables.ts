/*
C# Variable-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; Variable declarations - important for local variables
(local_declaration_statement
  (variable_declaration
    (variable_declarator
      name: (identifier) @name.definition.variable))) @definition.variable

; Field declarations - important for class-level variables
(field_declaration
  (variable_declaration
    (variable_declarator
      name: (identifier) @name.definition.field))) @definition.field

; Constant declarations - important for constants
(constant_declaration
  (variable_declaration
    (variable_declarator
      name: (identifier) @name.definition.constant))) @definition.constant

; Loop variables - important for iteration
(for_each_statement
  left: (identifier) @name.definition.loop_variable) @definition.loop_variable

; Catch variables - important for exception handling
(catch_declaration
  name: (identifier) @name.definition.catch_variable) @definition.catch_variable

; Using declaration - important for resource management
(using_declaration
  (variable_declaration
    (variable_declarator
      name: (identifier) @name.definition.using_variable))) @definition.using_variable

; Declaration expressions - important for pattern matching
(declaration_expression
 name: (identifier) @name.definition.declaration_expression) @definition.declaration_expression
`;