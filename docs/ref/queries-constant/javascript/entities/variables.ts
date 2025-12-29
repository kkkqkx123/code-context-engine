/*
JavaScript Variable-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; Variable declarations
(lexical_declaration) @name.definition.lexical_declaration
(variable_declaration) @name.definition.variable_declaration

; Constants (UPPER_CASE naming convention)
(lexical_declaration
  (variable_declarator
    name: (identifier) @name.definition.constant
    (#match? @name.definition.constant "^[A-Z_][A-Z0-9_]*$"))) @definition.constant

; JSON object definitions
(object) @object.definition

; JSON object key-value pairs
(pair
  key: (string) @property.name.definition
  value: [
    (object) @object.value
    (array) @array.value
    (string) @string.value
    (number) @number.value
    (true) @boolean.value
    (false) @boolean.value
    (null) @null.value
  ]
) @property.definition

; JSON array definitions
(array) @array.definition
`;