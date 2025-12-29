/*
CSS Property-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; Declarations and properties - primary code structure
(declaration
  (property_name) @name.definition.property) @definition.declaration

; Values in declarations - important for understanding styling
(declaration
  (integer_value) @definition.integer_value)
(declaration
  (float_value) @definition.float_value)
(declaration
  (color_value) @definition.color_value)
(declaration
  (string_value) @definition.string_value)
(declaration
  (plain_value) @definition.plain_value)

; CSS variables (custom properties) - important for theming
(declaration
  (property_name) @_prop
  (#match? @_prop "^--")) @definition.css_variable

; Important declarations - important for priority rules
(declaration
  (important) @definition.important)

; Function calls in values - important for dynamic styling
(call_expression
  (function_name) @name.definition.function) @definition.function_call

; Binary expressions in values - important for calculations
(declaration
  (binary_expression) @definition.binary_expression)

; Unit values - important for understanding dimensions
(integer_value
  (unit) @name.definition.unit) @definition.unit_value

(float_value
  (unit) @name.definition.unit) @definition.unit_value
`;